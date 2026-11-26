### Dōri v0.1.0-alpha.10

Welcome to Dōri (道理) v0.1.0-alpha.10, a pure-Rust cross-platform digital audio workstation.

#### Downloads & Release Packages

| Package | Platform | Architecture | Details |
|---|---|---|---|
| `Dori-Setup-0.1.0-alpha.10-x86_64.exe` | Windows | x86_64 | Guided dark minimal setup installer |
| `Dori-0.1.0-alpha.10-win64-portable.zip` | Windows | x86_64 | Portable zero-install zip archive |
| `Dori-Setup-0.1.0-alpha.10-x86_64.AppImage` | Linux | x86_64 | Standalone dark minimal installer |
| `Dori-0.1.0-alpha.10-linux-x86_64-portable.tar.gz` | Linux | x86_64 | Portable standalone tarball |
| `dori-0.1.0-alpha.10-1.x86_64.rpm` | Fedora / RHEL / openSUSE | x86_64 | Native RPM package with desktop integration |
| `SHA256SUMS.txt` | All | Checksums | Cryptographic SHA-256 integrity verification |

## What's new in 0.1.0-alpha.10

### Stem Separation & Studio Workflow
- **Local AI Stem Separation & Stem Splitter Sheet**: Integrated local AI stem separation (with settings wiring and a clean Logic Pro-style Stem Splitter sheet) alongside the Windows startup deadlock fix (`#5`).
- **Command Palette & Diagnostics**: Added quick Command Palette navigation and privacy-redacted Diagnostics reporting.

### Metering, Envelopes & Drum Pads (`ouestlabs/audio-ui`)
- **MultiMeter Goniometer & IEC Ballistics (`multimeter.rs`, `widgets.rs`)**: Added a 45°-rotated Mid/Side stereo vectorscope with trail history alongside L/R IEC VU (`300 ms` rise, `0.81` damping) and Type I PPM (`5 ms` attack, `20 dB / 1.5 s` release) ballistic meters.
- **Interactive Sampler ADSR Envelope (`sampler_ui.rs`)**: Wired `daw_adsr_envelope` with draggable Attack, Decay/Sustain, and Release breakpoints bound directly to `Zone` envelope parameters.
- **Velocity-Metered Drum Pads (`drum_machine_ui.rs`)**: Added vertical click-position velocity mapping and smooth decaying `daw_pad_meter_glow` feedback across all 16 pads.

### UI Polish & Credits
- **Modulation Drawer & Control Bar Cleanup**: Relocated `A/B Diff` into the **Collaborate** window and `Modulation` into **Smart Controls**, and fixed the bottom Modulation Matrix drawer layout.
- **Attributions**: Credited `ouestlabs/audio-ui` (MIT License) in `README.md`, `THIRD_PARTY_LICENSES.md`, and **About Dōri**.

Licensed under the GNU General Public License v3 or later.
