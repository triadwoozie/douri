//! Logic Pro's key commands for the transport, the selection and the regions of the Tracks area
//! (Logic Pro: Key Commands, U.S. preset). Each one runs a menu entry (`menus::apply`), so a key,
//! the menu and the control interface's `menu.invoke` do the same thing.
//!
//! Where this differs from Logic, and why:
//! - Return and the keypad's Enter come from egui as the same key, and so do 0 and the keypad's
//!   0. Return (either one) is Go to Beginning and 0 (either one) is Stop; Logic's keypad Enter
//!   for Play has no key here (Space plays).
//! - Option-Left and Option-Right move by the nudge value (Edit > Nudge Value, Control-Option-T,
//!   -D, -B and -M as in Logic): a tick, a division (a sixteenth note), a beat or a bar. Logic's
//!   SMPTE, sample and millisecond nudge values are not offered.
//! - Logic's Reverse Region has no key in its U.S. preset, and has none here.
//!
//! A key is matched with exactly its modifiers: Shift-I (Invert Selection) is not I (Show
//! Inspector), and Shift-Comma (Fast Rewind) is not Comma (Rewind). No key runs while a text
//! field has the keyboard, while a dialog is open or while Musical Typing is open (its keys
//! are notes). The region and selection keys wait while the pointer is over the Piano Roll,
//! which has the same keys for notes.

use crate::model::Project;
use crate::UiState;
use egui::{Event, Key, Modifiers};

/// A key with exactly these modifiers held.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Combo {
    pub key: Key,
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub cmd: bool,
}

const fn plain(key: Key) -> Combo {
    Combo {
        key,
        shift: false,
        alt: false,
        ctrl: false,
        cmd: false,
    }
}

const fn shift(key: Key) -> Combo {
    Combo {
        shift: true,
        ..plain(key)
    }
}

const fn alt(key: Key) -> Combo {
    Combo {
        alt: true,
        ..plain(key)
    }
}

const fn alt_shift(key: Key) -> Combo {
    Combo {
        shift: true,
        ..alt(key)
    }
}

const fn ctrl(key: Key) -> Combo {
    Combo {
        ctrl: true,
        ..plain(key)
    }
}

const fn ctrl_alt(key: Key) -> Combo {
    Combo {
        alt: true,
        ..ctrl(key)
    }
}

const fn ctrl_shift(key: Key) -> Combo {
    Combo {
        shift: true,
        ..ctrl(key)
    }
}

const fn cmd(key: Key) -> Combo {
    Combo {
        cmd: true,
        ..plain(key)
    }
}

const fn cmd_shift(key: Key) -> Combo {
    Combo {
        shift: true,
        ..cmd(key)
    }
}

/// Control and Command together: a key of its own on macOS only (elsewhere Control is the
/// command key, so these combinations do not exist there; see [`Combo::exists`]).
const fn ctrl_cmd(key: Key) -> Combo {
    Combo {
        ctrl: true,
        ..cmd(key)
    }
}

const fn alt_cmd(key: Key) -> Combo {
    Combo {
        alt: true,
        ..cmd(key)
    }
}

impl Combo {
    /// Whether `m` are this combination's modifiers and no others. On macOS Control and
    /// Command are two keys; elsewhere Control is the command key and stands for both.
    fn held(self, m: Modifiers) -> bool {
        if !self.exists() {
            return false;
        }
        let keys = if cfg!(target_os = "macos") {
            let cmd_down = m.mac_cmd || (m.command && !m.ctrl);
            cmd_down == self.cmd && m.ctrl == self.ctrl
        } else if m.command != m.ctrl {
            let cmd_down = m.command;
            cmd_down == self.cmd && m.ctrl == self.ctrl
        } else {
            (m.command || m.ctrl) == (self.cmd || self.ctrl)
        };
        keys && m.shift == self.shift && m.alt == self.alt
    }

    /// Whether this key can be pressed here: Control-Command combinations exist on macOS only.
    pub(crate) fn exists(self) -> bool {
        cfg!(target_os = "macos") || !(self.ctrl && self.cmd)
    }

