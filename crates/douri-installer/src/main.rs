//! Dōri Custom Installer (Longshot-inspired dark minimal UI).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui::{self, pos2, vec2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Vec2};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Embedded application icon (256x256 PNG).
const ICON_BYTES: &[u8] = include_bytes!("../../prowave-app/assets/icon-256.png");

/// Embedded installer payload (zip file created by xtask package-installers).
/// Falls back to an empty zip when building standalone.
const PAYLOAD_ZIP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.zip"));

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct LocaleChoice {
    name: &'static str,
    code: &'static str,
}

const LOCALES: &[LocaleChoice] = &[
    LocaleChoice {
        name: "English (Default)",
        code: "en",
    },
    LocaleChoice {
        name: "日本語",
        code: "ja",
    },
    LocaleChoice {
        name: "简体中文",
        code: "zh-Hans",
    },
    LocaleChoice {
        name: "Français",
        code: "fr",
    },
    LocaleChoice {
        name: "Deutsch",
        code: "de",
    },
    LocaleChoice {
        name: "Español",
        code: "es",
    },
    LocaleChoice {
        name: "Português (Brasil)",
        code: "pt-BR",
    },
    LocaleChoice {
        name: "Português (Portugal)",
        code: "pt-PT",
    },
    LocaleChoice {
        name: "Norsk",
        code: "nb",
    },
    LocaleChoice {
        name: "Svenska",
        code: "sv",
    },
    LocaleChoice {
        name: "Suomi",
        code: "fi",
    },
    LocaleChoice {
        name: "Polski",
        code: "pl",
    },
    LocaleChoice {
        name: "Українська",
        code: "uk",
    },
    LocaleChoice {
        name: "हिन्दी",
        code: "hi",
    },
    LocaleChoice {
        name: "Bahasa Indonesia",
        code: "id",
    },
    LocaleChoice {
        name: "Filipino",
        code: "fil",
    },
];

#[derive(Clone, Debug)]
enum InstallMsg {
    Progress(f32, String),
    Finished(PathBuf),
    Error(String),
}

#[derive(PartialEq, Eq)]
enum InstallState {
    Ready,
    Installing,
    Finished(PathBuf),
    Failed(String),
}

struct InstallerApp {
    target_dir: PathBuf,
    selected_locale_idx: usize,
    show_locale_menu: bool,
    create_shortcuts: bool,
    state: InstallState,
    progress: f32,
    status_label: String,
    tx: Sender<InstallMsg>,
    rx: Receiver<InstallMsg>,
    icon_texture: Option<egui::TextureHandle>,
    cancel_flag: Arc<AtomicBool>,
    screenshot_path: Option<PathBuf>,
    screenshot_frames: u32,
}

impl InstallerApp {
    fn new(cc: &eframe::CreationContext<'_>, screenshot_path: Option<PathBuf>) -> Self {
        install_phosphor_fonts(&cc.egui_ctx);

        let (tx, rx) = channel();

        // Load 256x256 icon into egui texture
        let icon_texture = image::load_from_memory(ICON_BYTES).ok().map(|img| {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let pixels = rgba.into_raw();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
            cc.egui_ctx
                .load_texture("douri_logo", color_image, egui::TextureOptions::LINEAR)
        });

        // Set default installation directory according to OS
        let default_dir = default_install_dir();

        Self {
            target_dir: default_dir,
            selected_locale_idx: 0,
            show_locale_menu: false,
            create_shortcuts: true,
            state: InstallState::Ready,
            progress: 0.0,
            status_label: "Ready to install".to_string(),
            tx,
            rx,
            icon_texture,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            screenshot_path,
            screenshot_frames: 0,
        }
    }

