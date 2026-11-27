//! Logic Pro-grade Quick Help system: a comprehensive registry of controls across the
//! Control Bar, Transport, Arranger, Inspector, Channel Strip, and Effects, plus a rich
//! floating Quick Help card when Quick Help (`?` / `Shift+/`) is active and a searchable
//! Quick Help Reference window (`Help -> Quick Help`).
use crate::i18n::tr;
use crate::UiState;
use egui::{
    pos2, vec2, Align2, Color32, Context, CornerRadius, FontId, Frame, Margin, Order, Pos2,
    Response, RichText, Sense, Stroke, StrokeKind,
};

const CARD_BG: Color32 = Color32::from_rgb(0x25, 0x25, 0x27);
const CARD_BORDER: Color32 = Color32::from_rgb(0x38, 0x38, 0x3c);
const BADGE_BG: Color32 = Color32::from_rgb(0x34, 0x35, 0x3a);
const TEXT_MAIN: Color32 = Color32::from_rgb(0xf2, 0xf2, 0xf5);
const TEXT_SUB: Color32 = Color32::from_rgb(0xc6, 0xc7, 0xcd);
const TEXT_TIP: Color32 = Color32::from_rgb(0x8e, 0xc2, 0xff);

#[derive(Clone, Copy)]
pub struct HelpEntry {
    pub id: &'static str,
    pub category: &'static str,
    pub title: &'static str,
    pub shortcut: Option<&'static str>,
    pub summary: &'static str,
    pub tip: &'static str,
}