    /// As the menus print it.
    pub(crate) fn shortcut(self) -> egui::KeyboardShortcut {
        let mut m = Modifiers::NONE;
        if self.shift {
            m |= Modifiers::SHIFT;
        }
        if self.alt {
            m |= Modifiers::ALT;
        }
        if self.ctrl {
            m |= Modifiers::CTRL;
        }
        if self.cmd {
            m |= Modifiers::COMMAND;
        }
        egui::KeyboardShortcut::new(m, self.key)
    }
}

/// Which keys a command belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scope {
    /// Works wherever the pointer is.
    Transport,
    /// The Tracks area's regions and selection: not while the pointer is over the Piano Roll.
    Regions,
}

/// Every key command: the menu entry it runs, its key, and where it works.
const COMMANDS: &[(&str, Combo, Scope)] = &[
    ("Go to Beginning", plain(Key::Enter), Scope::Transport),
    ("Stop", plain(Key::Num0), Scope::Transport),
    ("Rewind", plain(Key::Comma), Scope::Transport),
    ("Forward", plain(Key::Period), Scope::Transport),
    ("Fast Rewind", shift(Key::Comma), Scope::Transport),
    ("Fast Forward", shift(Key::Period), Scope::Transport),
    ("Record", plain(Key::R), Scope::Transport),
    ("Cycle Mode", plain(Key::C), Scope::Transport),
    ("Go to Position…", plain(Key::Slash), Scope::Transport),
    ("Play from Selection", shift(Key::Space), Scope::Transport),
    (
        "Go to Left Locator",
        ctrl_shift(Key::Comma),
        Scope::Transport,
    ),
    (
        "Go to Right Locator",
        ctrl_shift(Key::Period),
        Scope::Transport,
    ),
    ("Set Locators by Regions", cmd(Key::U), Scope::Regions),
    (
        "Move Locators Forward by Cycle Length",
        cmd_shift(Key::Period),
        Scope::Transport,
    ),
    (
        "Move Locators Backward by Cycle Length",
        cmd_shift(Key::Comma),
        Scope::Transport,
    ),
    ("Create Marker", alt(Key::Quote), Scope::Transport),
    ("Go to Next Marker", alt(Key::Period), Scope::Transport),
    ("Go to Previous Marker", alt(Key::Comma), Scope::Transport),
    ("Go to Marker Number…", alt(Key::Slash), Scope::Transport),
    (
        "Create Marker Without Rounding",
        ctrl_alt(Key::Quote),
        Scope::Transport,
    ),
    (
        "Create Markers by Regions",
        alt_shift(Key::Quote),
        Scope::Regions,
    ),
    (
        "Set Locators by Marker and Enable Cycle",
        ctrl_alt(Key::C),
        Scope::Transport,
    ),
    ("Select All", cmd(Key::A), Scope::Regions),
    ("Deselect All", shift(Key::D), Scope::Regions),
    ("Invert Selection", shift(Key::I), Scope::Regions),
    ("Select All Following", shift(Key::F), Scope::Regions),
    (
        "Select All Following of Same Track",
        ctrl_shift(Key::F),
        Scope::Regions,
    ),
    ("Copy", cmd(Key::C), Scope::Regions),
    ("Cut", cmd(Key::X), Scope::Regions),
    ("Paste", cmd(Key::V), Scope::Regions),
    ("Split at Playhead", cmd(Key::T), Scope::Regions),
    ("Join", cmd(Key::J), Scope::Regions),
    ("Mute On/Off", ctrl(Key::M), Scope::Regions),
    ("Loop On/Off", plain(Key::L), Scope::Regions),
    ("Move to Playhead", plain(Key::Semicolon), Scope::Regions),
    ("Nudge Left", alt(Key::ArrowLeft), Scope::Regions),
    ("Nudge Right", alt(Key::ArrowRight), Scope::Regions),
    (
        "Shuffle Left within Selection",
        alt(Key::OpenBracket),
        Scope::Regions,
    ),
    (
        "Shuffle Right within Selection",
        alt(Key::CloseBracket),
        Scope::Regions,
    ),
    (
        "Set Region Start to Playhead",
        cmd(Key::OpenBracket),
        Scope::Regions,
    ),
    (
        "Set Region End to Playhead",
        cmd(Key::CloseBracket),
        Scope::Regions,
    ),
    ("Repeat…", cmd(Key::R), Scope::Regions),
    ("Rename Regions", shift(Key::N), Scope::Regions),
    ("Quantize Selected Events", plain(Key::Q), Scope::Regions),
    ("Set Nudge Value to Tick", ctrl_alt(Key::T), Scope::Regions),
    (
        "Set Nudge Value to Division",
        ctrl_alt(Key::D),
        Scope::Regions,
    ),
    ("Set Nudge Value to Beat", ctrl_alt(Key::B), Scope::Regions),
    ("Set Nudge Value to Bar", ctrl_alt(Key::M), Scope::Regions),
    ("Select Next Region", plain(Key::ArrowRight), Scope::Regions),
    (
        "Select Previous Region",
        plain(Key::ArrowLeft),
        Scope::Regions,
    ),
    (
        "Add Next Region to Selection",
        shift(Key::ArrowRight),
        Scope::Regions,
    ),
    (
        "Add Previous Region to Selection",
        shift(Key::ArrowLeft),
        Scope::Regions,
    ),
    ("Select Next Track", plain(Key::ArrowDown), Scope::Regions),
    ("Select Previous Track", plain(Key::ArrowUp), Scope::Regions),
    (
        "Add Next Track to Selection",
        shift(Key::ArrowDown),
        Scope::Regions,
    ),
    (
        "Add Previous Track to Selection",
        shift(Key::ArrowUp),
        Scope::Regions,
    ),
    ("Select Inside Locators", shift(Key::L), Scope::Regions),
    ("Select Muted Regions", shift(Key::M), Scope::Regions),
    ("Select Overlapped Regions", shift(Key::O), Scope::Regions),
    ("Select Similar Regions", shift(Key::S), Scope::Regions),
    ("Select Equal Regions", shift(Key::E), Scope::Regions),
    ("Select Same Colored Regions", shift(Key::C), Scope::Regions),
    ("Zoom Horizontal In", cmd(Key::ArrowRight), Scope::Regions),
    ("Zoom Horizontal Out", cmd(Key::ArrowLeft), Scope::Regions),
    ("Zoom Vertical In", cmd(Key::ArrowDown), Scope::Regions),
    ("Zoom Vertical Out", cmd(Key::ArrowUp), Scope::Regions),
    (
        "Toggle Zoom to Fit Selection or All Contents",
        plain(Key::Z),
        Scope::Regions,
    ),
    ("Snap to Grid", cmd(Key::G), Scope::Regions),
    (
        "Show Git Version Control",
        cmd_shift(Key::G),
        Scope::Transport,
    ),
    ("Quick Checkpoint", ctrl_alt(Key::S), Scope::Transport),
    // Edit > Cut/Insert Time. Control-Command-X comes from the system as Cut (see `handle`).
    (
        "Cut Section Between Locators",
        ctrl_cmd(Key::X),
        Scope::Regions,
    ),
    (
        "Insert Silence Between Locators",
        ctrl_cmd(Key::I),
        Scope::Regions,
    ),
    (
        "Repeat Section Between Locators",
        ctrl_cmd(Key::R),
        Scope::Regions,
    ),
    // The Mac's delete key comes as Backspace; the forward delete key as Delete.
    ("Delete Track", cmd(Key::Backspace), Scope::Regions),
    ("Delete Track", cmd(Key::Delete), Scope::Regions),
    ("New Tracks…", alt_cmd(Key::N), Scope::Transport),
    ("Show Global Tracks", plain(Key::G), Scope::Regions),
    ("Show Hidden Tracks", plain(Key::H), Scope::Regions),
    ("Hide Track", ctrl(Key::H), Scope::Regions),
    // Logic's Toggle Track Mute and Toggle Track Solo (Logic 10.1 U.S. preset: M and S, listed
    // there as Toggle Channel Strip Mute and Solo). Logic has no default key to record-enable
    // a track, and none is given here.
    ("Toggle Track Mute", plain(Key::M), Scope::Regions),
    ("Toggle Track Solo", plain(Key::S), Scope::Regions),
    (
        "New Track with Duplicate Settings",
        cmd(Key::D),
        Scope::Regions,
    ),
    ("Show Colors", alt(Key::C), Scope::Regions),
    (
        "Color Regions by Track Color",
        alt_shift(Key::C),
        Scope::Regions,
    ),
    ("Create Track Stack…", cmd_shift(Key::D), Scope::Regions),
    // Logic's Name Track.
    ("Rename Track", shift(Key::Enter), Scope::Regions),
    ("Show Editor", plain(Key::E), Scope::Regions),
    ("Regions in Place…", ctrl(Key::B), Scope::Regions),
    ("Tracks in Place…", ctrl_cmd(Key::B), Scope::Regions),
    (
        "Convert Regions to New Audio Files",
        alt_cmd(Key::F),
        Scope::Regions,
    ),
];