    fn start_installation(&mut self) {
        self.state = InstallState::Installing;
        self.progress = 0.05;
        self.status_label = "Preparing installation...".to_string();

        let tx = self.tx.clone();
        let target_dir = self.target_dir.clone();
        let locale_code = LOCALES[self.selected_locale_idx].code;
        let create_shortcuts = self.create_shortcuts;
        let cancel = self.cancel_flag.clone();

        std::thread::spawn(move || {
            let res = run_install(
                &target_dir,
                locale_code,
                create_shortcuts,
                &cancel,
                |prog, msg| {
                    let _ = tx.send(InstallMsg::Progress(prog, msg.to_string()));
                },
            );

            match res {
                Ok(exe_path) => {
                    let _ = tx.send(InstallMsg::Progress(
                        1.0,
                        "Installation complete!".to_string(),
                    ));
                    let _ = tx.send(InstallMsg::Finished(exe_path));
                }
                Err(e) => {
                    let _ = tx.send(InstallMsg::Error(e));
                }
            }
        });
    }

    fn poll_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                InstallMsg::Progress(p, status) => {
                    self.progress = p;
                    self.status_label = status;
                }
                InstallMsg::Finished(exe_path) => {
                    self.state = InstallState::Finished(exe_path);
                    self.status_label = "Dōri was successfully installed.".to_string();
                }
                InstallMsg::Error(err) => {
                    self.state = InstallState::Failed(err.clone());
                    self.status_label = format!("Installation failed: {err}");
                }
            }
        }
    }
}

fn install_phosphor_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "phosphor".into(),
        egui::FontData::from_static(phosphor_egui::bytes::regular::FONT).into(),
    );
    fonts.font_data.insert(
        "phosphor-bold".into(),
        egui::FontData::from_static(phosphor_egui::bytes::bold::FONT).into(),
    );
    if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        vec.insert(vec.len().min(1), "phosphor".into());
        vec.insert(vec.len().min(1), "phosphor-bold".into());
    }
    ctx.set_fonts(fonts);
}

fn default_install_dir() -> PathBuf {
    if cfg!(windows) {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local_app_data).join("Programs").join("Dori")
        } else if let Ok(user_profile) = std::env::var("USERPROFILE") {
            PathBuf::from(user_profile)
                .join("AppData")
                .join("Local")
                .join("Programs")
                .join("Dori")
        } else {
            PathBuf::from("C:\\Program Files\\Dori")
        }
    } else {
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("douri")
        } else {
            PathBuf::from("/usr/local/share/douri")
        }
    }
}

fn write_language_config(code: &str) {
    let write_to = |dir: PathBuf| {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("language.txt"), code);
    };

    if cfg!(windows) {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let base = PathBuf::from(appdata);
            write_to(base.join("prowave"));
            write_to(base.join("douri"));
        }
    } else if let Ok(home) = std::env::var("HOME") {
        let config_base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(home).join(".config"));
        write_to(config_base.join("prowave"));
        write_to(config_base.join("douri"));
    }
}

fn run_install(
    target_dir: &Path,
    locale_code: &str,
    create_shortcuts: bool,
    _cancel: &Arc<AtomicBool>,
    mut on_progress: impl FnMut(f32, &str),
) -> Result<PathBuf, String> {
    on_progress(0.10, "Creating target directory...");
    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Cannot create {}: {e}", target_dir.display()))?;

    // Extract embedded payload if non-empty
    if PAYLOAD_ZIP.len() > 30 {
        on_progress(0.20, "Extracting application files...");
        let cursor = std::io::Cursor::new(PAYLOAD_ZIP);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("Invalid zip: {e}"))?;
        let count = archive.len();

        for i in 0..count {
            let mut file = archive.by_index(i).map_err(|e| format!("Zip entry: {e}"))?;
            let raw_name = file
                .name()
                .map_err(|e| format!("Zip entry name: {e}"))?
                .to_string();
            let entry_name = raw_name.replace('\\', "/");
            let out_path = target_dir.join(&entry_name);

            if file.is_dir() {
                let _ = std::fs::create_dir_all(&out_path);
            } else {
                if let Some(parent) = out_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let mut out_file = std::fs::File::create(&out_path)
                    .map_err(|e| format!("Cannot create {}: {e}", out_path.display()))?;
                std::io::copy(&mut file, &mut out_file)
                    .map_err(|e| format!("Cannot extract {}: {e}", out_path.display()))?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Some(mode) = file.unix_mode() {
                        let _ = std::fs::set_permissions(
                            &out_path,
                            std::fs::Permissions::from_mode(mode),
                        );
                    } else if out_path.to_string_lossy().contains("/bin/") {
                        let _ = std::fs::set_permissions(
                            &out_path,
                            std::fs::Permissions::from_mode(0o755),
                        );
                    }
                }
            }

            let prog = 0.20 + (i as f32 / count.max(1) as f32) * 0.50;
            if i % 10 == 0 || i + 1 == count {
                on_progress(
                    prog,
                    &format!("Extracting {} ({}/{count})...", raw_name, i + 1),
                );
            }
        }
    }

    on_progress(0.75, "Configuring language and user settings...");
    write_language_config(locale_code);

    let installed_exe = if cfg!(windows) {
        target_dir.join("douri.exe")
    } else {
        let in_bin = target_dir.join("bin").join("douri");
        if in_bin.exists() {
            in_bin
        } else {
            target_dir.join("douri")
        }
    };

    if create_shortcuts {
        on_progress(0.85, "Creating system shortcuts and file associations...");
        configure_system_integration(target_dir, &installed_exe)?;
    }

    on_progress(0.98, "Finishing installation...");
    std::thread::sleep(std::time::Duration::from_millis(150));

    Ok(installed_exe)
}

