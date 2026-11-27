# Dōri — Technical Roadmap & Architecture Specification

**Repository:** `triadwoozie/douri` | **Stack:** Pure Rust + `egui` (`wgpu` Vulkan / `glow`) + `cpal` | **Targets:** Linux, Windows, macOS

> **Dōri** (*道理* — principle, reason, way)

---

## 0. Vision & Goal

Build a fast, lightweight digital audio workstation in pure Rust—combining a lock-free real-time audio engine, 64-bit summing mixer, comprehensive MIDI/audio editing, native CLAP/VST3 plugin hosting, and a GPU-accelerated Liquid Glass interface without Electron or JUCE bloat.

This document serves as the architectural source of truth for Dōri's shipped capabilities (`v0.1.0-alpha.10`) and upcoming development phases, audited against the Logic Pro User Guides (`references/docs/`).

---

## 1. Core Feature Specification

### 1.1 Transport, Timeline & Project Management
- **Transport & Global Tracks:** Sample-accurate Play, Stop, Record, Cycle/Loop, Count-in, Metronome, Varispeed, Tempo Track, Time Signature Track (including odd/compound meters), Arrangement Track, Marker Track, and Beat Mapping.
- **Smart Tempo & Tuning:** Region tempo detection/alignment, Equal Tempered, Hermode Tuning (HMT), Scala `.tun` user tuning, and variable `A4` pitch (`427–453 Hz`).
- **Project Archive (`.douri`):** Self-contained zip container (`project.json`, `media/`, `irs/`) with `.prowave` backward compatibility, Project Alternatives, Autosave & Crash Recovery, up to 1,024 tracks, 44.1–192 kHz sample rates, and 64-bit summing.

### 1.2 Track Types, Arranging & Audio Editing
- **Tracks:** Audio, Software Instrument, Drummer/Session Player, Pattern, Aux, VCA, Folder/Summing Track Stacks, and External MIDI tracks with Inspector and searchable Sound Library.
- **Editing & Comping:** Non-destructive clip gain, beat-based fades, overlap crossfades, Marquee selection, gap-free snap grid, Option/Alt-drag duplication, Take Folders, and Quick Swipe Comping.
- **Time & Pitch:** Flex/time-stretch modes (`Slicing`, `Rhythmic`, `Monophonic`, `Polyphonic`, `Tempophone`, `Speed`), Groove Track extraction, and Per-Note PSOLA Pitch Editor with audio-to-MIDI conversion.

### 1.3 MIDI, Instruments & DSP Effects
- **MIDI & Notation:** Piano Roll (swing/scale quantize, velocity/CC lanes, flam, humanize), Step Sequencer (1–64 steps, Chance/Repeat/Tie), Event List (with hex SysEx), 8-slot MIDI FX chain (including sandboxed Rhai **Scripter**), and Score Editor with MusicXML export.
- **Instruments:** Transient-Slicing Sampler (`Pads`/`Slices`/`Instrument` with interactive ADSR), Modal Voice, TripleOscillator (`TriOsc`), WaveDraw wavetable synth, 808 Bass, Subtractive Synth, Electric Piano, Tonewheel Organ, velocity-metered Drum Machine, SoundFont (`.sf2`), and `.exs`/`.sfz` players.
- **DSP & Metering:** Multi-model Compressor, Lemon Studio Dynamics (Limiter, Soft Clipper, Fast Distortion, Transient Processor), 8-band Channel EQ, Pult EQ, Convolution Reverb, Bode Frequency Shifter, Auto-Pan, local AI **Stem Splitter**, and **MultiMeter** (ITU-R BS.1770-4 LUFS, 4× True Peak, 45° goniometer, IEC VU/PPM ballistics).

### 1.4 Mixing, Hosting, Bounce & Automation
- **Mixer & Automation:** 64-bit summing channel strips, 15 Audio FX slots, 8 pre/post Sends, VCA/Mix Groups, Track/Region automation (`Read`/`Touch`/`Latch`/`Write`), automatic PDC, and Low Latency Mode.
- **Hosting & Export:** Native CLAP, VST3, LADSPA, and AU hosting with out-of-process crash isolation and X11/XWayland windows; faster-than-realtime offline bounce (`500×`) with TPDF dithering and multi-track stem export; built-in MCP server (`127.0.0.1:38420` / `--mcp-stdio`).

---

## 2. Technical Architecture