/// Commands that run again while their key is held down.
const REPEATING: &[&str] = &[
    "Rewind",
    "Forward",
    "Fast Rewind",
    "Fast Forward",
    "Nudge Left",
    "Nudge Right",
    "Select Next Region",
    "Select Previous Region",
    "Add Next Region to Selection",
    "Add Previous Region to Selection",
    "Select Next Track",
    "Select Previous Track",
    "Add Next Track to Selection",
    "Add Previous Track to Selection",
    "Zoom Horizontal In",
    "Zoom Horizontal Out",
    "Zoom Vertical In",
    "Zoom Vertical Out",
];

/// The key of a menu entry, if it has one here.
pub(crate) fn combo_of(label: &str) -> Option<Combo> {
    COMMANDS
        .iter()
        .position(|(l, _, _)| *l == label)
        .filter(|&i| !shadowed(i, cfg!(target_os = "macos")))
        .map(|i| COMMANDS[i].1)
        .filter(|c| c.exists())
}

/// Whether command `i` has no key of its own: off macOS (`mac` false) Control and Command are
/// one key, so a command whose key reads the same as an earlier command's there loses it (the
/// earlier one is what the key runs, see `handle`). On macOS no command loses its key.
fn shadowed(i: usize, mac: bool) -> bool {
    if mac {
        return false;
    }
    let (label, c, _) = COMMANDS[i];
    // Control and Command together do not exist there at all (`Combo::exists`).
    let there = |o: Combo| !(o.ctrl && o.cmd);
    let off_mac = |o: Combo| (o.key, o.shift, o.alt, o.cmd || o.ctrl);
    there(c)
        && COMMANDS[..i]
            .iter()
            .any(|&(l, o, _)| l != label && there(o) && off_mac(o) == off_mac(c))
}

