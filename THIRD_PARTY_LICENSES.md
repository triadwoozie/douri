# Third-Party Licenses & Attributions

Dōri builds upon and incorporates open-source software, artwork, and resources under free and open-source licenses.

---

## 1. Upstream Architecture & DAW Core
- **Project**: Pro-Wave ([fabox-os/Pro-Wave](https://github.com/fabox-os/Pro-Wave))
- **Authors**: Pro-Wave contributors
- **License**: GNU General Public License v3.0 or later (GPL-3.0-or-later)
- **Description**: Real-time lock-free audio engine, egui user interface, mixer, and DSP effects.

- **Project**: Yuri's Version ([yuribeats/prowave](https://github.com/yuribeats/prowave))
- **Authors**: Yuri Beats contributors
- **License**: GNU General Public License v3.0 or later (GPL-3.0-or-later)
- **Description**: Arranging tools (marquee selection, snap grid, option-drag), crash recovery, and 808 bass synthesizer.

- **Project**: Soundcraft ([storytold/soundcraft](https://github.com/storytold/soundcraft))
- **Authors**: storytold contributors
- **License**: MIT License / Apache License 2.0
- **Description**: Offline mixdown architecture, TPDF dither generator, ITU-R BS.1770 / EBU R128 metering, General MIDI synthesized drum voices, and subtractive synthesizer preset DSP mappings.

- **Project**: Ardour ([Ardour](https://github.com/Ardour/ardour))
- **Authors**: Ardour contributors
- **License**: GNU General Public License v2.0 or later (GPL-2.0-or-later)
- **Description**: Hardware MIDI patch definition library (.midnam) and Scala microtonal tuning references.

- **Project**: Transit ([transit-audio/transit](https://github.com/transit-audio/transit))
- **Authors**: Transit audio contributors
- **License**: MIT License OR Apache License 2.0
- **Description**: Protocol-level Windows ASIO and low-latency WASAPI backend abstractions, PipeWire library loader discovery, crash-safe privacy-redacted diagnostics reporting (`--report`), loopback round-trip latency calibrator, physical modeling Modal Voice synth, and soft-knee dynamics Compressor.

- **Project**: LMMS ([LMMS/lmms](https://github.com/LMMS/lmms))
- **Authors**: Tobias Doerffel, Andreas Brandmaier, and LMMS developers
- **License**: GNU General Public License v2.0 or later (GPL-2.0-or-later)
- **Description**: TripleOscillator (TriOsc) 3-oscillator synth architecture, BitInvader-inspired WaveDraw customizable single-cycle wavetable synth, Peak Follower & LFO modulators, and CC-0 / Public Domain single-cycle geometric waveforms (`shapes/`).

---

## 2. Icon & Graphic Assets
- **Project**: Phosphor Icons ([phosphoricons.com](https://phosphoricons.com))
- **Library**: `phosphor-egui`
- **Copyright**: (c) Helena Zhang & Tobias Fried
- **License**: MIT License
- **Description**: Clean vector iconography used throughout Dōri's menus, headers, toolbars, and studio controls.

- **Project**: audio-ui ([ouestlabs/audio-ui](https://github.com/ouestlabs/audio-ui))
- **Copyright**: (c) 2025 Ouest Labs
- **License**: MIT License
- **Description**: Reference implementations for IEC VU/PPM meter ballistics, 45-degree Mid/Side stereo goniometer, interactive ADSR breakpoint envelope, and velocity-metered drum pad feedback.

- **Project**: Eclipsa Studio ([google/eclipsa-studio](https://github.com/google/eclipsa-studio))
- **Copyright**: (c) Google LLC
- **License**: Apache License 2.0
- **Description**: Eclipsa Audio / IAMF 7.1.2 & 7.1.4 spatial audio architecture, OAR monitoring layouts, and binaural spatialization reference.

- **Project**: GenerativeMIDI ([joshband/GenerativeMIDI](https://github.com/joshband/GenerativeMIDI))
- **Copyright**: (c) 2025 GenerativeMIDI Contributors
- **License**: MIT License
- **Description**: Euclidean rhythm distribution, swing, ratchet, and LFO modulation engines ported for Dōri's Session Players.

- **Project**: AccoMontage2 ([billyblu2000/accomontage2](https://github.com/billyblu2000/accomontage2))
- **Copyright**: (c) 2021 Billy Yi
- **License**: MIT License
- **Description**: Chord voicing and accompaniment style taxonomy for Keyboard Session Players.

- **Artwork**: Dōri 3D Studio App Icon & Custom Artwork
- **Creator**: triadwoozie
- **Description**: Blender-rendered high-resolution application icon and 3D studio visual assets.

---

## 3. Rust Ecosystem Dependencies
Dōri utilizes various open-source Rust crates including `egui`, `eframe`, `cpal`, `wgpu`, `serde`, `zip`, `ureq`, `rustls`, and `ring`. Each library is governed by its respective open-source license (MIT, Apache-2.0, or BSD). Full license terms for each crate are distributed with their source distributions on crates.io.