pub const REGISTRY: &[HelpEntry] = &[
    // Control Bar: Left & Right Clusters
    HelpEntry {
        id: "view0",
        category: "Control Bar",
        title: "Library",
        shortcut: Some("Y"),
        summary: "Shows or hides the Sound Library and Patch Browser on the left.",
        tip: "Click any patch to load its instrument, EQ, and effects onto the selected track.",
    },
    HelpEntry {
        id: "view1",
        category: "Control Bar",
        title: "Inspector",
        shortcut: Some("I"),
        summary: "Shows or hides the Region & Track Inspector and dual Channel Strips.",
        tip: "Parameters automatically adapt between MIDI Defaults, MIDI regions, and Audio regions.",
    },
    HelpEntry {
        id: "view2",
        category: "Control Bar",
        title: "Quick Help",
        shortcut: Some("Shift+/"),
        summary: "Toggles interactive Quick Help mode for instant context-sensitive guidance.",
        tip: "Hover over any button, knob, or parameter while active. Open Help > Quick Help Window for the full reference.",
    },
    HelpEntry {
        id: "view3",
        category: "Control Bar",
        title: "Toolbar",
        shortcut: None,
        summary: "Shows or hides the secondary Workspace Toolbar beneath the Control Bar.",
        tip: "Provides one-click access to Split, Join, Mute, Loop, Color, Bounce, Locators, and Zoom.",
    },
    HelpEntry {
        id: "panes0",
        category: "Control Bar",
        title: "Macro Controls",
        shortcut: Some("B"),
        summary: "Opens or closes the Macro Controls Moog-style faceplate and inline EQ for the selected track or Master.",
        tip: "Hold Shift while dragging any Moog knob for fine adjustment; double-click to reset.",
    },
    HelpEntry {
        id: "panes1",
        category: "Control Bar",
        title: "Mixer",
        shortcut: Some("X"),
        summary: "Opens or closes the full Mixer pane showing all tracks, Aux buses, and Stereo Out.",
        tip: "Click again or press X to close the bottom pane.",
    },
    HelpEntry {
        id: "panes2",
        category: "Control Bar",
        title: "Editors",
        shortcut: Some("E"),
        summary: "Opens the context-sensitive editor for the selected region or track.",
        tip: "Opens Piano Roll for MIDI, Step Sequencer for Patterns, or Pitch Editor for Audio regions.",
    },
    HelpEntry {
        id: "right0",
        category: "Control Bar",
        title: "List Editors",
        shortcut: Some("D"),
        summary: "Shows or hides the Event List, Marker, Tempo, and Signature editors on the right.",
        tip: "Inspect and edit exact note positions, velocities, and controller events numerically.",
    },
    HelpEntry {
        id: "right1",
        category: "Control Bar",
        title: "Note Pad",
        shortcut: None,
        summary: "Opens the Project Notes pad saved inside your project.",
        tip: "Useful for storing lyrics, chord progressions, or mix revision notes.",
    },
    HelpEntry {
        id: "right2",
        category: "Control Bar",
        title: "Loop Browser",
        shortcut: Some("O"),
        summary: "Opens the Apple Loops and audio loop browser on the right.",
        tip: "Drag any loop directly onto the Tracks area to import it.",
    },
    HelpEntry {
        id: "right3",
        category: "Control Bar",
        title: "Media Browser",
        shortcut: Some("F"),
        summary: "Opens the All Files media browser to audition and import audio or MIDI files.",
        tip: "Double-click or drag audio files onto an empty track lane.",
    },
    // Transport & LCD
    HelpEntry {
        id: "rew",
        category: "Transport & LCD",
        title: "Rewind",
        shortcut: Some(","),
        summary: "Moves the playhead back by one bar.",
        tip: "Hold Shift+, to rewind rapidly.",
    },
    HelpEntry {
        id: "ffw",
        category: "Transport & LCD",
        title: "Forward",
        shortcut: Some("."),
        summary: "Moves the playhead forward by one bar.",
        tip: "Hold Shift+. to fast-forward rapidly.",
    },
    HelpEntry {
        id: "start",
        category: "Transport & LCD",
        title: "Go to Beginning",
        shortcut: Some("Enter"),
        summary: "Jumps the playhead back to bar 1 beat 1.",
        tip: "Double-tap Stop (0) to return to the start or left locator.",
    },
    HelpEntry {
        id: "play",
        category: "Transport & LCD",
        title: "Play / Stop",
        shortcut: Some("Space"),
        summary: "Starts or pauses project playback from the current playhead position.",
        tip: "Press Shift+Space to play directly from the selected region.",
    },
    HelpEntry {
        id: "rec",
        category: "Transport & LCD",
        title: "Record",
        shortcut: Some("R"),
        summary: "Starts recording MIDI or audio on all armed tracks.",
        tip: "Arm a track with its R button first, and enable Count-in for a 1-bar lead-in.",
    },
    HelpEntry {
        id: "capture",
        category: "Transport & LCD",
        title: "Capture Recording",
        shortcut: Some("Shift+R"),
        summary: "Recovers MIDI notes you just played on your keyboard even when not recording.",
        tip: "Play freely anytime, then click Capture Recording to turn your performance into a region.",
    },
    HelpEntry {
        id: "cycle",
        category: "Transport & LCD",
        title: "Cycle Mode",
        shortcut: Some("C"),
        summary: "Loops playback continuously between the Left and Right Locators.",
        tip: "Press Cmd+U to set locators around the currently selected regions.",
    },
    HelpEntry {
        id: "lowlat",
        category: "Transport & LCD",
        title: "Low Latency Mode",
        shortcut: None,
        summary: "Temporarily bypasses high-latency plugins while recording live instruments.",
        tip: "Keep active when tracking vocals or MIDI if you hear monitoring delay.",
    },
    HelpEntry {
        id: "tuner",
        category: "Transport & LCD",
        title: "Tuner",
        shortcut: None,
        summary: "Opens the chromatic instrument tuner for live audio input.",
        tip: "Select your audio input track first to see live pitch deviation in cents.",
    },
    HelpEntry {
        id: "countin",
        category: "Transport & LCD",
        title: "Count-in",
        shortcut: Some("Shift+K"),
        summary: "Plays one bar of metronome clicks before recording begins.",
        tip: "Configure count-in bars in Project Settings > Metronome.",
    },
    HelpEntry {
        id: "metronome",
        category: "Transport & LCD",
        title: "Metronome",
        shortcut: Some("K"),
        summary: "Toggles the rhythmic click track during playback and recording.",
        tip: "Right-click or open Project Settings > Metronome to adjust click volume.",
    },
    HelpEntry {
        id: "lcd_pos",
        category: "Transport & LCD",
        title: "Playhead Position",
        shortcut: Some("/"),
        summary: "Displays the current bar/beat or SMPTE timecode position.",
        tip: "Click to toggle display mode or press / to jump to an exact bar.",
    },
    HelpEntry {
        id: "lcd_tempo",
        category: "Transport & LCD",
        title: "Tempo",
        shortcut: None,
        summary: "Sets the project tempo in Beats Per Minute (BPM).",
        tip: "Drag vertically to change tempo smoothly or double-click to type an exact BPM.",
    },
    HelpEntry {
        id: "lcd_sig",
        category: "Transport & LCD",
        title: "Time Signature",
        shortcut: None,
        summary: "Sets the project meter (numerator / denominator).",
        tip: "Drag or click to adjust beats per bar.",
    },
    HelpEntry {
        id: "master_vol",
        category: "Transport & LCD",
        title: "Master Volume",
        shortcut: None,
        summary: "Adjusts the overall monitoring output level of the DAW.",
        tip: "Drag left/right to attenuate or boost master monitoring.",
    },
    // Tracks Area & Tools
    HelpEntry {
        id: "tool_pointer",
        category: "Tracks Area",
        title: "Pointer Tool",
        shortcut: Some("T then T"),
        summary: "Selects, moves, resizes, and loops regions in the Tracks area.",
        tip: "Drag the upper-right corner of a region to loop it; drag the lower-right corner to resize.",
    },
    HelpEntry {
        id: "tool_pencil",
        category: "Tracks Area",
        title: "Pencil Tool",
        shortcut: None,
        summary: "Creates empty MIDI regions or draws automation points directly.",
        tip: "Click in an empty instrument lane to create a 1-bar MIDI region.",
    },
    HelpEntry {
        id: "tool_eraser",
        category: "Tracks Area",
        title: "Eraser Tool",
        shortcut: None,
        summary: "Deletes clicked regions or selected notes immediately.",
        tip: "Hold Shift to select multiple regions before erasing.",
    },
    HelpEntry {
        id: "tool_scissors",
        category: "Tracks Area",
        title: "Scissors Tool",
        shortcut: None,
        summary: "Splits a region cleanly at the clicked bar/beat position.",
        tip: "Use Cmd+T to split all selected regions at the playhead.",
    },
    HelpEntry {
        id: "tool_join",
        category: "Tracks Area",
        title: "Join / Glue Tool",
        shortcut: Some("Cmd+J"),
        summary: "Merges multiple adjacent or selected regions into a single region.",
        tip: "Select two or more regions on the same track and press Cmd+J.",
    },
    HelpEntry {
        id: "tool_marquee",
        category: "Tracks Area",
        title: "Marquee Tool",
        shortcut: None,
        summary: "Selects an exact time range across one or more tracks.",
        tip: "Click inside a Marquee selection with the Pointer to split it out.",
    },
    HelpEntry {
        id: "snap_mode",
        category: "Tracks Area",
        title: "Snap Mode",
        shortcut: Some("Cmd+G"),
        summary: "Controls how region moves and edits quantize to the timeline grid.",
        tip: "Choose Smart, Bar, Beat, Division, or Ticks for precise alignment.",
    },
    HelpEntry {
        id: "drag_mode",
        category: "Tracks Area",
        title: "Drag Mode",
        shortcut: None,
        summary: "Determines how overlapping or adjacent regions behave when moved.",
        tip: "Use X-Fade for automatic crossfades between audio regions, or No Overlap to trim.",
    },
    HelpEntry {
        id: "M",
        category: "Tracks Area",
        title: "Mute",
        shortcut: Some("M"),
        summary: "Silences this track without removing its regions or plugins.",
        tip: "Option-click Mute to unmute all tracks at once.",
    },
    HelpEntry {
        id: "S",
        category: "Tracks Area",
        title: "Solo",
        shortcut: Some("S"),
        summary: "Isolates this track so only soloed tracks are heard.",
        tip: "Solo multiple tracks together to audition submixes.",
    },
    HelpEntry {
        id: "R",
        category: "Tracks Area",
        title: "Record Enable",
        shortcut: None,
        summary: "Arms the track to record incoming audio or MIDI.",
        tip: "Arm multiple tracks simultaneously for multi-track recording.",
    },
    HelpEntry {
        id: "I",
        category: "Tracks Area",
        title: "Input Monitoring",
        shortcut: None,
        summary: "Lets you hear the live audio input through this track's effects.",
        tip: "Use headphones when monitoring microphones to prevent feedback.",
    },
    HelpEntry {
        id: "track_auto",
        category: "Tracks Area",
        title: "Automation",
        shortcut: Some("A"),
        summary: "Toggles track and region automation lanes for volume, pan, sends, and FX.",
        tip: "Click on the automation curve to add nodes and shape parameter changes over time.",
    },
    // Region & Track Inspector
    HelpEntry {
        id: "insp_quantize",
        category: "Inspector",
        title: "Quantize",
        shortcut: Some("Q"),
        summary: "Non-destructively snaps MIDI note timing to the chosen rhythmic grid.",
        tip: "Scroll wheel or click to pick 1/4, 1/8, 1/16, Triplet, or Off.",
    },
    HelpEntry {
        id: "insp_q_swing",
        category: "Inspector",
        title: "Q-Swing",
        shortcut: None,
        summary: "Offsets off-beat quantized notes to create a swing or shuffle groove.",
        tip: "Drag vertically: 50% is straight timing; 55%-66% adds classic swing.",
    },
    HelpEntry {
        id: "insp_transpose",
        category: "Inspector",
        title: "Transpose",
        shortcut: None,
        summary: "Shifts pitch in semitones or octaves non-destructively.",
        tip: "Drag vertically or double-click to type exact semitones (+/- 36).",
    },
    HelpEntry {
        id: "insp_velocity",
        category: "Inspector",
        title: "Velocity Offset",
        shortcut: None,
        summary: "Adds or subtracts MIDI velocity across all notes in the region.",
        tip: "Drag vertically to brighten or soften a performance without altering dynamics.",
    },
    HelpEntry {
        id: "insp_dynamics",
        category: "Inspector",
        title: "Dynamics",
        shortcut: None,
        summary: "Expands or compresses the velocity range of MIDI notes in the region.",
        tip: "Values below 100% even out dynamics; values above 100% accentuate accents.",
    },
    HelpEntry {
        id: "insp_gate_time",
        category: "Inspector",
        title: "Gate Time",
        shortcut: None,
        summary: "Scales the duration of MIDI notes proportionally.",
        tip: "Below 100% creates staccato phrasing; Legato extends notes cleanly.",
    },
    HelpEntry {
        id: "insp_q_vel",
        category: "Inspector",
        title: "Q-Velocity",
        shortcut: None,
        summary: "Blends note velocities toward the quantize template intensity.",
        tip: "Drag vertically from 0% to 100% under Region > More.",
    },
    HelpEntry {
        id: "insp_q_len",
        category: "Inspector",
        title: "Q-Length",
        shortcut: None,
        summary: "Blends note durations toward the quantize grid step length.",
        tip: "Great for tightening rhythmic chord stabs or basslines.",
    },
    HelpEntry {
        id: "insp_q_flam",
        category: "Inspector",
        title: "Q-Flam",
        shortcut: None,
        summary: "Spreads simultaneous chord notes slightly in milliseconds for a strummed feel.",
        tip: "Positive values strum upward; negative values strum downward.",
    },
    HelpEntry {
        id: "insp_q_range",
        category: "Inspector",
        title: "Q-Range",
        shortcut: None,
        summary: "Restricts quantization only to notes inside or outside a tick threshold.",
        tip: "Use positive values to preserve subtle human micro-timing near the beat.",
    },
    HelpEntry {
        id: "insp_q_strength",
        category: "Inspector",
        title: "Q-Strength",
        shortcut: None,
        summary: "Controls how strongly notes are pulled toward the quantize grid (0%-100%).",
        tip: "60%-80% tightens timing while keeping a natural human groove.",
    },
    HelpEntry {
        id: "insp_key_limit",
        category: "Inspector",
        title: "Key Limit",
        shortcut: None,
        summary: "Filters incoming MIDI notes outside the chosen pitch range.",
        tip: "Useful for keyboard splits across layered instrument tracks.",
    },
    HelpEntry {
        id: "insp_vel_limit",
        category: "Inspector",
        title: "Velocity Limit",
        shortcut: None,
        summary: "Filters incoming MIDI notes outside the chosen velocity range.",
        tip: "Trigger different layers depending on how hard you play.",
    },
    HelpEntry {
        id: "insp_freeze",
        category: "Inspector",
        title: "Freeze Mode",
        shortcut: None,
        summary: "Selects Pre-Fader or Source-Only freezing to save CPU on heavy tracks.",
        tip: "Pre-Fader freezes instruments and Audio FX while keeping volume/pan live.",
    },
    HelpEntry {
        id: "insp_delay",
        category: "Inspector",
        title: "Track Delay",
        shortcut: None,
        summary: "Shifts track playback timing forward or backward in milliseconds.",
        tip: "Use negative delay (-10 ms) to compensate for slow-attack orchestral patches.",
    },
    HelpEntry {
        id: "insp_no_transpose",
        category: "Inspector",
        title: "No Transpose",
        shortcut: None,
        summary: "Excludes this track from region and global transposition.",
        tip: "Enable on drum and percussion tracks so key changes don't shift drum hits.",
    },
    HelpEntry {
        id: "insp_no_reset",
        category: "Inspector",
        title: "No Reset",
        shortcut: None,
        summary: "Prevents MIDI Reset messages from resetting controllers on playback stop.",
        tip: "Keep enabled for sustain-heavy pads or hardware synths.",
    },
    // Channel Strip & FX
    HelpEntry {
        id: "cs_setting",
        category: "Channel Strip & FX",
        title: "Setting",
        shortcut: None,
        summary: "Loads, saves, copies, or resets the complete Channel Strip patch.",
        tip: "Click Setting to copy/paste an entire plugin chain between tracks.",
    },
    HelpEntry {
        id: "cs_gr",
        category: "Channel Strip & FX",
        title: "Gain Reduction",
        shortcut: None,
        summary: "Displays real-time compression gain reduction in dB.",
        tip: "Shows active Compressor activity directly on the strip.",
    },
    HelpEntry {
        id: "eq_thumb",
        category: "Channel Strip & FX",
        title: "EQ Thumbnail",
        shortcut: None,
        summary: "Shows a live preview of the track's Channel EQ frequency curve.",
        tip: "Click to open the 8-band Channel EQ window (or auto-insert one).",
    },
    HelpEntry {
        id: "cs_inst",
        category: "Channel Strip & FX",
        title: "Instrument Slot",
        shortcut: None,
        summary: "Selects or opens the software instrument for this track.",
        tip: "Click the center to open the instrument UI, or the right arrows to swap instruments.",
    },
    HelpEntry {
        id: "cs_fx",
        category: "Channel Strip & FX",
        title: "Audio FX",
        shortcut: None,
        summary: "Inserts and manages real-time audio effects in series.",
        tip: "Click the left power button to bypass; click the slot center to open the plugin editor.",
    },
    HelpEntry {
        id: "cs_sends",
        category: "Channel Strip & FX",
        title: "Sends",
        shortcut: None,
        summary: "Routes a parallel copy of the track's signal to an Aux Bus.",
        tip: "Drag the inline rotary knob vertically to adjust send level.",
    },
    HelpEntry {
        id: "cs_output",
        category: "Channel Strip & FX",
        title: "Output",
        shortcut: None,
        summary: "Sets the destination bus or Stereo Out for this channel strip.",
        tip: "Route multiple tracks to a Bus to process them together on an Aux strip.",
    },
    HelpEntry {
        id: "cs_group",
        category: "Channel Strip & FX",
        title: "Group",
        shortcut: None,
        summary: "Assigns this strip to a Mix Group for linked volume, mute, or solo.",
        tip: "Create groups in the Mixer to ride drum or vocal faders together.",
    },
    HelpEntry {
        id: "cs_auto",
        category: "Channel Strip & FX",
        title: "Automation Mode",
        shortcut: None,
        summary: "Chooses Off, Read, Touch, Latch, or Write automation behavior.",
        tip: "Use Touch to write fader rides that return smoothly when you release.",
    },
    HelpEntry {
        id: "cs_pan",
        category: "Channel Strip & FX",
        title: "Pan / Balance",
        shortcut: None,
        summary: "Positions the track across the stereo field (-64 Left to +63 Right).",
        tip: "Drag vertically or double-click to reset to center.",
    },
    HelpEntry {
        id: "cs_fader",
        category: "Channel Strip & FX",
        title: "Volume Fader",
        shortcut: None,
        summary: "Adjusts the channel strip output level in dB (-inf to +6.0 dB).",
        tip: "Double-click the fader cap to reset to 0.0 dB; hold Shift for fine adjustment.",
    },
    HelpEntry {
        id: "cs_bnc",
        category: "Channel Strip & FX",
        title: "Bounce (Bnc)",
        shortcut: None,
        summary: "Renders the Stereo Out mix down to a WAV/audio file.",
        tip: "Set Cycle Locators first to bounce a specific song section.",
    },
    HelpEntry {
        id: "fx_eq",
        category: "Channel Strip & FX",
        title: "Channel EQ",
        shortcut: None,
        summary: "8-band parametric equalizer with HPF, Shelves, 4 Bells, LPF, and Analyzer.",
        tip: "Drag graph nodes directly, scroll to adjust Q, or toggle Stereo/Mid/Side routing.",
    },
    HelpEntry {
        id: "fx_comp",
        category: "Channel Strip & FX",
        title: "Compressor",
        shortcut: None,
        summary: "Dynamic range compressor with Threshold, Ratio, Attack, Release, and Make-up.",
        tip: "Watch the transfer curve and Gain Reduction meter to dial in punch.",
    },
    // Macro Controls knobs
    HelpEntry {
        id: "sc_tone",
        category: "Macro Controls",
        title: "Tone",
        shortcut: None,
        summary: "Adjusts the High Shelf brightness (-12 dB to +12 dB) on the track's Channel EQ.",
        tip: "Double-click to reset to 0.0 dB; hold Shift while dragging for fine control.",
    },
    HelpEntry {
        id: "sc_cutoff",
        category: "Macro Controls",
        title: "Filter Cutoff",
        shortcut: None,
        summary: "Sweeps the low-pass filter cutoff frequency (120 Hz to 20 kHz).",
        tip: "Pull below 19.5 kHz to darken or warm up bright instruments.",
    },
    HelpEntry {
        id: "sc_res",
        category: "Macro Controls",
        title: "Resonance",
        shortcut: None,
        summary: "Adjusts the Q resonance peak at the filter cutoff frequency.",
        tip: "Higher Q values accentuate harmonics near the cutoff frequency.",
    },
    HelpEntry {
        id: "sc_attack",
        category: "Macro Controls",
        title: "Attack",
        shortcut: None,
        summary: "Sets the Compressor attack time (0.1 ms to 100 ms).",
        tip: "Slower attack (20-40 ms) preserves drum transients; fast attack tames peaks.",
    },
    HelpEntry {
        id: "sc_release",
        category: "Macro Controls",
        title: "Release",
        shortcut: None,
        summary: "Sets the Compressor release time (10 ms to 1000 ms).",
        tip: "Match release to the track's tempo for natural breathing.",
    },
    HelpEntry {
        id: "sc_drive",
        category: "Macro Controls",
        title: "Drive",
        shortcut: None,
        summary: "Adds warm harmonic saturation via the track's Overdrive effect.",
        tip: "Turn above 0.0 dB to add analog grit; reset to 0.0 dB to bypass.",
    },
    HelpEntry {
        id: "sc_send1",
        category: "Macro Controls",
        title: "Send 1",
        shortcut: None,
        summary: "Controls the send level to Bus 1 (e.g., Reverb or Delay Aux).",
        tip: "Automatically routes to Bus 1 if no send is configured yet.",
    },
    HelpEntry {
        id: "sc_output",
        category: "Macro Controls",
        title: "Output",
        shortcut: None,
        summary: "Adjusts the selected track's Channel Strip output fader level in dB.",
        tip: "Double-click to reset cleanly to 0.0 dB.",
    },
    HelpEntry {
        id: "list_event",
        category: "List Editors",
        title: "Event List",
        shortcut: Some("D"),
        summary: "Displays and edits MIDI notes, control changes, pitch bend, and SysEx events numerically.",
        tip: "Toggle the filter pills below to focus on specific event types.",
    },
    HelpEntry {
        id: "list_marker",
        category: "List Editors",
        title: "Marker List",
        shortcut: None,
        summary: "Lists all project arrangement markers with bar/beat positions for quick navigation.",
        tip: "Click any marker row to jump the playhead directly to that section.",
    },
    HelpEntry {
        id: "list_tempo",
        category: "List Editors",
        title: "Tempo List",
        shortcut: None,
        summary: "Shows and edits project tempo changes and BPM events across the timeline.",
        tip: "Use this list to enter exact BPM changes at specific bar positions.",
    },
    HelpEntry {
        id: "list_signature",
        category: "List Editors",
        title: "Signature List",
        shortcut: None,
        summary: "Displays and edits time signature and key signature changes in the project.",
        tip: "Add meter changes here to support compound or odd-time sections.",
    },
    HelpEntry {
        id: "list_back",
        category: "List Editors",
        title: "Hierarchy Up",
        shortcut: None,
        summary: "Navigates up one level in the Event List hierarchy (region events vs. track/project view).",
        tip: "Useful when inspecting nested MIDI regions.",
    },
    HelpEntry {
        id: "list_follow",
        category: "List Editors",
        title: "Link / Catch Playhead",
        shortcut: None,
        summary: "Automatically scrolls the List Editor to keep the current playhead position in view during playback.",
        tip: "Toggle off when editing events away from the playhead.",
    },
    HelpEntry {
        id: "list_filter",
        category: "List Editors",
        title: "Event Type Filter",
        shortcut: None,
        summary: "Shows or hides specific MIDI event types (Notes, Program Change, Pitch Bend, CC, Aftertouch, SysEx).",
        tip: "Hide dense controller curves to inspect MIDI notes clearly.",
    },
    HelpEntry {
        id: "flex_toggle",
        category: "Flex Pitch",
        title: "Show/Hide Flex Pitch",
        shortcut: None,
        summary: "Toggles Flex Pitch note beams, pitch trace, and hotspot editing in the Audio Track Editor.",
        tip: "If Flex Mode is Off, enabling Flex automatically switches the track to Flex Pitch.",
    },
    HelpEntry {
        id: "flex_fit",
        category: "Flex Pitch",
        title: "Fit Region",
        shortcut: None,
        summary: "Frames the active audio region horizontally and centers detected notes vertically.",
        tip: "Use Ctrl+Scroll to zoom horizontally around the cursor or Alt+Scroll vertically.",
    },
    HelpEntry {
        id: "flex_tool_pointer",
        category: "Flex Pitch",
        title: "Pointer Tool",
        shortcut: None,
        summary: "Selects, moves, resizes, and transposes Flex Pitch notes and hotspot nodes.",
        tip: "Hold Shift while dragging vertically to fine-tune pitch in cents.",
    },
    HelpEntry {
        id: "flex_tool_scissors",
        category: "Flex Pitch",
        title: "Scissors Tool",
        shortcut: None,
        summary: "Splits a Flex Pitch note segment into two independent notes at the click position.",
        tip: "Use Join to merge adjacent split notes back together.",
    },
    HelpEntry {
        id: "flex_tool_join",
        category: "Flex Pitch",
        title: "Join Tool",
        shortcut: Some("Cmd+J"),
        summary: "Merges selected or clicked adjacent Flex Pitch note segments into one note.",
        tip: "Useful for smoothing over brief pitch detection splits.",
    },
    HelpEntry {
        id: "flex_tool_vibrato",
        category: "Flex Pitch",
        title: "Vibrato Tool",
        shortcut: None,
        summary: "Adjusts pitch modulation / vibrato depth directly by dragging a note up or down.",
        tip: "100% keeps natural vibrato; 0% flattens pitch modulation.",
    },
    HelpEntry {
        id: "flex_tool_volume",
        category: "Flex Pitch",
        title: "Volume Tool",
        shortcut: None,
        summary: "Adjusts individual note gain (-24 dB to +24 dB) directly on the note beam.",
        tip: "Balance quiet syllables without automating the entire track.",
    },
    HelpEntry {
        id: "tb_split",
        category: "Toolbar",
        title: "Split at Playhead",
        shortcut: Some("Cmd+T"),
        summary: "Splits the selected region(s) cleanly at the current playhead position.",
        tip: "Audio splits apply short crossfades automatically to prevent clicks.",
    },
    HelpEntry {
        id: "tb_join",
        category: "Toolbar",
        title: "Join Regions",
        shortcut: Some("Cmd+J"),
        summary: "Joins selected adjacent regions on the same track into a single region.",
        tip: "Select two or more regions on a track first.",
    },
    HelpEntry {
        id: "tb_mute",
        category: "Toolbar",
        title: "Mute Region",
        shortcut: Some("Ctrl+M"),
        summary: "Mutes or unmutes the selected region(s) without muting the whole track.",
        tip: "Muted regions appear dimmed on the timeline.",
    },
    HelpEntry {
        id: "tb_loop",
        category: "Toolbar",
        title: "Loop Region",
        shortcut: Some("L"),
        summary: "Toggles continuous looping for the selected region(s).",
        tip: "Drag the upper-right corner of a region to adjust loop length.",
    },
    HelpEntry {
        id: "tb_color",
        category: "Toolbar",
        title: "Show Colors",
        shortcut: Some("Opt+C"),
        summary: "Opens the color palette to assign custom colors to tracks or regions.",
        tip: "Helps visually organize drums, bass, vocals, and synths.",
    },
    HelpEntry {
        id: "tb_bounce",
        category: "Toolbar",
        title: "Bounce Project or Section",
        shortcut: Some("Cmd+B"),
        summary: "Exports the project or Cycle range offline to WAV, AIFF, or FLAC.",
        tip: "Includes all track effects, automation, and master processing.",
    },
    HelpEntry {
        id: "tb_locators",
        category: "Toolbar",
        title: "Set Locators by Selection",
        shortcut: Some("U"),
        summary: "Sets the Cycle / Locator range to match the currently selected region(s).",
        tip: "Press C to toggle Cycle playback over the new range.",
    },
    HelpEntry {
        id: "tb_zoom_fit",
        category: "Toolbar",
        title: "Zoom to Fit",
        shortcut: Some("Z"),
        summary: "Zooms the Tracks Area to fit the selection or the entire arrangement.",
        tip: "Press Z again to restore your previous zoom level.",
    },
];

