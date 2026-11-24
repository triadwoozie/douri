# Dōri — Multi-Source Architecture Audit & Implementation Plan

**Date:** November 2026  
**Target:** Linux-First (Fedora/Nobara, PipeWire/ALSA, Wayland/X11) + Tier-1 Windows Cross-Builds (`cargo-zigbuild`)  
**Base Repository:** `fabox-os/Pro-Wave` (GPL-3.0-or-later)  
**Donor & Reference Sources:**
- `yuribeats/prowave` (GPL-3.0-or-later)
- `storytold/soundcraft` (MIT / Apache-2.0)
- `Ardour` (GPL-2.0-or-later)
- Logic Pro Documentation (Clean-Room Functional Spec Only)

---

## 1. Executive Summary & Architectural Tenets

**Dōri** is an open-source digital audio workstation implemented in pure Rust + `egui`, engineered for low-latency music production, deep Logic Pro workflow parity, and uncompromising real-time audio thread discipline.

### Core Non-Negotiables (from `AGENTS.md`)
1. **Zero Allocations on Audio Thread (`assert_no_alloc`):**
   - The audio rendering callback (`process()`) in `prowave-engine` and DSP crates must never allocate, deallocate, acquire blocking locks, or perform filesystem/socket I/O.
   - Lock-free ringbuffers (`rtrb`) and triple-buffering are the sole permitted inter-thread communication mechanisms.
   - Memory must be pre-allocated during graph configuration.
2. **Linux-First & Cross-Platform Integrity:**
   - Linux is primary: native PipeWire/ALSA support via `cpal`, pure Wayland support with optional X11 fallback (`winit` / `eframe`), and zero hard dependencies on macOS frameworks.
   - Windows cross-compilation via `cargo-zigbuild` and GitHub Actions CI must remain completely green.
3. **Clean-Room Logic Pro Parity:**
   - Apple documentation (`references/docs/*.pdf`) is used strictly for parameter range alignment, control scaling, and signal-flow modeling. No proprietary code or assets are copied.

---

## 2. Upstream Audit: `fabox-os/Pro-Wave` (Baseline Inventory & Gaps)

### Working Baseline Features (Verified in 0.3.0)
- **Engine Core:** Lock-free transport, tempo/meter maps, 64-bit float summing, plug-in delay compensation (PDC).
- **Instruments:**
  - `.sf2` SoundFont player via `rustysynth` (`crates/prowave-engine/src/soundfont.rs`).
  - Physical/analog modeled Electric Piano (`crates/prowave-engine/src/epiano.rs`).
  - Additive Vintage B3 Organ with 9 drawbars, key click, and Leslie speaker emulation.
  - Drum Machine with sample pad triggers (`crates/prowave-engine/src/drum_machine.rs`).
- **DSP Effects:**
  - `Pult EQ` (Pultec EQP-1A / MEQ-5 style tube equalizer).
  - Multi-Band Compressor (4-band with linkable crossovers).
  - Space Designer (convolution reverb with envelope/filter shapes, predelay, reverse, length/size stretching, and mastering presets).
  - Amp Designer & Bass Amp Designer.
  - Channel EQ (8-band), Compressor (vintage circuit modes), Tape Delay, Chorus, Phaser, Noise Gate, Limiter.
- **Arranging & Editing:**
  - Split at Playhead and Join for Audio, Pattern, Drummer, and take-folder regions.
  - Automation curves (Read, Touch, Latch, Write).
  - Score editor with MusicXML export.
- **Import Formats:**
  - Logic Pro `.logicx` bundle metadata & ProjectData parsing (`prowave-logicx-import`).
  - GarageBand `.band` audio project import (`prowave-garageband-import`).
  - Hydrogen `.h2song` sessions (`prowave-hydrogen-import`).
  - SFZ & EXS24 parsing (`prowave-sfz`, `prowave-exs`).

