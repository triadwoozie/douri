### Dōri v0.1.0-alpha.11

Welcome to Dōri (道理) v0.1.0-alpha.11, a pure-Rust cross-platform digital audio workstation.

#### Downloads & Release Packages

| Platform / Format | File |
| --- | --- |
| Windows x86_64 (Guided Installer) | `Dori-Setup-0.1.0-alpha.11-x86_64.exe` |
| Windows x86_64 (Portable ZIP) | `Dori-0.1.0-alpha.11-win64-portable.zip` |
| Linux x86_64 (Standalone AppImage Installer) | `Dori-Setup-0.1.0-alpha.11-x86_64.AppImage` |
| Linux x86_64 (Portable Tarball) | `Dori-0.1.0-alpha.11-linux-x86_64-portable.tar.gz` |
| Fedora / RHEL / openSUSE (RPM) | `dori-0.1.0-alpha.11-1.x86_64.rpm` |
| Checksums | `SHA256SUMS.txt` |

## What's new in 0.1.0-alpha.11

### Spatial Audio (Eclipsa Audio 7.1.2 / 7.1.4)
- Zero-allocation `eclipsa.rs` 3D Object / Surround Panner DSP and Binaural downmixer.
- `Eclipsa Audio` Master Renderer, `3D Object Panner`, and `Surround Panner` UI, Startup Screen surround format selector, `Project Settings > Audio`, `Mix > Eclipsa Audio…`, and MCP spatial tools.

### Session Player & Step Sequencer
- Revamped Session Player engine (`Keyboard`, `Bass`, `Drummer`) with 5-tab Logic Pro-style editor, Chord ID, New Track style integration, and compact empty Session Player bar.
- Redesigned Step Sequencer with centered tools, yellow sliders, and expanded row spacing.

### Marbble DSP, Synths, Sampler & prowave-dsp-kit
- Ported Marbble PDC and 17 built-in effects (`marbble-dsp`), 6 synthesizers (`Tre`, `Sei`, `Cinque`, `Uno`, `Sedici`, `Onda`), multisample Sampler with auto-mapping, crossfade loops, and SFZ support, per-strip PDC latency readouts in Mixer, and embedded WebKit2 WebView instrument panels.
- Ported `prowave-dsp-kit` (`matrix4`, `matrix4_mb`, `Mosaic Decimator`, `Dustbowl Lo-Fi`, `Kestrel EQ`) and upstream `fabox-os` plugin-host, capture, and meter readability improvements.

### Audio Track Editor, PitchNet & Audio Cleanup
- Replaced Flex Pitch with a themed Audio Track Editor (`#141518` canvas, deterministic top-edge resize, Audacity-ported DC Offset / De-Click / Denoise tools, and aubio-ported onset/YIN pitch analysis).
- Added 1-click external PitchNet launcher with region audio loading, DAW auto-pause, single-instance guard, and automatic language sync.

### Plugin Hosting, Playback & Format Support
- Ported Linux LV2 plugin hosting and native LV2 editor UI support (optional `lv2` feature).
- Unified case-insensitive WAV/FLAC/OGG/AIFF/MP3/OPUS import and browser drag-and-drop.
- Fixed reverse disk streaming choppiness (`65_536`-frame reverse cache + compressed seek overshoot recovery) and JUCE VST3 X11 `RunLoop` event pumping.

### UI, Configuration & Localization
- Added Logic Pro auto-unlatching **Catch Playhead** (`Scroll in Play`) across Tracks Area, Piano Roll, and Audio Track Editor.
- Separated `~/.config/douri` from `prowave` with a one-time migration prompt covering Pro-Wave and older Dōri versions.
- Added full **Bahasa Melayu (`ms`)** UI and installer translation (20 supported languages).
- Improved slider visibility with white unfilled rails/handles and Logic blue trailing fill.

Licensed under the GNU General Public License v3 or later.
