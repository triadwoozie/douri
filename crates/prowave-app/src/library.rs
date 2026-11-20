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

const PANEL_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x1a);
const ROW_H: f32 = 22.0;
const SEL_BG: Color32 = Color32::from_rgb(0x34, 0x35, 0x39);

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
    All,
    Loops,
    Instruments,
    Effects,
    SoundLibrary,
    Files,
    Sounds,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundChip {
    Synth,
    Bass,
    Drums,
    Keys,
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
    pub show_filters_popover: bool,
    pub sort_mode: usize,
    pub audition_playing: bool,
    pub audition_volume: f32,
    pub audition_auto: bool,
}

impl Default for LibraryView {
    fn default() -> Self {
        Self {
            category: Some(1),
            search: String::new(),
            loaded: None,
            saved: None,
            scroll_left: 0.0,
            scroll_right: 0.0,
            tab: LibraryTab::All,
            rail_collapsed: false,
            active_chip: None,
            show_filters_popover: false,
            sort_mode: 0,
            audition_playing: false,
            audition_volume: 0.8,
            audition_auto: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundBadge {
    Synth,
    Bass,
    Drums,
    Keys,
    Audio,
}

pub fn badge_for_patch(patch_name: &str, cat_name: &str) -> SoundBadge {
    let lower_cat = cat_name.to_lowercase();
    let lower_p = patch_name.to_lowercase();
    if lower_cat.contains("synth") || lower_cat.contains("arp") || lower_cat.contains("cinematic") {
        SoundBadge::Synth
    } else if lower_cat.contains("bass") || lower_p.contains("bass") {
        SoundBadge::Bass
    } else if lower_cat.contains("drum") || lower_cat.contains("percussion") {
        SoundBadge::Drums
    } else if lower_cat.contains("keyboard")
        || lower_cat.contains("piano")
        || lower_cat.contains("organ")
        || lower_cat.contains("clav")
    {
        SoundBadge::Keys
    } else {
        SoundBadge::Audio
    }
}

fn draw_badge(g: &Painter, rect: Rect, badge: SoundBadge) {
    let (bg, stroke_col, glyph) = match badge {
        SoundBadge::Synth => (
            Color32::from_rgb(0x28, 0xb4, 0x46),
            Color32::from_rgb(0x34, 0xc7, 0x59),
            crate::icons::WAVEFORM,
        ),
        SoundBadge::Bass => (
            Color32::from_rgb(0x29, 0x79, 0xff),
            Color32::from_rgb(0x52, 0x94, 0xff),
            crate::icons::WAVEFORM,
        ),
        SoundBadge::Keys => (
            Color32::from_rgb(0x2c, 0x2d, 0x30),
            Color32::from_rgb(0x44, 0x45, 0x4a),
            crate::icons::PIANO_KEYS,
        ),
        SoundBadge::Drums => (
            Color32::from_rgb(0xf5, 0xa6, 0x23),
            Color32::from_rgb(0xff, 0xb8, 0x43),
            crate::icons::MUSIC_NOTES,
        ),
        SoundBadge::Audio => (
            Color32::from_rgb(0x8e, 0x8e, 0x93),
            Color32::from_rgb(0xae, 0xae, 0xb2),
            crate::icons::MICROPHONE_STAGE,
        ),
    };
    g.rect_filled(rect, 4.0, bg);
    g.rect_stroke(rect, 4.0, Stroke::new(1.0, stroke_col), egui::StrokeKind::Inside);
    crate::icons::paint_fill(g, rect, glyph, 11.0, Color32::WHITE);
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
    badge: Option<SoundBadge>,
) -> egui::Response {
    let resp = ui.interact(r, id(key), Sense::click());
    if selected {
        g.rect_filled(r, 4.0, SEL_BG);
    } else if resp.hovered() {
        g.rect_filled(r, 4.0, Color32::from_rgb(0x26, 0x26, 0x2a));
    }
    g.line_segment(
        [r.left_bottom(), r.right_bottom()],
        Stroke::new(1.0, Color32::from_white_alpha(13)),
    );
    let font = FontId::proportional(12.0);
    let mut text_x = r.left() + 8.0;
    if let Some(b) = badge {
        let b_rect = Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(16.0, 16.0));
        draw_badge(g, b_rect, b);
        text_x = r.left() + 28.0;
    }
    let max_w = (r.right() - if chevron { 20.0 } else { 8.0 } - text_x).max(10.0);
    let shown = crate::mixer::elide(g, text, &font, max_w);
    g.text(
        pos2(text_x, r.center().y),
        Align2::LEFT_CENTER,
        shown,
        font,
        if selected { Color32::from_rgb(0xf5, 0xf5, 0xf7) } else { TEXT },
    );
    if chevron {
        let c = pos2(r.right() - 9.0, r.center().y);
        let chevron_color = if selected { Color32::WHITE } else { TEXT_DIM };
        g.add(Shape::line(
            vec![
                c + vec2(-2.0, -4.0),
                c + vec2(2.0, 0.0),
                c + vec2(-2.0, 4.0),
            ],
            Stroke::new(1.0, chevron_color),
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
    badges: Option<&[SoundBadge]>,
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

fn draw_audition_waveform(g: &Painter, rect: Rect, playing: bool) {
    g.rect_filled(rect, 3.0, Color32::from_rgb(0x18, 0x18, 0x1a));
    g.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2a, 0x2e)),
        egui::StrokeKind::Inside,
    );
    let bars = [4.0, 7.0, 11.0, 6.0, 13.0, 10.0, 15.0, 12.0, 8.0, 14.0, 9.0, 5.0, 11.0, 7.0, 3.0];
    let n = bars.len();
    let pad = 4.0;
    let avail_w = rect.width() - pad * 2.0;
    let bar_w = (avail_w / n as f32 - 1.5).max(1.5);
    let gold = Color32::from_rgb(0xd4, 0xaf, 0x37);
    let idle_gold = Color32::from_rgba_premultiplied(0xd4, 0xaf, 0x37, 90);
    let cy = rect.center().y;

    for (i, &amp) in bars.iter().enumerate() {
        let bx = rect.left() + pad + i as f32 * (avail_w / n as f32);
        let bh = (amp / 16.0 * (rect.height() - 4.0)).clamp(2.0, rect.height() - 4.0);
        let br = Rect::from_min_size(pos2(bx, cy - bh * 0.5), vec2(bar_w, bh));
        g.rect_filled(br, 1.0, if playing { gold } else { idle_gold });
    }
    let playhead_x = rect.left() + pad;
    g.line_segment(
        [pos2(playhead_x, rect.top() + 2.0), pos2(playhead_x, rect.bottom() - 2.0)],
        Stroke::new(1.0, Color32::WHITE),
    );
}

fn footer_button(ui: &Ui, g: &Painter, r: Rect, key: &str, label: &str, enabled: bool) -> bool {
    let resp = enabled.then(|| ui.interact(r, id(("footer", key)), Sense::click()));
    let hovered = resp.as_ref().is_some_and(|r| r.hovered());
    g.rect_filled(
        r,
        4.0,
        if hovered {
            Color32::from_rgb(0x30, 0x30, 0x35)
        } else {
            Color32::from_rgb(0x26, 0x26, 0x2a)
        },
    );
    g.rect_stroke(
        r,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
        egui::StrokeKind::Inside,
    );
    g.text(
        r.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        if enabled { TEXT } else { TEXT_DIM },
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
        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
    );
    let v = &mut s.library;

    let left = panel.left();
    let right = panel.right();
    let w = panel.width();

    // Header: "Library  All Sounds ⇕"
    let hy = panel.top() + 14.0;
    let cx = left + w * 0.5;
    g.text(
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

    // Modern Tag-Driven Search Field (vertically centered magnifying glass & text)
    let search_r = Rect::from_min_size(pos2(left + 8.0, panel.top() + 32.0), vec2(w - 16.0, 26.0));
    g.rect_filled(search_r, 6.0, Color32::from_rgb(0x16, 0x16, 0x18));
    g.rect_stroke(
        search_r,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
        egui::StrokeKind::Inside,
    );
    let mag_r = Rect::from_center_size(pos2(search_r.left() + 14.0, search_r.center().y), vec2(14.0, 14.0));
    crate::icons::paint_regular(&g, mag_r, crate::icons::MAGNIFYING_GLASS, 14.0, Color32::from_rgb(0x8e, 0x8e, 0x93));
    let edit_r = Rect::from_min_max(
        pos2(search_r.left() + 26.0, search_r.top() + 2.5),
        pos2(search_r.right() - 8.0, search_r.bottom() - 2.5),
    );
    ui.put(
        edit_r,
        egui::TextEdit::singleline(&mut v.search)
            .hint_text(tr("Search Sounds"))
            .frame(egui::Frame::NONE)
            .text_color(TEXT)
            .font(FontId::proportional(12.0)),
    );

    // Color-coded filter chips (All Filters button + Synth, Bass, Drums, Keys)
    let chips_y = search_r.bottom() + 6.0;
    let filters_btn_w = 60.0;
    let filters_r = Rect::from_min_size(pos2(left + 8.0, chips_y), vec2(filters_btn_w, 20.0));
    let filters_resp = ui.interact(filters_r, id("all_filters_btn"), Sense::click());
    if filters_resp.clicked() {
        v.show_filters_popover = !v.show_filters_popover;
    }
    g.rect_filled(
        filters_r,
        4.0,
        if v.show_filters_popover {
            Color32::from_rgb(0x3a, 0x7b, 0xd5)
        } else if filters_resp.hovered() {
            Color32::from_rgb(0x30, 0x30, 0x35)
        } else {
            Color32::from_rgb(0x26, 0x26, 0x2a)
        },
    );
    g.rect_stroke(
        filters_r,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
        egui::StrokeKind::Inside,
    );
    g.text(
        filters_r.center(),
        Align2::CENTER_CENTER,
        "Filters ▾",
        FontId::proportional(10.5),
        if v.show_filters_popover {
            Color32::WHITE
        } else {
            TEXT
        },
    );

    let chips = [
        (
            SoundChip::Synth,
            "Synth",
            Color32::from_rgb(0x28, 0xb4, 0x46),
        ),
        (SoundChip::Bass, "Bass", Color32::from_rgb(0x29, 0x79, 0xff)),
        (
            SoundChip::Drums,
            "Drums",
            Color32::from_rgb(0xf5, 0xa6, 0x23),
        ),
        (SoundChip::Keys, "Keys", Color32::from_rgb(0x10, 0xb9, 0x81)),
    ];
    let chip_gap = 4.0;
    let rem_w = w - 16.0 - filters_btn_w - 6.0;
    let chip_w = (rem_w - 3.0 * chip_gap) / 4.0;
    for (idx, (chip, lbl, col)) in chips.into_iter().enumerate() {
        let cr = Rect::from_min_size(
            pos2(
                left + 8.0 + filters_btn_w + 6.0 + idx as f32 * (chip_w + chip_gap),
                chips_y,
            ),
            vec2(chip_w, 20.0),
        );
        let active = v.active_chip == Some(chip);
        let resp = ui.interact(cr, id(("chip", lbl)), Sense::click());
        if resp.clicked() {
            v.active_chip = if active { None } else { Some(chip) };
        }
        g.rect_filled(
            cr,
            4.0,
            if active {
                col.gamma_multiply(0.35)
            } else if resp.hovered() {
                Color32::from_rgb(0x30, 0x30, 0x35)
            } else {
                Color32::from_rgb(0x26, 0x26, 0x2a)
            },
        );
        g.rect_stroke(
            cr,
            4.0,
            Stroke::new(
                1.0,
                if active {
                    col
                } else {
                    Color32::from_rgb(0x34, 0x34, 0x39)
                },
            ),
            egui::StrokeKind::Inside,
        );
        g.text(
            cr.center(),
            Align2::CENTER_CENTER,
            lbl,
            FontId::proportional(10.5),
            if active { Color32::WHITE } else { TEXT_DIM },
        );
    }

    if v.show_filters_popover {
        crate::liquid_glass::glass_window("Sound Filter Tags")
            .resizable(false)
            .collapsible(false)
            .default_pos(pos2(left + 16.0, chips_y + 28.0))
            .show(ui.ctx(), |ui| {
                ui.set_width(220.0);
                ui.label(
                    egui::RichText::new("Filter by Character")
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.add_space(4.0);
                let tags = [
                    "Lead",
                    "Pad",
                    "Pluck",
                    "Sub Bass",
                    "Acoustic",
                    "Electric",
                    "808",
                    "Vintage",
                    "Clean",
                    "Distorted",
                    "Lo-Fi",
                    "Cinematic",
                ];
                ui.horizontal_wrapped(|ui| {
                    for tag in tags {
                        let active = v.search.contains(tag);
                        if ui.selectable_label(active, tag).clicked() {
                            if active {
                                v.search = v.search.replace(tag, "").trim().to_string();
                            } else {
                                if !v.search.is_empty() {
                                    v.search.push(' ');
                                }
                                v.search.push_str(tag);
                            }
                        }
                    }
                });
                ui.add_space(6.0);
                if ui.button(tr("Clear All Filters")).clicked() {
                    v.search.clear();
                    v.active_chip = None;
                    v.show_filters_popover = false;
                }
            });
    }

    let footer_h = 24.0;
    let cols = Rect::from_min_max(
        pos2(left, chips_y + 26.0),
        pos2(right, panel.bottom() - footer_h - 8.0),
    );

    let query = v.search.trim().to_lowercase();
    if !query.is_empty() {
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
        let badges: Vec<SoundBadge> = hits
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
    } else if let Some(chip) = v.active_chip {
        let hits: Vec<(usize, usize)> = CATEGORIES
            .iter()
            .enumerate()
            .flat_map(|(ci, c)| {
                let cat_matches = match chip {
                    SoundChip::Synth => {
                        let n = c.name.to_lowercase();
                        n.contains("synth") || n.contains("arp") || n.contains("cinematic")
                    }
                    SoundChip::Bass => c.name.to_lowercase().contains("bass"),
                    SoundChip::Drums => {
                        let n = c.name.to_lowercase();
                        n.contains("drum") || n.contains("percussion")
                    }
                    SoundChip::Keys => {
                        let n = c.name.to_lowercase();
                        n.contains("piano") || n.contains("organ") || n.contains("keyboard") || n.contains("key") || n.contains("clav")
                    }
                };
                c.patches
                    .iter()
                    .enumerate()
                    .filter(move |(_, _)| cat_matches)
                    .map(move |(pi, _)| (ci, pi))
            })
            .collect();
        let items: Vec<String> = hits
            .iter()
            .map(|(ci, pi)| CATEGORIES[*ci].patches[*pi].to_string())
            .collect();
        let badges: Vec<SoundBadge> = hits
            .iter()
            .map(|(ci, pi)| badge_for_patch(CATEGORIES[*ci].patches[*pi], CATEGORIES[*ci].name))
            .collect();
        let sel = v.loaded.and_then(|l| hits.iter().position(|h| *h == l));
        if let Some(i) = column(
            ui,
            &g,
            cols,
            3,
            &mut v.scroll_right,
            &items,
            sel,
            false,
            Some(&badges),
        ) {
            let (ci, pi) = hits[i];
            load_patch(p, v, ci, pi);
        }
    } else {
        // Restore Logic Pro's side-by-side two-column Miller browser
        let left_w = 135.0f32.min(cols.width() * 0.5);
        let left_rect = Rect::from_min_size(cols.min, vec2(left_w, cols.height()));
        let right_rect = Rect::from_min_max(pos2(left_rect.right() + 1.0, cols.top()), cols.max);

        // Vertical divider between columns
        g.line_segment(
            [pos2(left_rect.right(), cols.top()), pos2(left_rect.right(), cols.bottom())],
            Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2a, 0x2e)),
        );

        // Left Column (Categories)
        let cat_items: Vec<String> = CATEGORIES
            .iter()
            .map(|c| tr(c.name).to_string())
            .collect();
        if let Some(ci) = column(
            ui,
            &g,
            left_rect,
            0,
            &mut v.scroll_left,
            &cat_items,
            v.category,
            true,
            None,
        ) {
            v.category = Some(ci);
            v.scroll_right = 0.0;
        }

        // Right Column (Patches for the selected category)
        let sel_cat = v.category.unwrap_or(1);
        if sel_cat < CATEGORIES.len() {
            let patches = CATEGORIES[sel_cat].patches;
            let patch_items: Vec<String> = patches.iter().map(|n| n.to_string()).collect();
            let patch_badges: Vec<SoundBadge> = patches
                .iter()
                .map(|pname| badge_for_patch(pname, CATEGORIES[sel_cat].name))
                .collect();
            let sel_patch = v.loaded.filter(|(c, _)| *c == sel_cat).map(|(_, pi)| pi);

            if let Some(pi) = column(
                ui,
                &g,
                right_rect,
                1,
                &mut v.scroll_right,
                &patch_items,
                sel_patch,
                false,
                Some(&patch_badges),
            ) {
                load_patch(p, v, sel_cat, pi);
            }
        }
    }

    // Single-row compact 24px footer: Sorted by: Relevance + Audition toggle with waveform preview
    let fy = panel.bottom() - footer_h - 2.0;
    g.line_segment(
        [pos2(left, fy - 4.0), pos2(right, fy - 4.0)],
        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
    );

    let sort_w = 78.0;
    let sort_r = Rect::from_min_size(pos2(left + 8.0, fy), vec2(sort_w, 20.0));
    let sort_labels = ["Relevance", "Name", "Category"];
    let sort_resp = crate::widgets::popup_button(
        ui,
        &g,
        sort_r,
        id("sort_mode"),
        sort_labels[v.sort_mode.min(2)],
    );
    egui::Popup::menu(&sort_resp).show(|ui| {
        for (idx, lbl) in sort_labels.iter().enumerate() {
            if ui.selectable_label(v.sort_mode == idx, *lbl).clicked() {
                v.sort_mode = idx;
                ui.close();
            }
        }
    });

    let can_revert = v.saved.is_some();
    let mut rx = right - 8.0;

    // Auto-audition toggle
    let auto_w = 34.0;
    let auto_r = Rect::from_min_size(pos2(rx - auto_w, fy), vec2(auto_w, 20.0));
    let auto_resp = ui.interact(auto_r, id("audition_auto"), Sense::click());
    if auto_resp.clicked() {
        v.audition_auto = !v.audition_auto;
    }
    g.rect_filled(
        auto_r,
        4.0,
        if v.audition_auto {
            Color32::from_rgb(0x3a, 0x7b, 0xd5)
        } else if auto_resp.hovered() {
            Color32::from_rgb(0x30, 0x30, 0x35)
        } else {
            Color32::from_rgb(0x26, 0x26, 0x2a)
        },
    );
    g.text(
        auto_r.center(),
        Align2::CENTER_CENTER,
        "Auto",
        FontId::proportional(10.5),
        if v.audition_auto { Color32::WHITE } else { TEXT_DIM },
    );
    rx = auto_r.left() - 4.0;

    // Mini #D4AF37 golden waveform strip
    let wave_w = (rx - (sort_r.right() + 48.0)).clamp(36.0, 70.0);
    let wave_r = Rect::from_min_size(pos2(rx - wave_w, fy + 2.0), vec2(wave_w, 16.0));
    draw_audition_waveform(&g, wave_r, v.audition_playing);
    rx = wave_r.left() - 4.0;

    // Audition speaker icon
    let spk_r = Rect::from_center_size(pos2(rx - 7.0, fy + 10.0), vec2(13.0, 13.0));
    crate::icons::paint_regular(&g, spk_r, crate::icons::SPEAKER_HIGH, 13.0, Color32::from_rgb(0x8e, 0x8e, 0x93));
    rx = spk_r.left() - 4.0;

    // Audition Play/Stop button
    let play_w = 22.0;
    let play_r = Rect::from_min_size(pos2(rx - play_w, fy), vec2(play_w, 20.0));
    let play_resp = ui.interact(play_r, id("audition_play"), Sense::click());
    if play_resp.clicked() {
        v.audition_playing = !v.audition_playing;
    }
    g.rect_filled(
        play_r,
        4.0,
        if v.audition_playing {
            Color32::from_rgb(0x28, 0xb4, 0x46)
        } else if play_resp.hovered() {
            Color32::from_rgb(0x30, 0x30, 0x35)
        } else {
            Color32::from_rgb(0x26, 0x26, 0x2a)
        },
    );
    crate::icons::paint_fill(
        &g,
        play_r,
        if v.audition_playing {
            crate::icons::STOP
        } else {
            crate::icons::PLAY
        },
        13.0,
        if v.audition_playing {
            Color32::WHITE
        } else {
            TEXT
        },
    );
    rx = play_r.left() - 4.0;

    if can_revert {
        let rev_w = 46.0;
        let rev_r = Rect::from_min_size(pos2(rx - rev_w, fy), vec2(rev_w, 20.0));
        if footer_button(ui, &g, rev_r, "revert", tr("Revert"), true) {
            revert(p, v);
        }
    }
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
