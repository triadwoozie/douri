//! Methods for the interface itself: the open question (dialog), the menus, the windows and
//! panels, the view, the selection, pictures of the window, the audio settings and quitting.
use super::params::{place, P};
use super::protocol::{Code, RpcError};
use super::registry::{m, p, Method, Reply, Ty};
use super::{ok, state, Core, Cx};
use crate::file_commands::{Answer, FileCmd};
use crate::i18n::{tr, trf};
use crate::{menus, App, Bottom};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ------------------------------------------------------------------ the open question

/// A question the app is asking in a modal window. While one is open the methods that change
/// the project are refused.
pub struct Dialog {
    pub id: &'static str,
    pub text: String,
    pub buttons: Vec<&'static str>,
    pub detail: Vec<String>,
}

impl Dialog {
    pub fn to_json(&self) -> Value {
        json!({"id": self.id, "text": self.text, "buttons": self.buttons, "detail": self.detail})
    }
}

/// The question open now, if any (the one on top when there are several).
pub fn open_dialog(app: &App) -> Option<Dialog> {
    if let Some(cmd) = app.ui.ask_save {
        return Some(Dialog {
            id: "save_changes",
            text: trf(
                "Do you want to save the changes made to the project \"{}\"?",
                &[&app.project.name],
            ),
            buttons: vec!["save", "dont_save", "cancel"],
            detail: vec![format!(
                "asked before: {}",
                match cmd {
                    FileCmd::Quit => "quitting",
                    FileCmd::New | FileCmd::NewFrom(_) => "a new project",
                    FileCmd::Close => "closing the project",
                    FileCmd::Open | FileCmd::OpenRecent => "opening another project",
                    _ => "a file command",
                }
            )],
        });
    }
    if app.ui.ask_revert {
        return Some(Dialog {
            id: "revert",
            text: trf(
                "Do you want to revert the project \"{}\" to the last saved version?",
                &[&app.project.name],
            ),
            buttons: vec!["revert", "cancel"],
            detail: Vec::new(),
        });
    }
    if !app.ui.missing_audio.is_empty() {
        return Some(Dialog {
            id: "missing_audio",
            text: tr("These audio files are missing:").to_string(),
            buttons: vec!["ok"],
            detail: app.ui.missing_audio.clone(),
        });
    }
    if let Some(c) = app.autosave.offers.first() {
        return Some(Dialog {
            id: "recover",
            text: trf(
                "Recover the project \"{}\" as it was on {}?",
                &[&c.name, &crate::recovery::local_time_text(c.written)],
            ),
            buttons: vec!["recover", "discard"],
            detail: vec![tr(
                "If you discard it, the recovery copy and its recordings are deleted.",
            )
            .to_string()],
        });
    }
    None
}

fn ui_dialog(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    ok(json!({"dialog": open_dialog(cx.app).map(|d| d.to_json())}))
}

fn ui_dialog_answer(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let button = params.req_str("button")?;
    let Some(d) = open_dialog(cx.app) else {
        return Err(RpcError::new(
            Code::NotFound,
            "no question is open in the app (ui.dialog shows the one that is)",
        ));
    };
    if !d.buttons.contains(&button) {
        return Err(RpcError::invalid(
            "button",
            format!(
                "the open question (\"{}\") has the buttons: {}; got \"{button}\"",
                d.text,
                d.buttons.join(", ")
            ),
        )
        .with("dialog", d.to_json()));
    }
    let app = &mut *cx.app;
    match d.id {
        "save_changes" => {
            let Some(cmd) = app.ui.ask_save else {
                return ok(json!({"answered": d.id, "button": button}));
            };
            match button {
                "save" => {
                    // Saving a project that has no file asks for one in a system window,
                    // which nobody is there to answer: the file comes in `path`.
                    let target = match (app.path.clone(), params.path("path")?) {
                        (_, Some(given)) => given,
                        (Some(own), None) => own,
                        (None, None) => {
                            return Err(RpcError::invalid(
                                "path",
                                "the project has never been saved: give `path`, the .prowave file to save it to",
                            ))
                        }
                    };
                    if !app.save_to(&target) {
                        return Err(RpcError::failed(
                            app.ui
                                .status
                                .clone()
                                .unwrap_or_else(|| format!("could not save {}", target.display())),
                        ));
                    }
                    app.ui.ask_save = None;
                    app.answer_save_prompt(cmd, Answer::DontSave, cx.ctx);
                }
                "dont_save" => {
                    app.ui.ask_save = None;
                    app.answer_save_prompt(cmd, Answer::DontSave, cx.ctx);
                }
                _ => {
                    app.ui.ask_save = None;
                    app.answer_save_prompt(cmd, Answer::Cancel, cx.ctx);
                }
            }
            if cmd == FileCmd::Quit && button != "cancel" {
                cx.core.quit = true;
            }
        }
        "revert" => {
            app.ui.ask_revert = false;
            if button == "revert" {
                if let Some(path) = app.path.clone() {
                    app.open(&path, cx.ctx);
                }
            }
        }
        "missing_audio" => app.ui.missing_audio.clear(),
        "recover" => {
            if let Some(c) = app.autosave.offers.first().cloned() {
                if button == "recover" {
                    app.recover(&c, cx.ctx);
                    app.autosave.offers.clear();
                } else {
                    crate::recovery::discard(&c);
                    app.autosave.offers.remove(0);
                }
            }
        }
        _ => {}
    }
    cx.ctx.request_repaint();
    let next = open_dialog(cx.app).map(|d| d.to_json());
    ok(json!({"answered": d.id, "button": button, "dialog": next}))
}

