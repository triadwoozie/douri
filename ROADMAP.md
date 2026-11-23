# Dōri — Technical Roadmap & Architecture Specification

**Repository:** `triadwoozie/douri` | **Stack:** Pure Rust + `egui` (`wgpu` Vulkan / `glow`) + `cpal` | **Targets:** Linux, Windows, macOS, WebAssembly

> **Dōri** (*道理* — principle, reason, way)

---

## 0. Vision & Goal

Build a fast, lightweight digital audio workstation in pure Rust—combining a lock-free real-time audio engine, 64-bit summing mixer, comprehensive MIDI/audio editing, native CLAP/VST3 plugin hosting, and a GPU-accelerated Liquid Glass interface without Electron or JUCE bloat.

This document serves as the architectural source of truth for Dōri's shipped capabilities (`v0.1.0-alpha.1`) and upcoming development phases.

---

## 1. Core Feature Specification

### 1.1 Transport, Timeline & Project Management
- **Transport:** Sample-accurate Play, Stop, Record, Play from Selection, Play from Last Locate, Pause, Cycle/Loop, Count-in, Metronome with custom accent/click levels, and Varispeed (Speed Only, Varispeed + Pitch).
- **Timeline & Global Tracks:** Bars|Beats, Time (SMPTE 24/25/29.97/30, HMS, Samples), Tempo Track with linear ramps, Time Signature Track (including compound and odd meters like 6/8 and 7/8), Beat Mapping Track, Arrangement Track (Intro, Verse, Chorus markers), and Marker Track.
- **Smart Tempo:** Tempo detection and beat tracking on audio regions (`Keep Project Tempo`, `Adapt Project Tempo`, `Automatic`), Smart Tempo Editor for adjusting downbeats, and multitrack tempo analysis.
- **Project Management (`.douri` Archive):** Self-contained `.douri` zip container (`project.json`, `media/`, `irs/`) with full `.prowave` backward compatibility, Project Alternatives, Autosave & Crash Recovery, Project Clean Up, Consolidate, up to 1,024 tracks, 44.1–192 kHz sample rates, and 64-bit summing.
- **Tuning:** Equal Tempered, Hermode Tuning (HMT), User Tuning (Scala `.tun`), and variable `A4` reference pitch (`427–453 Hz`).

### 1.2 Track Types & Track Management
- **Audio Track:** Mono/Stereo routing, Input Monitoring, Record Enable, Freeze, Track Stacks (Folder Stack = grouping, Summing Stack = submix bus with aux), Track Alternatives, Groove Track, and custom `phosphor-egui` Track Icons & Colors.
- **Software Instrument Track:** Multi-timbral MIDI channel routing, layered instruments, and pre-instrument MIDI FX chains.
- **Session Player & Pattern Tracks:** Algorithmic Drummer and Session Players (Rock, Pop, Funk, Jazz, Hip-Hop, Electronic) with Complexity/Loudness X/Y pad, fills, kit-piece toggles, and one-click conversion to editable MIDI regions.
- **External MIDI Track:** Hardware MIDI output port/channel routing, Bank Select, Program Change, and sample-scheduled MIDI Clock / MTC output.
- **Aux, Bus, VCA & Master Strips:** Dedicated Aux return strips, 8 pre/post sends per strip, VCA fader groups, and Stereo Out / Master strips with integrated loudness metering.
- **Inspector & Library:** Dual Region Inspector (Quantize, Velocity, Transpose, Clip Gain, Fades, Pitch/Time) and Track Inspector (Instrument, MIDI FX, 15 Audio FX slots, 8 Sends, live EQ thumbnail), paired with a searchable Sound Library and Patch browser.

