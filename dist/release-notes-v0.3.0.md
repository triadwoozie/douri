# Dōri v0.3.0

**Dōri 0.3.0** introduces a high-performance **Vulkan (`wgpu`) graphics backend**, **Liquid Glass floating capsules and overlays** with dynamic WGSL refraction shaders, a refined **matte obsidian workspace**, redesigned library and mixer strips, and a **built-in Model Context Protocol (MCP) Server** with zero real-time audio-thread locking.

### Vulkan (`wgpu`) Backend & Liquid Glass Floating UI
- **Native Vulkan Hardware Acceleration**: `eframe` defaults to `Renderer::Wgpu` on Vulkan (Linux & Windows), delivering smooth, low-latency UI rendering with graceful fallback to Glow OpenGL.
- **Liquid Glass Pills & Frames**: Ported WGSL glass refraction and chromatic dispersion shader pipeline (`liquid_glass.rs`) wrapping floating utility capsules, dialog windows, and toolbar containers.
- **Matte Obsidian Workspace**: Deep obsidian palette (`#0D0F14` chrome canvas, `#13161E` panels, `#1A1E29` cards/capsules, `#252A37` borders, and `#00E5FF` electric cyan accents).
- **Appearance Settings**: Toggle glass blur/refraction shaders, UI scale factor, and GPU diagnostics from the Appearance settings tab.

### Floating Capsules, Track Headers & Professional Mixer
- **Capsule Top Bar**: Grouped into floating capsule pills: Left Project Info & Views, Center Transport Pill with recessed LCD readout, and Right Master Volume + Panels cluster with live MCP status indicator dot.
- **Library Browser Evolution**: Vertical collapsible icon rail (`Sound Library`, `Loops`, `Project Files`), rounded search pill with color-coded filter chips (`Synth`, `Bass`, `Drums`, etc.), and bottom audition preview bar.
- **Obsidian Track Headers**: Tactile `#1A1E29` cards with 28×28px instrument badges, pill M/S/R/I buttons, segmented green-amber-red LED level meters, and radial-arc pan dials.
- **Professional Channel Strips**: Mixer strips featuring radial-arc pan dials, tabular dB readouts in recessed cavities, segmented LED peak meters with dark segment gaps, tactile white fader caps with center grooves, and full-width track color nameplates.

### Built-in Model Context Protocol (MCP) Server
- **Dual Transport Architecture**: Embedded MCP server supporting both TCP listener on `127.0.0.1:38420` (enabled via Settings) and headless `--mcp-stdio` CLI flag.
- **Lock-Free Audio Safety**: Asynchronous non-blocking message passing bridge maintaining `cargo xtask rt-check` zero-allocation guarantees on the real-time audio thread.
- **Full DAW Tool & Resource Parity**:
  - **4 Live Resources**: `douri://project/state`, `douri://mixer/meters`, `douri://plugins/catalog`, `douri://library/index`.
  - **19 Control & Automation Tools**: Spanning transport control, track creation, MIDI clip creation, mixer levels and pans, 808 bass synth parameters, Flex Pitch per-note edits, Lemon Studio FX configurations, third-party CLAP/VST3 instrument hosting, plugin editor window toggling, marquee edits, and offline bounce.

---

### Installation

#### Fedora / Nobara / RHEL 9+ (RPM)
```bash
sudo dnf install ./douri-0.3.0-1.x86_64.rpm
```

#### Generic Linux Tarball
```bash
tar -xzf douri-v0.3.0-linux-x86_64.tar.gz
cd douri-v0.3.0-linux-x86_64
./install.sh
```