### Critical Baseline Gaps on Linux
| Area | Gap / Limitation in Upstream | Impact | Recommended Solution |
| :--- | :--- | :--- | :--- |
| **Instrument Hosting** | `prowave-plugin-host` only hosts Audio FX; instrument plugins (MIDI in -> Audio out) are explicitly not supported. | Users cannot host VST3/CLAP synth plugins. | Adapt instrument hosting from `yuribeats` & `soundcraft`. |
| **Plugin UI Windows** | Plugin GUI window embedding is only implemented for macOS (`mac.rs`). Linux (X11 `xcb`/Wayland) and Windows (`HWND`) are stubs. | Opening a third-party VST3/CLAP shows generic parameter sliders rather than the plugin's GUI. | Implement X11 socket embedding / child window handling. |
| **Bounce Architecture** | Realtime-only bounce loop in `run_bounce` (audio plays while bouncing to file). | Slow mixdowns, cannot perform 10x–50x faster-than-realtime offline export. | Port faster-than-realtime offline bounce engine from `soundcraft`. |
| **Dithering & Metering** | Missing true-peak intersample overs detection and EBU R128 integrated/momentary LUFS metering. | Mastering workflow incomplete; export lacks TPDF dither. | Port EBU R128 meter and dither algorithms from `soundcraft`. |
| **Flex Pitch UI** | Pitch lane is stubbed in `flex_edit.rs`; per-note pitch correction grid not interactive. | Cannot do Melodyne/Logic-style note pitch/drift/formant tweaking. | Adapt Pitch Editor from `yuribeats`. |

---

## 3. Donor Audit: `yuribeats/prowave` (`upstream/main..yuribeats/main`)

`yuribeats` contains 5 substantial commits with significant additions, but also major breaking subtractions that must **NOT** be merged blindly.

### Commit Inventory & Classification

#### 1. Commit `9246da8` ("Pro Wave Mac 0.1.0")
- **Contents:** Audio Unit hosting, VST3 hosting, Ableton-style automation, Lemon Studio effects (distortion, limiter, clipper, transient processor), Pro Wave Tune autotune, SP-303 Sampler.
- **Audit Assessment:** **Needs Adaptation**.
  - *Keep:* Lemon FX suite (`crates/prowave-engine/src/native_fx/lemon_*.rs`), pitch tracker (`tune/tracker.rs`), transient processor. Pure Rust DSP, realtime-safe.
  - *Skip:* Hard macOS dependencies (`objc2`, `block2`, `coremidi`).
  - *Action:* Port Lemon DSP modules into `crates/prowave-engine` without macOS-only window glue.

#### 2. Commit `d861069` ("Pro Wave (Yuri's Version) 0.1.1")
- **Contents:** Genre/artist templates removed, rebrand.
- **Audit Assessment:** **Skip**.

#### 3. Commit `7b5a2e5` ("Pro Wave (Yuri's Version) 0.1.2")
- **Contents:** Option-drag copy, timing fixes, loop drag. **Removed Sampler, SoundFonts, Drum Machine, and EPiano!**
- **Audit Assessment:** **Needs Adaptation**.
  - **CRITICAL WARNING:** `yuribeats` stripped out `sampler.rs`, `sf2.rs`, `drum_machine.rs`, and `epiano.rs`. Merging this commit would destroy Pro-Wave's built-in instruments!
  - *Keep:* Option-drag region copy in `tracks_area.rs`, snap grid adjustments.
  - *Skip:* Any deletion of sampler or synth instruments.

#### 4. Commit `d53c0fa` ("Pro Wave (Yuri's Version) 0.1.3")
- **Contents:** `Window > Get Free Plug-ins` script installer, engine edit-refusal fixes, audio-thread allocation counting (`rt_alloc.rs`).
- **Audit Assessment:** **Needs Adaptation**.
  - *Safe Cherry-Pick:* `crates/prowave-engine/src/rt_alloc.rs` and `rt_alloc_counting.rs` (realtime allocation counter that tracks debug heap allocations).
  - *Needs Adaptation:* "Get Free Plug-ins" shell script assumes macOS paths (`/Library/Audio/Plug-Ins/`). Needs Linux paths (`~/.vst3`, `~/.clap`).

#### 5. Commit `9ac3160` ("Pro Wave (Yuri's Version) 0.2.0")
- **Contents:** Logic-style arranging tools (Marquee tool, 12 tools menu, crossfades, shuffle modes), 808 synth (`bass808.rs`), CHIMERA voice engine (`crates/chimera`), Vocal Top Line/Singer, autosave & crash recovery (`recovery.rs`), control interface & MCP (`prowavectl`).
- **Audit Assessment:** **High Value — Selective Porting**.
  - **Safe Cherry-Pick / Direct Port:**
    - `crates/prowave-app/src/marquee.rs` (Logic-style marquee selection and split).
    - `crates/prowave-app/src/snap_grid.rs` (Gap-free snap grid).
    - `crates/prowave-app/src/recovery.rs` (Project autosave & crash recovery copies).
    - `crates/prowave-engine/src/bass808.rs` (808 Bass instrument: sine/square layers, glide, envelope).
    - `crates/prowavectl` (CLI & stdio MCP server for headless automation).
  - **Needs Adaptation / Caution:**
    - `crates/chimera` & `crates/prowave-engine/world/`: Uses WORLD vocoder C++ library compiled via `cc` in `build.rs`. Could break Windows `cargo-zigbuild` if C++ compiler toolchain is mismatched. Should be gated behind an optional feature flag (`--features vocal-engine`).

