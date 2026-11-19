//! Window > Plugin Manager (Logic Pro: Settings > Plug-in Manager), laid out as Logic's: the
//! manufacturers in a list on the left ("Show All" first), the plug-ins of the chosen one in a
//! table (Name, Type, Category, Manufacturer, Path, Status), a search field at the top right,
//! and the scan and Done buttons along the bottom.
use crate::hosted_plugins::{self, Scan};
use crate::i18n::{tr, trf};
use crate::mixer::PLUGIN_NOT_FOUND;
use crate::model::Project;
use crate::plugins;
use crate::UiState;

const ROW: f32 = 18.0;
const COL_FORMAT: f32 = 64.0;
const COL_CATEGORY: f32 = 78.0;
const COL_VENDOR: f32 = 110.0;
const COL_STATUS: f32 = 80.0;

/// One row of the table: six cells.
fn row(ui: &mut egui::Ui, cells: [(egui::RichText, Option<String>); 6], widths: [f32; 6]) {
    ui.horizontal(|ui| {
        for ((cell, tooltip), width) in cells.into_iter().zip(widths) {
            ui.allocate_ui_with_layout(
                egui::vec2(width, ROW),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_width(width);
                    let resp = ui.add(egui::Label::new(cell).truncate());
                    if let Some(tip) = tooltip {
                        resp.on_hover_text(tip);
                    }
                },
            );
        }
    });
}

/// The plug-ins the project uses that are not installed here, once each: name and format.
fn not_found(p: &Project) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    let inserts = p
        .tracks
        .iter()
        .flat_map(|t| t.inserts.iter())
        .chain(p.master_inserts.iter());
    for insert in inserts {
        let used = if insert.hosted != 0 && hosted_plugins::missing(insert.hosted) {
            hosted_plugins::info(insert.hosted).map(|i| (i.name, i.format.name().to_string()))
        } else if let Some(id) = insert.plugin {
            plugins::lookup(id)
                .filter(|r| plugins::resolve_library(r).is_none())
                .map(|r| (r.name, "LADSPA".to_string()))
        } else {
            None
        };
        if let Some(used) = used {
            if !found.contains(&used) {
                found.push(used);
            }
        }
    }
    found
}

/// One plug-in of the table, whatever its format.
struct Entry {
    name: String,
    format: String,
    category: String,
    vendor: String,
    path: String,
    status: String,
    latency: String,
}

/// Everything installed: the hosted plug-ins and the LADSPA effects, by manufacturer.
fn entries(hosted: &[hosted_plugins::PluginInfo], ladspa: &plugins::Catalog) -> Vec<Entry> {
    let loaded = hosted_plugins::loaded();
    let mut all: Vec<Entry> = hosted
        .iter()
        .map(|info| {
            let latency = loaded
                .iter()
                .find(|(name, format, _)| name == &info.name && *format == info.format)
                .map_or(String::new(), |(_, _, samples)| samples.to_string());
            let category = match info.category {
                hosted_plugins::PluginCategory::Instrument => "Instrument",
                hosted_plugins::PluginCategory::Effect => "Effect",
            };
            let status = hosted_plugins::status_for(info);
            let path = if info.format == hosted_plugins::Format::AudioUnit {
                "System".to_string()
            } else {
                info.path.to_string_lossy().to_string()
            };
            Entry {
                name: info.name.clone(),
                format: info.format.name().to_string(),
                category: category.to_string(),
                vendor: info.vendor.clone(),
                path,
                status: status.to_string(),
                latency,
            }
        })
        .collect();
    all.extend(ladspa.effects().map(|info| Entry {
        name: info.name.clone(),
        format: "LADSPA".to_string(),
        category: "Effect".to_string(),
        vendor: info.maker.clone(),
        path: "LADSPA".to_string(),
        status: "Working".to_string(),
        latency: String::new(),
    }));
    for (path, why) in &ladspa.failed {
        all.push(Entry {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string(),
            format: "LADSPA".to_string(),
            category: "Effect".to_string(),
            vendor: "—".to_string(),
            path: format!("{} — {why}", path.to_string_lossy()),
            status: "Incompatible".to_string(),
            latency: String::new(),
        });
    }
    for path in &ladspa.skipped {
        all.push(Entry {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string(),
            format: "LADSPA".to_string(),
            category: "Effect".to_string(),
            vendor: "—".to_string(),
            path: path.to_string_lossy().to_string(),
            status: "Crashed".to_string(),
            latency: String::new(),
        });
    }
    all.sort_by_cached_key(|e| (e.vendor.to_lowercase(), e.name.to_lowercase()));
    all
}

