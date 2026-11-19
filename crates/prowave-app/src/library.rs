//! Library (Logic Pro: Library pane): "Library / All Sounds" header, the selected patch's
//! icon and name, Search Sounds, the category column with the patch column next to it,
//! and the Revert / Delete / Save… footer.
//!
//! Choosing a patch loads it on the selected track: the track takes the patch name, its
//! instrument slot and Audio FX chain change, and the icon follows the category. Revert
//! restores what the track had before. The built-in engine has a single synth voice, so a
//! patch does not change the sound yet. Patch names are ProWave's own.
use crate::i18n::tr;
use crate::model::{Project, TrackKind};
use crate::track_icons::{draw_icon, track_icon};
use crate::widgets::{stroke, BTN_BG, TEXT, TEXT_DIM};
use crate::UiState;
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Rect, Sense, Shape, Stroke, Ui};

const PANEL_BG: Color32 = Color32::from_rgb(0x13, 0x16, 0x1e);
const ROW_H: f32 = 25.0;
const SEL_BG: Color32 = Color32::from_rgb(0x1f, 0x26, 0x36);

pub struct Category {
    pub name: &'static str,
    /// Instrument shown in the channel strip's instrument slot.
    pub instrument: Option<&'static str>,
    pub fx: &'static [&'static str],
    pub patches: &'static [&'static str],
}

const EQ_COMP: &[&str] = &["Channel EQ", "Compressor"];
const EQ_COMP_VERB: &[&str] = &["Channel EQ", "Compressor", "ChromaVerb"];
const KEYS_FX: &[&str] = &["Channel EQ", "Tremolo", "Tape Delay"];
const SYNTH_FX: &[&str] = &["Channel EQ", "Chorus", "Tape Delay", "Space D"];