---

## 4. Donor Audit: `references/soundcraft` (MIT / Apache-2.0)

`soundcraft` is a production-grade, highly modular DAW codebase with clean abstractions.

### Top Capabilities to Port into Dōri

1. **Faster-Than-Realtime Offline Bounce (`soundcraft/crates/dsp/src/offline.rs`)**
   - *Status:* **Implement / Port (Priority 1)**.
   - *Why:* Pro-Wave's bounce plays the project in realtime at 1x speed. `soundcraft` provides a headless render loop that pulls blocks from the mix engine as fast as CPU cores can compute (e.g. 20x–50x realtime).
   - *Features:* Triangular PDF (TPDF) dither, bit-depth truncation (16-bit, 24-bit, 32-bit float), normalization to 0 dBFS or target peak, and stem bounce generator.

2. **EBU R128 & Intersample True-Peak Metering (`soundcraft/crates/dsp/src/meter.rs`)**
   - *Status:* **Implement / Port (Priority 1)**.
   - *Why:* Meets ITU-R BS.1770 / EBU R128 standards for mastering. Includes:
     - K-weighting pre-filter and RLB weighting filter.
     - Momentary (400ms), short-term (3s), and integrated LUFS calculation with gating.
     - 4x oversampled polyphase FIR filter for intersample True Peak detection (`true_peak_db()`).

3. **Missing Specialized DSP Blocks (`soundcraft/crates/dsp/src/plugins/`)**
   - *Status:* **Port Selectively**.
   - `freq_shifter.rs`: Bode frequency shifter (Hilbert transform via allpass cascade).
   - `auto_pan.rs`: LFO-driven panning with stereo phase offset and pulse/triangle shapes.
   - `strip.rs`: Consolidated channel strip processing.

---

## 5. Donor Audit: `references/ardour` (GPL-2.0-or-later)

`ardour` is a mature, complex C++ DAW. GTK and complex C++ engine subsystems are **strictly skipped**.

### Lightweight Assets & Math to Extract
1. **Hardware MIDI Patch Name Definitions (`references/ardour/share/patchfiles/`)**
   - *Status:* **Asset Drop-in**.
   - Over 250 `.midnam` XML patch files covering Roland, Yamaha, Korg, Moog, Alesis, Access Virus, and Nord synthesizers.
   - Allows Dōri's External MIDI tracks to display patch/preset names by device manufacturer and model instead of raw MSB/LSB numbers.
2. **Scala Microtuning Math (`references/ardour/libs/ardour/scala_*.cc`)**
   - *Status:* **Port Math to Pure Rust**.
   - Parses `.scl` scale intervals (cents / ratio fractions) and `.kbm` keyboard mapping files into frequency lookup tables.
3. **Pan Laws & Biquad Math:**
   - Equal-power pan law curves (-3dB, -4.5dB, -6dB center attenuation) verified against Ardour's standard audio math.

---

## 6. Clean-Room Logic Pro Documentation Reference (`references/docs/*.pdf`)

The 4 Apple reference PDFs are located in `references/docs/` for clean-room functional alignment:

| Document | Page Count | Functional Domain & Clean-Room Lookup Scope |
| :--- | :---: | :--- |
| `logic-pro-mac-user-guide.pdf` | 1,349 | Core arranging workflow, Marquee tool behaviors, Flex Time algorithm definitions, Track Stack hierarchy (Folder vs Summing), Automation modes, Marker navigation, Shortcut defaults. |
| `logic-pro-mac-effects-user-guide.pdf` | 389 | Complete parameter specifications, min/max ranges, default values, and dB scales for Channel EQ, Compressor circuit models (Opto/FET/VCA), Space Designer parameters, Limiter, and Loudness Meter. |
| `logic-pro-mac-instruments-user-guide.pdf` | 774 | Synth architecture (ES2 mod matrix, Retro Synth oscillators), Sampler zone/group mapping rules, Vintage Electric Piano physical modeling parameters, Vintage B3 drawbar scales. |
| `logic-pro-mac-control-surfaces-support-guide.pdf` | 220 | Mackie Control Universal (MCU) protocol MIDI message format, rotary encoder response curves, 7-segment/LCD byte mapping for physical controller support. |