```text
[ UI Thread: egui (wgpu / glow) ] ──(rtrb SPSC: Command)──> [ Engine Thread: prowave-engine ]
              ▲                                                       │
              └────────(Triple Buffer & rtrb: State/Meters)───────────┘
                                                                      ▼
                                                       [ Audio Thread: #![forbid(alloc)] ]
                                                                      │
                                                       cpal (PipeWire/ALSA/WASAPI/CoreAudio)
```

### Real-Time Safety Invariants
- **Zero Audio-Thread Allocation:** `#![forbid(alloc)]` and `assert_no_alloc` enforced across the entire audio callback path.
- **Lock-Free Parameter Smoothing:** Atomic parameter updates paired with 1-pole smoothers (`ParamSmoother`).
- **Deferred Reclamation:** Old audio graphs, songs, and tempo maps are returned via a bounded credit-based garbage queue to be dropped off the audio thread.

---

## 3. Release Status & Phased Roadmap

### Shipped in `v0.1.0-alpha.10`

- [x] **Engine & Audio Safety:** Lock-free `rtrb` + triple-buffer engine, 64-bit summing, automatic PDC, memory-mapped PCM streaming (`1,000+` stereo tracks), native Windows ASIO / `IAudioClient3` WASAPI, PipeWire-native JACK discovery, loopback latency calibrator, and `cargo xtask rt-check`.
- [x] **Arranging & Editing:** Multi-track arranger, Marquee selection, gap-free snap grid, Option/Alt-drag duplication, clip gain, beat-based fades, overlap crossfades, autosave/crash recovery, global tracks, and Per-Note Pitch Editor (PSOLA).
- [x] **MIDI & Sequencer:** Piano Roll, Step Sequencer, Score Editor + MusicXML export, Event List (with hex SysEx), MIDI Transform, 8-slot MIDI FX chain (including sandboxed Rhai Scripter), and MIDI Clock / MTC sync.
- [x] **Instruments & DSP:** `.exs`/`.sfz` Multisample Player, Transient-Slicing Sampler with interactive ADSR envelope, Modal Voice, `TriOsc`, `WaveDraw`, `.sf2` SoundFont Player, 808 Bass, Subtractive Synth, Electric Piano, Tonewheel Organ, velocity-metered Drum Machine, Convolution Reverb, Pult EQ, Lemon Studio Dynamics, SoundCraft FX, and MultiMeter (LUFS, True Peak, 45° goniometer, IEC VU/PPM ballistics).
- [x] **Bounce, Interop, MCP & Packaging:** Faster-than-realtime offline bounce (`500×`), TPDF dithering, stem export, `.logicx`/`.band`/`.h2song`/`.mid` importers, CLAP/VST3/LADSPA/AU hosting, built-in MCP server, Command Palette, Privacy-Redacted Diagnostics, and `egui` installers (`.exe`, `.AppImage`, `.rpm`, portable archives).

### Phase 1: Hardware Audio, Menu Parity & Sandboxing (`v0.1.x`)

- [x] **Interactive Audio Device & Buffer Selector:** Live output/input device switching and buffer size selection in `Project Settings > Audio`.
- [x] **Native PipeWire & JACK Backends:** Direct low-latency PipeWire and JACK detection with live quantum diagnostics.
- [x] **Windows Startup Deadlock Fix (`#5`):** Avoid `InnerSize` + `Maximized` Win32 message-loop deadlock on chooser-to-DAW transition.
- [ ] **Audio & Pattern Split / Join:** Sample-accurate split, join, and trim tools for audio, Pattern, Drummer, and take-folder regions (matching MIDI region parity).
- [ ] **Complete Control Bar & Menu Parity:** Wire remaining disabled menu entries (`Save as Template`, `Project Alternatives`, `Show Chord / Arrangement Track`, `Pre-Fader Metering`, `Low Latency Mode` toggle).
- [ ] **Windows & Wayland Native Plugin Windows:** Port out-of-process crash isolation and embedded plugin editor windows to Windows (`HWND`) and native Wayland surfaces.

### Phase 2: Advanced Synthesis, Sampling & Effects (`v0.2.0`)