// ------------------------------------------------------------------ menus

/// Menu entries that open a system file window, which blocks the app until a person answers
/// it: the control method named does the same without one.
const SYSTEM_WINDOWS: &[(&str, &str)] = &[
    ("Open…", "project.open"),
    ("Save As…", "project.save_as"),
    ("Save A Copy As…", "project.save_copy"),
    ("MIDI File…", "import.midi"),
    ("Audio File…", "import.audio"),
    ("Selection as MIDI File…", "export.midi"),
    ("Paint Image to Audio…", ""),
    ("SMAF File…", ""),
    ("Region through EffeTune Chain…", ""),
];

fn system_window(app: &App, label: &str) -> Option<&'static str> {
    if label == "Save" && app.path.is_none() {
        return Some("project.save_as");
    }
    SYSTEM_WINDOWS
        .iter()
        .find(|(l, _)| *l == label)
        .map(|(_, method)| *method)
}

fn menu_entry(app: &App, menu: &str, parent: Option<&str>, label: &'static str) -> Value {
    let checkable = menus::is_checkable(label);
    let mut o = json!({
        "id": label,
        "title": tr(label),
        "menu": menu,
        "enabled": menus::enabled(label, &app.project, &app.ui),
    });
    if let Some(map) = o.as_object_mut() {
        if let Some(parent) = parent {
            map.insert("submenu".into(), parent.into());
        }
        if checkable {
            map.insert(
                "checked".into(),
                menus::checked(label, &app.project, &app.ui).into(),
            );
        }
        if let Some(method) = system_window(app, label) {
            map.insert("opens_system_window".into(), true.into());
            if !method.is_empty() {
                map.insert("use_instead".into(), method.into());
            }
        }
    }
    o
}

fn menu_entries(app: &App) -> Vec<Value> {
    let mut out = Vec::new();
    for (menu, items) in menus::MENUS {
        for &item in menus::shown(items).iter() {
            if item == "-" || item == "Open Recent" {
                continue;
            }
            match menus::submenu(item) {
                Some(children) => {
                    for &child in menus::shown(children).iter() {
                        if child != "-" {
                            out.push(menu_entry(app, menu, Some(item), child));
                        }
                    }
                }
                None => out.push(menu_entry(app, menu, None, item)),
            }
        }
    }
    out
}

fn menu_list(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let mut entries = menu_entries(cx.app);
    if let Some(menu) = params.str("menu") {
        entries.retain(|e| {
            e.get("menu")
                .and_then(Value::as_str)
                .is_some_and(|m| m.eq_ignore_ascii_case(menu))
        });
    }
    let recent: Vec<String> = cx
        .app
        .ui
        .recent
        .paths()
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    ok(json!({"entries": entries, "open_recent": recent}))
}