pub fn lookup(id: &str) -> Option<&'static HelpEntry> {
    REGISTRY.iter().find(|e| e.id == id)
}

#[derive(Clone, Default)]
struct HoverHelpState {
    active_id: Option<&'static str>,
    last_id: Option<&'static str>,
}

fn state_id() -> egui::Id {
    egui::Id::unique("quick_help_hover_state")
}

/// Call at the start of each frame before UI widgets run so hover tracking resets cleanly.
pub fn begin_frame(ctx: &Context) {
    ctx.data_mut(|d| {
        let mut st = d.get_temp::<HoverHelpState>(state_id()).unwrap_or_default();
        st.active_id = None;
        d.insert_temp(state_id(), st);
    });
}

/// Registers a hovered response with the Quick Help registry.
pub fn hover(ctx: &Context, resp: &Response, id: &'static str) {
    if resp.hovered() || resp.contains_pointer() {
        ctx.data_mut(|d| {
            let mut st = d.get_temp::<HoverHelpState>(state_id()).unwrap_or_default();
            st.active_id = Some(id);
            st.last_id = Some(id);
            d.insert_temp(state_id(), st);
        });
    }
}

/// Renders the rich dark floating Quick Help card near the pointer when Quick Help (`?`) is active.
pub fn show_overlay(ctx: &Context, s: &mut UiState) {
    if !s.show_quick_help {
        return;
    }

    let active_id = ctx.data(|d| {
        d.get_temp::<HoverHelpState>(state_id())
            .and_then(|st| st.active_id)
    });

    let pointer = ctx.input(|i| i.pointer.hover_pos());
    let Some(id) = active_id else {
        // Show a subtle top-center status pill so the user knows Quick Help mode is active.
        let screen = ctx.content_rect();
        let pill_pos = pos2(screen.center().x - 165.0, screen.top() + 92.0);
        egui::Area::new(egui::Id::unique("quick_help_active_banner"))
            .order(Order::Tooltip)
            .fixed_pos(pill_pos)
            .interactable(false)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(CARD_BG)
                    .stroke(Stroke::new(1.0, CARD_BORDER))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("?")
                                    .strong()
                                    .color(TEXT_TIP)
                                    .font(FontId::proportional(12.5)),
                            );
                            ui.label(
                                RichText::new(tr("Quick Help: Hover any control for guidance"))
                                    .color(TEXT_MAIN)
                                    .font(FontId::proportional(11.5)),
                            );
                        });
                    });
            });
        return;
    };

    let Some(entry) = lookup(id) else {
        return;
    };

    let screen = ctx.content_rect();
    let card_w = 290.0_f32;
    let est_h = 118.0_f32;
    let raw_pos = pointer
        .map(|p| pos2(p.x + 14.0, p.y + 18.0))
        .unwrap_or_else(|| pos2(screen.center().x - card_w * 0.5, screen.top() + 96.0));
    let clamped_x = raw_pos.x.clamp(
        screen.left() + 8.0,
        (screen.right() - card_w - 8.0).max(screen.left() + 8.0),
    );
    let clamped_y = raw_pos.y.clamp(
        screen.top() + 8.0,
        (screen.bottom() - est_h - 8.0).max(screen.top() + 8.0),
    );

    egui::Area::new(egui::Id::unique("quick_help_floating_card"))
        .order(Order::Tooltip)
        .fixed_pos(Pos2::new(clamped_x, clamped_y))
        .interactable(false)
        .show(ctx, |ui| {
            let frame_resp = Frame::new()
                .fill(CARD_BG)
                .stroke(Stroke::new(1.0, CARD_BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_max_width(card_w - 24.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(entry.title)
                                .strong()
                                .color(TEXT_MAIN)
                                .font(FontId::proportional(13.0)),
                        );
                        if let Some(sc) = entry.shortcut {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let (rect, _) = ui.allocate_exact_size(
                                        vec2((sc.len() as f32 * 7.2 + 12.0).max(24.0), 18.0),
                                        Sense::hover(),
                                    );
                                    let p = ui.painter();
                                    p.rect_filled(rect, 4.0, BADGE_BG);
                                    p.rect_stroke(
                                        rect,
                                        4.0,
                                        Stroke::new(1.0, CARD_BORDER),
                                        StrokeKind::Inside,
                                    );
                                    p.text(
                                        rect.center(),
                                        Align2::CENTER_CENTER,
                                        sc,
                                        FontId::monospace(10.5),
                                        TEXT_MAIN,
                                    );
                                },
                            );
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(entry.summary)
                            .color(TEXT_SUB)
                            .font(FontId::proportional(11.5)),
                    );
                    ui.add_space(5.0);
                    ui.label(
                        RichText::new(format!("{}: {}", tr("Tip"), entry.tip))
                            .color(TEXT_TIP)
                            .font(FontId::proportional(11.0)),
                    );
                });
            let _ = frame_resp;
        });
}