/// The LADSPA libraries that could not be read, and where plug-ins are looked for.
fn ladspa_problems(ui: &mut egui::Ui, ladspa: &plugins::Catalog) {
    if !ladspa.failed.is_empty() {
        ui.strong(tr("Could not be read"));
        for (path, why) in &ladspa.failed {
            ui.label(format!("{} — {why}", path.display()));
        }
    }
    if !ladspa.skipped.is_empty() {
        ui.strong(tr("Left alone after failing three times"));
        for path in &ladspa.skipped {
            ui.label(path.display().to_string());
        }
    }
    ui.strong(tr("Plug-in folders"));
    for dir in plugins::search_paths() {
        ui.weak(dir.display().to_string());
    }
}

/// The Plugin Manager window, while `UiState::show_plugin_manager` is on.
pub fn show(ctx: &egui::Context, p: &Project, s: &mut UiState) {
    if !s.show_plugin_manager {
        return;
    }
    hosted_plugins::scan(false);
    plugins::ensure_scanned();
    let status = hosted_plugins::scan_status();
    let ladspa = plugins::catalog();
    let catalog = entries(&hosted_plugins::catalog(), &ladspa);
    let in_use = not_found(p);
    let mut open = true;
    let mut done = false;
    crate::liquid_glass::glass_window(tr("Plugin Manager"))
        .id(egui::Id::unique("plugin_manager_window"))
        .open(&mut open)
        .default_width(780.0)
        .default_height(500.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                match status {
                    Scan::Running(done, total) => {
                        ui.weak(format!("{} {done} / {total}", tr("Scanning plug-ins…")));
                        // The scan runs on another thread: keep drawing until it is done.
                        ui.ctx().request_repaint();
                    }
                    _ if ladspa.scanning => {
                        ui.weak(tr("Scanning plug-ins…"));
                        ui.ctx().request_repaint();
                    }
                    _ => {
                        ui.weak(trf("{} plug-ins installed", &[&catalog.len()]));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut s.plugin_manager_search)
                            .hint_text(tr("Search"))
                            .desired_width(180.0),
                    );
                });
            });
            ui.separator();
            egui::Panel::bottom("plugin_manager_buttons").show(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let scanning = matches!(status, Scan::Running(..)) || ladspa.scanning;
                    if ui
                        .add_enabled(!scanning, egui::Button::new(tr("Rescan")))
                        .clicked()
                    {
                        hosted_plugins::scan(true);
                        plugins::rescan(false);
                    }
                    if ui
                        .add_enabled(
                            !scanning && !ladspa.skipped.is_empty(),
                            egui::Button::new(tr("Retry Failed Plug-ins")),
                        )
                        .clicked()
                    {
                        plugins::rescan(true);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr("Done")).clicked() {
                            done = true;
                        }
                    });
                });
            });

            // Left sidebar: Category filter, Format filter, Vendor list.
            let mut vendors: Vec<&str> = catalog.iter().map(|i| i.vendor.as_str()).collect();
            vendors.sort_by_key(|v| v.to_lowercase());
            vendors.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

            egui::Panel::left("plugin_manager_sidebar")
                .resizable(false)
                .exact_size(175.0)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Category").strong());
                    let cat_all = s.plugin_manager_category.is_none();
                    if ui.selectable_label(cat_all, "All").clicked() {
                        s.plugin_manager_category = None;
                    }
                    let cat_inst = s.plugin_manager_category.as_deref() == Some("Instruments");
                    if ui.selectable_label(cat_inst, "Instruments").clicked() {
                        s.plugin_manager_category = Some("Instruments".to_string());
                    }
                    let cat_fx = s.plugin_manager_category.as_deref() == Some("Effects");
                    if ui.selectable_label(cat_fx, "Effects").clicked() {
                        s.plugin_manager_category = Some("Effects".to_string());
                    }
                    ui.separator();

                    ui.label(egui::RichText::new("Format").strong());
                    let fmt_all = s.plugin_manager_format.is_none();
                    if ui.selectable_label(fmt_all, "All Formats").clicked() {
                        s.plugin_manager_format = None;
                    }
                    for fmt in ["CLAP", "VST3", "Audio Unit", "LADSPA"] {
                        let chosen = s.plugin_manager_format.as_deref() == Some(fmt);
                        if ui.selectable_label(chosen, fmt).clicked() {
                            s.plugin_manager_format = Some(fmt.to_string());
                        }
                    }
                    ui.separator();

                    ui.label(egui::RichText::new("Manufacturer").strong());
                    egui::ScrollArea::vertical()
                        .id_salt("plugin_manager_vendor_list")
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            let all = s.plugin_manager_vendor.is_none();
                            if ui.selectable_label(all, tr("Show All")).clicked() {
                                s.plugin_manager_vendor = None;
                            }
                            for vendor in vendors {
                                let chosen = s.plugin_manager_vendor.as_deref() == Some(vendor);
                                let label = if vendor.is_empty() { "—" } else { vendor };
                                if ui.selectable_label(chosen, label).clicked() {
                                    s.plugin_manager_vendor = Some(vendor.to_string());
                                }
                            }
                        });
                });

            egui::CentralPanel::default().show(ui, |ui| {
                // The project's plug-ins that are not installed: they keep their place in the
                // project and pass the sound through, until the plug-in is installed again.
                if !in_use.is_empty() {
                    ui.weak(tr("In Use"));
                    for (name, format) in &in_use {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(name).color(PLUGIN_NOT_FOUND));
                            ui.weak(format);
                            ui.label(egui::RichText::new(tr("Not found")).color(PLUGIN_NOT_FOUND));
                        });
                    }
                    ui.separator();
                }

                let query = s.plugin_manager_search.trim().to_lowercase();
                let rows: Vec<&Entry> = catalog
                    .iter()
                    .filter(|info| {
                        match s.plugin_manager_category.as_deref() {
                            Some("Instruments") => info.category == "Instrument",
                            Some("Effects") => info.category == "Effect",
                            _ => true,
                        }
                    })
                    .filter(|info| {
                        s.plugin_manager_format
                            .as_deref()
                            .is_none_or(|f| f.eq_ignore_ascii_case(&info.format))
                    })
                    .filter(|info| {
                        s.plugin_manager_vendor
                            .as_ref()
                            .is_none_or(|v| v.eq_ignore_ascii_case(&info.vendor))
                    })
                    .filter(|info| {
                        query.is_empty()
                            || info.name.to_lowercase().contains(&query)
                            || info.vendor.to_lowercase().contains(&query)
                            || info.path.to_lowercase().contains(&query)
                    })
                    .collect();

                let fixed = COL_FORMAT + COL_CATEGORY + COL_VENDOR + COL_STATUS;
                let rem = (ui.available_width() - fixed - 30.0).max(180.0);
                let col_name = (rem * 0.45).max(90.0);
                let col_path = (rem * 0.55).max(90.0);
                let widths = [col_name, COL_FORMAT, COL_CATEGORY, COL_VENDOR, col_path, COL_STATUS];

                let head = |text: &str| egui::RichText::new(text).weak();
                row(
                    ui,
                    [
                        (head(tr("Name")), None),
                        (head(tr("Type")), None),
                        (head("Category"), None),
                        (head(tr("Manufacturer")), None),
                        (head("Path"), None),
                        (head("Status"), None),
                    ],
                    widths,
                );
                ui.separator();

                if rows.is_empty() && status == Scan::Done {
                    ui.weak(tr("No plug-ins match"));
                }

                egui::ScrollArea::vertical()
                    .id_salt("plugin_manager_table")
                    .auto_shrink(false)
                    .max_height(220.0)
                    .show_rows(ui, ROW, rows.len(), |ui, range| {
                        for info in &rows[range] {
                            let status_color = match info.status.as_str() {
                                "Working" => egui::Color32::from_rgb(100, 200, 100),
                                "Incompatible" => egui::Color32::from_rgb(220, 160, 60),
                                "Crashed" => egui::Color32::from_rgb(230, 80, 80),
                                _ => egui::Color32::GRAY,
                            };
                            let status_tip = if info.latency.is_empty() {
                                Some(info.status.clone())
                            } else {
                                Some(format!("{}: {} ({}: {} samples)", tr("Type"), info.status, tr("Latency"), info.latency))
                            };
                            let full_path = info.path.clone();
                            row(
                                ui,
                                [
                                    (egui::RichText::new(&info.name), Some(info.name.clone())),
                                    (egui::RichText::new(&info.format), None),
                                    (egui::RichText::new(&info.category), None),
                                    (egui::RichText::new(&info.vendor), Some(info.vendor.clone())),
                                    (egui::RichText::new(&info.path).weak(), Some(full_path)),
                                    (egui::RichText::new(&info.status).color(status_color), status_tip),
                                ],
                                widths,
                            );
                        }
                    });

                ui.add_space(8.0);
                // Custom Scan Paths section
                egui::CollapsingHeader::new("Custom Scan Paths")
                    .id_salt("plugin_manager_custom_paths")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut paths = hosted_plugins::custom_paths();
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            if ui.button("Add Path…").clicked() {
                                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                    if !paths.contains(&folder) {
                                        paths.push(folder);
                                        changed = true;
                                    }
                                }
                            }
                            if let Some(idx) = s.plugin_manager_selected_path {
                                if idx < paths.len() {
                                    if ui.button("Remove").clicked() {
                                        paths.remove(idx);
                                        s.plugin_manager_selected_path = None;
                                        changed = true;
                                    }
                                }
                            }
                            if ui.button("Rescan All").clicked() {
                                hosted_plugins::scan(true);
                            }
                        });

                        if paths.is_empty() {
                            ui.weak("No custom plugin directories configured. (Default system paths are always scanned)");
                        } else {
                            for (idx, path) in paths.iter().enumerate() {
                                let is_sel = s.plugin_manager_selected_path == Some(idx);
                                if ui.selectable_label(is_sel, path.to_string_lossy()).clicked() {
                                    s.plugin_manager_selected_path = if is_sel { None } else { Some(idx) };
                                }
                            }
                        }

                        if changed {
                            hosted_plugins::save_custom_paths(&paths);
                            hosted_plugins::scan(true);
                        }
                    });

                ui.add_space(4.0);
                egui::CollapsingHeader::new(tr("LADSPA scan"))
                    .id_salt("plugin_manager_ladspa")
                    .show(ui, |ui| ladspa_problems(ui, &ladspa));
            });
        });
    if !open || done {
        s.show_plugin_manager = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_opens_from_the_menu_and_closes_again() {
        let mut p = crate::model::Project::demo();
        let mut s = UiState::new();
        assert!(crate::menus::enabled("Plugin Manager", &p, &s));
        crate::menus::apply("Plugin Manager", &mut p, &mut s);
        assert!(s.show_plugin_manager);
        let mut h = crate::ui_test::Harness::new(900.0, 600.0);
        h.settle(|ui| show(ui.ctx(), &p, &mut s));
        assert!(s.show_plugin_manager, "drawing it does not close it");
        assert_eq!(
            hosted_plugins::scan_status(),
            Scan::Done,
            "the window starts the scan"
        );
    }
}