### 1.3 Audio Recording, Editing & Comping
- **Recording:** Multi-track simultaneous capture (`32-bit float` crash-safe WAV writer), Punch-in/out, Autopunch, Replace mode, Take Folders, Quick Swipe Comping, and Capture Recording.
- **Non-Destructive Region Editing:** Clip Gain (`+6 / -24 dB`), beat-based Fade In/Out (`Linear`, `Equal Power`, `S-Curve`), automatic Overlap Crossfades, Reverse, Non-Destructive Normalize, Split at Playhead / Silence / Transient, and Option/Alt-drag duplication.
- **Audio Editors:** Waveform Track Editor with transient detection, Strip Silence, and destructive Audio File Editor.
- **Marquee Selection Tool:** Precision time-slice selection across tracks with `Cmd/Ctrl+T` split, drag, delete, and selection-based processing.
- **Project Audio Browser:** Asset pool management, sample rate conversion, and file optimization.

### 1.4 Time-Stretching & Per-Note Pitch Editing
- **Time-Stretching Modes:** Slicing (percussion/transients), Rhythmic, Monophonic (vocals/bass via WSOLA/PSOLA), Polyphonic, Tempophone, and Speed (linked time/pitch).
- **Interactive Stretch Markers:** Drag transient/warp markers directly on audio regions, quantize audio to the project grid, and extract/apply Groove Track timing.
- **Per-Note Pitch Editor:** Piano-roll melodic pitch and formant editor for monophonic audio regions with cent-level pitch correction, entry/exit pitch drift curves, vibrato depth, formant shift, per-note gain leveling, scale quantization, and pure-Rust PSOLA audio-to-MIDI extraction.

### 1.5 MIDI Sequencing & Editors
- **Piano Roll:** Quantize (with Swing, Strength, Q-Swing), Velocity tool, Brush tool, Time Handles (stretch MIDI), Note Repeat, Humanize, MIDI Draw (CC automation lanes), Scale Quantize, Flam, and Velocity/Pitch note coloring.
- **Score Editor:** Grand staff notation with clefs, 30 key signatures, time signatures, accidentals, rests, ties, lyrics, chord symbols, tablature, and MusicXML export.
- **Step Sequencer:** Pattern regions (`1–64` steps, up to 16 rows) with per-step Velocity, Gate, Tie, Chance (seeded probability), Repeat (`1–4×`), playback direction (`Forward`, `Backward`, `Ping-Pong`, `Random`), and per-row automation.
- **Event List & Step Editor:** Raw MIDI event editor (Note, CC, 14-bit Pitch Bend, Aftertouch, hex SysEx entry) and multi-lane drum/CC step editor.
- **MIDI Transform & Pre-Instrument MIDI FX (8 Slots):** Fixed and conditional MIDI transforms plus real-time Arpeggiator, Chord Trigger, Modulator (LFO/Env to CC/Bend), Note Repeater, Randomizer, Scale Quantizer, Transposer, Velocity Processor, and sandboxed Rhai **Scripter**.
- **MIDI 2.0 & MPE:** Per-note Pitch Bend, Pressure, Timbre, and high-resolution controllers.

### 1.6 Built-In Instruments & Sound Engines
- **Multisample Player (`.exs` & `.sfz`):** Multi-zone sample playback with 4-point cubic resampling, velocity layering, round-robin, choke groups, ADSR envelopes, LFOs, modulation matrix, and automatic sample resolution across project folders and the user Sound Library.
- **Quick Sampler & Drum Machine:** Drag-and-drop audio slicing (Transient, Beat, Equal), One-Shot/Classic/Loop modes, plus a 16/48-pad Drum Machine with synthesized drum voices (Kick, Snare, Hats, Toms, Claps), custom sample loading, and choke groups.
- **Acoustic Drum Kits:** Multi-mic acoustic drum kits with Close, Overhead, and Room bleed controls.
- **Synthesizers:**
  - **Subtractive Synthesizer:** 48-voice virtual analog polyphonic synth with resonant filters and modulation routing.
  - **Vector & Wavetable / FM Synths:** Multi-oscillator vector synthesis with X/Y morph pad, 4-mode Retro Synth (Analog, Sync, Wavetable, FM), and multi-operator FM synth.
  - **808 Bass Synthesizer:** Pure sine-drop sub-bass synth (`bass808.rs`) with glide, tune, and harmonic saturation.
  - **SoundFont Player:** Built-in `.sf2` player with velocity layers and filter envelopes.