/// Detailed Quick Help Reference window opened via `Help > Quick Help Window` (`Alt+Shift+?`).
pub fn show_reference_window(ctx: &Context, s: &mut UiState) {
    if !s.show_quick_help_window {
        return;
    }

    let last_id = ctx.data(|d| {
        d.get_temp::<HoverHelpState>(state_id())
            .and_then(|st| st.last_id)
    });

    let mut open = s.show_quick_help_window;
    egui::Window::new(tr("Quick Help Reference"))
        .open(&mut open)
        .resizable(true)
        .default_size(vec2(420.0, 520.0))
        .frame(
            Frame::new()
                .fill(CARD_BG)
                .stroke(Stroke::new(1.0, CARD_BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::same(12)),
        )
        .show(ctx, |ui| {
            if let Some(entry) = last_id.and_then(lookup) {
                Frame::new()
                    .fill(Color32::from_rgb(0x1d, 0x1e, 0x22))
                    .stroke(Stroke::new(1.0, CARD_BORDER))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(entry.title)
                                    .strong()
                                    .color(TEXT_MAIN)
                                    .font(FontId::proportional(13.5)),
                            );
                            if let Some(sc) = entry.shortcut {
                                ui.label(
                                    RichText::new(format!("[{sc}]"))
                                        .color(TEXT_TIP)
                                        .font(FontId::monospace(11.0)),
                                );
                            }
                        });
                        ui.add_space(3.0);
                        ui.label(RichText::new(entry.summary).color(TEXT_SUB));
                        ui.add_space(3.0);
                        ui.label(
                            RichText::new(format!("{}: {}", tr("Tip"), entry.tip))
                                .color(TEXT_TIP)
                                .font(FontId::proportional(11.0)),
                        );
                    });
                ui.add_space(8.0);
            }

            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Search")).color(TEXT_SUB));
                ui.add(
                    egui::TextEdit::singleline(&mut s.quick_help_query)
                        .desired_width(ui.available_width()),
                );
            });
            ui.add_space(8.0);

            let q = s.quick_help_query.trim().to_ascii_lowercase();
            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut current_cat = "";
                for entry in REGISTRY {
                    if !q.is_empty()
                        && !entry.title.to_ascii_lowercase().contains(&q)
                        && !entry.summary.to_ascii_lowercase().contains(&q)
                        && !entry.category.to_ascii_lowercase().contains(&q)
                    {
                        continue;
                    }
                    if entry.category != current_cat {
                        current_cat = entry.category;
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(current_cat)
                                .strong()
                                .color(TEXT_TIP)
                                .font(FontId::proportional(12.0)),
                        );
                        ui.separator();
                    }
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(entry.title)
                                .strong()
                                .color(TEXT_MAIN)
                                .font(FontId::proportional(12.0)),
                        );
                        if let Some(sc) = entry.shortcut {
                            ui.label(
                                RichText::new(format!("({sc})"))
                                    .color(TEXT_SUB)
                                    .font(FontId::monospace(10.5)),
                            );
                        }
                    });
                    ui.label(
                        RichText::new(entry.summary)
                            .color(TEXT_SUB)
                            .font(FontId::proportional(11.5)),
                    );
                    ui.label(
                        RichText::new(entry.tip)
                            .color(Color32::from_rgb(0x8e, 0x90, 0x98))
                            .font(FontId::proportional(10.5)),
                    );
                    ui.add_space(5.0);
                }
            });
        });
    s.show_quick_help_window = open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_are_unique_and_non_empty() {
        assert!(REGISTRY.len() >= 60);
        for (i, e) in REGISTRY.iter().enumerate() {
            assert!(!e.id.is_empty() && !e.title.is_empty() && !e.summary.is_empty());
            assert!(
                REGISTRY[i + 1..].iter().all(|o| o.id != e.id),
                "duplicate id: {}",
                e.id
            );
        }
    }
}
