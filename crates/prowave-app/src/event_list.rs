//! Logic's List Editors (View > Show List Editors, D): the Event list on the right. It lists
//! the MIDI events of the selected region (or of the selected track) with their position,
//! note, velocity and length, and a line marks where the playhead is. While the song plays the
//! line moves down the list at the speed of the notes (fast passages go quickly, slow ones
//! slowly) and the list pages along with it.

use crate::i18n::tr;
use crate::model::{Project, TrackKind, CHANNEL_PRESSURE, PITCH_BEND, SYSEX_BYTE};
use crate::widgets::TEXT_DIM;
use crate::UiState;
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Rect, Sense, Stroke, Ui};

pub const WIDTH: f32 = 360.0;
const ROW_H: f32 = 19.0;
const TICKS_PER_BEAT: i64 = 960;
const TICKS_PER_DIVISION: i64 = 240;

// Authentic Liquid Glass Palette
const WELL_BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x18); // #141518 deep recessed well
const TAB_BAR_BG: Color32 = Color32::from_rgb(0x18, 0x19, 0x1c); // #18191C
const TAB_ACTIVE_BG: Color32 = Color32::from_rgb(0x22, 0x25, 0x2a); // #22252A
const TAB_INACTIVE_BG: Color32 = Color32::from_rgb(0x18, 0x19, 0x1c);
const TOOLBAR_BG: Color32 = Color32::from_rgb(0x1e, 0x20, 0x24); // #1E2024
const FILTER_BAR_BG: Color32 = Color32::from_rgb(0x18, 0x19, 0x1d); // #18191D
const HEADER_BG: Color32 = Color32::from_rgb(0x1e, 0x20, 0x24); // #1E2024
const BORDER_COLOR: Color32 = Color32::from_rgb(0x2e, 0x31, 0x38); // #2E3138
const ROW_DIVIDER: Color32 = Color32::from_rgb(0x23, 0x26, 0x2c); // #23262C
const ROW_BG_EVEN: Color32 = Color32::from_rgb(0x16, 0x18, 0x1c); // #16181C
const ROW_BG_ODD: Color32 = Color32::from_rgb(0x1b, 0x1d, 0x22); // #1B1D22

const PILL_ACTIVE_BG: Color32 = Color32::from_rgb(0xf2, 0xf3, 0xf5); // #F2F3F5
const PILL_ACTIVE_TEXT: Color32 = Color32::from_rgb(0x11, 0x12, 0x15); // #111215
const PILL_INACTIVE_BG: Color32 = Color32::from_rgb(0x20, 0x22, 0x26); // #202226
const PILL_INACTIVE_TEXT: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xac); // #9AA0AC

/// Tabs available in the List Editors panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ListTab {
    #[default]
    Event,
    Marker,
    Tempo,
    Signature,
}

/// The list's own state.
#[derive(Clone, Debug)]
pub struct ListView {
    /// Pixels the list is scrolled down.
    pub scroll: f32,
    /// Logic's Catch Playhead: the list follows the playhead while the song plays.
    pub follow: bool,
    /// Currently active list editor tab.
    pub active_tab: ListTab,
    pub filter_notes: bool,
    pub filter_program: bool,
    pub filter_pitch_bend: bool,
    pub filter_controller: bool,
    pub filter_aftertouch: bool,
    pub filter_poly_pressure: bool,
    pub filter_sysex: bool,
    pub filter_extra_info: bool,
}

impl Default for ListView {
    fn default() -> Self {
        Self {
            scroll: 0.0,
            follow: true,
            active_tab: ListTab::Event,
            filter_notes: true,
            filter_program: true,
            filter_pitch_bend: true,
            filter_controller: true,
            filter_aftertouch: true,
            filter_poly_pressure: true,
            filter_sysex: true,
            filter_extra_info: true,
        }
    }
}

/// One note or MIDI control event of the list.
struct Event {
    /// Position in beats from the start of the song.
    beats: f64,
    /// Belongs to another region of the track: shown grey, as context.
    dim: bool,
    /// A controller event (the sustain pedal) rather than a note.
    control: bool,
    /// The note's pitch, or the controller's number.
    pitch: u8,
    /// The note's velocity.
    vel: u8,
    /// A controller's value in full (pitch bend is 14 bits); for a System Exclusive message its
    /// number of bytes.
    value: u16,
    /// A System Exclusive message as hex words; empty for the other events.
    text: String,
    len: f64,
}

/// The events to show: those of the selected MIDI region, with the other regions of its track
/// in grey around them (Logic shows them as context, and while a take is recorded the notes
/// of the earlier ones stay in view). With no region selected: all of the selected track's.
fn events(p: &Project, s: &UiState) -> Vec<Event> {
    let (track, selected) = match s.tracks.selected {
        Some((t, r))
            if p.tracks
                .get(t)
                .is_some_and(|tr| tr.kind != TrackKind::Audio && r < tr.regions.len()) =>
        {
            (t, Some(r))
        }
        _ => (p.selected_track, None),
    };
    let Some(track) = p.tracks.get(track).filter(|t| t.kind != TrackKind::Audio) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (ri, r) in track.regions.iter().enumerate() {
        let offset = p.region_start_beats(r);
        let dim = selected.is_some_and(|sr| sr != ri);
        // The bytes of a System Exclusive message are consecutive controls at one start: they are
        // one row.
        let mut i = 0;
        while i < r.controls.len() {
            let c = r.controls[i];
            if c.num == SYSEX_BYTE {
                let mut bytes = Vec::new();
                while i < r.controls.len()
                    && r.controls[i].num == SYSEX_BYTE
                    && r.controls[i].start == c.start
                {
                    bytes.push(r.controls[i].val.min(255) as u8);
                    i += 1;
                }
                if s.lists.filter_sysex {
                    out.push(Event {
                        beats: offset + c.start as f64,
                        dim,
                        control: true,
                        pitch: SYSEX_BYTE,
                        vel: 0,
                        value: bytes.len() as u16,
                        text: crate::sysex::text(&bytes),
                        len: 0.0,
                    });
                }
                continue;
            }
            let keep = match c.num {
                PITCH_BEND => s.lists.filter_pitch_bend,
                CHANNEL_PRESSURE => s.lists.filter_aftertouch,
                _ => s.lists.filter_controller,
            };
            if keep {
                out.push(Event {
                    beats: offset + c.start as f64,
                    dim,
                    control: true,
                    pitch: c.num,
                    vel: 0,
                    value: c.val,
                    text: String::new(),
                    len: 0.0,
                });
            }
            i += 1;
        }
        if s.lists.filter_notes {
            for n in &r.notes {
                out.push(Event {
                    beats: offset + n.start as f64,
                    dim,
                    control: false,
                    pitch: n.pitch,
                    vel: n.vel,
                    value: 0,
                    text: String::new(),
                    len: n.len as f64,
                });
            }
        }
    }
    // A controller at the same place as a note comes first, as in Logic.
    out.sort_by(|a, b| {
        a.beats
            .total_cmp(&b.beats)
            .then(b.control.cmp(&a.control))
            .then(a.pitch.cmp(&b.pitch))
    });
    out
}

