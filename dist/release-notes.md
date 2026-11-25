### Dōri v0.1.0-alpha.9

Welcome to Dōri (道理) v0.1.0-alpha.9, a pure-Rust cross-platform digital audio workstation.

#### Downloads & Release Packages

| Package | Platform | Architecture | Details |
|---|---|---|---|
| `Dori-Setup-0.1.0-alpha.9-x86_64.exe` | Windows | x86_64 | Guided dark minimal setup installer |
| `Dori-0.1.0-alpha.9-win64-portable.zip` | Windows | x86_64 | Portable zero-install zip archive |
| `Dori-Setup-0.1.0-alpha.9-x86_64.AppImage` | Linux | x86_64 | Standalone dark minimal installer |
| `Dori-0.1.0-alpha.9-linux-x86_64-portable.tar.gz` | Linux | x86_64 | Portable standalone tarball |
| `dori-0.1.0-alpha.9-1.x86_64.rpm` | Fedora / RHEL / openSUSE | x86_64 | Native RPM package with desktop integration |
| `SHA256SUMS.txt` | All | Checksums | Cryptographic SHA-256 integrity verification |

## What's new in 0.1.0-alpha.9

### Sampler, Drum Kit Designer & User Loops
- **Drag Audio to Track Header (`Create New Track using:`)**: Dragging Tracks Area audio regions or OS audio files onto the Track Header gutter opens a compact dark modal with `Quick Sampler (Original)`, `Quick Sampler (Optimized)`, `Drum Kit Designer`, and `Sampler (Zone Per Note)`.
- **Drum Kit Designer Auto-Slicing & Track Stack**: Automatically detects up to 32 transient slices, builds a collapsible Summing Track Stack (`#1 <Name>`, `#2 <Name>`, ...) with per-slice samplers, and generates a quantized MIDI groove region on the parent stack track.
- **Tagged User Loops (`Add Region to Loops Library`)**: Dragging any MIDI or Audio region into the Sound Library opens a tagging modal (`Name`, `Loop`/`One-shot`, `Scale`, `Genre`, `Key`, `Tempo`, Instrument Descriptors, and Mood/Character pills), saving bundles to `<config_dir>/user_loops/` for one-click instantiation.

### Modern Vector DAW Controls & Hardware Widgets
- **Flat-Bevel Vector Knobs (`daw_knob.rs`)**: Replaced all legacy Moog/AudioKnobs controls with clean flat-bevel vector rotary knobs (`Continuous`, `Bipolar`, `Stepped`, `RingLight`) featuring `log_taper` scaling, inline text entry, and smooth continuous `Drive` control under Macro Controls.
- **Hardware FX & Synth Widgets (`widgets.rs`, `fx_ui.rs`, `prowave-plug`)**: Wired `daw_rocker_switch`, `daw_led_button`, `daw_backlit_plate`, `daw_radial_selector`, `daw_xyz_sliders`, and live `daw_phosphor_scope` displays across built-in FX editors and standalone `prowave-plug` VST3/CLAP/AU targets.

### UI Polish, Transport & Documentation
- **Opaque Workspace Panels & Floating Windows**: Restored solid `#1c1c1e` opaque panel fills across the `Loops` browser, `Library`, `Inspector`, `Macro Controls`, `Mixer`, and floating windows.
- **Translucent Window Control Inner Glow**: Updated title-bar window control hover states (`Minimize`, `Maximize/Restore`, `Close`) with layered translucent inner glow in Yellow (`#febc2e`), Green (`#28c840`), and Red (`#ff5f57`).
- **Cycle Repeat Playback Start**: Pressing `Space` with Cycle Repeat enabled now starts playback from the selected range / cycle start (`#4`).
- **Documentation & Branding**: Updated `AGENTS.md` to Dōri / Logic Pro 13 inspiration and added Dicky Jiang's `DAW UI Design` and `webaudio-controls` attributions to `README.md`.

Licensed under the GNU General Public License v3 or later.