- **Physically Modeled & Vintage Keyboards:**
  - **Electric Piano:** Reed-based physical model (Wurlitzer 200A / Rhodes style) with pickup saturation, tremolo, and tone bark.
  - **Tonewheel Organ:** Additive drawbar organ (`16'` to `1'`) with percussion harmonics, key click, and rotary speaker simulation.
  - **Clavinet & Tape Sampler:** Plucked-string physical model and multi-bank tape instrument.
  - **String / Resonator Physical Modeling:** Bowed/plucked waveguide resonator with Object/Material morphing.

### 1.7 Built-In DSP Effects & Metering
- **Dynamics:**
  - **Multi-Topology Compressor:** 6 circuit models (Clean Digital, Studio VCA, Studio FET, Vintage VCA, Vintage FET, Vintage Opto) with threshold, ratio, knee, attack/release, auto-gain, sidechain filter, and wet/dry mix.
  - **Lemon Studio Dynamics:** Lookahead Peak Limiter, Soft Clipper, Fast Distortion, and Transient Processor.
  - **Multi-Band Compressor, Noise Gate, Expander & De-Esser.**
- **Equalizers:**
  - **Channel EQ & Linear Phase EQ:** 8-band parametric EQ with pre/post FFT spectrum analyzer and `4×` oversampling.
  - **Pult EQ & Console/Graphic EQs:** Pultec-style tube EQ (low boost/attenuation + high shelf saturation), 6-band Multi-Band EQ, proportional-Q graphic EQ, and EQ matching.
- **Reverb & Delay:**
  - **Convolution Reverb:** Stereo convolution engine with built-in IRs, custom `.wav`/`.aiff`/`.flac` impulse response loading, volume/filter envelopes, crossover, and output EQ.
  - **Algorithmic Reverb & Delays:** Multi-model algorithmic reverb (Hall, Room, Plate), Tap Delay Designer (up to 26 taps), Tape Delay (flutter/saturation), Stereo Delay, Echo, and Sample Delay.
- **Modulation, Distortion & Amp Modeling:**
  - **SoundCraft Modulation:** Zero-alloc Bode Frequency Shifter and LFO Auto-Pan, plus Chorus, Ensemble, Flanger, Phaser, Tremolo, Rotary Vibrato, and Stereo Spreader.
  - **Guitar & Bass Amp Designer + Pedalboard:** 5 tube amplifier models, multi-stage asymmetric valve clipping, cabinet/mic position modeling, Overdrive, Bitcrusher, Phase Distortion, and modular stompbox chain.
- **Loudness & True Peak MultiMeter:**
  - Dedicated analyzer (`Window > MultiMeter`) implementing **ITU-R BS.1770-4 / EBU R128** K-weighted LUFS metering (Momentary `400 ms`, Short-term `3 s`, Gated Integrated LUFS with `-14 LUFS` streaming and `-23 LUFS` broadcast markers) and `4×` oversampled polyphase windowed sinc True Peak detection.
  - Live Integrated LUFS and True Peak readouts embedded directly in Master and Stereo Out mixer strips.