fn menu_invoke(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let id = params.req_str("id")?;
    let entries = menu_entries(cx.app);
    let find = |want: &str| {
        entries.iter().find(|e| {
            e.get("id")
                .and_then(Value::as_str)
                .is_some_and(|l| l == want || l.trim_end_matches('…') == want)
        })
    };
    let Some(entry) = find(id) else {
        return Err(RpcError::not_found(format!(
            "there is no menu entry \"{id}\" (menu.list gives every entry's id; ids are the English titles)"
        )));
    };
    let label = entry.get("id").and_then(Value::as_str).unwrap_or(id);
    // The app's own list holds the 'static label the menus work with.
    let Some(label) = static_label(label) else {
        return Err(RpcError::not_found(format!(
            "there is no menu entry \"{id}\""
        )));
    };
    if !menus::enabled(label, &cx.app.project, &cx.app.ui) {
        return Err(RpcError::new(
            Code::NotSupported,
            format!("the menu entry \"{label}\" is greyed out now (nothing for it to act on, or it is not built)"),
        ));
    }
    if let Some(method) = system_window(cx.app, label) {
        if params.bool("allow_system_window") != Some(true) {
            let instead = if method.is_empty() {
                String::new()
            } else {
                format!(" Use `{method}` instead.")
            };
            return Err(RpcError::new(
                Code::NotSupported,
                format!(
                    "\"{label}\" opens a system file window that only a person at the Mac can answer, and the app takes no requests until it is closed.{instead} To open it anyway pass allow_system_window: true."
                ),
            ));
        }
    }
    if open_dialog(cx.app).is_some() && FileCmd::from_menu(label).is_some() {
        return Err(RpcError::new(
            Code::DialogOpen,
            "a question is open in the app: answer it first (ui.dialog, ui.dialog.answer)",
        ));
    }
    let checkable = menus::is_checkable(label);
    menus::apply(label, &mut cx.app.project, &mut cx.app.ui);
    cx.ctx.request_repaint();
    let mut out = json!({"invoked": label});
    if let Some(map) = out.as_object_mut() {
        if checkable {
            map.insert(
                "checked".into(),
                menus::checked(label, &cx.app.project, &cx.app.ui).into(),
            );
        }
        if FileCmd::from_menu(label).is_some() {
            map.insert(
                "note".into(),
                "file commands run at the end of the next frame; a question may open (ui.dialog)"
                    .into(),
            );
        }
    }
    ok(out)
}

/// The label as the menus' own lists hold it.
fn static_label(label: &str) -> Option<&'static str> {
    for (_, items) in menus::MENUS {
        for &item in items {
            if item == label {
                return Some(item);
            }
            if let Some(children) = menus::submenu(item) {
                if let Some(found) = children.iter().find(|c| **c == label) {
                    return Some(*found);
                }
            }
        }
    }
    None
}

// ------------------------------------------------------------------ windows and panels

const WINDOWS: &[&str] = &[
    "mixer",
    "piano_roll",
    "score",
    "channel_rack",
    "pitch_editor",
    "piano_roll_window",
    "inspector",
    "list_editors",
    "toolbar",
    "notes",
    "tips",
    "midi_monitor",
    "midi_fx",
    "free_plugins",
    "kits",
    "project_settings",
    "tempo_track",
    "marker_track",
    "signature_track",
    "master_track",
    "automation",
];

const SETTINGS_TABS: &[&str] = &[
    "general",
    "synchronization",
    "metronome",
    "recording",
    "tuning",
    "audio",
    "midi",
];

fn settings_tab(name: &str) -> Option<crate::project_settings::SettingsTab> {
    use crate::project_settings::SettingsTab as T;
    Some(match name {
        "general" => T::General,
        "synchronization" => T::Synchronization,
        "metronome" => T::Metronome,
        "recording" => T::Recording,
        "tuning" => T::Tuning,
        "audio" => T::Audio,
        "midi" => T::Midi,
        _ => return None,
    })
}

fn window_open(app: &App, id: &str) -> bool {
    let s = &app.ui;
    match id {
        "mixer" => s.show_mixer && s.bottom == Bottom::Mixer,
        "piano_roll" => s.bottom == Bottom::PianoRoll && !s.score_tab,
        "score" => s.bottom == Bottom::PianoRoll && s.score_tab,
        "channel_rack" => s.bottom == Bottom::ChannelRack,
        "pitch_editor" => s.bottom == Bottom::PitchEditor,
        "piano_roll_window" => s.piano_window,
        "inspector" => s.show_inspector,
        "list_editors" => s.show_lists,
        "toolbar" => s.show_toolbar,
        "notes" => s.show_notes,
        "tips" => s.show_tips,
        "midi_monitor" => s.show_midi_monitor,
        "midi_fx" => s.show_midi_fx,
        "free_plugins" => s.free_plugins.open,
        "kits" => s.kits.open,
        "project_settings" => s.settings_tab.is_some(),
        "tempo_track" => s.tracks.show_tempo,
        "marker_track" => s.tracks.show_markers,
        "signature_track" => s.tracks.show_signatures,
        "master_track" => app.project.show_master_track,
        "automation" => s.auto.mode,
        _ => false,
    }
}