pub static CATEGORIES: &[Category] = &[
    Category {
        name: "User Patches",
        instrument: None,
        fx: &[],
        patches: &[],
    },
    Category {
        name: "Bass",
        instrument: Some("Sampler"),
        fx: EQ_COMP,
        patches: &[
            "Deep Finger Bass",
            "Fretless Glide",
            "Picked Rock Bass",
            "Round Upright",
            "Slap Funk Bass",
            "Sub Synth Bass",
            "Vintage Muted Bass",
        ],
    },
    Category {
        name: "Drum Kit",
        instrument: Some("Drum Kit"),
        fx: EQ_COMP_VERB,
        patches: &[
            "Brushed Jazz Kit",
            "Dry Studio Kit",
            "Garage Room Kit",
            "Live Arena Kit",
            "Tight Funk Kit",
            "Warm Vintage Kit",
        ],
    },
    Category {
        name: "Electronic Drum Kit",
        instrument: Some("Drum Machine"),
        fx: EQ_COMP,
        patches: &[
            "Boom Bap Machine",
            "Classic 808 Kit",
            "Classic 909 Kit",
            "Electro Pulse Kit",
            "Lo-Fi Beat Box",
            "Trap Night Kit",
        ],
    },
    Category {
        name: "Guitar",
        instrument: Some("Sampler"),
        fx: &["Channel EQ", "Compressor", "Space D"],
        patches: &[
            "Bright Steel String",
            "Clean Strat Chords",
            "Nylon Concert",
            "Twelve String Shimmer",
            "Warm Jazz Hollow",
        ],
    },
    Category {
        name: "Guitar Amp",
        instrument: Some("Sampler"),
        fx: &["Amp Designer", "Channel EQ", "Space D"],
        patches: &[
            "British Crunch",
            "Clean Combo",
            "Plexi Lead",
            "Studio Clean",
            "Surf Combo",
            "Tweed Drive",
        ],
    },
    Category {
        name: "Keyboards",
        instrument: Some("E-Piano"),
        fx: KEYS_FX,
        patches: &[
            "Bright Tine Piano",
            "Classic Tine Piano",
            "Dark Tine Ballad",
            "Glassy Reed Piano",
            "Phased Tines",
            "Stage Suitcase",
            "Velvet Reed Piano",
            "Wide Tine Stereo",
        ],
    },
    Category {
        name: "Mallet",
        instrument: Some("Sampler"),
        fx: &["Channel EQ", "Space D"],
        patches: &[
            "Concert Marimba",
            "Soft Vibraphone",
            "Toy Glockenspiel",
            "Warm Xylophone",
        ],
    },
    Category {
        name: "Orchestral",
        instrument: Some("Studio Strings"),
        fx: &["Channel EQ", "Space D"],
        patches: &[
            "Chamber Ensemble",
            "Film Score Tutti",
            "Pizzicato Section",
            "Soft Woodwinds",
        ],
    },
    Category {
        name: "Percussion",
        instrument: Some("Sampler"),
        fx: EQ_COMP,
        patches: &[
            "Conga Set",
            "Frame Drum",
            "Shaker Loop Kit",
            "Tambourine Kit",
        ],
    },
    Category {
        name: "Piano",
        instrument: Some("Sampler"),
        fx: &["Channel EQ", "Compressor", "Space D"],
        patches: &[
            "Bright Concert Grand",
            "Felt Upright",
            "Intimate Studio Grand",
            "Mellow Grand",
            "Pop Rock Grand",
        ],
    },
    Category {
        name: "Quick Sampler",
        instrument: Some("Q-Sampler"),
        fx: EQ_COMP,
        patches: &["Chopped Vocal", "One Shot Hit", "Sliced Breakbeat"],
    },
    Category {
        name: "Studio Horns",
        instrument: Some("Studio Horns"),
        fx: &["Channel EQ", "Space D"],
        patches: &[
            "Brass Section",
            "Soft Flugelhorn",
            "Tenor Sax Solo",
            "Trombone Section",
        ],
    },
    Category {
        name: "Studio Strings",
        instrument: Some("Studio Strings"),
        fx: &["Channel EQ", "Space D"],
        patches: &[
            "Cello Section",
            "Full Strings Legato",
            "Strings Spiccato",
            "Violin Section",
        ],
    },
    Category {
        name: "Synthesizer",
        instrument: Some("Alchemy"),
        fx: SYNTH_FX,
        patches: &[
            "Airy Glass Pad",
            "Analog Drift Pad",
            "Bright Saw Lead",
            "Classic Analog Pad",
            "Detuned Supersaw",
            "Evolving Motion Pad",
            "Fat Mono Bass",
            "Hollow Pulse Lead",
            "Lush Poly Strings",
            "Night Drive Arp",
            "Soft Brass Synth",
            "Warm Juno-style Pad",
        ],
    },
    Category {
        name: "Vintage B3 Organ",
        instrument: Some("Vintage B3"),
        fx: &["Channel EQ", "Overdrive"],
        patches: &[
            "Classic Rock Organ",
            "Gospel Drawbars",
            "Jazz Combo Organ",
            "Soft Ballad Organ",
        ],
    },
    Category {
        name: "Vintage Clav",
        instrument: Some("Vintage Clav"),
        fx: &["Channel EQ", "Phaser"],
        patches: &["Funky Clav", "Muted Clav", "Wah Clav"],
    },
    Category {
        name: "Vintage Electric Piano",
        instrument: Some("E-Piano"),
        fx: KEYS_FX,
        patches: &[
            "Classic Tine Piano",
            "Mellow Reed Piano",
            "Suitcase Tremolo",
        ],
    },
    Category {
        name: "Vintage Mellotron",
        instrument: Some("Mellotron"),
        fx: &["Channel EQ", "Space D"],
        patches: &["Tape Choir", "Tape Flutes", "Tape Strings"],
    },
    Category {
        name: "World",
        instrument: Some("Sampler"),
        fx: &["Channel EQ", "Space D"],
        patches: &["Kalimba", "Koto", "Sitar", "Steel Drums"],
    },
    Category {
        name: "Arpeggiator",
        instrument: Some("Alchemy"),
        fx: SYNTH_FX,
        patches: &["Bouncing Arp", "Gated Pluck Arp", "Rising Sequence"],
    },
    Category {
        name: "Cinematic",
        instrument: Some("Alchemy"),
        fx: SYNTH_FX,
        patches: &["Dark Atmosphere", "Epic Riser", "Tension Drone"],
    },
    Category {
        name: "Legacy",
        instrument: Some("ES2"),
        fx: EQ_COMP,
        patches: &["Analog Brass", "Classic Pluck", "Retro Lead"],
    },
];