### 1.8 Mixing, Automation & Plugin Hosting
- **Mixer:** Channel strips with 64-bit summing, `0` to `+6 dB` faders, Pan/Stereo Balance, 15 Audio FX slots, 8 pre/post Sends, Sends on Faders, VCA assignment, Mix Groups (Volume, Pan, Mute, Solo linking), EQ thumbnail, and Channel Strip presets.
- **Automation:** Track and Region automation (`Off`, `Read`, `Touch`, `Latch`, `Write`, `Trim`) with linear, exponential, and Bezier curves across fader, pan, sends, and every plugin parameter.
- **Plug-in Delay Compensation (PDC) & Low Latency Mode:** Automatic sample-accurate delay compensation across tracks, buses, and lookahead effects, plus Low Latency Mode for live tracking.
- **Third-Party Plugin Hosting (`prowave-plugin-host`):** Native CLAP, VST3, LADSPA, and AU (macOS) instrument and effect hosting with out-of-process crash isolation on Linux/macOS, dynamic X11/XWayland native plugin windows via `libloading`, and a searchable Plug-in Manager.

### 1.9 Export, File Interoperability & Control
- **Offline & Real-Time Bounce:** Headless freewheeling offline mixdown at CPU memory speeds (up to `500×` real-time), Bounce in Place, Bounce & Join, and concurrent multi-track stem export (`Export All Tracks as Audio Files…`).
- **Bit Depths & Dithering:** 16-bit, 24-bit, and 32-bit float WAV/AIFF/FLAC/MP3 export with Triangular Probability Density Function (**TPDF**) dithering (automatically bypassing digital silence) and atomic `.part` writes.
- **Project & Format Interoperability:** Import support for `.logicx` project bundles, `.band` projects (audio), Hydrogen `.h2song` drum kits/patterns, REX loops, Standard MIDI Files (`.mid`), Guitar Pro, and MusicXML scores.
- **Built-In Model Context Protocol (MCP) Server:** Zero-alloc real-time safe MCP bridge over local TCP (`127.0.0.1:38420`) and `--mcp-stdio` CLI mode exposing live project resources and transport/track/mixing automation tools.
- **Hardware & Key Commands:** MIDI Learn controller assignments, Mackie Control support, Smart Controls macro mapping, and customizable key command presets.

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

### Workspace Layout

| Crate / Directory | Responsibility |
| --- | --- |
| `crates/prowave-app` | Desktop UI (`egui`, Liquid Glass WGSL shaders, `phosphor-egui`), MCP client, in-app updater |
| `crates/prowave-engine` | Lock-free audio graph, 64-bit summing, PDC, synths, DSP FX, offline bounce, EBU R128 metering |
| `crates/prowave-project` | `.douri` / `.prowave` archive serialization, media pool, hostile-file hardening bounds |
| `crates/prowave-plugin-host` | Out-of-process & in-process CLAP, VST3, LADSPA, and AU hosting + X11 windowing |
| `crates/prowave-logicx-import` | Apple `.logicx` bundle & `ProjectData` parser |
| `crates/prowave-garageband-import` | `.band` project importer |
| `crates/prowave-exs` | `.exs` multisample instrument parser |
| `crates/prowave-sfz` | `.sfz` multisample instrument parser |
| `crates/douri-installer` | Cross-platform `egui` self-extracting installer (Windows `.exe` & Linux `.AppImage`) |
| `xtask` | Build, test, `rt-check`, `wasm-check`, and release packaging automation |

### Real-Time Safety Invariants

* **Zero Audio-Thread Allocation:** `#![forbid(alloc)]` and `assert_no_alloc` enforced across the entire audio callback path—no heap allocations, no mutexes, no blocking I/O, and no `println!`.
* **Lock-Free Parameter Smoothing:** Atomic parameter updates paired with 1-pole smoothers (`ParamSmoother`) to eliminate zipper noise.
* **Deferred Reclamation:** Old audio graphs, songs, and tempo maps are returned via a bounded credit-based garbage queue to be dropped safely off the audio thread.
* **Graphics Pipeline:** Native Vulkan GPU rendering via `eframe` (`Renderer::Wgpu`) with Liquid Glass WGSL floating capsules and automatic fallback to OpenGL (`Renderer::Glow`).

---

## 3. Release Status & Phased Roadmap