fn ui_windows(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    let list: Vec<Value> = WINDOWS
        .iter()
        .map(|id| json!({"id": id, "open": window_open(cx.app, id)}))
        .collect();
    let bottom = match cx.app.ui.bottom {
        Bottom::Mixer if cx.app.ui.show_mixer => "mixer",
        Bottom::Mixer => "none",
        Bottom::PianoRoll if cx.app.ui.score_tab => "score",
        Bottom::PianoRoll => "piano_roll",
        Bottom::ChannelRack => "channel_rack",
        Bottom::PitchEditor => "pitch_editor",
    };
    ok(json!({
        "windows": list,
        "bottom_panel": bottom,
        "project_chooser": cx.app.ui.chooser.is_some(),
        "note": "mixer, piano_roll, score, channel_rack and pitch_editor share the bottom panel: opening one closes the other; plug-in windows are opened with plugin.window",
    }))
}

fn ui_window(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let id = params.req_str("id")?;
    let open = params.bool("open").unwrap_or(true);
    let app = &mut *cx.app;
    let s = &mut app.ui;
    match id {
        "mixer" => {
            s.show_mixer = open;
            if open {
                s.bottom = Bottom::Mixer;
            }
        }
        "piano_roll" | "score" | "channel_rack" | "pitch_editor" => {
            let want = match id {
                "channel_rack" => Bottom::ChannelRack,
                "pitch_editor" => Bottom::PitchEditor,
                _ => Bottom::PianoRoll,
            };
            if open {
                if id == "pitch_editor" && s.bottom != Bottom::PitchEditor {
                    s.pitch_ed.region = None;
                }
                s.bottom = want;
                if want == Bottom::PianoRoll {
                    s.score_tab = id == "score";
                }
            } else if s.bottom == want {
                s.bottom = Bottom::Mixer;
            }
        }
        "piano_roll_window" => s.piano_window = open,
        "inspector" => s.show_inspector = open,
        "list_editors" => s.show_lists = open,
        "toolbar" => s.show_toolbar = open,
        "notes" => s.show_notes = open,
        "tips" => s.show_tips = open,
        "midi_monitor" => s.show_midi_monitor = open,
        "midi_fx" => s.show_midi_fx = open,
        "free_plugins" => s.free_plugins.open = open,
        "kits" => s.kits.open = open,
        "project_settings" => {
            s.settings_tab = if open {
                Some(
                    params
                        .str("tab")
                        .and_then(settings_tab)
                        .or(s.settings_tab)
                        .unwrap_or(crate::project_settings::SettingsTab::General),
                )
            } else {
                None
            };
        }
        "tempo_track" => s.tracks.show_tempo = open,
        "marker_track" => s.tracks.show_markers = open,
        "signature_track" => s.tracks.show_signatures = open,
        "master_track" => app.project.show_master_track = open,
        "automation" => crate::automation_ui::set_mode(&mut app.project, &mut s.auto, open),
        _ => {}
    }
    cx.ctx.request_repaint();
    ok(json!({"id": id, "open": window_open(cx.app, id)}))
}

// ------------------------------------------------------------------ view and selection

fn ui_view(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let p = &cx.app.project;
    let start = place(p, params, "")?;
    let end = place(p, params, "end_")?;
    let v = &mut cx.app.ui.tracks;
    let bpb = cx.app.project.bar_len(0).max(1e-6) as f32;
    if params.bool("fit") == Some(true) {
        let w = v.lanes_w;
        crate::tracks_area::zoom_to_fit(&cx.app.project, v, w);
    }
    if let Some(w) = params.f32("bar_width") {
        v.bar_w = w.clamp(6.0, 600.0);
    }
    if let (Some(a), Some(b)) = (start, end) {
        if b <= a {
            return Err(RpcError::invalid(
                "end_bar",
                "the end of the view must be after its start",
            ));
        }
        let bars = cx.app.project.beats_to_bar(b) - cx.app.project.beats_to_bar(a);
        // The width of the lanes as last drawn (1000 points before the first frame).
        let lanes = if v.lanes_w > 50.0 { v.lanes_w } else { 1000.0 };
        v.bar_w = (lanes / bars.max(1e-3) as f32).clamp(6.0, 600.0);
    }
    if let Some(a) = start {
        v.scroll_x = (a as f32 * v.bar_w / bpb).max(0.0);
        v.catch = false;
    }
    if let Some(h) = params.f32("track_height") {
        v.row_h = h.clamp(20.0, 400.0);
    }
    if let Some(on) = params.bool("follow_playhead") {
        v.catch = on;
    }
    cx.ctx.request_repaint();
    let v = &cx.app.ui.tracks;
    ok(json!({
        "bar_width": v.bar_w,
        "track_height": v.row_h,
        "first_bar": super::params::round3(f64::from(v.scroll_x / v.bar_w.max(1e-3)) + 1.0),
        "follow_playhead": v.catch,
        "lanes_width": v.lanes_w,
    }))
}