- [x] **Visual Multisample Zone & Keymap Editor:** Three-tab Transient-Slicing Sampler (`Pads`, `Slices`, `Instrument`) with draggable ADSR breakpoint envelope and automatic transient loop slicing.
- [x] **Vector, Modal & Wavetable Synth Engines:** Modal Voice physical modeling, `TriOsc` 3-oscillator synth, `WaveDraw` wavetable synth, and vector X/Y morphing.
- [x] **Full Channel EQ & Multi-Model Compressor:** 8-band Channel EQ with pre/post FFT overlay and selectable VCA/FET/Opto compressor circuit models with external sidechain.
- [~] **Algorithmic Reverb & Tap Delay Designer:** Multi-model algorithmic reverb (Hall, Room, Plate) and visual multi-tap delay editor (`space_designer_ui.rs` convolution shipped; ChromaVerb/Delay Designer UI in progress).
- [ ] **Two-Handle Bezier Automation & Output Strip Lanes:** Full Bezier curve handles on automation segments and automation lanes for Stereo Out / Master strips.

### Phase 3: Live Performance, Smart Controls & Scoring (`v0.3.0`)

- [x] **Smart Controls, Macro Mapping & Modulation Drawer:** Auto-mapped macro knobs per patch, MIDI Learn, and dockable Modulation Matrix / LFO / Env drawer.
- [x] **Chord Track & Arrangement Sections:** Global Chord Track driving Session Players and scale-aware editors, plus Arrangement Track section reordering.
- [~] **Control Surfaces & Mackie Control:** Hardware MIDI Learn and Mackie Control protocol groundwork (`control.rs`; full MCU fader-bank/scribble-strip parity remaining).
- [ ] **Live Loops Performance Grid & Remix FX:** Clip/scene launching matrix with quantized triggers, one-shot/loop modes, performance recording into the linear arranger, and X/Y Remix FX.
- [ ] **Full Score Engraving & Video Track:** Beamed multi-voice notation, tablature, and floating Video Track with SMPTE sync and audio-to-movie export.

### Phase 4: Stem Separation, Mastering & Spatial Audio (`v0.4.0+`)

- [x] **Local AI Stem Separation (`Mix > Stem Splitter…`):** Built-in `stems.rs` STFT + HPSS + Mid/Side Wiener stem splitter with Logic Pro-style `stems_ui.rs` sheet and settings integration.
- [x] **Command Palette & Privacy-Redacted Diagnostics:** `Cmd/Ctrl+K` Command Palette and `Diagnostics…` / `douri --report` system inspector.
- [ ] **Neural ONNX Stem Model & Mastering Assistant:** Optional local ONNX (`ort`/`tract`) neural stem separation backend and automated spectral/loudness Mastering Assistant.
- [ ] **MPE & MIDI 2.0 Property Exchange:** Full per-note pitch, pressure, and timbre expression lanes inside the Piano Roll.
- [x] **Eclipsa Audio (7.1.2 / 7.1.4) Spatial Renderer & 3-Pane Panner UI:** Zero-alloc `eclipsa.rs` 3D Object / Surround Panner DSP + Binaural downmixer, `Eclipsa Audio` Master Renderer + `3D Object Panner` + `Surround Panner` UI (`eclipsa_ui.rs`), Startup Screen Surround Format, `Project Settings > Audio`, and `Mix > Eclipsa Audio…`.
- [ ] **Eclipsa `.iamf` Bitstream Export & Spatial Automation Lanes:** Offline `.iamf` bitstream muxer/export and per-track spatial coordinate automation lanes.

---

## 4. Performance Targets & Verification Gates

| Metric | Target | Verification Gate |
| --- | --- | --- |
| Audio-thread heap allocations | `0` per callback | `cargo xtask rt-check` (`#![forbid(alloc)]` + `assert_no_alloc`) |
| Round-trip latency (`128` samples @ `48 kHz`) | `< 8 ms` Linux/macOS, `< 10 ms` Windows | `cargo run -p prowave-engine --example beep` |
| High-track-count streaming | `1,000` stereo tracks, `0` xruns | `cargo test --release -p prowave-engine --test thousand_tracks` |
| Plug-in Delay Compensation (PDC) | Sample-accurate (`< -90 dB` null residual) | `prowave-engine` PDC null test suite |
| Cold startup & idle footprint | `< 400 ms` startup, `< 150 MB` idle RSS | Release binary RSS & startup benchmark |
| Workspace code quality | Zero lints, formatting diffs, or test regressions | `cargo xtask check` (`fmt`, `clippy -D warnings`, `test`, `rt-check`) |