### Shipped in `v0.1.0-alpha.1` (Current `main`)

* [x] **Engine & Audio Safety:** Lock-free `rtrb` + triple-buffer architecture, 64-bit summing bus, automatic PDC, memory-mapped PCM streaming (`1,000+` stereo track benchmark verified), `IAudioClient3` low-latency WASAPI (`12 ms`), Linux real-time thread priority, and `cargo xtask rt-check`.
* [x] **Arranging & Editing:** Multi-track arranger, Marquee selection, gap-free snap grid, Option/Alt-drag duplication, clip gain, beat-based fades, overlap crossfades, non-destructive normalize, reverse, autosave/crash recovery, Tempo/Signature/Marker global tracks, and Per-Note Pitch Editor (PSOLA).
* [x] **MIDI & Sequencer:** Piano Roll, Step Sequencer (Chance/Repeat/Tie/direction), Score Editor + MusicXML export, Event List (with hex SysEx), MIDI Transform, 8-slot MIDI FX chain (including sandboxed Rhai Scripter), and hardware MIDI Clock / MTC sync.
* [x] **Instruments & DSP:** `.exs`/`.sfz` Multisample Player, `.sf2` SoundFont Player, 808 Bass Synth, Subtractive Synth, Electric Piano, Tonewheel Organ, Drum Machine, Amp Designer, Convolution Reverb, Pult EQ, Multi-Band EQ/Compressor, Lemon Studio Dynamics, SoundCraft FX, and ITU-R BS.1770-4 / EBU R128 MultiMeter.
* [x] **Bounce & Interop:** Faster-than-realtime offline bounce (`500×`), TPDF dithering, concurrent multi-track stem export, and `.logicx` / `.band` / `.h2song` / `.mid` importers.
* [x] **Hosting, MCP & Packaging:** CLAP/VST3/LADSPA/AU plugin hosting with X11/XWayland windows, built-in MCP server, in-app GitHub release updater, and custom `egui` installers for Windows (`.exe`), Linux (`.AppImage`), `.rpm`, and portable archives.

### Phase 1: Stability, Hardware Audio & Core Workflow Polish (`v0.1.x`)

* [ ] **Interactive Audio Device & Buffer Selector:** Live output/input device switching and buffer size configuration in `Project Settings > Audio` with seamless engine stream restart.
* [ ] **Audio & Pattern Split / Join:** Sample-accurate split, join, and trim tools for audio, Pattern, Drummer, and take-folder regions (matching MIDI region parity).
* [ ] **Native PipeWire & JACK Backends:** Direct low-latency PipeWire and JACK audio/MIDI clients on Linux alongside ALSA (`< 8 ms` round-trip target).
* [ ] **Complete Control Bar & Menu Parity:** Wire remaining grayed-out menu entries (`Save as Template`, `Project Alternatives`, `Show Chord / Arrangement Track`, `Pre-Fader Metering`, `Low Latency Mode` toggle) so zero visible controls are inert.
* [ ] **Windows Out-of-Process Plugin Sandboxing & Native Windows:** Port out-of-process crash isolation and embedded plugin editor windows to Windows (`HWND`) and Wayland native surfaces.

### Phase 2: Advanced Synthesis, Sampling & Effects (`v0.2.0`)

* [ ] **Visual Multisample Zone & Keymap Editor:** Drag-and-drop key/velocity zone mapping UI for the `.exs`/`.sfz` sampler and Quick Sampler auto-slicing (`Convert to New Sampler Track`).
* [ ] **Vector & Spectral/Granular Synth Engines:** Multi-oscillator vector synth with X/Y envelope morphing and dual-source granular/additive synth engine.
* [ ] **Full Channel EQ & Compressor Circuit Models:** 8-band Channel EQ with integrated pre/post FFT curve overlay and selectable VCA/FET/Opto compressor topologies with external sidechain routing.
* [ ] **Algorithmic Reverb & Tap Delay Designer:** Multi-model algorithmic reverb (Hall, Room, Plate) and visual multi-tap delay editor.
* [ ] **Two-Handle Bezier Automation & Output Strip Lanes:** Full Bezier curve handles on automation segments and automation lanes for Stereo Out / Master strips.