fn ui_select(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    cx.core.ids.reconcile(&cx.app.project);
    if let Some(v) = params.get("track") {
        let ti = state::track_ref(&cx.app.project, &cx.core.ids, v, "track")?;
        cx.app.project.selected_track = ti;
    }
    if let Some(list) = params.arr("regions") {
        let mut picked = Vec::new();
        for v in list {
            picked.push(state::region_ref(
                &cx.app.project,
                &cx.core.ids,
                v,
                "regions",
            )?);
        }
        cx.app.ui.tracks.select_many(&picked);
        if let Some((t, _)) = picked.first() {
            if !params.has("track") {
                cx.app.project.selected_track = *t;
            }
        }
    }
    if params.bool("all_regions") == Some(true) {
        cx.app.ui.tracks.select_all(&cx.app.project);
    }
    if params.bool("none") == Some(true) {
        cx.app.ui.tracks.clear_selection();
        cx.app.ui.piano.selected.clear();
    }
    cx.ctx.request_repaint();
    let ids = &cx.core.ids;
    ok(json!({
        "track": ids.track_id(cx.app.project.selected_track),
        "regions": cx.app.ui.tracks.selection().into_iter().map(|(t, r)| ids.region_id(t, r)).collect::<Vec<_>>(),
    }))
}

fn ui_status(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let text = cx.app.ui.status.clone();
    if params.bool("clear") == Some(true) {
        cx.app.ui.status = None;
    }
    ok(json!({
        "status": text,
        "cpu_load": super::params::round3(f64::from(cx.app.ui.cpu_load)),
        "playback_load": super::params::round3(f64::from(cx.app.ui.playback_load)),
        "overloads": cx.app.ui.overloads,
        "audio_running": cx.app.sync.is_some(),
    }))
}

// ------------------------------------------------------------------ pictures

/// A picture asked for and not written yet.
pub struct Shot {
    pub conn: u64,
    pub id: Option<Value>,
    pub path: PathBuf,
    pub scale: f32,
    pub asked: Instant,
    /// Set by whoever answers the request first: the picture arriving, or the time limit.
    pub done: Arc<AtomicBool>,
}

/// The longest a picture is waited for.
const SHOT_LIMIT: Duration = Duration::from_secs(15);

/// A numbered file in the app's scratch folder.
pub fn scratch_file(core: &Core, stem: &str, ext: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = if core.headless || cfg!(test) {
        std::env::temp_dir().join(format!("prowave-control-{}", std::process::id()))
    } else {
        crate::project_io::recordings_dir().join("control")
    };
    dir.join(format!("{stem}-{n:03}.{ext}"))
}

/// Writes `image` (as the screen has it) to `path` as a PNG, `scale` times its size.
pub fn save_png(
    image: &egui::ColorImage,
    path: &std::path::Path,
    scale: f32,
) -> Result<(u32, u32), String> {
    let [w, h] = image.size;
    let img = image::RgbaImage::from_raw(w as u32, h as u32, image.as_raw().to_vec())
        .ok_or("the picture has no pixels")?;
    save_rgba(img, path, scale)
}