fn typing_id() -> egui::Id {
    egui::Id::unique("key_commands_typing")
}

/// Called before the panels: notes whether a text field has the keyboard now. The Return that
/// ends a rename takes the keyboard away from the field in the same frame, and must not be
/// Go to Beginning as well.
pub fn begin_frame(ctx: &egui::Context) {
    let typing = ctx.egui_wants_keyboard_input();
    ctx.data_mut(|d| d.insert_temp(typing_id(), typing));
}

/// Whether a bounce is in progress.
pub(crate) fn bounce_dialog_open(s: &UiState) -> bool {
    s.bouncing
}

/// Takes this frame's key commands out of the input and runs them. Called after the panels,
/// so a key an editor used first (a range selected on an automation lane, say) is left to it.
pub fn handle(ctx: &egui::Context, p: &mut Project, s: &mut UiState) {
    if ctx.egui_wants_keyboard_input()
        || ctx.data(|d| d.get_temp::<bool>(typing_id()).unwrap_or(false))
        || s.typing.open
        || s.chooser.is_some()
        || s.track_dialog.is_some()
        || s.settings_tab.is_some()
        || bounce_dialog_open(s)
    {
        return;
    }
    // While the Solo tool holds a region, no key edits regions or tracks (the hold names a
    // region by its number).
    let regions_ok = !s.piano_focus && !s.tracks.solo_holding();
    let clip_ok = regions_ok;
    let mut run: Vec<&'static str> = Vec::new();
    ctx.input_mut(|i| {
        let section_cut = regions_ok
            && combo_of("Cut Section Between Locators").is_some_and(|c| c.held(i.modifiers));
        i.events.retain(|e| {
            let label = match e {
                Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers,
                    ..
                } => COMMANDS
                    .iter()
                    .find(|(_, c, _)| c.key == *key && c.held(*modifiers))
                    .filter(|(_, _, scope)| *scope == Scope::Transport || regions_ok)
                    .filter(|(l, _, _)| !matches!(*l, "Copy" | "Cut" | "Paste") || clip_ok)
                    // Return over the Piano Roll begins a lyric there.
                    .filter(|(l, _, _)| *l != "Go to Beginning" || regions_ok)
                    .map(|(l, _, _)| (*l, *repeat)),
                // Command-C, -X and -V come from the system as these, and so does
                // Control-Command-X (Cut Section Between Locators).
                Event::Cut if section_cut => Some(("Cut Section Between Locators", false)),
                Event::Copy if clip_ok => Some(("Copy", false)),
                Event::Cut if clip_ok => Some(("Cut", false)),
                Event::Paste(_) if clip_ok => Some(("Paste", false)),
                _ => None,
            };
            match label {
                Some((l, repeat)) => {
                    if !repeat || REPEATING.contains(&l) {
                        run.push(l);
                    }
                    false
                }
                None => true,
            }
        });
    });
    for label in run {
        if !crate::menus::enabled(label, p, s) {
            continue;
        }
        if matches!(label, "Copy" | "Cut") {
            // The system sends Command-V on only when its clipboard holds text.
            let names: Vec<String> = s
                .tracks
                .selection()
                .into_iter()
                .filter_map(|(t, r)| Some(p.tracks.get(t)?.regions.get(r)?.name.clone()))
                .collect();
            ctx.copy_text(names.join("\n"));
        }
        crate::menus::apply(label, p, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Off macOS, where Control and Command are one key, only the Move Locators by Cycle Length
    /// commands read the same as earlier ones (Go to Left and Right Locator), so only they have
    /// no key there. On macOS every command keeps its key.
    #[test]
    fn off_macos_only_the_move_locators_keys_give_way() {
        let lost: Vec<&str> = (0..COMMANDS.len())
            .filter(|&i| shadowed(i, false))
            .map(|i| COMMANDS[i].0)
            .collect();
        assert_eq!(
            lost,
            vec![
                "Move Locators Forward by Cycle Length",
                "Move Locators Backward by Cycle Length"
            ]
        );
        assert!((0..COMMANDS.len()).all(|i| !shadowed(i, true)));
    }

    /// No two commands share a key, and no command's key is a key the menus, the View keys,
    /// the Tool menu (T) or Automation Mode (A, B) already use: a key does one thing.
    #[test]
    fn every_key_does_one_thing() {
        let mut seen: Vec<(&str, Combo)> = Vec::new();
        for &(label, combo, _) in COMMANDS {
            if !combo.exists() {
                continue;
            }
            if let Some((other, _)) = seen.iter().find(|(l, c)| *c == combo && *l != label) {
                panic!("{label} and {other} share {combo:?}");
            }
            seen.push((label, combo));
        }
        // Keys taken elsewhere: menu shortcuts that are not in the table, and the plain
        // letters the Tracks area reads itself.
        let mut taken: Vec<(String, egui::KeyboardShortcut)> = crate::menus::all_entries()
            .into_iter()
            .filter(|l| combo_of(l).is_none())
            .filter_map(|l| Some((l.to_string(), crate::menus::shortcut(l)?)))
            .collect();
        for (what, key) in [
            ("Tool menu", Key::T),
            ("Automation Mode", Key::A),
            ("Draw Mode", Key::B),
        ] {
            taken.push((
                what.into(),
                egui::KeyboardShortcut::new(Modifiers::NONE, key),
            ));
        }
        for (label, combo) in seen {
            let k = combo.shortcut();
            if let Some((what, _)) = taken.iter().find(|(_, t)| *t == k) {
                panic!("{label} has the key of {what}");
            }
        }
    }
    use crate::model::{Note, Region, RegionParams, TrackKind};
    use crate::ui_test::Harness;

    fn region(name: &str, start_bar: f32, len_bars: f32) -> Region {
        Region {
            missing: None,
            name: name.into(),
            start_bar,
            len_bars,
            notes: vec![Note {
                lyric: crate::lyrics::Lyric::NONE,
                mute: false,
                start: 0.0,
                len: 1.0,
                pitch: 60,
                vel: 90,
            }],
            controls: Vec::new(),
            folder: None,
            audio: None,
            params: RegionParams::default(),
            generated: None,
        }
    }

    /// Two instrument tracks in 4/4: track 0 has "A" (bars 2-3) and "B" (bars 5-6), track 1
    /// has "C" (bars 3-4) and "D" (bars 9-10).
    fn rig() -> (Project, UiState, Harness) {
        let mut p = Project::empty();
        p.add_track(TrackKind::Instrument);
        p.tracks[0].regions = vec![region("A", 2.0, 2.0), region("B", 5.0, 2.0)];
        p.tracks[1].regions = vec![region("C", 3.0, 2.0), region("D", 9.0, 2.0)];
        p.selected_track = 0;
        (p, UiState::new(), Harness::new(1400.0, 600.0))
    }

    /// One frame as the app runs it: the view keys of the menus, then the key commands.
    fn frame(h: &mut Harness, p: &mut Project, s: &mut UiState, events: Vec<Event>) {
        h.frame(events, |ui| {
            let ctx = ui.ctx().clone();
            begin_frame(&ctx);
            crate::menus::handle_keys(&ctx, p, s);
            handle(&ctx, p, s);
        });
    }

    fn press(h: &mut Harness, p: &mut Project, s: &mut UiState, key: Key, mods: Modifiers) {
        h.modifiers = mods;
        let ev = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: mods,
        };
        frame(h, p, s, vec![ev(true), ev(false)]);
        h.modifiers = Modifiers::NONE;
    }

    const NONE: Modifiers = Modifiers::NONE;
    const SHIFT: Modifiers = Modifiers::SHIFT;
    const CMD: Modifiers = Modifiers::COMMAND;
    const CTRL: Modifiers = Modifiers::CTRL;

    fn ctrl_shift_mods() -> Modifiers {
        Modifiers::CTRL | Modifiers::SHIFT
    }

    #[test]
    fn the_transport_keys_do_what_they_do_in_logic() {
        let (mut p, mut s, mut h) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        // Period and Comma: a bar on, a bar back. With Shift: eight bars.
        press(h, p, s, Key::Period, NONE);
        assert_eq!(p.position_beats, 4.0);
        press(h, p, s, Key::Period, SHIFT);
        assert_eq!(p.position_beats, 36.0, "eight bars on");
        press(h, p, s, Key::Comma, NONE);
        assert_eq!(p.position_beats, 32.0);
        press(h, p, s, Key::Comma, SHIFT);
        assert_eq!(p.position_beats, 0.0);
        // Return: to the beginning.
        p.position_beats = 13.0;
        press(h, p, s, Key::Enter, NONE);
        assert_eq!(p.position_beats, 0.0);
        // R: record. C: cycle.
        press(h, p, s, Key::R, NONE);
        assert!(p.recording);
        press(h, p, s, Key::R, NONE);
        assert!(!p.recording);
        let cycle = p.cycle;
        press(h, p, s, Key::C, NONE);
        assert_eq!(p.cycle, !cycle);
        // The locators.
        (p.cycle_start, p.cycle_end) = (8.0, 24.0);
        press(h, p, s, Key::Comma, ctrl_shift_mods());
        assert_eq!(p.position_beats, 8.0);
        press(h, p, s, Key::Period, ctrl_shift_mods());
        assert_eq!(p.position_beats, 24.0);
        // Shift-Space: play from the earliest selected region (bar 3 is beat 8, bar 5 is 16).
        s.tracks.select_many(&[(0, 1), (1, 0)]);
        press(h, p, s, Key::Space, SHIFT);
        assert!(p.playing);
        assert_eq!(p.position_beats, 8.0);
        // Slash: the Go to Position box, with the playhead's position in it.
        p.playing = false;
        press(h, p, s, Key::Slash, NONE);
        assert_eq!(
            s.track_dialog,
            Some(crate::track_dialogs::TrackDialog::GoTo { text: "3.1".into() })
        );
        // No key runs while a dialog is open.
        press(h, p, s, Key::R, NONE);
        assert!(!p.recording);
        s.track_dialog = None;
        // Play from Left Locator has no key in Logic's preset; the menu entry runs it.
        crate::menus::apply("Play from Left Locator", p, s);
        assert!(p.playing);
        assert_eq!(p.position_beats, 8.0);
    }

    #[test]
    fn stop_stops_and_stop_again_goes_to_the_start_or_to_the_left_locator() {
        let (mut p, mut s, mut h) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        p.cycle = false;
        p.playing = true;
        p.position_beats = 21.0;
        press(h, p, s, Key::Num0, NONE);
        assert!(!p.playing);
        assert_eq!(p.position_beats, 21.0, "the first Stop leaves the playhead");
        press(h, p, s, Key::Num0, NONE);
        assert_eq!(p.position_beats, 0.0, "the second goes to the start");
        // With Cycle on, the second Stop goes to the left locator.
        (p.cycle, p.cycle_start, p.cycle_end) = (true, 8.0, 16.0);
        p.playing = true;
        p.position_beats = 12.0;
        crate::transport::stop(p);
        crate::transport::stop(p);
        assert!(!p.playing);
        assert_eq!(p.position_beats, 8.0);
    }

    #[test]
    fn the_selection_keys_select_as_in_logic() {
        let (mut p, mut s, mut h) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        press(h, p, s, Key::A, CMD);
        assert_eq!(s.tracks.selection().len(), 4);
        press(h, p, s, Key::D, SHIFT);
        assert!(s.tracks.selection().is_empty());
        // Shift-I inverts, and is not I (Show Inspector); Shift-D was not D (List Editors).
        let (inspector, lists) = (s.show_inspector, s.show_lists);
        s.tracks.select_only(0, 0);
        press(h, p, s, Key::I, SHIFT);
        assert_eq!(s.tracks.selection(), vec![(0, 1), (1, 0), (1, 1)]);
        assert_eq!((s.show_inspector, s.show_lists), (inspector, lists));
        press(h, p, s, Key::I, NONE);
        assert_eq!(
            s.show_inspector, !inspector,
            "I alone still shows the Inspector"
        );
        // Shift-F: everything from the selected region on ("C" at bar 3, then "B" and "D").
        s.tracks.select_only(1, 0);
        press(h, p, s, Key::F, SHIFT);
        assert_eq!(s.tracks.selection(), vec![(0, 1), (1, 0), (1, 1)]);
        assert_eq!(s.tracks.selected, Some((1, 0)), "the primary one stays");
        // Control-Shift-F: only on the selected region's track.
        s.tracks.select_only(1, 0);
        press(h, p, s, Key::F, ctrl_shift_mods());
        assert_eq!(s.tracks.selection(), vec![(1, 0), (1, 1)]);
    }

    #[test]
    fn the_region_keys_edit_the_selected_regions() {
        let (mut p, mut s, mut h) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        // Command-U: the locators around the selection, Cycle on.
        p.cycle = false;
        s.tracks.select_many(&[(0, 0), (1, 0)]);
        press(h, p, s, Key::U, CMD);
        assert_eq!((p.cycle, p.cycle_start, p.cycle_end), (true, 4.0, 16.0));
        // Control-M: both take the state the primary one changes to.
        press(h, p, s, Key::M, CTRL);
        assert!(p.tracks[0].regions[0].params.mute && p.tracks[1].regions[0].params.mute);
        assert!(!p.tracks[0].regions[1].params.mute);
        press(h, p, s, Key::M, CTRL);
        assert!(!p.tracks[0].regions[0].params.mute && !p.tracks[1].regions[0].params.mute);
        // L: "A" loops up to "B" (a bar away); "C" up to "D" (four bars). L again: no loop.
        press(h, p, s, Key::L, NONE);
        assert_eq!(p.tracks[0].regions[0].params.loop_beats, 4.0);
        assert_eq!(p.tracks[1].regions[0].params.loop_beats, 16.0);
        press(h, p, s, Key::L, NONE);
        assert_eq!(p.tracks[0].regions[0].params.loop_beats, 0.0);
        assert_eq!(p.tracks[1].regions[0].params.loop_beats, 0.0);
        // Option-Right and Option-Left: a sixteenth note later, and back.
        press(h, p, s, Key::ArrowRight, Modifiers::ALT);
        assert_eq!(p.region_start_beats(&p.tracks[0].regions[0]), 4.25);
        assert_eq!(p.region_start_beats(&p.tracks[1].regions[0]), 8.25);
        press(h, p, s, Key::ArrowLeft, Modifiers::ALT);
        assert_eq!(p.region_start_beats(&p.tracks[0].regions[0]), 4.0);
        // Semicolon: the earliest goes to the playhead, the other keeps its distance.
        p.position_beats = 40.0;
        press(h, p, s, Key::Semicolon, NONE);
        assert_eq!(p.tracks[0].regions[0].start_bar, 11.0);
        assert_eq!(p.tracks[1].regions[0].start_bar, 12.0);
        // Command-T: the selected regions under the playhead are cut (bar 12 is beat 44).
        p.position_beats = 44.0;
        s.tracks.select_only(0, 0);
        let before = p.tracks[0].regions.len();
        press(h, p, s, Key::T, CMD);
        assert_eq!(p.tracks[0].regions.len(), before + 1);
        // Command-J: and joined again.
        s.tracks.select_many(&[(0, 0), (0, 1)]);
        press(h, p, s, Key::J, CMD);
        assert_eq!(p.tracks[0].regions.len(), before);
    }

    #[test]
    fn copy_cut_and_paste_work_on_regions() {
        let (mut p, mut s, mut h) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        s.tracks.select_only(0, 0);
        frame(h, p, s, vec![Event::Copy]);
        p.position_beats = 48.0; // bar 13
        frame(h, p, s, vec![Event::Paste("A".into())]);
        let names: Vec<(&str, f32)> = p.tracks[0]
            .regions
            .iter()
            .map(|r| (r.name.as_str(), r.start_bar))
            .collect();
        assert_eq!(names, vec![("A", 2.0), ("B", 5.0), ("A", 13.0)]);
        frame(h, p, s, vec![Event::Cut]);
        assert_eq!(
            p.tracks[0].regions.len(),
            2,
            "the pasted copy was selected and is cut"
        );
    }

    #[test]
    fn keys_wait_while_a_text_field_or_the_piano_roll_has_them() {
        let (mut p, mut s, mut h) = rig();
        let mut text = String::new();
        let r = Event::Key {
            key: Key::R,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: NONE,
        };
        let mut typing = |h: &mut Harness, p: &mut Project, s: &mut UiState, ev: Vec<Event>| {
            h.frame(ev, |ui| {
                let ctx = ui.ctx().clone();
                begin_frame(&ctx);
                ui.text_edit_singleline(&mut text).request_focus();
                handle(&ctx, p, s);
            });
        };
        typing(&mut h, &mut p, &mut s, vec![]);
        typing(&mut h, &mut p, &mut s, vec![r.clone()]);
        assert!(!p.recording, "R typed into a text field does not record");
        // Over the Piano Roll the region keys are its own; the transport keys still work.
        let (mut p, mut s, mut h) = rig();
        s.piano_focus = true;
        press(&mut h, &mut p, &mut s, Key::A, CMD);
        assert!(s.tracks.selection().is_empty());
        press(&mut h, &mut p, &mut s, Key::R, NONE);
        assert!(p.recording);
        // Musical Typing has every letter.
        s.piano_focus = false;
        s.typing.open = true;
        press(&mut h, &mut p, &mut s, Key::R, NONE);
        assert!(p.recording, "unchanged");
    }

    #[test]
    fn a_key_needs_exactly_its_modifiers() {
        let i = plain(Key::I);
        assert!(i.held(Modifiers::NONE));
        assert!(!i.held(Modifiers::SHIFT), "Shift-I is another command");
        assert!(shift(Key::I).held(Modifiers::SHIFT));
        assert!(!shift(Key::Comma).held(Modifiers::SHIFT | Modifiers::CTRL));
        assert!(ctrl_shift(Key::Comma).held(Modifiers::SHIFT | Modifiers::CTRL));
        assert!(cmd(Key::T).held(Modifiers::COMMAND));
        assert!(!cmd(Key::T).held(Modifiers::NONE));
        assert!(alt(Key::ArrowLeft).held(Modifiers::ALT));
    }

    #[test]
    fn no_two_commands_share_a_key_and_each_runs_a_menu_entry() {
        for (n, (label, combo, _)) in COMMANDS.iter().enumerate() {
            assert!(
                crate::menus::all_entries().contains(label),
                "{label} is in a menu"
            );
            assert!(crate::menus::implemented(label), "{label} is built");
            for (other, c2, _) in &COMMANDS[n + 1..] {
                assert!(combo != c2, "{label} and {other} have the same key");
            }
        }
    }
}