fn configure_system_integration(target_dir: &Path, installed_exe: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        // 1. Windows Shortcuts (.lnk)
        let exe_str = installed_exe.to_string_lossy().into_owned();

        if let Ok(appdata) = std::env::var("APPDATA") {
            let start_menu_dir = PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            let _ = std::fs::create_dir_all(&start_menu_dir);
            let lnk_path = start_menu_dir.join("Dōri.lnk");
            let mut link = mslnk::ShellLink::new(&exe_str).map_err(|e| e.to_string())?;
            link.set_working_dir(Some(target_dir.to_string_lossy().into_owned()));
            let _ = link.create_lnk(&lnk_path);
        }

        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            let desktop_dir = PathBuf::from(userprofile).join("Desktop");
            let lnk_path = desktop_dir.join("Dōri.lnk");
            if let Ok(mut link) = mslnk::ShellLink::new(&exe_str) {
                link.set_working_dir(Some(target_dir.to_string_lossy().into_owned()));
                let _ = link.create_lnk(&lnk_path);
            }
        }

        // 2. Registry File Associations (.douri and .prowave)
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok((classes, _)) = hkcu.create_subkey("Software\\Classes") {
            // .douri
            if let Ok((k, _)) = classes.create_subkey(".douri") {
                let _ = k.set_value("", &"Dori.Project");
            }
            // .prowave
            if let Ok((k, _)) = classes.create_subkey(".prowave") {
                let _ = k.set_value("", &"Dori.Project");
            }
            // Dori.Project ProgID
            if let Ok((progid, _)) = classes.create_subkey("Dori.Project") {
                let _ = progid.set_value("", &"Dōri Project");
                if let Ok((icon_k, _)) = progid.create_subkey("DefaultIcon") {
                    let _ = icon_k.set_value("", &format!("\"{exe_str}\",0"));
                }
                if let Ok((cmd_k, _)) = progid.create_subkey("shell\\open\\command") {
                    let _ = cmd_k.set_value("", &format!("\"{exe_str}\" \"%1\""));
                }
            }
        }

        // 3. Uninstall Entry
        if let Ok((uninstall, _)) =
            hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Dori")
        {
            let _ = uninstall.set_value("DisplayName", &"Dōri");
            let _ = uninstall.set_value("DisplayVersion", &"0.3.0");
            let _ = uninstall.set_value("Publisher", &"triadwoozie");
            let _ = uninstall.set_value("InstallLocation", &target_dir.to_string_lossy().as_ref());
            let _ = uninstall.set_value("DisplayIcon", &exe_str);
            let _ = uninstall.set_value("UninstallString", &format!("\"{exe_str}\" --uninstall"));
        }
    }

    #[cfg(unix)]
    {
        let _ = target_dir;
        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(home);

            // 1. Create symlink ~/.local/bin/douri -> installed executable
            let bin_dir = home_path.join(".local").join("bin");
            let _ = std::fs::create_dir_all(&bin_dir);
            let symlink_path = bin_dir.join("douri");
            let _ = std::fs::remove_file(&symlink_path);
            let _ = std::os::unix::fs::symlink(installed_exe, &symlink_path);

            // 2. Install desktop entry in ~/.local/share/applications/io.github.triadwoozie.douri.desktop
            let apps_dir = home_path.join(".local").join("share").join("applications");
            let _ = std::fs::create_dir_all(&apps_dir);
            let desktop_path = apps_dir.join("io.github.triadwoozie.douri.desktop");

            let desktop_content = format!(
                "[Desktop Entry]\n\
                 Type=Application\n\
                 Name=Dōri\n\
                 Comment=Digital Audio Workstation\n\
                 Exec={} %F\n\
                 Icon=io.github.triadwoozie.douri\n\
                 StartupWMClass=io.github.triadwoozie.douri\n\
                 Terminal=false\n\
                 Categories=AudioVideo;Audio;Music;Midi;Sequencer;\n\
                 MimeType=application/x-douri;application/x-prowave;\n",
                installed_exe.display()
            );
            let _ = std::fs::write(&desktop_path, desktop_content);

            // 3. Install icon to ~/.local/share/icons/hicolor/512x512/apps/
            let icon_dir = home_path
                .join(".local")
                .join("share")
                .join("icons")
                .join("hicolor")
                .join("512x512")
                .join("apps");
            let _ = std::fs::create_dir_all(&icon_dir);
            let _ = std::fs::write(icon_dir.join("io.github.triadwoozie.douri.png"), ICON_BYTES);
            let _ = std::fs::write(icon_dir.join("douri.png"), ICON_BYTES);

            // 4. Update desktop database & icon cache if available
            let _ = std::process::Command::new("update-desktop-database")
                .arg(&apps_dir)
                .output();
            let _ = std::process::Command::new("gtk-update-icon-cache")
                .args(["-f", "-t"])
                .arg(
                    home_path
                        .join(".local")
                        .join("share")
                        .join("icons")
                        .join("hicolor"),
                )
                .output();
        }
    }

    Ok(())
}