pub fn save_rgba(
    mut img: image::RgbaImage,
    path: &std::path::Path,
    scale: f32,
) -> Result<(u32, u32), String> {
    // A window has no see-through parts: the picture is opaque.
    for px in img.pixels_mut() {
        px.0[3] = 255;
    }
    if scale < 0.999 {
        let w = ((img.width() as f32 * scale).round() as u32).max(1);
        let h = ((img.height() as f32 * scale).round() as u32).max(1);
        img = image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    img.save_with_format(path, image::ImageFormat::Png)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((img.width(), img.height()))
}

pub fn shot_result(path: &std::path::Path, size: (u32, u32), window: &str) -> Value {
    json!({
        "path": path.display().to_string(),
        "width": size.0,
        "height": size.1,
        "window": window,
    })
}

fn ui_screenshot(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let path = match params.path("path")? {
        Some(p) => p,
        None => scratch_file(cx.core, "screenshot", "png"),
    };
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        != Some("png".into())
    {
        return Err(RpcError::invalid(
            "path",
            "the picture is written as a PNG: give a path that ends in .png",
        ));
    }
    let scale = params.f32("scale").unwrap_or(0.5).clamp(0.05, 1.0);
    let window = params.str("window").unwrap_or("main");
    let viewport = match window {
        "main" => egui::ViewportId::ROOT,
        "piano_roll_window" => {
            if !cx.app.ui.piano_window {
                return Err(RpcError::not_found(
                    "the Piano Roll's own window is not open (ui.window {\"id\":\"piano_roll_window\"} opens it)",
                ));
            }
            crate::piano_roll::viewport_id()
        }
        _ => egui::ViewportId::ROOT,
    };
    let shot = Shot {
        conn: cx.conn,
        id: cx.id.clone(),
        path: path.clone(),
        scale,
        asked: Instant::now(),
        done: Arc::new(AtomicBool::new(false)),
    };
    if cx.core.headless {
        // The headless loop draws the frame itself and writes the picture after it.
        cx.core.shots.push(shot);
        return Ok(Reply::Later);
    }
    let (shared, conn, id, done) = (
        cx.core.shared.clone(),
        cx.conn,
        cx.id.clone(),
        shot.done.clone(),
    );
    let window_name = window.to_string();
    let callback = egui::ScreenshotCallback::new(move |image: Arc<egui::ColorImage>| {
        if done.swap(true, Ordering::AcqRel) {
            return;
        }
        let result = save_png(&image, &path, scale)
            .map(|size| shot_result(&path, size, &window_name))
            .map_err(RpcError::failed);
        if let Some(id) = &id {
            shared.respond(conn, id, &result);
        }
    });
    cx.ctx
        .send_viewport_cmd_to(viewport, egui::ViewportCommand::Screenshot(callback));
    cx.ctx.request_repaint_of(viewport);
    cx.core.shots.push(shot);
    Ok(Reply::Later)
}

/// Answers the pictures that never came (the window could not be painted).
pub fn tend_shots(core: &mut Core) {
    if core.headless {
        return;
    }
    let shared = core.shared.clone();
    core.shots.retain(|s| {
        if s.done.load(Ordering::Acquire) {
            return false;
        }
        if s.asked.elapsed() < SHOT_LIMIT {
            return true;
        }
        if !s.done.swap(true, Ordering::AcqRel) {
            if let Some(id) = &s.id {
                shared.respond(
                    s.conn,
                    id,
                    &Err(RpcError::new(
                        Code::Timeout,
                        "the window was not painted within 15 seconds, so there is no picture",
                    )),
                );
            }
        }
        false
    });
}

// ------------------------------------------------------------------ audio settings

fn set_checked(app: &mut App, label: &'static str, on: bool) {
    if menus::checked(label, &app.project, &app.ui) != on {
        menus::apply(label, &mut app.project, &mut app.ui);
    }
}

fn audio_json(app: &mut App) -> Value {
    app.ui.mixer.inputs.refresh();
    let info = app.sync.as_ref().map(|s| s.audio_info());
    let inputs = &app.ui.mixer.inputs;
    let buffers: Vec<u32> = menus::submenu("I/O Buffer Size")
        .unwrap_or(&[])
        .iter()
        .filter_map(|l| l.split(' ').next()?.parse().ok())
        .collect();
    json!({
        "running": app.sync.is_some(),
        "output": info.as_ref().map(|(name, _)| name.clone()),
        "sample_rate": info.as_ref().map(|(_, rate)| *rate),
        "buffer_frames": app.ui.io_buffer,
        "buffer_choices": buffers,
        "process_buffer_frames": app.ui.process_buffer,
        "low_latency": app.ui.low_latency,
        "software_monitoring": app.ui.software_monitoring,
        "round_trip_ms": super::params::round3(f64::from(app.ui.round_trip_ms)),
        "output_device": inputs.output,
        "input_device": inputs.device,
        "output_devices": inputs.outputs.iter().map(|d| json!({"id": d.id, "name": d.name, "default": d.is_default})).collect::<Vec<_>>(),
        "input_devices": inputs.devices.iter().map(|d| json!({"id": d.id, "name": d.name, "channels": d.channels, "default": d.is_default})).collect::<Vec<_>>(),
    })
}

fn audio_get(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    ok(audio_json(cx.app))
}

fn audio_set(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let app = &mut *cx.app;
    app.ui.mixer.inputs.refresh();
    if let Some(frames) = params.i64("buffer_frames") {
        let label = menus::submenu("I/O Buffer Size")
            .unwrap_or(&[])
            .iter()
            .find(|l| l.split(' ').next().and_then(|n| n.parse::<i64>().ok()) == Some(frames));
        let Some(label) = label else {
            return Err(RpcError::invalid(
                "buffer_frames",
                format!(
                    "the I/O buffer can be: {}; got {frames}",
                    menus::submenu("I/O Buffer Size")
                        .unwrap_or(&[])
                        .iter()
                        .filter_map(|l| l.split(' ').next())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        };
        menus::apply(label, &mut app.project, &mut app.ui);
    }
    if let Some(on) = params.bool("low_latency") {
        set_checked(app, "Low Latency Mode", on);
    }
    if let Some(on) = params.bool("software_monitoring") {
        set_checked(app, "Software Monitoring", on);
    }
    if let Some(id) = params.str("output_device") {
        let pick = if id == "default" {
            None
        } else if app.ui.mixer.inputs.outputs.iter().any(|d| d.id == id) {
            Some(id.to_string())
        } else {
            return Err(RpcError::not_found(format!(
                "there is no output device with the id \"{id}\" (audio.get lists them; \"default\" is the system's)"
            )));
        };
        app.ui.mixer.inputs.pick_output(pick);
    }
    if let Some(id) = params.str("input_device") {
        let pick = if id == "default" {
            None
        } else if app.ui.mixer.inputs.devices.iter().any(|d| d.id == id) {
            Some(id.to_string())
        } else {
            return Err(RpcError::not_found(format!(
                "there is no input device with the id \"{id}\" (audio.get lists them; \"default\" is the system's)"
            )));
        };
        app.ui.mixer.inputs.pick(pick);
    }
    cx.ctx.request_repaint();
    ok(audio_json(cx.app))
}

// ------------------------------------------------------------------ events, quitting

fn events_subscribe(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    cx.core.shared.set_subscribed(cx.conn, true);
    cx.ctx.request_repaint();
    ok(json!({"subscribed": true, "events": super::events::NAMES}))
}

fn events_unsubscribe(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    cx.core.shared.set_subscribed(cx.conn, false);
    ok(json!({"subscribed": false}))
}

fn app_quit(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let choice = params.req_str("if_unsaved")?;
    let unsaved = cx.app.has_unsaved_changes();
    if unsaved {
        match choice {
            "cancel" => {
                return ok(json!({"quitting": false, "unsaved_changes": true}));
            }
            "save" => {
                let target = match (cx.app.path.clone(), params.path("path")?) {
                    (_, Some(given)) => given,
                    (Some(own), None) => own,
                    (None, None) => {
                        return Err(RpcError::invalid(
                            "path",
                            "the project has never been saved: give `path`, the .prowave file to save it to",
                        ))
                    }
                };
                if !cx.app.save_to(&target) {
                    return Err(RpcError::failed(
                        cx.app
                            .ui
                            .status
                            .clone()
                            .unwrap_or_else(|| format!("could not save {}", target.display())),
                    ));
                }
            }
            _ => {
                // As the question's Don't Save: the exit keeps what would be lost for good.
                cx.app.autosave.dont_save = true;
            }
        }
    }
    cx.app.ui.ask_save = None;
    cx.app.autosave.quit_confirmed = true;
    cx.core.quit = true;
    cx.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    cx.ctx.request_repaint();
    ok(json!({"quitting": true, "unsaved_changes": unsaved, "choice": choice}))
}

pub fn methods() -> Vec<Method> {
    let [bar, position, seconds, beat] = super::params::place_params("");
    let [end_bar, end_position, end_seconds, end_beat] = super::params::place_params("end_");
    vec![
        m("menu.list", "Every menu command the app shows, with whether it can be chosen now.",
            vec![p("menu", Ty::Str, "only this menu (File, Edit, Track, Navigate, Record, Mix, View, Window, Help)")],
            "{entries: [{id, title, menu, submenu?, enabled, checked?, opens_system_window?, use_instead?}], open_recent: [paths]}; an id is the entry's English title",
            menu_list),
        m("menu.invoke", "Chooses a menu command, as a click on it does.",
            vec![
                p("id", Ty::Str, "the entry's id from menu.list (its English title; the trailing … may be left out)").req(),
                p("allow_system_window", Ty::Bool, "also choose an entry that opens a system file window (the app then waits for a person)"),
            ],
            "{invoked, checked?}; changes made by the entry get their Undo step within half a second, as menu clicks do",
            menu_invoke),
        m("ui.dialog", "The question the app is asking in a modal window, if any.",
            vec![],
            "{dialog: null | {id: save_changes|revert|missing_audio|recover, text, buttons, detail}}",
            ui_dialog),
        m("ui.dialog.answer", "Answers the open question, as a click on one of its buttons does.",
            vec![
                p("button", Ty::Str, "one of the buttons ui.dialog lists (save, dont_save, cancel, revert, ok, recover, discard)").req(),
                p("path", Ty::Str, "for save on a project that has no file yet: the .prowave file to save to"),
            ],
            "{answered, button, dialog: the next open question or null}",
            ui_dialog_answer),
        m("ui.windows", "The app's panels and windows and which are open.",
            vec![], "{windows: [{id, open}], bottom_panel, project_chooser}", ui_windows),
        m("ui.window", "Opens or closes a panel or window of the app.",
            vec![
                p("id", Ty::Str, "which one").req().vals(WINDOWS),
                p("open", Ty::Bool, "true to open (the default), false to close"),
                p("tab", Ty::Str, "for project_settings: the tab to show").vals(SETTINGS_TABS),
            ],
            "{id, open}", ui_window),
        m("ui.view", "Zooms and scrolls the Tracks area.",
            vec![
                bar, position, seconds, beat,
                end_bar, end_position, end_seconds, end_beat,
                p("fit", Ty::Bool, "zoom out to show every region (Navigate > Zoom to Fit)"),
                p("bar_width", Ty::Num, "width of one bar").unit("points").range(6.0, 600.0),
                p("track_height", Ty::Num, "height of a track").unit("points").range(20.0, 400.0),
                p("follow_playhead", Ty::Bool, "scroll with the playhead while playing (Catch)"),
            ],
            "{bar_width, track_height, first_bar, follow_playhead, lanes_width}; a start place scrolls there, a start and an end zoom to that range",
            ui_view),
        m("ui.select", "Selects a track and/or regions, as clicks in the Tracks area do.",
            vec![
                p("track", Ty::Any, "the track to select: id, name or number"),
                p("regions", Ty::Arr, "the regions to select (ids or names); the first is the one the Inspector and the editors show"),
                p("all_regions", Ty::Bool, "select every region (Edit > Select All)"),
                p("none", Ty::Bool, "select no region"),
            ],
            "{track, regions}", ui_select),
        m("ui.status", "The app's status line (import errors, engine warnings) and the load meters.",
            vec![p("clear", Ty::Bool, "empty the status line after reading it")],
            "{status, cpu_load, playback_load, overloads, audio_running}", ui_status),
        m("ui.screenshot", "Saves a picture of the app's window as a PNG, also while the app is in the background.",
            vec![
                p("path", Ty::Str, "where to write the .png (default: a numbered file in the session's folder, which is deleted when the app quits)"),
                p("scale", Ty::Num, "size of the picture relative to the window's pixels (default 0.5)").range(0.05, 1.0),
                p("window", Ty::Str, "which window (default main)").vals(&["main", "piano_roll_window"]),
            ],
            "{path, width, height, window}; plug-in windows are the plug-ins' own and are not in the picture",
            ui_screenshot),
        m("audio.get", "The audio engine's device, buffer and monitoring settings, and the devices to choose from.",
            vec![],
            "{running, output, sample_rate, buffer_frames, buffer_choices, process_buffer_frames, low_latency, software_monitoring, round_trip_ms, output_device, input_device, output_devices, input_devices}",
            audio_get),
        m("audio.set", "Changes audio settings of this Mac (not of the project). Sound may drop out while a device or the buffer changes.",
            vec![
                p("buffer_frames", Ty::Int, "the I/O buffer (Record > I/O Buffer Size)").unit("frames"),
                p("low_latency", Ty::Bool, "Record > Low Latency Mode"),
                p("software_monitoring", Ty::Bool, "Record > Software Monitoring"),
                p("output_device", Ty::Str, "the id of an output device from audio.get, or \"default\""),
                p("input_device", Ty::Str, "the id of an input device from audio.get, or \"default\""),
            ],
            "the settings as audio.get gives them", audio_set),
        m("events.subscribe", "Sends this connection a line for each thing that happens in the app, until it closes or unsubscribes.",
            vec![],
            "{subscribed: true, events: [names]}; each event is a line {\"jsonrpc\":\"2.0\",\"method\":\"event\",\"params\":{\"event\":name,...}}",
            events_subscribe),
        m("events.unsubscribe", "Stops the events on this connection.", vec![], "{subscribed: false}", events_unsubscribe),
        m("app.quit", "Quits the app. What happens to unsaved changes must be said; it never waits on a question.",
            vec![
                p("if_unsaved", Ty::Str, "save: save first (to `path`, or the project's file); discard: quit without saving; cancel: do not quit when there are unsaved changes").req().vals(&["save", "discard", "cancel"]),
                p("path", Ty::Str, "for save on a project that has no file yet: the .prowave file to save to"),
            ],
            "{quitting, unsaved_changes}", app_quit),
    ]
}