/// What the track had before patches were auditioned on it (for Revert).
#[derive(Clone)]
struct Saved {
    track: usize,
    name: String,
    instrument: Option<&'static str>,
    inserts: Vec<crate::model::Insert>,
    category: Option<&'static str>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LibraryTab {
    Sounds,
    Loops,
    Files,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundChip {
    Synth,
    Bass,
    Drums,
}

pub struct LibraryView {
    pub category: Option<usize>,
    pub search: String,
    /// (category, patch) last loaded.
    pub loaded: Option<(usize, usize)>,
    saved: Option<Saved>,
    scroll_left: f32,
    scroll_right: f32,
    pub tab: LibraryTab,
    pub rail_collapsed: bool,
    pub active_chip: Option<SoundChip>,
    pub audition_playing: bool,
    pub audition_volume: f32,
    pub audition_auto: bool,
}

impl Default for LibraryView {
    fn default() -> Self {
        Self {
            category: None,
            search: String::new(),
            loaded: None,
            saved: None,
            scroll_left: 0.0,
            scroll_right: 0.0,
            tab: LibraryTab::Sounds,
            rail_collapsed: false,
            active_chip: None,
            audition_playing: false,
            audition_volume: 0.8,
            audition_auto: true,
        }
    }
}

pub fn badge_for_patch(patch_name: &str, cat_name: &str) -> (Color32, &'static str) {
    let lower_cat = cat_name.to_lowercase();
    let lower_p = patch_name.to_lowercase();
    if lower_cat.contains("synth") || lower_cat.contains("arp") || lower_cat.contains("cinematic") {
        (Color32::from_rgb(0x8b, 0x5c, 0xf6), "S")
    } else if lower_cat.contains("bass") || lower_p.contains("bass") {
        (Color32::from_rgb(0x06, 0xb6, 0xd4), "B")
    } else if lower_cat.contains("drum") || lower_cat.contains("percussion") {
        (Color32::from_rgb(0xf5, 0x9e, 0x0b), "D")
    } else if lower_cat.contains("keyboard")
        || lower_cat.contains("piano")
        || lower_cat.contains("organ")
        || lower_cat.contains("clav")
    {
        (Color32::from_rgb(0x3b, 0x82, 0xf6), "K")
    } else if lower_cat.contains("guitar") {
        (Color32::from_rgb(0xec, 0x48, 0x99), "G")
    } else if lower_cat.contains("string")
        || lower_cat.contains("horn")
        || lower_cat.contains("orchestral")
    {
        (Color32::from_rgb(0x10, 0xb9, 0x81), "O")
    } else {
        (Color32::from_rgb(0x63, 0x66, 0xf1), "P")
    }
}

fn id(key: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    egui::Id::unique(("library", key))
}

fn load_patch(p: &mut Project, v: &mut LibraryView, cat: usize, patch: usize) {
    let Some(c) = CATEGORIES.get(cat) else { return };
    let Some(name) = c.patches.get(patch) else {
        return;
    };
    let ti = p.selected_track;
    let Some(t) = p.tracks.get_mut(ti) else {
        return;
    };
    // A folder only groups tracks; there is no channel strip to load a patch into.
    if t.kind == TrackKind::Folder {
        return;
    }
    if v.saved.as_ref().is_none_or(|s| s.track != ti) {
        v.saved = Some(Saved {
            track: ti,
            name: t.name.clone(),
            instrument: t.instrument,
            inserts: t.inserts.clone(),
            category: t.category,
        });
    }
    t.name = (*name).to_string();
    t.category = Some(c.name);
    t.inserts = crate::model::inserts_from_names(c.fx);
    // Only software instruments get an instrument: audio tracks keep their input and an aux
    // track its bus.
    if matches!(t.kind, TrackKind::Instrument | TrackKind::Drummer) {
        t.instrument = c.instrument;
    }
    v.loaded = Some((cat, patch));
}

fn revert(p: &mut Project, v: &mut LibraryView) {
    if let Some(s) = v.saved.take() {
        if let Some(t) = p.tracks.get_mut(s.track) {
            t.name = s.name;
            t.instrument = s.instrument;
            t.inserts = s.inserts;
            t.category = s.category;
        }
        v.loaded = None;
    }
}

/// A list row; returns the click response.
fn list_row(
    ui: &Ui,
    g: &Painter,
    r: Rect,
    key: (u8, usize, usize),
    text: &str,
    selected: bool,
    chevron: bool,
    badge: Option<(Color32, &'static str)>,
) -> egui::Response {
    let resp = ui.interact(r, id(key), Sense::click());
    if selected {
        g.rect_filled(r, 4.0, SEL_BG);
        g.rect_stroke(
            r,
            4.0,
            Stroke::new(1.0, Color32::from_rgb(0x38, 0x8b, 0xfd)),
            egui::StrokeKind::Inside,
        );
    } else if resp.hovered() {
        g.rect_filled(r, 4.0, Color32::from_rgb(0x1a, 0x1e, 0x29));
    }
    let font = FontId::proportional(12.0);
    let mut text_x = r.left() + 6.0;
    if let Some((badge_col, glyph)) = badge {
        let b_size = 20.0;
        let b_rect =
            Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(b_size, b_size));
        g.rect_filled(b_rect, 4.0, badge_col.gamma_multiply(0.25));
        g.rect_stroke(
            b_rect,
            4.0,
            Stroke::new(1.0, badge_col),
            egui::StrokeKind::Inside,
        );
        g.text(
            b_rect.center(),
            Align2::CENTER_CENTER,
            glyph,
            FontId::proportional(10.5),
            badge_col.lerp_to_gamma(Color32::WHITE, 0.6),
        );
        text_x = r.left() + 28.0;
    }
    let max_w = (r.right() - if chevron { 22.0 } else { 8.0 } - text_x).max(10.0);
    let shown = crate::mixer::elide(g, text, &font, max_w);
    g.text(
        pos2(text_x, r.center().y),
        Align2::LEFT_CENTER,
        shown,
        font,
        if selected { Color32::WHITE } else { TEXT },
    );
    if chevron {
        let c = pos2(r.right() - 9.0, r.center().y);
        g.add(Shape::line(
            vec![
                c + vec2(-2.0, -4.0),
                c + vec2(2.0, 0.0),
                c + vec2(-2.0, 4.0),
            ],
            stroke(TEXT_DIM),
        ));
    }
    resp
}

/// Scrollable column of rows; returns the clicked index.
#[allow(clippy::too_many_arguments)]
fn column(
    ui: &Ui,
    g: &Painter,
    rect: Rect,
    which: u8,
    scroll: &mut f32,
    items: &[String],
    selected: Option<usize>,
    chevrons: bool,
    badges: Option<&[(Color32, &'static str)]>,
) -> Option<usize> {
    if ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|pt| rect.contains(pt))
    {
        *scroll -= ui.input(|i| i.smooth_scroll_delta.y);
    }
    let content = items.len() as f32 * ROW_H;
    *scroll = scroll.clamp(0.0, (content - rect.height()).max(0.0));
    let gc = g.with_clip_rect(rect.intersect(g.clip_rect()));
    let mut clicked = None;
    for (i, text) in items.iter().enumerate() {
        let y = rect.top() + i as f32 * ROW_H - *scroll;
        if y + ROW_H < rect.top() || y > rect.bottom() {
            continue;
        }
        let r = Rect::from_min_size(pos2(rect.left() + 4.0, y), vec2(rect.width() - 8.0, ROW_H))
            .intersect(rect);
        let badge = badges.and_then(|b| b.get(i).copied());
        if list_row(
            ui,
            &gc,
            r,
            (which, i, items.len()),
            text,
            selected == Some(i),
            chevrons,
            badge,
        )
        .clicked()
        {
            clicked = Some(i);
        }
    }
    clicked
}

fn footer_button(ui: &Ui, g: &Painter, r: Rect, key: &str, label: &str, enabled: bool) -> bool {
    let resp = enabled.then(|| ui.interact(r, id(("footer", key)), Sense::click()));
    let hovered = resp.as_ref().is_some_and(|r| r.hovered());
    g.rect_filled(
        r,
        6.0,
        if hovered {
            Color32::from_rgb(0x25, 0x2a, 0x37)
        } else {
            Color32::from_rgb(0x1a, 0x1e, 0x29)
        },
    );
    g.rect_stroke(
        r,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(0x25, 0x2a, 0x37)),
        egui::StrokeKind::Inside,
    );
    g.text(
        r.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.0),
        if enabled {
            TEXT
        } else {
            Color32::from_rgb(0x60, 0x68, 0x7a)
        },
    );
    resp.is_some_and(|r| r.clicked())
}

pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState) {
    let panel = ui.available_rect_before_wrap();
    ui.allocate_rect(panel, Sense::hover());
    let g = ui.painter_at(panel);
    g.rect_filled(panel, 0.0, PANEL_BG);
    g.line_segment(
        [panel.right_top(), panel.right_bottom()],
        Stroke::new(1.0, Color32::from_rgb(0x25, 0x2a, 0x37)),
    );
    let v = &mut s.library;

    // Far-left collapsible icon rail: Sound Library, Loops, Project Files.
    let rail_w = if v.rail_collapsed { 16.0 } else { 38.0 };
    let rail_r = Rect::from_min_max(
        panel.left_top(),
        pos2(panel.left() + rail_w, panel.bottom()),
    );
    g.rect_filled(rail_r, 0.0, Color32::from_rgb(0x0d, 0x0f, 0x14));
    g.line_segment(
        [rail_r.right_top(), rail_r.right_bottom()],
        Stroke::new(1.0, Color32::from_rgb(0x25, 0x2a, 0x37)),
    );

    if v.rail_collapsed {
        let expand_r = Rect::from_center_size(
            pos2(rail_r.center().x, rail_r.top() + 20.0),
            vec2(14.0, 20.0),
        );
        if ui
            .interact(expand_r, id("rail_expand"), Sense::click())
            .clicked()
        {
            v.rail_collapsed = false;
        }
        g.text(
            expand_r.center(),
            Align2::CENTER_CENTER,
            "›",
            FontId::proportional(14.0),
            TEXT_DIM,
        );
    } else {
        let mut ry = rail_r.top() + 10.0;
        let tabs = [
            (LibraryTab::Sounds, "♬", "Sound Library"),
            (LibraryTab::Loops, "⟳", "Loops"),
            (LibraryTab::Files, "🗀", "Project Files"),
        ];
        for (tab, icon_lbl, tip) in tabs {
            let active = v.tab == tab;
            let br = Rect::from_center_size(pos2(rail_r.center().x, ry + 14.0), vec2(28.0, 28.0));
            let resp = ui.interact(br, id(("rail_tab", tip)), Sense::click());
            resp.clone().on_hover_text(tip);
            if resp.clicked() {
                v.tab = tab;
            }
            if active {
                g.rect_filled(br, 7.0, Color32::from_rgb(0x1a, 0x24, 0x38));
                g.rect_stroke(
                    br,
                    7.0,
                    Stroke::new(1.0, Color32::from_rgb(0x38, 0x8b, 0xfd)),
                    egui::StrokeKind::Inside,
                );
            } else if resp.hovered() {
                g.rect_filled(br, 7.0, Color32::from_rgb(0x1a, 0x1e, 0x29));
            }
            g.text(
                br.center(),
                Align2::CENTER_CENTER,
                icon_lbl,
                FontId::proportional(14.0),
                if active {
                    Color32::from_rgb(0x58, 0xa6, 0xff)
                } else {
                    TEXT_DIM
                },
            );
            ry += 36.0;
        }
        // Collapse toggle at bottom
        let coll_r = Rect::from_center_size(
            pos2(rail_r.center().x, rail_r.bottom() - 20.0),
            vec2(24.0, 24.0),
        );
        let coll_resp = ui.interact(coll_r, id("rail_collapse"), Sense::click());
        coll_resp.clone().on_hover_text("Collapse Library Rail");
        if coll_resp.clicked() {
            v.rail_collapsed = true;
        }
        g.text(
            coll_r.center(),
            Align2::CENTER_CENTER,
            "‹",
            FontId::proportional(14.0),
            TEXT_DIM,
        );
    }

    let left = panel.left() + rail_w;
    let right = panel.right();
    let w = right - left;

    // Header: "Library  All Sounds ⇕"
    let hy = panel.top() + 14.0;
    let cx = left + w * 0.5;
    let lib = g.text(
        pos2(cx - 6.0, hy),
        Align2::RIGHT_CENTER,
        tr("Library"),
        FontId::proportional(13.0),
        TEXT,
    );
    let all = g.text(
        pos2(cx + 2.0, hy),
        Align2::LEFT_CENTER,
        tr("All Sounds"),
        FontId::proportional(12.5),
        TEXT_DIM,
    );
    let c = pos2(all.right() + 8.0, hy);
    g.add(Shape::line(
        vec![
            c + vec2(-2.5, -1.5),
            c + vec2(0.0, -4.0),
            c + vec2(2.5, -1.5),
        ],
        stroke(TEXT_DIM),
    ));
    g.add(Shape::line(
        vec![c + vec2(-2.5, 1.5), c + vec2(0.0, 4.0), c + vec2(2.5, 1.5)],
        stroke(TEXT_DIM),
    ));
    let _ = lib;

    // Big icon card of the selected track's sound, and its name.
    let icon_card = Rect::from_center_size(pos2(cx, panel.top() + 72.0), vec2(72.0, 72.0));
    g.rect_filled(icon_card, 8.0, Color32::from_rgb(0x1a, 0x1e, 0x29));
    g.rect_stroke(
        icon_card,
        8.0,
        Stroke::new(1.0, Color32::from_rgb(0x25, 0x2a, 0x37)),
        egui::StrokeKind::Inside,
    );
    if let Some(t) = p.tracks.get(p.selected_track) {
        draw_icon(&g, icon_card.shrink(6.0), track_icon(t));
        let font = FontId::proportional(12.5);
        let name = crate::mixer::elide(&g, &t.name, &font, w - 24.0);
        g.text(
            pos2(cx, icon_card.bottom() + 12.0),
            Align2::CENTER_CENTER,
            name,
            font,
            TEXT,
        );
    }

    // Search field as rounded pill.
    let search_r = Rect::from_min_size(
        pos2(left + 8.0, icon_card.bottom() + 24.0),
        vec2(w - 16.0, 24.0),
    );
    g.rect_filled(search_r, 12.0, Color32::from_rgb(0x1a, 0x1e, 0x29));
    g.rect_stroke(
        search_r,
        12.0,
        Stroke::new(1.0, Color32::from_rgb(0x25, 0x2a, 0x37)),
        egui::StrokeKind::Inside,
    );
    let mc = pos2(search_r.left() + 14.0, search_r.center().y);
    g.circle_stroke(mc + vec2(-1.0, -1.0), 4.0, stroke(TEXT_DIM));
    g.line_segment([mc + vec2(2.0, 2.0), mc + vec2(5.0, 5.0)], stroke(TEXT_DIM));
    let edit_r = Rect::from_min_max(
        pos2(search_r.left() + 24.0, search_r.top() + 2.0),
        pos2(search_r.right() - 8.0, search_r.bottom() - 2.0),
    );
    ui.put(
        edit_r,
        egui::TextEdit::singleline(&mut v.search)
            .hint_text(tr("Search Sounds"))
            .frame(egui::Frame::NONE)
            .text_color(TEXT)
            .font(FontId::proportional(12.0)),
    );

    // Color-coded filter chips (violet Synth, cyan Bass, amber Drums)
    let chips_y = search_r.bottom() + 6.0;
    let chips = [
        (
            SoundChip::Synth,
            "Synth",
            Color32::from_rgb(0x8b, 0x5c, 0xf6),
        ),
        (SoundChip::Bass, "Bass", Color32::from_rgb(0x06, 0xb6, 0xd4)),
        (
            SoundChip::Drums,
            "Drums",
            Color32::from_rgb(0xf5, 0x9e, 0x0b),
        ),
    ];
    let chip_gap = 5.0;
    let chip_w = (w - 16.0 - 2.0 * chip_gap) / 3.0;
    for (idx, (chip, lbl, col)) in chips.into_iter().enumerate() {
        let cr = Rect::from_min_size(
            pos2(left + 8.0 + idx as f32 * (chip_w + chip_gap), chips_y),
            vec2(chip_w, 20.0),
        );
        let active = v.active_chip == Some(chip);
        let resp = ui.interact(cr, id(("chip", lbl)), Sense::click());
        if resp.clicked() {
            v.active_chip = if active { None } else { Some(chip) };
        }
        g.rect_filled(
            cr,
            10.0,
            if active {
                col.gamma_multiply(0.25)
            } else if resp.hovered() {
                Color32::from_rgb(0x22, 0x27, 0x35)
            } else {
                Color32::from_rgb(0x1a, 0x1e, 0x29)
            },
        );
        g.rect_stroke(
            cr,
            10.0,
            Stroke::new(
                if active { 1.2 } else { 1.0 },
                if active {
                    col
                } else {
                    Color32::from_rgb(0x25, 0x2a, 0x37)
                },
            ),
            egui::StrokeKind::Inside,
        );
        g.text(
            cr.center(),
            Align2::CENTER_CENTER,
            lbl,
            FontId::proportional(11.0),
            if active {
                col.lerp_to_gamma(Color32::WHITE, 0.4)
            } else {
                TEXT_DIM
            },
        );
    }

    // Audition preview controls height + footer height
    let audition_h = 30.0;
    let footer_h = 34.0;
    let cols = Rect::from_min_max(
        pos2(left, chips_y + 26.0),
        pos2(right, panel.bottom() - footer_h - audition_h),
    );

    let query = v.search.trim().to_lowercase();
    if !query.is_empty() {
        // Search results: one column of "Patch" rows across every category.
        let hits: Vec<(usize, usize)> = CATEGORIES
            .iter()
            .enumerate()
            .flat_map(|(ci, c)| {
                c.patches
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.to_lowercase().contains(&query))
                    .map(move |(pi, _)| (ci, pi))
            })
            .collect();
        let items: Vec<String> = hits
            .iter()
            .map(|(ci, pi)| CATEGORIES[*ci].patches[*pi].to_string())
            .collect();
        let badges: Vec<(Color32, &'static str)> = hits
            .iter()
            .map(|(ci, pi)| badge_for_patch(CATEGORIES[*ci].patches[*pi], CATEGORIES[*ci].name))
            .collect();
        let sel = v.loaded.and_then(|l| hits.iter().position(|h| *h == l));
        if let Some(i) = column(
            ui,
            &g,
            cols,
            2,
            &mut v.scroll_right,
            &items,
            sel,
            false,
            Some(&badges),
        ) {
            let (ci, pi) = hits[i];
            load_patch(p, v, ci, pi);
        }
        if hits.is_empty() {
            g.text(
                pos2(cols.center().x, cols.top() + 20.0),
                Align2::CENTER_CENTER,
                tr("No results"),
                FontId::proportional(12.0),
                TEXT_DIM,
            );
        }
    } else {
        // Filter categories if active_chip is set
        let filtered_cat_indices: Vec<usize> = CATEGORIES
            .iter()
            .enumerate()
            .filter(|(_, c)| match v.active_chip {
                None => true,
                Some(SoundChip::Synth) => {
                    let n = c.name.to_lowercase();
                    n.contains("synth") || n.contains("arp") || n.contains("cinematic")
                }
                Some(SoundChip::Bass) => c.name.to_lowercase().contains("bass"),
                Some(SoundChip::Drums) => {
                    let n = c.name.to_lowercase();
                    n.contains("drum") || n.contains("percussion")
                }
            })
            .map(|(i, _)| i)
            .collect();

        let split = if v.category.is_some() {
            left + w * 0.5
        } else {
            right
        };
        let left_col = Rect::from_min_max(cols.min, pos2(split, cols.bottom()));
        let cat_items: Vec<String> = filtered_cat_indices
            .iter()
            .map(|&ci| tr(CATEGORIES[ci].name).to_string())
            .collect();
        let cat_sel = v
            .category
            .and_then(|sel_ci| filtered_cat_indices.iter().position(|&ci| ci == sel_ci));

        if let Some(i) = column(
            ui,
            &g,
            left_col,
            0,
            &mut v.scroll_left,
            &cat_items,
            cat_sel,
            true,
            None,
        ) {
            let real_ci = filtered_cat_indices[i];
            v.category = if v.category == Some(real_ci) {
                None
            } else {
                Some(real_ci)
            };
            v.scroll_right = 0.0;
        }

        if let Some(ci) = v.category {
            let right_col = Rect::from_min_max(pos2(split, cols.top()), cols.max);
            g.line_segment(
                [pos2(split, cols.top()), pos2(split, cols.bottom())],
                Stroke::new(1.0, Color32::from_rgb(0x25, 0x2a, 0x37)),
            );
            let items: Vec<String> = CATEGORIES[ci]
                .patches
                .iter()
                .map(|n| n.to_string())
                .collect();
            let badges: Vec<(Color32, &'static str)> = CATEGORIES[ci]
                .patches
                .iter()
                .map(|pname| badge_for_patch(pname, CATEGORIES[ci].name))
                .collect();
            let sel = v.loaded.filter(|(c, _)| *c == ci).map(|(_, pi)| pi);
            if let Some(pi) = column(
                ui,
                &g,
                right_col,
                1,
                &mut v.scroll_right,
                &items,
                sel,
                false,
                Some(&badges),
            ) {
                load_patch(p, v, ci, pi);
            }
        }
    }

    // Bottom Audition Preview Controls bar
    let aud_r = Rect::from_min_size(
        pos2(left + 8.0, panel.bottom() - footer_h - audition_h + 4.0),
        vec2(w - 16.0, 24.0),
    );
    g.rect_filled(aud_r, 6.0, Color32::from_rgb(0x1a, 0x1e, 0x29));
    g.rect_stroke(
        aud_r,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(0x25, 0x2a, 0x37)),
        egui::StrokeKind::Inside,
    );

    // Audition Play/Stop pill
    let play_btn_r = Rect::from_min_size(
        pos2(aud_r.left() + 4.0, aud_r.top() + 2.0),
        vec2(70.0, 20.0),
    );
    let play_resp = ui.interact(play_btn_r, id("audition_play"), Sense::click());
    if play_resp.clicked() {
        v.audition_playing = !v.audition_playing;
    }
    g.rect_filled(
        play_btn_r,
        5.0,
        if v.audition_playing {
            Color32::from_rgb(0x10, 0xb9, 0x81)
        } else if play_resp.hovered() {
            Color32::from_rgb(0x25, 0x2a, 0x37)
        } else {
            Color32::from_rgb(0x13, 0x16, 0x1e)
        },
    );
    g.text(
        play_btn_r.center(),
        Align2::CENTER_CENTER,
        if v.audition_playing {
            "■ Stop"
        } else {
            "▶ Audition"
        },
        FontId::proportional(11.0),
        if v.audition_playing {
            Color32::WHITE
        } else {
            TEXT
        },
    );

    // Auto-audition toggle
    let auto_btn_r = Rect::from_min_size(
        pos2(play_btn_r.right() + 4.0, aud_r.top() + 2.0),
        vec2(42.0, 20.0),
    );
    let auto_resp = ui.interact(auto_btn_r, id("audition_auto"), Sense::click());
    if auto_resp.clicked() {
        v.audition_auto = !v.audition_auto;
    }
    g.rect_filled(
        auto_btn_r,
        5.0,
        if v.audition_auto {
            Color32::from_rgb(0x1f, 0x2a, 0x40)
        } else if auto_resp.hovered() {
            Color32::from_rgb(0x25, 0x2a, 0x37)
        } else {
            Color32::from_rgb(0x13, 0x16, 0x1e)
        },
    );
    if v.audition_auto {
        g.rect_stroke(
            auto_btn_r,
            5.0,
            Stroke::new(1.0, Color32::from_rgb(0x38, 0x8b, 0xfd)),
            egui::StrokeKind::Inside,
        );
    }
    g.text(
        auto_btn_r.center(),
        Align2::CENTER_CENTER,
        "Auto",
        FontId::proportional(11.0),
        if v.audition_auto {
            Color32::from_rgb(0x58, 0xa6, 0xff)
        } else {
            TEXT_DIM
        },
    );

    // Mini volume slider
    let vol_w = (aud_r.right() - auto_btn_r.right() - 14.0).clamp(20.0, 80.0);
    let vol_r = Rect::from_min_size(
        pos2(aud_r.right() - vol_w - 6.0, aud_r.center().y - 4.0),
        vec2(vol_w, 8.0),
    );
    g.rect_filled(vol_r, 4.0, Color32::from_rgb(0x13, 0x16, 0x1e));
    let vol_fill_w = vol_w * v.audition_volume.clamp(0.0, 1.0);
    g.rect_filled(
        Rect::from_min_size(vol_r.min, vec2(vol_fill_w, 8.0)),
        4.0,
        Color32::from_rgb(0x38, 0x8b, 0xfd),
    );
    let vol_resp = ui.interact(
        vol_r.expand2(vec2(2.0, 6.0)),
        id("audition_vol"),
        Sense::click_and_drag(),
    );
    if let Some(pos) = vol_resp.interact_pointer_pos() {
        v.audition_volume = ((pos.x - vol_r.left()) / vol_w).clamp(0.0, 1.0);
    }

    // Footer: Revert / Delete / Save…
    let fy = panel.bottom() - footer_h + 6.0;
    g.line_segment(
        [pos2(left, fy - 6.0), pos2(right, fy - 6.0)],
        Stroke::new(1.0, Color32::from_rgb(0x25, 0x2a, 0x37)),
    );
    let bw = (w - 32.0) / 3.0;
    let can_revert = v.saved.is_some();
    if footer_button(
        ui,
        &g,
        Rect::from_min_size(pos2(left + 8.0, fy), vec2(bw, 22.0)),
        "revert",
        tr("Revert"),
        can_revert,
    ) {
        revert(p, v);
    }
    footer_button(
        ui,
        &g,
        Rect::from_min_size(pos2(left + 16.0 + bw, fy), vec2(bw, 22.0)),
        "delete",
        tr("Delete"),
        false,
    );
    footer_button(
        ui,
        &g,
        Rect::from_min_size(pos2(left + 24.0 + 2.0 * bw, fy), vec2(bw, 22.0)),
        "save",
        tr("Save…"),
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn category(name: &str) -> usize {
        CATEGORIES
            .iter()
            .position(|c| c.name == name)
            .unwrap_or(usize::MAX)
    }

    #[test]
    fn loading_a_patch_updates_the_selected_track_and_revert_restores_it() {
        let mut p = Project::demo();
        p.selected_track = 0;
        let before = (
            p.tracks[0].name.clone(),
            p.tracks[0].instrument,
            p.tracks[0].inserts.clone(),
        );
        let mut v = LibraryView::default();
        let synth = category("Synthesizer");
        load_patch(&mut p, &mut v, synth, 0);
        let t = &p.tracks[0];
        assert_eq!(t.name, CATEGORIES[synth].patches[0]);
        assert_eq!(t.instrument, Some("Alchemy"));
        assert_eq!(
            t.inserts,
            crate::model::inserts_from_names(CATEGORIES[synth].fx)
        );
        assert_eq!(t.category, Some("Synthesizer"));
        // A second audition keeps the original state for Revert.
        load_patch(&mut p, &mut v, category("Bass"), 1);
        revert(&mut p, &mut v);
        let t = &p.tracks[0];
        assert_eq!((t.name.clone(), t.instrument, t.inserts.clone()), before);
        assert_eq!(t.category, None);
        assert!(v.loaded.is_none());
    }

    #[test]
    fn the_guitar_amp_patches_put_the_amp_on_the_track() {
        let mut p = Project::demo();
        p.selected_track = 0;
        let mut v = LibraryView::default();
        load_patch(&mut p, &mut v, category("Guitar Amp"), 0);
        assert_eq!(p.tracks[0].category, Some("Guitar Amp"));
        assert!(p.tracks[0]
            .inserts
            .iter()
            .any(|i| i.kind == prowave_engine::FxKind::Amp));
    }

    #[test]
    fn aux_tracks_take_the_channel_strip_but_not_an_instrument_and_folders_nothing() {
        let mut p = Project::demo();
        let aux = p.add_track(TrackKind::Aux);
        let mut v = LibraryView::default();
        load_patch(&mut p, &mut v, category("Guitar"), 0);
        assert_eq!(p.tracks[aux].instrument, None);
        assert_eq!(p.tracks[aux].category, Some("Guitar"));

        let folder = p.add_track(TrackKind::Folder);
        let name = p.tracks[folder].name.clone();
        load_patch(&mut p, &mut v, category("Bass"), 0);
        assert_eq!(p.tracks[folder].name, name);
        assert_eq!(p.tracks[folder].category, None);
    }

    #[test]
    fn audio_tracks_keep_their_input_instead_of_an_instrument() {
        let mut p = Project::demo();
        let audio = p
            .tracks
            .iter()
            .position(|t| t.kind == TrackKind::Audio)
            .unwrap_or(0);
        p.selected_track = audio;
        let mut v = LibraryView::default();
        load_patch(&mut p, &mut v, category("Guitar"), 0);
        assert_eq!(p.tracks[audio].instrument, None);
        assert_eq!(p.tracks[audio].category, Some("Guitar"));
    }

    #[test]
    fn every_category_except_user_patches_has_patches() {
        for c in CATEGORIES.iter().skip(1) {
            assert!(!c.patches.is_empty(), "{}", c.name);
        }
    }
}
