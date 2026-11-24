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
- **Presets:** nih-plug offers hosts no preset list, so the registry's presets (Engine, Tune,
  Texture) are a menu on the window's faceplate.

What the plug-ins leave out:

- FM6 cannot open `.syx` banks (the program's window does that with a file dialog).
- Pro Wave Tune's Target "MIDI" follows the notes of the program's MIDI track; a plug-in has no
  such track, so in it MIDI mode corrects to nothing. Use Scale mode, or Pitch Override. Its
  Key and Scale "Project" are C major (A4 = 440 Hz) in the plug-in: choose the key.
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