impl eframe::App for InstallerApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.086, 0.094, 0.118, 1.0] // #16181E
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_messages();

        if let Some(ref path) = self.screenshot_path {
            self.screenshot_frames += 1;
            ui.ctx().request_repaint();
            if self.screenshot_frames == 2 {
                let path = path.clone();
                ui.ctx().request_screenshot(move |image| {
                    let [w, h] = image.size;
                    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                    let _ = image::save_buffer(
                        &path,
                        &bytes,
                        w as u32,
                        h as u32,
                        image::ColorType::Rgba8,
                    );
                });
            }
            if self.screenshot_frames >= 4 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // Repaint continuously during installation
        if self.state == InstallState::Installing {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }

        let bg_color = Color32::from_rgb(0x16, 0x18, 0x1E);
        let border_color = Color32::from_rgb(0x26, 0x29, 0x32);
        let rect = ui.max_rect();

        // Fill background and draw outer rounded border
        ui.painter()
            .rect_filled(rect, CornerRadius::same(12), bg_color);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(12),
            Stroke::new(1.0, border_color),
            egui::StrokeKind::Inside,
        );

        // Custom Titlebar
        let titlebar_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 38.0));
        let free_drag_rect =
            Rect::from_min_max(rect.min, pos2(rect.max.x - 70.0, rect.min.y + 38.0));
        let drag_resp = ui.interact(
            free_drag_rect,
            ui.scope_id().with("window_drag"),
            Sense::click_and_drag(),
        );
        if drag_resp.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        // Top-Left: Icon + Title
        if let Some(ref tex) = self.icon_texture {
            let icon_pos = pos2(rect.min.x + 14.0, rect.min.y + 10.0);
            let icon_rect = Rect::from_min_size(icon_pos, vec2(18.0, 18.0));
            ui.painter().image(
                tex.id(),
                icon_rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        ui.painter().text(
            pos2(rect.min.x + 38.0, rect.min.y + 19.0),
            egui::Align2::LEFT_CENTER,
            "Dōri",
            egui::FontId::proportional(13.0),
            Color32::from_rgb(0xE5, 0xE7, 0xEB),
        );

        // Top-Right: Minimize & Close buttons
        let close_pos = pos2(rect.max.x - 28.0, rect.min.y + 19.0);
        let close_rect = Rect::from_center_size(close_pos, vec2(22.0, 22.0));
        let close_resp = ui.interact(close_rect, ui.scope_id().with("btn_close"), Sense::click());
        let close_hover = close_resp.hovered();
        ui.painter().circle_filled(
            close_pos,
            11.0,
            if close_hover {
                Color32::from_rgb(0xEF, 0x44, 0x44)
            } else {
                Color32::from_rgb(0x23, 0x26, 0x2F)
            },
        );
        ui.painter().text(
            close_pos,
            egui::Align2::CENTER_CENTER,
            phosphor_egui::regular::X,
            egui::FontId::proportional(12.0),
            Color32::from_rgb(0xFA, 0xFA, 0xFA),
        );
        if close_resp.clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }

        let min_pos = pos2(rect.max.x - 56.0, rect.min.y + 19.0);
        let min_rect = Rect::from_center_size(min_pos, vec2(22.0, 22.0));
        let min_resp = ui.interact(min_rect, ui.scope_id().with("btn_min"), Sense::click());
        let min_hover = min_resp.hovered();
        ui.painter().circle_filled(
            min_pos,
            11.0,
            if min_hover {
                Color32::from_rgb(0x2E, 0x32, 0x3E)
            } else {
                Color32::from_rgb(0x23, 0x26, 0x2F)
            },
        );
        ui.painter().text(
            min_pos,
            egui::Align2::CENTER_CENTER,
            phosphor_egui::regular::MINUS,
            egui::FontId::proportional(12.0),
            Color32::from_rgb(0xFA, 0xFA, 0xFA),
        );
        if min_resp.clicked() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }

        // Hero and main content
        let content_rect = Rect::from_min_max(pos2(rect.min.x, titlebar_rect.max.y), rect.max);

        ui.allocate_ui(content_rect.size(), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);

                // Centered 56x56 icon with dark-blue halo
                let (icon_rect, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                ui.painter().circle_filled(
                    icon_rect.center(),
                    38.0,
                    Color32::from_rgba_unmultiplied(18, 105, 204, 35),
                );
                if let Some(ref tex) = self.icon_texture {
                    ui.painter().image(
                        tex.id(),
                        icon_rect,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }

                ui.add_space(10.0);

                // Headline
                ui.label(
                    RichText::new("Install Dōri")
                        .size(22.0)
                        .color(Color32::from_rgb(0xFF, 0xFF, 0xFF))
                        .strong(),
                );

                // Subtitle
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Set up Dōri Digital Audio Workstation on your system.")
                        .size(12.0)
                        .color(Color32::from_rgb(0x9C, 0xA3, 0xAF)),
                );

                ui.add_space(16.0);

                // Primary Action Button / Progress Area
                match &self.state {
                    InstallState::Ready => {
                        let (btn_rect, btn_resp) =
                            ui.allocate_exact_size(vec2(260.0, 42.0), Sense::click());
                        let hovered = btn_resp.hovered();
                        let bg_btn = if hovered {
                            Color32::from_rgb(0xF3, 0xF4, 0xF6)
                        } else {
                            Color32::from_rgb(0xFF, 0xFF, 0xFF)
                        };
                        ui.painter()
                            .rect_filled(btn_rect, CornerRadius::same(8), bg_btn);

                        let center_x = btn_rect.center().x;
                        let center_y = btn_rect.center().y;
                        ui.painter().text(
                            pos2(center_x - 36.0, center_y),
                            egui::Align2::CENTER_CENTER,
                            phosphor_egui::regular::DOWNLOAD_SIMPLE,
                            egui::FontId::proportional(17.0),
                            Color32::from_rgb(0x14, 0x15, 0x18),
                        );
                        ui.painter().text(
                            pos2(center_x + 8.0, center_y),
                            egui::Align2::CENTER_CENTER,
                            "Install",
                            egui::FontId::proportional(15.0),
                            Color32::from_rgb(0x14, 0x15, 0x18),
                        );

                        if btn_resp.clicked() {
                            self.start_installation();
                        }
                    }
                    InstallState::Installing => {
                        ui.add_space(4.0);
                        let (bar_rect, _) =
                            ui.allocate_exact_size(vec2(320.0, 8.0), Sense::hover());
                        ui.painter().rect_filled(
                            bar_rect,
                            CornerRadius::same(4),
                            Color32::from_rgb(0x26, 0x29, 0x32),
                        );
                        let fill_width = bar_rect.width() * self.progress.clamp(0.0, 1.0);
                        let fill_rect =
                            Rect::from_min_size(bar_rect.min, vec2(fill_width, bar_rect.height()));
                        ui.painter().rect_filled(
                            fill_rect,
                            CornerRadius::same(4),
                            Color32::from_rgb(0x12, 0x69, 0xCC),
                        );

                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(&self.status_label)
                                .size(11.0)
                                .color(Color32::from_rgb(0x9C, 0xA3, 0xAF)),
                        );
                    }
                    InstallState::Finished(exe_path) => {
                        let (btn_rect, btn_resp) =
                            ui.allocate_exact_size(vec2(260.0, 42.0), Sense::click());
                        let hovered = btn_resp.hovered();
                        let bg_btn = if hovered {
                            Color32::from_rgb(0xF3, 0xF4, 0xF6)
                        } else {
                            Color32::from_rgb(0xFF, 0xFF, 0xFF)
                        };
                        ui.painter()
                            .rect_filled(btn_rect, CornerRadius::same(8), bg_btn);

                        let center_x = btn_rect.center().x;
                        let center_y = btn_rect.center().y;
                        ui.painter().text(
                            pos2(center_x - 50.0, center_y),
                            egui::Align2::CENTER_CENTER,
                            phosphor_egui::regular::PLAY,
                            egui::FontId::proportional(16.0),
                            Color32::from_rgb(0x14, 0x15, 0x18),
                        );
                        ui.painter().text(
                            pos2(center_x + 8.0, center_y),
                            egui::Align2::CENTER_CENTER,
                            "Launch Dōri",
                            egui::FontId::proportional(15.0),
                            Color32::from_rgb(0x14, 0x15, 0x18),
                        );

                        if btn_resp.clicked() {
                            let _ = std::process::Command::new(exe_path).spawn();
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }

                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Installation completed successfully!")
                                .size(11.0)
                                .color(Color32::from_rgb(0x10, 0xB9, 0x81)),
                        );
                    }
                    InstallState::Failed(err) => {
                        ui.label(
                            RichText::new(format!("Error: {err}"))
                                .size(12.0)
                                .color(Color32::from_rgb(0xEF, 0x44, 0x44)),
                        );
                        ui.add_space(8.0);
                        if ui.button("Retry").clicked() {
                            self.state = InstallState::Ready;
                        }
                    }
                }

                ui.add_space(18.0);

                // Three Options Panel (Language, Location, Integration)
                let options_width = 460.0;
                ui.allocate_ui_with_layout(
                    vec2(options_width, 100.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        // Option 1: Language
                        ui.horizontal(|ui| {
                            ui.set_width(options_width);
                            ui.label(
                                RichText::new("Language")
                                    .size(12.0)
                                    .color(Color32::from_rgb(0x9C, 0xA3, 0xAF)),
                            );
                            ui.add_space(8.0);

                            let cur_locale = LOCALES[self.selected_locale_idx].name;
                            ui.label(
                                RichText::new(cur_locale)
                                    .size(12.0)
                                    .color(Color32::from_rgb(0xE5, 0xE7, 0xEB))
                                    .strong(),
                            );

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if self.state == InstallState::Ready
                                        && ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new("Change")
                                                        .size(11.0)
                                                        .color(Color32::from_rgb(0x60, 0xA5, 0xFA)),
                                                )
                                                .frame(false),
                                            )
                                            .clicked()
                                    {
                                        self.show_locale_menu = !self.show_locale_menu;
                                    }
                                },
                            );
                        });

                        if self.show_locale_menu && self.state == InstallState::Ready {
                            ui.add_space(4.0);
                            egui::Frame::new()
                                .fill(Color32::from_rgb(0x1F, 0x22, 0x2A))
                                .corner_radius(CornerRadius::same(6))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(0x2E, 0x32, 0x3E)))
                                .inner_margin(6.0)
                                .show(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .max_height(80.0)
                                        .show(ui, |ui| {
                                            for (idx, loc) in LOCALES.iter().enumerate() {
                                                let is_selected = idx == self.selected_locale_idx;
                                                let label = if is_selected {
                                                    format!("✓ {}", loc.name)
                                                } else {
                                                    format!("  {}", loc.name)
                                                };
                                                if ui.selectable_label(is_selected, label).clicked()
                                                {
                                                    self.selected_locale_idx = idx;
                                                    self.show_locale_menu = false;
                                                }
                                            }
                                        });
                                });
                        }

                        ui.add_space(8.0);

                        // Option 2: Location
                        ui.horizontal(|ui| {
                            ui.set_width(options_width);
                            ui.label(
                                RichText::new("Location")
                                    .size(12.0)
                                    .color(Color32::from_rgb(0x9C, 0xA3, 0xAF)),
                            );
                            ui.add_space(8.0);

                            let path_str = self.target_dir.to_string_lossy();
                            let display_path = if path_str.len() > 36 {
                                format!("...{}", &path_str[path_str.len() - 33..])
                            } else {
                                path_str.into_owned()
                            };

                            ui.label(
                                RichText::new(display_path)
                                    .size(12.0)
                                    .color(Color32::from_rgb(0xE5, 0xE7, 0xEB)),
                            )
                            .on_hover_text(self.target_dir.to_string_lossy());

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if self.state == InstallState::Ready
                                        && ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new("Change")
                                                        .size(11.0)
                                                        .color(Color32::from_rgb(0x60, 0xA5, 0xFA)),
                                                )
                                                .frame(false),
                                            )
                                            .clicked()
                                    {
                                        if let Some(folder) = rfd::FileDialog::new()
                                            .set_directory(&self.target_dir)
                                            .pick_folder()
                                        {
                                            self.target_dir = folder;
                                        }
                                    }
                                },
                            );
                        });

                        ui.add_space(8.0);

                        // Option 3: Desktop integration toggle
                        if self.state == InstallState::Ready {
                            ui.horizontal(|ui| {
                                ui.set_width(options_width);
                                ui.checkbox(
                                    &mut self.create_shortcuts,
                                    RichText::new("Create desktop shortcut & file associations")
                                        .size(11.0)
                                        .color(Color32::from_rgb(0x9C, 0xA3, 0xAF)),
                                );
                            });
                        }
                    },
                );
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    #[allow(unused_mut)]
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    #[cfg(windows)]
    {
        let backends = eframe::wgpu::Backends::from_env()
            .unwrap_or(eframe::wgpu::Backends::VULKAN | eframe::wgpu::Backends::GL);
        wgpu_options.wgpu_setup =
            eframe::egui_wgpu::WgpuSetup::CreateNew(eframe::egui_wgpu::WgpuSetupCreateNew {
                instance_descriptor: eframe::wgpu::InstanceDescriptor {
                    backends,
                    flags: eframe::wgpu::InstanceFlags::from_build_config().with_env(),
                    backend_options: eframe::wgpu::BackendOptions::from_env_or_default(),
                    memory_budget_thresholds: eframe::wgpu::MemoryBudgetThresholds::default(),
                    display: None,
                },
                ..eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle()
            });
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 400.0])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true),
        wgpu_options,
        ..Default::default()
    };

    let screenshot_path = std::env::args()
        .position(|a| a == "--screenshot")
        .and_then(|idx| std::env::args().nth(idx + 1))
        .map(PathBuf::from);

    eframe::run_native(
        "Dōri Setup",
        options,
        Box::new(move |cc| {
            // Configure custom dark palette
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = Color32::from_rgb(0x16, 0x18, 0x1E);
            visuals.window_fill = Color32::from_rgb(0x16, 0x18, 0x1E);
            visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(0x26, 0x29, 0x32));
            cc.egui_ctx.set_visuals(visuals);

            Ok(Box::new(InstallerApp::new(cc, screenshot_path)))
        }),
    )
}