---

## 7. Icon Replacement Guide

To customize the branding icon across all desktop platforms, replace the icon files at these exact paths:

| File Path | Dimensions / Format | Purpose |
| :--- | :--- | :--- |
| `crates/prowave-app/assets/icon-256.png` | 256×256 RGBA PNG | Embedded inside the application binary; used as runtime window icon and taskbar icon on Linux/X11/Wayland. |
| `crates/prowave-app/assets/prowave.ico` | Multi-res Windows ICO (16 to 256px) | Embedded Windows `.exe` resource via `winresource` in `crates/prowave-app/build.rs`. |
| `dist/linux/io.github.triadwoozie.douri.png` | 512×512 PNG | Linux desktop icon installed to `$HOME/.local/share/icons/hicolor/512x512/apps/io.github.triadwoozie.douri.png`. |
| `dist/linux/douri.png` | 512×512 PNG | Backup/symlink for generic desktop packaging. |
| `dist/icons/prowave-1024.png` | 1024×1024 Master PNG | High-resolution master source used by scripts to generate `.ico`, `.icns`, and downscaled `.png` assets. |
| `dist/macos/prowave.icns` | macOS Apple Icon Image | macOS Application bundle icon (`Contents/Resources/`). |
| `docs/logo.png` | Web / Doc PNG | Documentation and repository README header banner. |

---

## 8. Master Decision & Implementation Plan

```mermaid
flowchart TD
    subgraph Phase 1: Arranging & Stability ["Phase 1: Arranging & Stability"]
        A1["yuribeats Marquee Tool (marquee.rs)"]
        A2["yuribeats Snap Grid (snap_grid.rs)"]
        A3["yuribeats Crash Recovery (recovery.rs)"]
        A4["yuribeats 808 Synth (bass808.rs)"]
    end

    subgraph Phase 2: Mastering & Bounce ["Phase 2: Mastering & Bounce"]
        B1["soundcraft Offline Bounce Engine (faster-than-realtime)"]
        B2["soundcraft TPDF Dither & Bit-Depth Truncation"]
        B3["soundcraft EBU R128 & True Peak Meter (meter.rs)"]
    end

    subgraph Phase 3: Instrument & Linux Host ["Phase 3: Instrument & Linux Host"]
        C1["CLAP / VST3 Instrument Hosting in Engine"]
        C2["Linux X11/Wayland Plugin Window Embedding"]
        C3["Ardour .midnam Hardware MIDI Patch Definitions"]
    end

    subgraph Phase 4: Vocal & Pitch ["Phase 4: Vocal & Pitch (Optional/Feature Gated)"]
        D1["yuribeats Pro Wave Tune (native_fx/tune)"]
        D2["CHIMERA Voice Engine & Syllables"]
        D3["WORLD Vocoder C++ (Feature Flag)"]
    end

    Phase 1: Arranging & Stability --> Phase 2: Mastering & Bounce
    Phase 2: Mastering & Bounce --> Phase 3: Instrument & Linux Host
    Phase 3: Instrument & Linux Host --> Phase 4: Vocal & Pitch
```

### Action Recommendations

1. **Immediate Next Step (Arranging & Safety):**
   - Adapt `marquee.rs`, `snap_grid.rs`, and `recovery.rs` from `yuribeats/main`. These are pure Rust `egui` + model enhancements that do not alter audio thread behavior.
   - Add `bass808.rs` to `crates/prowave-engine` for dedicated trap/hip-hop bass synthesis.
2. **Second Step (Bounce & Metering):**
   - Port `soundcraft`'s offline bounce and EBU R128 meter. This eliminates the #1 workflow frustration (realtime-only bounce) without risking real-time audio thread stability.
3. **Third Step (Linux Plugin Hosting):**
   - Expand `prowave-plugin-host` to accept MIDI note events for instrument hosting, and develop X11 child-window embedding so plugin GUIs open natively on Fedora/Nobara.
