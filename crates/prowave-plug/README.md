# Pro Wave plug-ins

Every built-in instrument and effect of the public Pro Wave build as a plug-in of its own, for
other music programs: VST3, CLAP and Audio Unit (AUv2), universal (Apple silicon and Intel),
macOS 11 or later. The sound is the engine's own code (`crates/prowave-engine`, the tables in
`native_inst/mod.rs` and `native_fx/mod.rs`); this crate only wraps it. GPL-3.0-or-later.

## How it is made

- **One build per plug-in.** The plug-in framework, nih-plug (ISC, used at commit `de42101`),
  makes one plug-in per library, with its name and ids as constants. `build.rs` reads the
  registry entry named by `PROWAVE_PLUG_ID` (the Pro Wave Macro Oscillator when it is not set)
  and writes those constants; `tools/build-plugins.sh` builds the library once per id.
- **Its own workspace.** This crate is not a member of the program's workspace (it is in the
  root `Cargo.toml`'s `exclude`): nih-plug and its dependencies come from git and would
  otherwise enter the program's lock file, `cargo deny` and `cargo xtask check`. The engine is
  a path dependency built with `default-features = false`, so only the public parts are in.
- **Names and codes,** all made from the registry id (`src/codes.rs`), so they stay the same
  from one release to the next:
  - name: the registry name with "Pro Wave " in front unless it starts with it ("Kick" is
    "Pro Wave Kick", "Pro Wave Tune" stays), so a host lists them together; the bundles are
    named the same;
  - CLAP id `com.yuribeats.prowave.<id without "prowave:">`, bundle ids the CLAP id plus
    `.vst3`, `.clap` or `.component`;
  - VST3 class id: two 64-bit FNV-1a hashes of the id;
  - Audio Unit: manufacturer `PrWv` ("Pro Wave"), type `aumu` for an instrument and `aufx` for
    an effect, subtype four characters from a hash of the id (a test checks that none repeat);
    the name a host shows is "Pro Wave: Kick".