/// Logic's position "bar beat division" and the ticks, from a position in beats.
fn position_parts(beats: f64, bpb: f64) -> ([i64; 3], i64) {
    let total = (beats * TICKS_PER_BEAT as f64).round().max(0.0) as i64;
    let per_bar = ((bpb * TICKS_PER_BEAT as f64).round() as i64).max(1);
    let rem = total % per_bar;
    let in_beat = rem % TICKS_PER_BEAT;
    (
        [
            total / per_bar + 1,
            rem / TICKS_PER_BEAT + 1,
            in_beat / TICKS_PER_DIVISION + 1,
        ],
        in_beat % TICKS_PER_DIVISION + 1,
    )
}

/// [`position_parts`] for a position in the song: bars have the length of their signature.
fn position_in(p: &Project, beats: f64) -> ([i64; 3], i64) {
    let bar0 = p.beats_to_bar(beats).floor();
    let (mut parts, tick) = position_parts(beats - p.bar_to_beats(bar0), p.bar_len(bar0 as u32));
    parts[0] += bar0 as i64;
    (parts, tick)
}

/// A length as Logic shows it: bars, beats, divisions and ticks, all counted from zero.
fn length_parts(len: f64, bpb: f64) -> [i64; 4] {
    let total = (len * TICKS_PER_BEAT as f64).round().max(0.0) as i64;
    let per_bar = ((bpb * TICKS_PER_BEAT as f64).round() as i64).max(1);
    let rem = total % per_bar;
    let in_beat = rem % TICKS_PER_BEAT;
    [
        total / per_bar,
        rem / TICKS_PER_BEAT,
        in_beat / TICKS_PER_DIVISION,
        in_beat % TICKS_PER_DIVISION,
    ]
}

/// Logic names the octaves so that pitch 60 is C3.
fn note_name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!(
        "{}{}",
        NAMES[(pitch % 12) as usize],
        (pitch / 12) as i32 - 2
    )
}

/// What the Length/Info column says for a controller.
fn controller_name(num: u8) -> String {
    match num {
        crate::recorder::SUSTAIN => tr("Sustain").to_string(),
        n => format!("{} {n}", tr("Controller")),
    }
}

/// The type of a controller-kind event: a controller, pitch bend, channel pressure or SysEx.
fn control_type(num: u8) -> String {
    match num {
        PITCH_BEND => tr("Pitch Bend").to_string(),
        CHANNEL_PRESSURE => tr("Aftertouch").to_string(),
        SYSEX_BYTE => tr("Syst. Exclusive").to_string(),
        _ => tr("Control").to_string(),
    }
}

/// Index of the first event the playhead has not reached yet: the line goes above it.
fn playhead_row(events: &[Event], position: f64) -> usize {
    events.partition_point(|e| e.beats <= position + 1e-9)
}

/// The scroll that follows the playhead: it stays where it is while the line is in view and
/// pages along (line near the top) when the line reaches the bottom or leaves at the top.
fn followed_scroll(scroll: f32, line_y: f32, view_h: f32, max_scroll: f32) -> f32 {
    let rel = line_y - scroll;
    let new = if rel > view_h * 0.85 || rel < 0.0 {
        line_y - view_h * 0.15
    } else {
        scroll
    };
    new.clamp(0.0, max_scroll.max(0.0))
}

fn chevron_down(g: &Painter, c: egui::Pos2) {
    let st = Stroke::new(1.2, TEXT_DIM);
    g.line_segment([c + vec2(-3.0, -1.5), c + vec2(0.0, 1.5)], st);
    g.line_segment([c + vec2(0.0, 1.5), c + vec2(3.0, -1.5)], st);
}

/// The window that adds a System Exclusive message (typed in hex) to the region at the playhead.
fn sysex_window(ctx: &egui::Context, p: &mut Project, s: &mut UiState) {
    let Some(mut text) = s.sysex_entry.take() else {
        return;
    };
    let target = crate::sysex::target(p, s.tracks.selected);
    let mut open = true;
    let mut done = false;
    egui::Window::new(tr("Add SysEx"))
        .id(egui::Id::unique("sysex_entry"))
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.label(tr("Bytes in hex, from F0 to F7"));
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .desired_rows(3)
                    .desired_width(320.0),
            );
            let parsed = crate::sysex::parse(&text);
            if parsed.is_err() && !text.trim().is_empty() {
                ui.colored_label(
                    Color32::from_rgb(0xe0, 0x60, 0x50),
                    tr("The message must start with F0 and end with F7, in hex"),
                );
            }
            if target.is_none() {
                ui.label(tr("Select a MIDI region to add the message to"));
            }
            ui.horizontal(|ui| {
                let can = parsed.is_ok() && target.is_some();
                if ui
                    .add_enabled(can, egui::Button::new(tr("Insert")))
                    .clicked()
                {
                    if let (Ok(bytes), Some((t, r))) = (&parsed, target) {
                        if crate::sysex::insert(p, t, r, bytes) {
                            s.status = Some(crate::i18n::trf(
                                "Added a SysEx message of {} bytes",
                                &[&bytes.len()],
                            ));
                        }
                    }
                    done = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    done = true;
                }
            });
        });
    if open && !done {
        s.sysex_entry = Some(text);
    }
}

pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState) {
    sysex_window(ui.ctx(), p, s);
    let area = ui.max_rect();
    let g = ui.painter_at(area);
    g.rect_filled(area, 0.0, WELL_BG);

    // 1. Tabs Header: Event / Marker / Tempo / Signature
    let mut y = area.top();
    let tabs = [
        (ListTab::Event, tr("Event")),
        (ListTab::Marker, tr("Marker")),
        (ListTab::Tempo, tr("Tempo")),
        (ListTab::Signature, tr("Signature")),
    ];
    let tab_w = area.width() / tabs.len() as f32;
    let tab_bar_rect = Rect::from_min_size(pos2(area.left(), y), vec2(area.width(), 26.0));
    g.rect_filled(tab_bar_rect, 0.0, TAB_BAR_BG);
    g.line_segment(
        [tab_bar_rect.left_bottom(), tab_bar_rect.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    for (i, (tab, label)) in tabs.iter().enumerate() {
        let r = Rect::from_min_size(pos2(area.left() + i as f32 * tab_w, y), vec2(tab_w, 26.0));
        let is_active = s.lists.active_tab == *tab;
        let resp = ui.interact(r, egui::Id::unique(("list_tab", i)), Sense::click());
        if resp.clicked() {
            s.lists.active_tab = *tab;
            s.lists.scroll = 0.0;
        }

        let fill = if is_active {
            TAB_ACTIVE_BG
        } else if resp.hovered() {
            Color32::from_rgb(0x1e, 0x20, 0x24)
        } else {
            TAB_INACTIVE_BG
        };
        g.rect_filled(r, 0.0, fill);

        if is_active {
            // Crisp top highlight and subtle vertical borders
            g.line_segment(
                [r.left_top(), r.right_top()],
                Stroke::new(1.5, Color32::from_rgb(0xf2, 0xf3, 0xf5)),
            );
            g.line_segment(
                [r.left_top(), r.left_bottom()],
                Stroke::new(1.0, BORDER_COLOR),
            );
            g.line_segment(
                [r.right_top(), r.right_bottom()],
                Stroke::new(1.0, BORDER_COLOR),
            );
        }

        g.text(
            r.center(),
            Align2::CENTER_CENTER,
            *label,
            FontId::proportional(12.0),
            if is_active {
                Color32::from_rgb(0xf2, 0xf3, 0xf5)
            } else {
                Color32::from_rgb(0x8e, 0x92, 0x9b)
            },
        );
    }
    y += 26.0;

    // 2. Main List Toolbar: Hierarchy Back, Menus, Follow Playhead
    let bar = Rect::from_min_size(pos2(area.left(), y), vec2(area.width(), 28.0));
    g.rect_filled(bar, 0.0, TOOLBAR_BG);
    g.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    // Hierarchy back button (Phosphor CARET_LEFT)
    let back_rect =
        Rect::from_center_size(pos2(bar.left() + 16.0, bar.center().y), vec2(20.0, 20.0));
    let back_resp = ui
        .interact(
            back_rect,
            egui::Id::unique(("event_list", "back")),
            Sense::click(),
        )
        .on_hover_text("Hierarchy Up");
    if back_resp.hovered() {
        g.rect_filled(back_rect, 3.0, Color32::from_rgb(0x28, 0x2b, 0x32));
    }
    g.text(
        back_rect.center(),
        Align2::CENTER_CENTER,
        crate::icons::CARET_LEFT,
        FontId::proportional(13.0),
        Color32::from_rgb(0xdd, 0xe0, 0xe8),
    );

    // Edit, Functions, View action headers
    let mut x = bar.left() + 38.0;
    for label in [tr("Edit"), tr("Functions"), tr("View")] {
        let galley = g.layout_no_wrap(
            label.into(),
            FontId::proportional(12.0),
            Color32::from_rgb(0xdd, 0xe0, 0xe8),
        );
        let w = galley.size().x;
        g.text(
            pos2(x, bar.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.0),
            Color32::from_rgb(0xdd, 0xe0, 0xe8),
        );
        chevron_down(&g, pos2(x + w + 7.0, bar.center().y + 1.0));
        x += w + 20.0;
    }

    // Follow Playhead button (Phosphor LINK)
    let catch = Rect::from_center_size(pos2(bar.right() - 22.0, bar.center().y), vec2(24.0, 20.0));
    let resp = ui
        .interact(
            catch,
            egui::Id::unique(("event_list", "catch")),
            Sense::click(),
        )
        .on_hover_text("Link View (Follow Playhead)");
    if resp.clicked() {
        s.lists.follow = !s.lists.follow;
    }
    g.rect_filled(
        catch,
        3.0,
        if s.lists.follow {
            PILL_ACTIVE_BG
        } else {
            PILL_INACTIVE_BG
        },
    );
    if !s.lists.follow {
        g.rect_stroke(
            catch,
            3.0,
            Stroke::new(1.0, BORDER_COLOR),
            egui::StrokeKind::Inside,
        );
    }
    g.text(
        catch.center(),
        Align2::CENTER_CENTER,
        crate::icons::LINK,
        FontId::proportional(12.0),
        if s.lists.follow {
            PILL_ACTIVE_TEXT
        } else {
            PILL_INACTIVE_TEXT
        },
    );
    y += 28.0;

    match s.lists.active_tab {
        ListTab::Event => {
            show_event_tab(ui, p, s, area, &g, &mut y);
        }
        ListTab::Marker => {
            show_marker_tab(ui, p, s, area, &g, &mut y);
        }
        ListTab::Tempo => {
            show_tempo_tab(ui, p, s, area, &g, &mut y);
        }
        ListTab::Signature => {
            show_signature_tab(ui, p, s, area, &g, &mut y);
        }
    }

    // Border line towards tracks area
    g.line_segment(
        [area.left_top(), area.left_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );
}

fn show_event_tab(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    area: Rect,
    g: &Painter,
    y: &mut f32,
) {
    let events = events(p, s);

    // Filter pills grid (4 columns x 2 rows)
    let filter_rect = Rect::from_min_size(pos2(area.left(), *y), vec2(area.width(), 48.0));
    g.rect_filled(filter_rect, 0.0, FILTER_BAR_BG);
    g.line_segment(
        [filter_rect.left_bottom(), filter_rect.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    let filters: [[(&str, &str, bool); 4]; 2] = [
        [
            (tr("Notes"), "Filter Note Events", s.lists.filter_notes),
            (
                tr("Progr. Change"),
                "Filter Program Change Events",
                s.lists.filter_program,
            ),
            (
                tr("Pitch Bend"),
                "Filter Pitch Bend Events",
                s.lists.filter_pitch_bend,
            ),
            (
                tr("Controller"),
                "Filter Controller Events",
                s.lists.filter_controller,
            ),
        ],
        [
            (
                tr("Aftertouch"),
                "Filter Aftertouch Events",
                s.lists.filter_aftertouch,
            ),
            (
                tr("Poly Aftertouch"),
                "Filter Polyphonic Aftertouch Events",
                s.lists.filter_poly_pressure,
            ),
            (
                tr("Syst. Exclusive"),
                "Filter System Exclusive Events",
                s.lists.filter_sysex,
            ),
            (
                tr("Additional Info"),
                "Filter Additional Event Info",
                s.lists.filter_extra_info,
            ),
        ],
    ];

    let pad_x = 8.0;
    let gap = 4.0;
    let bw = (area.width() - 2.0 * pad_x - 3.0 * gap) / 4.0;
    for (row_idx, row) in filters.iter().enumerate() {
        for (col_idx, (label, tip, is_on)) in row.iter().enumerate() {
            let pill_rect = Rect::from_min_size(
                pos2(
                    area.left() + pad_x + col_idx as f32 * (bw + gap),
                    *y + 4.0 + row_idx as f32 * 20.0,
                ),
                vec2(bw, 17.0),
            );
            let resp = ui
                .interact(
                    pill_rect,
                    egui::Id::unique(("filter_pill", row_idx, col_idx)),
                    Sense::click(),
                )
                .on_hover_text(*tip);
            if resp.clicked() {
                match (row_idx, col_idx) {
                    (0, 0) => s.lists.filter_notes = !s.lists.filter_notes,
                    (0, 1) => s.lists.filter_program = !s.lists.filter_program,
                    (0, 2) => s.lists.filter_pitch_bend = !s.lists.filter_pitch_bend,
                    (0, 3) => s.lists.filter_controller = !s.lists.filter_controller,
                    (1, 0) => s.lists.filter_aftertouch = !s.lists.filter_aftertouch,
                    (1, 1) => s.lists.filter_poly_pressure = !s.lists.filter_poly_pressure,
                    (1, 2) => s.lists.filter_sysex = !s.lists.filter_sysex,
                    (1, 3) => s.lists.filter_extra_info = !s.lists.filter_extra_info,
                    _ => {}
                }
            }

            let bg_color = if *is_on {
                PILL_ACTIVE_BG
            } else if resp.hovered() {
                Color32::from_rgb(0x28, 0x2b, 0x31)
            } else {
                PILL_INACTIVE_BG
            };
            g.rect_filled(pill_rect, 3.0, bg_color);
            if !*is_on {
                g.rect_stroke(
                    pill_rect,
                    3.0,
                    Stroke::new(1.0, BORDER_COLOR),
                    egui::StrokeKind::Inside,
                );
            }

            let font = FontId::proportional(10.5);
            let text = crate::mixer::elide(g, label, &font, bw - 4.0);
            g.text(
                pill_rect.center(),
                Align2::CENTER_CENTER,
                text,
                font,
                if *is_on {
                    PILL_ACTIVE_TEXT
                } else {
                    PILL_INACTIVE_TEXT
                },
            );
        }
    }
    *y += 48.0;

    // Sub-toolbar: Add Event, Type Selector, SysEx, Quantize
    let head = Rect::from_min_size(pos2(area.left(), *y), vec2(area.width(), 26.0));
    g.rect_filled(head, 0.0, HEADER_BG);
    g.line_segment(
        [head.left_bottom(), head.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    // Add button (Phosphor PLUS)
    let plus_btn =
        Rect::from_center_size(pos2(head.left() + 18.0, head.center().y), vec2(20.0, 20.0));
    let plus_resp = ui
        .interact(
            plus_btn,
            egui::Id::unique(("event_list", "add_event")),
            Sense::click(),
        )
        .on_hover_text("Create Event at Playhead");
    if plus_resp.hovered() {
        g.rect_filled(plus_btn, 3.0, Color32::from_rgb(0x28, 0x2b, 0x32));
    }
    g.text(
        plus_btn.center(),
        Align2::CENTER_CENTER,
        crate::icons::PLUS,
        FontId::proportional(12.0),
        Color32::from_rgb(0xdd, 0xe0, 0xe8),
    );
    if plus_resp.clicked() {
        if let Some((t, r)) = s.tracks.selected {
            let offset = p
                .tracks
                .get(t)
                .and_then(|tr| tr.regions.get(r))
                .map(|rg| p.region_start_beats(rg));
            if let Some(offset) = offset {
                if let Some(track) = p.tracks.get_mut(t) {
                    if let Some(region) = track.regions.get_mut(r) {
                        let local_beat = (p.position_beats - offset).max(0.0) as f32;
                        region.notes.push(crate::model::Note {
                            start: local_beat,
                            len: 1.0,
                            pitch: 60,
                            vel: 100,
                            mute: false,
                            lyric: crate::lyrics::Lyric::default(),
                        });
                    }
                }
            }
        }
    }

    // Type Selector dropdown label
    g.text(
        pos2(head.left() + 40.0, head.center().y),
        Align2::LEFT_CENTER,
        tr("Notes"),
        FontId::proportional(12.0),
        Color32::from_rgb(0xf2, 0xf3, 0xf5),
    );
    chevron_down(g, pos2(head.left() + 82.0, head.center().y + 1.0));

    // SysEx modal entry button
    let sysex_btn = Rect::from_center_size(
        pos2(head.right() - 172.0, head.center().y),
        vec2(58.0, 18.0),
    );
    let sysex_resp = ui
        .interact(
            sysex_btn,
            egui::Id::unique(("event_list", "sysex")),
            Sense::click(),
        )
        .on_hover_text("Insert System Exclusive Message");
    if sysex_resp.clicked() {
        s.sysex_entry.get_or_insert_with(String::new);
    }
    g.rect_filled(
        sysex_btn,
        3.0,
        if sysex_resp.hovered() {
            Color32::from_rgb(0x2d, 0x30, 0x38)
        } else {
            Color32::from_rgb(0x22, 0x24, 0x2a)
        },
    );
    g.rect_stroke(
        sysex_btn,
        3.0,
        Stroke::new(1.0, BORDER_COLOR),
        egui::StrokeKind::Inside,
    );
    g.text(
        sysex_btn.center(),
        Align2::CENTER_CENTER,
        tr("SysEx…"),
        FontId::proportional(11.0),
        Color32::from_rgb(0xdd, 0xe0, 0xe8),
    );

    // Quantize section
    g.text(
        pos2(head.right() - 104.0, head.center().y),
        Align2::RIGHT_CENTER,
        tr("Quantize:"),
        FontId::proportional(11.0),
        Color32::from_rgb(0x8e, 0x92, 0x9b),
    );
    let qbox = Rect::from_min_size(
        pos2(head.right() - 100.0, head.top() + 4.0),
        vec2(64.0, 18.0),
    );
    g.rect_filled(qbox, 3.0, Color32::from_rgb(0x18, 0x19, 0x1d));
    g.rect_stroke(
        qbox,
        3.0,
        Stroke::new(1.0, BORDER_COLOR),
        egui::StrokeKind::Inside,
    );
    g.text(
        qbox.left_center() + vec2(6.0, 0.0),
        Align2::LEFT_CENTER,
        tr("Off"),
        FontId::proportional(11.0),
        Color32::from_rgb(0xdd, 0xe0, 0xe8),
    );
    chevron_down(g, qbox.right_center() - vec2(8.0, 0.0));

    // Quantize apply button (Phosphor CHECK)
    let q_rect =
        Rect::from_center_size(pos2(head.right() - 16.0, head.center().y), vec2(20.0, 20.0));
    let q_resp = ui
        .interact(
            q_rect,
            egui::Id::unique(("event_list", "apply_quantize")),
            Sense::click(),
        )
        .on_hover_text("Apply Quantize");
    if q_resp.hovered() {
        g.rect_filled(q_rect, 3.0, Color32::from_rgb(0x28, 0x2b, 0x32));
    }
    g.text(
        q_rect.center(),
        Align2::CENTER_CENTER,
        crate::icons::CHECK,
        FontId::proportional(12.0),
        Color32::from_rgb(0xdd, 0xe0, 0xe8),
    );
    *y += 26.0;

    // Column Titles Header (with 8px left margin)
    let cols = Rect::from_min_size(pos2(area.left(), *y), vec2(area.width(), 20.0));
    g.rect_filled(cols, 0.0, HEADER_BG);
    g.line_segment(
        [cols.left_bottom(), cols.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    let f = FontId::proportional(10.5);
    let col_color = Color32::from_rgb(0x9a, 0xa0, 0xac);
    let cx = |v: f32| area.left() + v;

    g.text(
        pos2(cx(14.0), cols.center().y),
        Align2::CENTER_CENTER,
        "L",
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(28.0), cols.center().y),
        Align2::CENTER_CENTER,
        "M",
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(78.0), cols.center().y),
        Align2::CENTER_CENTER,
        tr("Position"),
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(138.0), cols.center().y),
        Align2::CENTER_CENTER,
        tr("Status"),
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(178.0), cols.center().y),
        Align2::CENTER_CENTER,
        tr("Ch"),
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(214.0), cols.center().y),
        Align2::CENTER_CENTER,
        tr("Num"),
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(248.0), cols.center().y),
        Align2::CENTER_CENTER,
        tr("Val"),
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(296.0), cols.center().y),
        Align2::LEFT_CENTER,
        tr("Length/Info"),
        f.clone(),
        col_color,
    );
    *y += 20.0;

    // Table Rows Body
    let list = Rect::from_min_max(pos2(area.left(), *y), area.max);
    let content_h = events.len() as f32 * ROW_H;
    let max_scroll = (content_h - list.height() + ROW_H).max(0.0);
    let resp = ui.interact(
        list,
        egui::Id::unique(("event_list", "rows")),
        Sense::hover(),
    );
    if resp.hovered() {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        s.lists.scroll = (s.lists.scroll - dy).clamp(0.0, max_scroll);
    }
    let line_row = playhead_row(&events, p.position_beats);
    let line_y = line_row as f32 * ROW_H;
    if s.lists.follow && p.playing {
        s.lists.scroll = followed_scroll(s.lists.scroll, line_y, list.height(), max_scroll);
    }
    s.lists.scroll = s.lists.scroll.clamp(0.0, max_scroll);

    let gl = ui.painter_at(list);

    // Full viewport zebra striping & dividers
    let num_stripes = ((list.height() / ROW_H).ceil() as usize) + 2;
    let stripe_offset = (s.lists.scroll / ROW_H).floor() as usize;
    for idx in 0..num_stripes {
        let row_idx = stripe_offset + idx;
        let y_top = list.top() + (row_idx as f32 * ROW_H) - s.lists.scroll;
        if y_top >= list.bottom() {
            break;
        }
        let row_rect = Rect::from_min_size(
            pos2(list.left(), y_top.max(list.top())),
            vec2(list.width(), ROW_H.min(list.bottom() - y_top)),
        );
        let bg_color = if row_idx % 2 == 0 {
            ROW_BG_EVEN
        } else {
            ROW_BG_ODD
        };
        gl.rect_filled(row_rect, 0.0, bg_color);
        gl.line_segment(
            [
                pos2(list.left(), y_top + ROW_H),
                pos2(list.right(), y_top + ROW_H),
            ],
            Stroke::new(1.0, ROW_DIVIDER),
        );
    }

    let first = (s.lists.scroll / ROW_H).floor() as usize;
    let last = (((s.lists.scroll + list.height()) / ROW_H).ceil() as usize).min(events.len());
    let font = FontId::monospace(11.0);
    let ink = Color32::from_rgb(0xe6, 0xe6, 0xeb);

    for (i, e) in events.iter().enumerate().take(last).skip(first) {
        let cy = list.top() + i as f32 * ROW_H - s.lists.scroll + ROW_H / 2.0;
        let ([bar, beat, div], tick) = position_in(p, e.beats);
        let l = length_parts(e.len, p.bar_len_at(e.beats));
        let color = if e.dim { Color32::from_gray(0x76) } else { ink };
        let at = |x: f32, align: Align2, text: String| {
            gl.text(pos2(cx(x), cy), align, text, font.clone(), color);
        };
        at(94.0, Align2::RIGHT_CENTER, format!("{bar} {beat} {div}"));
        at(114.0, Align2::RIGHT_CENTER, format!("{tick}"));
        at(184.0, Align2::RIGHT_CENTER, "1".to_string());
        let value = if e.control { e.value } else { u16::from(e.vel) };
        at(256.0, Align2::RIGHT_CENTER, format!("{value}"));
        if e.control {
            at(122.0, Align2::LEFT_CENTER, control_type(e.pitch));
            match e.pitch {
                SYSEX_BYTE => {
                    let shown: String = e.text.chars().take(30).collect();
                    at(268.0, Align2::LEFT_CENTER, shown);
                }
                PITCH_BEND | CHANNEL_PRESSURE => {}
                num => {
                    at(214.0, Align2::CENTER_CENTER, format!("{num}"));
                    at(268.0, Align2::LEFT_CENTER, controller_name(num));
                }
            }
        } else {
            at(122.0, Align2::LEFT_CENTER, tr("Note").to_string());
            at(214.0, Align2::CENTER_CENTER, note_name(e.pitch));
            at(
                list.width() - 8.0,
                Align2::RIGHT_CENTER,
                format!("{} {} {} {}", l[0], l[1], l[2], l[3]),
            );
        }
    }

    // Playhead tracking line
    let ly = list.top() + line_y - s.lists.scroll;
    if ly >= list.top() && ly <= list.bottom() && !events.is_empty() {
        gl.line_segment(
            [pos2(list.left(), ly), pos2(list.right(), ly)],
            Stroke::new(1.5, crate::tracks_area::playhead_colors(p.recording).0),
        );
    }
}

fn show_marker_tab(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    area: Rect,
    g: &Painter,
    y: &mut f32,
) {
    let head = Rect::from_min_size(pos2(area.left(), *y), vec2(area.width(), 26.0));
    g.rect_filled(head, 0.0, HEADER_BG);
    g.line_segment(
        [head.left_bottom(), head.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    let plus_btn =
        Rect::from_center_size(pos2(head.left() + 18.0, head.center().y), vec2(20.0, 20.0));
    let plus_resp = ui
        .interact(
            plus_btn,
            egui::Id::unique(("marker_list", "add")),
            Sense::click(),
        )
        .on_hover_text("Create Marker at Playhead");
    if plus_resp.hovered() {
        g.rect_filled(plus_btn, 3.0, Color32::from_rgb(0x28, 0x2b, 0x32));
    }
    g.text(
        plus_btn.center(),
        Align2::CENTER_CENTER,
        crate::icons::PLUS,
        FontId::proportional(12.0),
        Color32::from_rgb(0xdd, 0xe0, 0xe8),
    );
    if plus_resp.clicked() {
        p.add_marker(p.position_beats);
    }

    g.text(
        pos2(head.left() + 40.0, head.center().y),
        Align2::LEFT_CENTER,
        format!("{} ({})", tr("Marker"), p.markers.len()),
        FontId::proportional(12.0),
        Color32::from_rgb(0xf2, 0xf3, 0xf5),
    );
    *y += 26.0;

    let cols = Rect::from_min_size(pos2(area.left(), *y), vec2(area.width(), 20.0));
    g.rect_filled(cols, 0.0, HEADER_BG);
    g.line_segment(
        [cols.left_bottom(), cols.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );
    let f = FontId::proportional(10.5);
    let col_color = Color32::from_rgb(0x9a, 0xa0, 0xac);
    let cx = |v: f32| area.left() + v;

    g.text(
        pos2(cx(16.0), cols.center().y),
        Align2::CENTER_CENTER,
        "L",
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(80.0), cols.center().y),
        Align2::CENTER_CENTER,
        tr("Position"),
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(150.0), cols.center().y),
        Align2::LEFT_CENTER,
        tr("Name"),
        f.clone(),
        col_color,
    );
    *y += 20.0;

    let list = Rect::from_min_max(pos2(area.left(), *y), area.max);
    let content_h = p.markers.len() as f32 * ROW_H;
    let max_scroll = (content_h - list.height() + ROW_H).max(0.0);
    let resp = ui.interact(
        list,
        egui::Id::unique(("marker_list", "rows")),
        Sense::hover(),
    );
    if resp.hovered() {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        s.lists.scroll = (s.lists.scroll - dy).clamp(0.0, max_scroll);
    }
    s.lists.scroll = s.lists.scroll.clamp(0.0, max_scroll);

    let gl = ui.painter_at(list);
    let num_stripes = ((list.height() / ROW_H).ceil() as usize) + 2;
    let stripe_offset = (s.lists.scroll / ROW_H).floor() as usize;
    for idx in 0..num_stripes {
        let row_idx = stripe_offset + idx;
        let y_top = list.top() + (row_idx as f32 * ROW_H) - s.lists.scroll;
        if y_top >= list.bottom() {
            break;
        }
        let row_rect = Rect::from_min_size(
            pos2(list.left(), y_top.max(list.top())),
            vec2(list.width(), ROW_H.min(list.bottom() - y_top)),
        );
        let bg_color = if row_idx % 2 == 0 {
            ROW_BG_EVEN
        } else {
            ROW_BG_ODD
        };
        gl.rect_filled(row_rect, 0.0, bg_color);
        gl.line_segment(
            [
                pos2(list.left(), y_top + ROW_H),
                pos2(list.right(), y_top + ROW_H),
            ],
            Stroke::new(1.0, ROW_DIVIDER),
        );
    }

    let first = (s.lists.scroll / ROW_H).floor() as usize;
    let last = (((s.lists.scroll + list.height()) / ROW_H).ceil() as usize).min(p.markers.len());
    let font = FontId::monospace(11.0);
    let ink = Color32::from_rgb(0xe6, 0xe6, 0xeb);

    for (i, m) in p.markers.iter().enumerate().take(last).skip(first) {
        let cy = list.top() + i as f32 * ROW_H - s.lists.scroll + ROW_H / 2.0;
        let row_rect = Rect::from_min_size(
            pos2(list.left(), cy - ROW_H / 2.0),
            vec2(list.width(), ROW_H),
        );
        let r_resp = ui.interact(
            row_rect,
            egui::Id::unique(("marker_row", i)),
            Sense::click(),
        );
        if r_resp.clicked() {
            p.position_beats = m.beat;
        }
        if r_resp.hovered() {
            gl.rect_filled(
                row_rect,
                0.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 12),
            );
        }

        gl.circle_filled(pos2(cx(16.0), cy), 4.5, m.color);
        let ([bar, beat, div], tick) = position_in(p, m.beat);
        gl.text(
            pos2(cx(94.0), cy),
            Align2::RIGHT_CENTER,
            format!("{bar} {beat} {div}"),
            font.clone(),
            ink,
        );
        gl.text(
            pos2(cx(114.0), cy),
            Align2::RIGHT_CENTER,
            format!("{tick}"),
            font.clone(),
            Color32::from_gray(0x9a),
        );
        gl.text(
            pos2(cx(150.0), cy),
            Align2::LEFT_CENTER,
            &m.name,
            FontId::proportional(12.0),
            ink,
        );
    }
}

fn show_tempo_tab(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    area: Rect,
    g: &Painter,
    y: &mut f32,
) {
    let points = p.tempo_points();
    let head = Rect::from_min_size(pos2(area.left(), *y), vec2(area.width(), 26.0));
    g.rect_filled(head, 0.0, HEADER_BG);
    g.line_segment(
        [head.left_bottom(), head.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    let plus_btn =
        Rect::from_center_size(pos2(head.left() + 18.0, head.center().y), vec2(20.0, 20.0));
    let plus_resp = ui
        .interact(
            plus_btn,
            egui::Id::unique(("tempo_list", "add")),
            Sense::click(),
        )
        .on_hover_text("Create Tempo Point at Playhead");
    if plus_resp.hovered() {
        g.rect_filled(plus_btn, 3.0, Color32::from_rgb(0x28, 0x2b, 0x32));
    }
    g.text(
        plus_btn.center(),
        Align2::CENTER_CENTER,
        crate::icons::PLUS,
        FontId::proportional(12.0),
        Color32::from_rgb(0xdd, 0xe0, 0xe8),
    );
    if plus_resp.clicked() {
        p.add_tempo_change(p.position_beats, p.tempo_at(p.position_beats));
    }

    g.text(
        pos2(head.left() + 40.0, head.center().y),
        Align2::LEFT_CENTER,
        format!("{} ({})", tr("Tempo"), points.len()),
        FontId::proportional(12.0),
        Color32::from_rgb(0xf2, 0xf3, 0xf5),
    );
    *y += 26.0;

    let cols = Rect::from_min_size(pos2(area.left(), *y), vec2(area.width(), 20.0));
    g.rect_filled(cols, 0.0, HEADER_BG);
    g.line_segment(
        [cols.left_bottom(), cols.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );
    let f = FontId::proportional(10.5);
    let col_color = Color32::from_rgb(0x9a, 0xa0, 0xac);
    let cx = |v: f32| area.left() + v;

    g.text(
        pos2(cx(80.0), cols.center().y),
        Align2::CENTER_CENTER,
        tr("Position"),
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(170.0), cols.center().y),
        Align2::CENTER_CENTER,
        tr("Tempo"),
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(240.0), cols.center().y),
        Align2::LEFT_CENTER,
        "Curve",
        f.clone(),
        col_color,
    );
    *y += 20.0;

    let list = Rect::from_min_max(pos2(area.left(), *y), area.max);
    let content_h = points.len() as f32 * ROW_H;
    let max_scroll = (content_h - list.height() + ROW_H).max(0.0);
    let resp = ui.interact(
        list,
        egui::Id::unique(("tempo_list", "rows")),
        Sense::hover(),
    );
    if resp.hovered() {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        s.lists.scroll = (s.lists.scroll - dy).clamp(0.0, max_scroll);
    }
    s.lists.scroll = s.lists.scroll.clamp(0.0, max_scroll);

    let gl = ui.painter_at(list);
    let num_stripes = ((list.height() / ROW_H).ceil() as usize) + 2;
    let stripe_offset = (s.lists.scroll / ROW_H).floor() as usize;
    for idx in 0..num_stripes {
        let row_idx = stripe_offset + idx;
        let y_top = list.top() + (row_idx as f32 * ROW_H) - s.lists.scroll;
        if y_top >= list.bottom() {
            break;
        }
        let row_rect = Rect::from_min_size(
            pos2(list.left(), y_top.max(list.top())),
            vec2(list.width(), ROW_H.min(list.bottom() - y_top)),
        );
        let bg_color = if row_idx % 2 == 0 {
            ROW_BG_EVEN
        } else {
            ROW_BG_ODD
        };
        gl.rect_filled(row_rect, 0.0, bg_color);
        gl.line_segment(
            [
                pos2(list.left(), y_top + ROW_H),
                pos2(list.right(), y_top + ROW_H),
            ],
            Stroke::new(1.0, ROW_DIVIDER),
        );
    }

    let first = (s.lists.scroll / ROW_H).floor() as usize;
    let last = (((s.lists.scroll + list.height()) / ROW_H).ceil() as usize).min(points.len());
    let font = FontId::monospace(11.0);
    let ink = Color32::from_rgb(0xe6, 0xe6, 0xeb);

    for (i, t) in points.iter().enumerate().take(last).skip(first) {
        let cy = list.top() + i as f32 * ROW_H - s.lists.scroll + ROW_H / 2.0;
        let row_rect = Rect::from_min_size(
            pos2(list.left(), cy - ROW_H / 2.0),
            vec2(list.width(), ROW_H),
        );
        let r_resp = ui.interact(row_rect, egui::Id::unique(("tempo_row", i)), Sense::click());
        if r_resp.clicked() {
            p.position_beats = t.beat;
        }
        if r_resp.hovered() {
            gl.rect_filled(
                row_rect,
                0.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 12),
            );
        }

        let ([bar, beat, div], tick) = position_in(p, t.beat);
        gl.text(
            pos2(cx(94.0), cy),
            Align2::RIGHT_CENTER,
            format!("{bar} {beat} {div}"),
            font.clone(),
            ink,
        );
        gl.text(
            pos2(cx(114.0), cy),
            Align2::RIGHT_CENTER,
            format!("{tick}"),
            font.clone(),
            Color32::from_gray(0x9a),
        );
        gl.text(
            pos2(cx(180.0), cy),
            Align2::RIGHT_CENTER,
            format!("{:.2} BPM", t.bpm),
            font.clone(),
            ink,
        );
        let ramp_text = if t.ramp { "Ramp" } else { "Hold" };
        gl.text(
            pos2(cx(240.0), cy),
            Align2::LEFT_CENTER,
            ramp_text,
            FontId::proportional(11.0),
            if t.ramp {
                Color32::from_rgb(0xf2, 0xf3, 0xf5)
            } else {
                Color32::from_gray(0x9a)
            },
        );
    }
}

fn show_signature_tab(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    area: Rect,
    g: &Painter,
    y: &mut f32,
) {
    let sigs = p.sigs();
    let mut sig_list = vec![(1u32, sigs.first)];
    for c in sigs.changes {
        sig_list.push((c.bar, (c.num, c.den)));
    }

    let head = Rect::from_min_size(pos2(area.left(), *y), vec2(area.width(), 26.0));
    g.rect_filled(head, 0.0, HEADER_BG);
    g.line_segment(
        [head.left_bottom(), head.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    let plus_btn =
        Rect::from_center_size(pos2(head.left() + 18.0, head.center().y), vec2(20.0, 20.0));
    let plus_resp = ui
        .interact(
            plus_btn,
            egui::Id::unique(("sig_list", "add")),
            Sense::click(),
        )
        .on_hover_text("Create Signature at Playhead Bar");
    if plus_resp.hovered() {
        g.rect_filled(plus_btn, 3.0, Color32::from_rgb(0x28, 0x2b, 0x32));
    }
    g.text(
        plus_btn.center(),
        Align2::CENTER_CENTER,
        crate::icons::PLUS,
        FontId::proportional(12.0),
        Color32::from_rgb(0xdd, 0xe0, 0xe8),
    );
    if plus_resp.clicked() {
        let cur_bar = (p.beats_to_bar(p.position_beats).floor() as u32).max(1);
        let cur_sig = p.signature_of_bar(cur_bar);
        p.set_signature_from(cur_bar, cur_sig);
    }

    g.text(
        pos2(head.left() + 40.0, head.center().y),
        Align2::LEFT_CENTER,
        format!("{} ({})", tr("Signature"), sig_list.len()),
        FontId::proportional(12.0),
        Color32::from_rgb(0xf2, 0xf3, 0xf5),
    );
    *y += 26.0;

    let cols = Rect::from_min_size(pos2(area.left(), *y), vec2(area.width(), 20.0));
    g.rect_filled(cols, 0.0, HEADER_BG);
    g.line_segment(
        [cols.left_bottom(), cols.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );
    let f = FontId::proportional(10.5);
    let col_color = Color32::from_rgb(0x9a, 0xa0, 0xac);
    let cx = |v: f32| area.left() + v;

    g.text(
        pos2(cx(50.0), cols.center().y),
        Align2::CENTER_CENTER,
        tr("Bar"),
        f.clone(),
        col_color,
    );
    g.text(
        pos2(cx(160.0), cols.center().y),
        Align2::LEFT_CENTER,
        tr("Signature"),
        f.clone(),
        col_color,
    );
    *y += 20.0;

    let list = Rect::from_min_max(pos2(area.left(), *y), area.max);
    let content_h = sig_list.len() as f32 * ROW_H;
    let max_scroll = (content_h - list.height() + ROW_H).max(0.0);
    let resp = ui.interact(list, egui::Id::unique(("sig_list", "rows")), Sense::hover());
    if resp.hovered() {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        s.lists.scroll = (s.lists.scroll - dy).clamp(0.0, max_scroll);
    }
    s.lists.scroll = s.lists.scroll.clamp(0.0, max_scroll);

    let gl = ui.painter_at(list);
    let num_stripes = ((list.height() / ROW_H).ceil() as usize) + 2;
    let stripe_offset = (s.lists.scroll / ROW_H).floor() as usize;
    for idx in 0..num_stripes {
        let row_idx = stripe_offset + idx;
        let y_top = list.top() + (row_idx as f32 * ROW_H) - s.lists.scroll;
        if y_top >= list.bottom() {
            break;
        }
        let row_rect = Rect::from_min_size(
            pos2(list.left(), y_top.max(list.top())),
            vec2(list.width(), ROW_H.min(list.bottom() - y_top)),
        );
        let bg_color = if row_idx % 2 == 0 {
            ROW_BG_EVEN
        } else {
            ROW_BG_ODD
        };
        gl.rect_filled(row_rect, 0.0, bg_color);
        gl.line_segment(
            [
                pos2(list.left(), y_top + ROW_H),
                pos2(list.right(), y_top + ROW_H),
            ],
            Stroke::new(1.0, ROW_DIVIDER),
        );
    }

    let first = (s.lists.scroll / ROW_H).floor() as usize;
    let last = (((s.lists.scroll + list.height()) / ROW_H).ceil() as usize).min(sig_list.len());
    let font = FontId::monospace(11.0);
    let ink = Color32::from_rgb(0xe6, 0xe6, 0xeb);

    for (i, &(bar, (num, den))) in sig_list.iter().enumerate().take(last).skip(first) {
        let cy = list.top() + i as f32 * ROW_H - s.lists.scroll + ROW_H / 2.0;
        let row_rect = Rect::from_min_size(
            pos2(list.left(), cy - ROW_H / 2.0),
            vec2(list.width(), ROW_H),
        );
        let r_resp = ui.interact(row_rect, egui::Id::unique(("sig_row", i)), Sense::click());
        if r_resp.clicked() {
            p.position_beats = p.bar_to_beats(bar.saturating_sub(1) as f64);
        }
        if r_resp.hovered() {
            gl.rect_filled(
                row_rect,
                0.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 12),
            );
        }

        gl.text(
            pos2(cx(50.0), cy),
            Align2::CENTER_CENTER,
            format!("Bar {bar}"),
            font.clone(),
            ink,
        );
        gl.text(
            pos2(cx(160.0), cy),
            Align2::LEFT_CENTER,
            format!("{num}/{den}"),
            FontId::proportional(12.5),
            Color32::from_rgb(0xf2, 0xf3, 0xf5),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_bars_beats_divisions_and_ticks_from_one() {
        assert_eq!(position_parts(0.0, 4.0), ([1, 1, 1], 1));
        // Bar 29, beat 2, division 3, 10 ticks in: Logic shows "29 2 3 11".
        let beats = 28.0 * 4.0 + 1.0 + 0.5 + 10.0 / 960.0;
        assert_eq!(position_parts(beats, 4.0), ([29, 2, 3], 11));
    }

    #[test]
    fn lengths_count_from_zero() {
        assert_eq!(length_parts(0.0, 4.0), [0, 0, 0, 0]);
        // Two divisions and 214 ticks, as in "0 0 2 214".
        let len = (2.0 * 240.0 + 214.0) / 960.0;
        assert_eq!(length_parts(len, 4.0), [0, 0, 2, 214]);
        assert_eq!(length_parts(4.0 + 1.0, 4.0), [1, 1, 0, 0]);
    }

    #[test]
    fn notes_are_named_like_logic() {
        assert_eq!(note_name(60), "C3");
        assert_eq!(note_name(63), "D#3");
        assert_eq!(note_name(0), "C-2");
    }

    #[test]
    fn the_line_sits_between_played_and_coming_notes() {
        let ev = |beats| Event {
            beats,
            dim: false,
            control: false,
            value: 0,
            text: String::new(),
            pitch: 60,
            vel: 100,
            len: 1.0,
        };
        let events = [ev(0.0), ev(1.0), ev(1.0), ev(2.0)];
        assert_eq!(playhead_row(&events, -0.5), 0);
        assert_eq!(playhead_row(&events, 0.0), 1);
        assert_eq!(playhead_row(&events, 1.0), 3);
        assert_eq!(playhead_row(&events, 9.0), 4);
    }

    #[test]
    fn the_list_pages_along_with_the_line() {
        // In view: stays.
        assert_eq!(followed_scroll(100.0, 200.0, 400.0, 1000.0), 100.0);
        // Past the bottom: the line goes back near the top.
        assert_eq!(followed_scroll(100.0, 480.0, 400.0, 1000.0), 420.0);
        // Above the view (after a loop): follows up.
        assert!((followed_scroll(500.0, 100.0, 400.0, 1000.0) - 40.0).abs() < 1e-3);
        // Never past the end.
        assert_eq!(followed_scroll(0.0, 2000.0, 400.0, 600.0), 600.0);
    }

    #[test]
    fn faster_notes_move_the_line_faster() {
        let ev = |beats| Event {
            beats,
            dim: false,
            control: false,
            value: 0,
            text: String::new(),
            pitch: 60,
            vel: 100,
            len: 0.25,
        };
        let fast: Vec<Event> = (0..32).map(|i| ev(i as f64 * 0.25)).collect();
        let slow: Vec<Event> = (0..32).map(|i| ev(i as f64 * 1.0)).collect();
        assert!(playhead_row(&fast, 2.0) > playhead_row(&slow, 2.0));
    }

    #[test]
    fn a_sysex_message_is_one_row_and_a_bend_keeps_its_14_bits() {
        use crate::model::{Control, CHANNEL_PRESSURE, PITCH_BEND, SYSEX_BYTE};
        let mut p = Project::demo();
        let controls = &mut p.tracks[0].regions[0].controls;
        controls.clear();
        controls.push(Control {
            start: 1.0,
            num: PITCH_BEND,
            val: 12345,
        });
        controls.push(Control {
            start: 2.0,
            num: CHANNEL_PRESSURE,
            val: 90,
        });
        for b in [0xf0u16, 0x41, 0xf7] {
            controls.push(Control {
                start: 3.0,
                num: SYSEX_BYTE,
                val: b,
            });
        }
        let mut s = UiState::new();
        s.tracks.selected = Some((0, 0));
        let ev = events(&p, &s);
        let sysex: Vec<&Event> = ev.iter().filter(|e| e.pitch == SYSEX_BYTE).collect();
        assert_eq!(sysex.len(), 1, "three bytes, one row");
        assert_eq!(sysex[0].text, "F0 41 F7");
        assert_eq!(sysex[0].value, 3, "its number of bytes");
        let bend = ev.iter().find(|e| e.pitch == PITCH_BEND).expect("the bend");
        assert_eq!(bend.value, 12345, "all 14 bits");
        let pressure = ev
            .iter()
            .find(|e| e.pitch == CHANNEL_PRESSURE)
            .expect("the pressure");
        assert_eq!(pressure.value, 90);
    }
}