### Phase 3: Live Performance, Smart Controls & Scoring (`v0.3.0`)

* [ ] **Live Loops Performance Grid:** Clip/scene launching matrix with quantized triggers, one-shot/loop modes, performance recording into the linear arranger, and X/Y Remix FX.
* [ ] **Smart Controls & Macro Mapping:** 8 auto-mapped macro knobs per patch with multi-parameter range scaling and MIDI Learn.
* [ ] **Chord Track & Arrangement Sections:** Global Chord Track driving Session Players and scale-aware editors, plus Arrangement Track section reordering.
* [ ] **Score Engraving & Video Track:** Beamed notation, multi-voice staves, tablature, and floating Video Track with SMPTE sync and audio-to-movie export.

### Phase 4: WebAssembly, ML Assisted Tools & Spatial Audio (`v0.4.0+`)

* [ ] **WebAssembly & AudioWorklet Target:** Compile `prowave-engine` to `wasm32` with `SharedArrayBuffer` lock-free ring buffers, OPFS project storage, and a browser-based Web Remote.
* [ ] **Local ML Stem Separation & Mastering Assistant:** Optional ONNX (`ort`/`tract`) stem separation (Drums, Bass, Vocals, Other) and reference-assisted loudness/EQ balancing.
* [ ] **MPE & MIDI 2.0 Property Exchange:** Full per-note pitch/pressure/timbre expression lanes inside the Piano Roll.
* [ ] **Spatial Audio / Multi-Channel Bed Mixing:** Surround/7.1.4 bed routing, 3D panner UI, and ADM BWF export.

### Phase 5: Eclipsa Audio (IAMF) Spatial Mixing & UI (`v0.5.0+`)

* [ ] **Spatial Engine & Format Support (`#![forbid(alloc)]`):**
  * Open-source Eclipsa Audio / IAMF spatial rendering pipeline (BSD-3-Clause Clear / royalty-free AOMedia spec, keeping Dōri strictly MIT / Apache-2.0).
  * Pre-allocated 7.1.4 Bed accumulator, 3D Object VBAP/elevation gain tables, and real-time Binaural HRIR downmixer with zero audio-thread heap allocations.
  * Offline faster-than-real-time Bounce export to `.iamf` alongside stereo/surround WAV and FLAC.
* [ ] **Mixer & Track Header Spatial Controls:**
  * Channel strip `Output` selector supporting `Stereo Out`, `Surround` (7.1.4 Bed), and `Object` (3D Audio Object).
  * Inline Mixer `Pan` widgets and Track Header badges: `4×4` 3D Object XY grid + vertical Elevation slider for `Object` tracks, and circular speaker-ring radar + LFE level slider for `Surround` bed tracks.
  * Floating **3D Object Panner** and **Surround Panner** windows with coordinate/spread controls, per-speaker mute toggles (for isolated LFE aux bed routing), and user presets (`Save As…`, `Save As Default`).
* [ ] **Spatial Automation & Multi-Channel Metering:**
  * Automation parameter pop-up under `Main ▸` exposing `Object Position Left/Right`, `Object Position Back/Front`, `Object Position Elevation`, `Object Size`, and `Surr. Spread`.
  * Master channel strip `Eclipsa` renderer slot (monitoring modes: `Binaural Headphones`, `4.0 Quad`, `5.1`, `7.1.4`) and a 12-channel `7.1.4` (`Ls`, `Lm`, `L`, `C`, `R`, `Rm`, `Rs`, `LFE`, `Lts`, `Lt`, `Rt`, `Rts`) peak/RMS **Level Meter** window.

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