- **Parameters** are the registry's, in its order, as whole numbers with the registry's range,
  default and value text ("-3.5dB", "20ms", a choice's name); a value typed into a host's field
  is read back through the same texts. Ids are `p0`, `p1`, ... (the index, which is also how a
  Pro Wave project stores them). A page's key ("OSC 1", "PAD 3") is the host's parameter group.
  The values are saved in the host's project.
- **Audio thread** (`src/core.rs`): MIDI notes, velocity, pitch bend, the sustain pedal and
  controllers 120 and 123 go to an instrument at their frame; a host block is cut into pieces of
  at most 256 frames; an effect's latency is reported to the host and again whenever it changes
  (Press's oversampling); a piece of an effect that comes out not finite is silenced. Nothing
  there allocates, locks or panics: the tests run it inside the allocation checker nih-plug
  installs in debug builds. Press's oversampling sizes its memory, so the effect is opened at
  16x and set down, and the host can raise it again.
- **Window** (`src/editor/`): the Hardware look of the program's units, drawn with egui
  through nih_plug_egui. `theme.rs`, `hw.rs` and `gear.rs` are copies of the program's files of
  the same names, changed only where nih_plug_egui's egui (0.31) differs from the program's
  (0.37); the layout follows the program's instrument and effect windows (`src/pages.rs`). Every
  knob turns (drag; Shift for fine; double-click for the default) and shows its value text;
  choices are keys or a menu; RESET puts every parameter back to its default; Wave Draw's
  display is drawn on with the pointer.
- **Presets:** the registry's presets (Engine, Tune, Texture) are the host's factory presets in
  every format, and a menu on the window's faceplate too. nih-plug has no presets of its own, so
  the crate uses a fork of it (github.com/yuribeats/nih-plug, branch `prowave-presets`, on
  `de42101`) that adds them: a CLAP preset-discovery factory and `clap.preset-load` (one location
  inside the plug-in, the preset's number as its key), which clap-wrapper turns into the Audio
  Unit's factory presets, and a VST3 program list with a program-change parameter ("Program",
  first entry "(none)"). Loading one sets the parameters it names and keeps the others, as the
  menu does. The numbering is the registry's order: new presets go at the end.
- **FM6 banks:** the SYX key on FM6's window opens a .syx bank with the system's file dialog
  (rfd, on a thread of its own, so the host keeps running), reads it with the program's code
  (`fm6::bank_voices`) and lists its voices; picking one sets the voice's 145 parameters
  through the host, one gesture each, so the host saves and undoes it; the other parameters
  (voices, level) are kept, as in the program.
- **Tune's MIDI target:** Pro Wave Tune takes MIDI notes (a note input in VST3 and CLAP; its
  Audio Unit is a music effect, `aumf`, which Logic lists with the effects and gives a MIDI
  side-chain menu). With Target "MIDI", the newest note held is the target; with none held it
  tunes to the key and scale, as the program does while stopped. A note counts from the start
  of the piece of at most 256 frames it comes in. In the program the MIDI target is the notes of
  a MIDI track instead.

What the plug-ins leave out:

- Pro Wave Tune's Key and Scale "Project" follow the program's project key, which a host does
  not give a plug-in: in the plug-in they are named "C (default)" and "Major (default)" and
  are C major (A4 = 440 Hz). The stored value is the same, so saved settings keep their
  meaning. Its "MIDI Track" knob has no use in a plug-in (the notes come in on its MIDI input).
- Pro Wave Tune's Audio Unit changed from an effect (`aufx`) to a music effect (`aumf`) in
  0.3.2, so a Logic project that used the 0.3.1 Tune Audio Unit does not find it again.
- In VST3, an instrument also shows 2080 hidden parameters: nih-plug's stand-ins for the MIDI
  controllers of 16 channels (VST3 passes controllers to a plug-in as parameters).
- When a host resets an effect, the sound already inside it (delay lines, reverb) is not
  cleared: the engine cannot be opened again on the audio thread.

## Building

On a Mac with Rust and both macOS targets (`rustup target add aarch64-apple-darwin
x86_64-apple-darwin`), cmake, git, python3 and the Xcode command-line tools:

```
bash crates/prowave-plug/tools/build-plugins.sh OUT            # every plug-in
bash crates/prowave-plug/tools/build-plugins.sh OUT prowave:kick prowave:tune
```

For each plug-in it builds the library for both architectures (`--release`, `PROWAVE_PLUG_ID`
set), joins them with `lipo`, makes the `.vst3` and `.clap` bundles, wraps the `.clap` as an
Audio Unit with clap-wrapper (`au/CMakeLists.txt`; fetched from GitHub at a fixed commit on the
first run), puts `LICENSE`, `THIRD-PARTY-NOTICES.txt` (`tools/notices.py`: the plug-in's own
engine first, then the rest of the engine library, the fonts and the Rust crates) and the
fonts' licences into each bundle's `Resources`, and signs each bundle ad hoc
(`codesign --force --deep -s -`). The bundles are not notarized: after a download, macOS
needs `xattr -dr com.apple.quarantine <bundle>` before a host will load them. `OUT/plugins.tsv`
lists what was built. A new instrument or effect in the registry is built with no change here,
except a line in `tools/notices.py` naming its notices (the script stops until there is one).

## Checking

```
cargo test --manifest-path crates/prowave-plug/Cargo.toml
PROWAVE_PLUG_ID=prowave:press cargo test --manifest-path crates/prowave-plug/Cargo.toml --test process
cargo run --manifest-path crates/prowave-plug/Cargo.toml --example check_bundle -- OUT/*.clap
```

- The unit tests run every plug-in's audio path (`PlugCore`) in blocks of up to 4096 frames,
  mono and stereo, with every parameter moved to its ends, notes, the pedal, the wheel and both
  "all off" controllers, inside the allocation checker; check that every value text reads back;
  that every parameter has a place in the window; and that no two plug-ins share a code.
- `tests/process.rs` runs the plug-in of the build (`PROWAVE_PLUG_ID`) through nih-plug's CLAP
  wrapper, as a host does: sound, every parameter moved while it plays, no allocation on the
  audio thread, the parameters and their texts, the saved state, the latency.
- `examples/check_bundle.rs` opens built `.clap` bundles from disk and renders two seconds of
  each offline (a chord, or a test signal through an effect): it fails on silence or on a value
  that is not finite.
- Audio Units: copy a `.component` into `~/Library/Audio/Plug-Ins/Components` and run
  `auval -v <type> <subtype> PrWv` (the codes are in `plugins.tsv`).
- VST3: `prowave --vst3-check <bundle>` (the program's own checker) loads each class and lists
  its parameters.
