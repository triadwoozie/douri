# Third-party notices

Pro Wave (Yuri's Version) is GPL-3.0-or-later (see LICENSE). It contains or is built with the
following works, under their own licences. Their notices are reproduced here as those licences
ask; the full texts are in the files named.

## CHIMERA voice engine

`crates/chimera`, by Jameson Hodge, from https://github.com/jamesondh/chimera-vocal-synth
(commit 09fb776; see `crates/chimera/NOTICE`). MIT licence: `crates/chimera/LICENSE`.

## CMU Pronouncing Dictionary

`crates/chimera/data/cmudict.txt.gz`, and `crates/prowave-app/assets/syllable_exceptions.txt`
made from it. Copyright (C) 1993-2015 Carnegie Mellon University. BSD-style licence:

```
Copyright (C) 1993-2015 Carnegie Mellon University. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.
   The contents of this file are deemed to be source code.

2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in
   the documentation and/or other materials provided with the
   distribution.

This work was supported in part by funding from the Defense Advanced
Research Projects Agency, the Office of Naval Research and the National
Science Foundation of the United States of America, and by member
companies of the Carnegie Mellon Sphinx Speech Consortium. We acknowledge
the contributions of many volunteers to the expansion and improvement of
this dictionary.

THIS SOFTWARE IS PROVIDED BY CARNEGIE MELLON UNIVERSITY ``AS IS'' AND
ANY EXPRESSED OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO,
THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL CARNEGIE MELLON UNIVERSITY
NOR ITS EMPLOYEES BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## DiffSinger singer (optional, installed by `tools/vocal/setup.sh`)

Nothing of it is in the program; the script fetches and builds it on your Mac when you run it.
- OpenUtau (https://github.com/stakira/OpenUtau), MIT; its DiffSinger runtime is built from a
  pinned commit. DiffSinger itself (OpenVPI, https://github.com/openvpi/DiffSinger) is
  Apache-2.0.
- The .NET SDK (Microsoft), MIT; NAudio, MIT; Serilog.Sinks.Console, Apache-2.0.
- The voice, Hoshino Hanami ~AI♥dol~ by Team L♥VE (LotteV), which you download yourself: its
  makers allow any use with credit, as their page states. Its vocoder, AI♥dolGAN (from
  OpenVPI's NSF-HiFiGAN), is CC BY-NC-SA 4.0: NON-COMMERCIAL use only. Singing made with it
  may not be used commercially.

## WORLD vocoder

`crates/prowave-engine/world`, by M. Morise. Modified BSD licence:

```
/* ----------------------------------------------------------------- */
/*           WORLD: High-quality speech analysis,                    */
/*           manipulation and synthesis system                       */
/*           developed by M. Morise                                  */
/*           http://www.kisc.meiji.ac.jp/~mmorise/world/english/     */
/* ----------------------------------------------------------------- */
/*                                                                   */
/*  Copyright (c) 2010  M. Morise                                    */
/*                                                                   */
/* All rights reserved.                                              */
/*                                                                   */
/* Redistribution and use in source and binary forms, with or        */
/* without modification, are permitted provided that the following   */
/* conditions are met:                                               */
/*                                                                   */
/* - Redistributions of source code must retain the above copyright  */
/*   notice, this list of conditions and the following disclaimer.   */
/* - Redistributions in binary form must reproduce the above         */
/*   copyright notice, this list of conditions and the following     */
/*   disclaimer in the documentation and/or other materials provided */
/*   with the distribution.                                          */
/* - Neither the name of the M. Morise nor the names of its          */
/*   contributors may be used to endorse or promote products derived */
/*   from this software without specific prior written permission.   */
/*                                                                   */
/* THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND            */
/* CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,       */
/* INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF          */
/* MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE          */
/* DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS */
/* BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,          */
/* EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED   */
/* TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,     */
/* DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON */
/* ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,   */
/* OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY    */
/* OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE           */
/* POSSIBILITY OF SUCH DAMAGE.                                       */
/* ----------------------------------------------------------------- */
```

## Signalsmith Stretch

The time stretch, through the `signalsmith-stretch` crate (the C++ library by Signalsmith Audio
and its Rust bindings). MIT licence; the texts come with the crate's source.

## Lemon Studio effects

The Lemon Limiter, Press, Lemon Transient Processor, Lemon Soft Clipper and Lemon Fast
Distortion are ports of Lemon Studio's effects (https://github.com/ItsAlphaNeon/Lemon-Studio),
GPL-3.0-or-later; each file names its source.

## Fonts

Barlow Condensed, Inter, JetBrains Mono and Saira Condensed, in `crates/prowave-app/assets/fonts`,
under the SIL Open Font License 1.1: the `OFL-*.txt` files next to them.

## Rust crates

The Rust libraries the program is built from, with the licence each declares (made from
`cargo metadata` when this source was written; `cargo deny check licenses` checks them against
`deny.toml`):

| Crate | Version | Licence |
|---|---|---|
| ab_glyph | 0.2.32 | Apache-2.0 |
| ab_glyph_rasterizer | 0.1.10 | Apache-2.0 |
| accesskit | 0.25.1 | MIT OR Apache-2.0 |
| accesskit_atspi_common | 0.21.0 | MIT OR Apache-2.0 |
| accesskit_consumer | 0.39.1 | MIT OR Apache-2.0 |
| accesskit_ios | 0.2.1 | MIT OR Apache-2.0 |
| accesskit_macos | 0.27.1 | MIT OR Apache-2.0 |
| accesskit_unix | 0.24.0 | MIT OR Apache-2.0 |
| accesskit_windows | 0.35.1 | MIT OR Apache-2.0 |
| accesskit_winit | 0.34.1 | Apache-2.0 |
| addr2line | 0.25.1 | Apache-2.0 OR MIT |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| ahash | 0.8.12 | MIT OR Apache-2.0 |
| aho-corasick | 1.1.5 | Unlicense OR MIT |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 |
| alsa | 0.11.0 | Apache-2.0/MIT |
| alsa-sys | 0.4.0 | MIT |
| android-activity | 0.6.1 | MIT OR Apache-2.0 |
| android-build | 0.1.4 | MIT |
| android-properties | 0.2.2 | MIT |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 |
| anstream | 1.0.0 | MIT OR Apache-2.0 |
| anstyle | 1.0.14 | MIT OR Apache-2.0 |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 |
| arboard | 3.7.0 | MIT OR Apache-2.0 |
| arrayref | 0.3.9 | BSD-2-Clause |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 |
| as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 |
| assert_no_alloc | 1.1.2 | BSD-1-Clause |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 |
| async-channel | 2.5.0 | Apache-2.0 OR MIT |
| async-executor | 1.14.0 | Apache-2.0 OR MIT |
| async-io | 2.6.0 | Apache-2.0 OR MIT |
| async-lock | 3.4.2 | Apache-2.0 OR MIT |
| async-process | 2.5.0 | Apache-2.0 OR MIT |
| async-recursion | 1.2.0 | MIT OR Apache-2.0 |
| async-signal | 0.2.14 | Apache-2.0 OR MIT |
| async-task | 4.7.1 | Apache-2.0 OR MIT |
| async-trait | 0.1.92 | MIT OR Apache-2.0 |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT |
| atspi | 0.29.0 | Apache-2.0 OR MIT |
| atspi-common | 0.13.0 | Apache-2.0 OR MIT |
| atspi-proxies | 0.13.0 | Apache-2.0 OR MIT |
| audio_thread_priority | 0.35.1 | MPL-2.0 |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| backtrace | 0.3.76 | MIT OR Apache-2.0 |
| base64 | 0.23.1 | MIT OR Apache-2.0 |
| bindgen | 0.70.1 | BSD-3-Clause |
| bit-set | 0.10.0 | Apache-2.0 OR MIT |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT |
| bitflags | 1.3.2 | MIT/Apache-2.0 |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 |
| block2 | 0.5.1 | MIT |
| block2 | 0.6.2 | MIT |
| blocking | 1.7.0 | Apache-2.0 OR MIT |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT |
| byteorder-lite | 0.1.0 | Unlicense OR MIT |
| bytes | 1.12.1 | MIT |
| calloop | 0.13.0 | MIT |
| calloop | 0.14.5 | MIT |
| calloop-wayland-source | 0.3.0 | MIT |
| calloop-wayland-source | 0.4.1 | MIT |
| castaway | 0.2.4 | MIT |
| cc | 1.8.0 | MIT OR Apache-2.0 |
| cesu8 | 1.1.0 | Apache-2.0/MIT |
| cexpr | 0.6.0 | Apache-2.0/MIT |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| cfg_aliases | 0.2.2 | MIT |
| cgl | 0.3.2 | MIT / Apache-2.0 |
| chimera | 0.1.0 | MIT |
| clang-sys | 1.9.1 | Apache-2.0 |
| clipboard-win | 5.4.1 | BSL-1.0 |
| codespan-reporting | 0.13.1 | Apache-2.0 |
| color | 0.3.3 | Apache-2.0 OR MIT |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 |
| com-scrape-types | 0.1.1 | MIT OR Apache-2.0 |
| combine | 4.6.9 | MIT |
| compact_str | 0.10.0 | MIT |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT |
| const-random | 0.1.18 | MIT OR Apache-2.0 |
| const-random-macro | 0.1.16 | MIT OR Apache-2.0 |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 |
| core-graphics | 0.23.2 | MIT OR Apache-2.0 |
| core-graphics-types | 0.1.3 | MIT OR Apache-2.0 |
| core_detect | 1.0.0 | MIT/Apache-2.0 |
| coreaudio-rs | 0.14.2 | MIT/Apache-2.0 |
| coremidi | 0.9.2 | MIT |
| coremidi-sys | 3.2.1 | MIT |
| cpal | 0.18.2 | Apache-2.0 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 |
| crunchy | 0.2.4 | MIT |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 |
| cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib |
| dasp | 0.11.0 | MIT OR Apache-2.0 |
| dasp_envelope | 0.11.0 | MIT OR Apache-2.0 |
| dasp_frame | 0.11.0 | MIT OR Apache-2.0 |
| dasp_interpolate | 0.11.0 | MIT OR Apache-2.0 |
| dasp_peak | 0.11.0 | MIT OR Apache-2.0 |
| dasp_ring_buffer | 0.11.0 | MIT OR Apache-2.0 |
| dasp_rms | 0.11.0 | MIT OR Apache-2.0 |
| dasp_sample | 0.11.0 | MIT OR Apache-2.0 |
| dasp_signal | 0.11.0 | MIT OR Apache-2.0 |
| dasp_slice | 0.11.0 | MIT OR Apache-2.0 |
| dasp_window | 0.11.1 | MIT OR Apache-2.0 |
| digest | 0.10.7 | MIT OR Apache-2.0 |
| dispatch | 0.2.0 | MIT |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 |
| dlib | 0.5.3 | MIT |
| dlopen2 | 0.9.0 | MIT |
| document-features | 0.2.12 | MIT OR Apache-2.0 |
| downcast-rs | 1.2.1 | MIT/Apache-2.0 |
| dpi | 0.1.2 | Apache-2.0 AND MIT |
| ecolor | 0.37.0 | MIT OR Apache-2.0 |
| eframe | 0.37.0 | MIT OR Apache-2.0 |
| egui | 0.37.0 | MIT OR Apache-2.0 |
| egui-wgpu | 0.37.0 | MIT OR Apache-2.0 |
| egui-winit | 0.37.0 | MIT OR Apache-2.0 |
| egui_glow | 0.37.0 | MIT OR Apache-2.0 |
| egui_system_font_provider | 0.37.0 | MIT OR Apache-2.0 |
| either | 1.19.0 | MIT OR Apache-2.0 |
| emath | 0.37.0 | MIT OR Apache-2.0 |
| encoding_rs | 0.8.42 | (Apache-2.0 OR MIT) AND BSD-3-Clause |
| endi | 1.1.1 | MIT |
| enumflags2 | 0.7.12 | MIT OR Apache-2.0 |
| enumflags2_derive | 0.7.12 | MIT OR Apache-2.0 |
| epaint | 0.37.0 | MIT OR Apache-2.0 |
| epaint_default_fonts | 0.37.0 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 |
| equivalent | 1.0.3 | Apache-2.0 OR MIT |
| errno | 0.3.14 | MIT OR Apache-2.0 |
| error-code | 3.4.0 | BSL-1.0 |
| euclid | 0.22.14 | MIT OR Apache-2.0 |
| event-listener | 5.4.2 | Apache-2.0 OR MIT |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT |
| extended | 0.1.0 | MIT |
| fastrand | 2.5.0 | Apache-2.0 OR MIT |
| fax | 0.2.7 | MIT |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| fearless_simd | 0.4.1 | Apache-2.0 OR MIT |
| filetime | 0.2.29 | MIT/Apache-2.0 |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 |
| fixedbitset | 0.5.7 | MIT OR Apache-2.0 |
| flate2 | 1.1.10 | MIT OR Apache-2.0 |
| foldhash | 0.1.5 | Zlib |
| foldhash | 0.2.0 | Zlib |
| font-types | 0.12.7 | MIT OR Apache-2.0 |
| fontique | 0.11.1 | Apache-2.0 OR MIT |
| foreign-types | 0.5.0 | MIT/Apache-2.0 |
| foreign-types-macros | 0.2.4 | MIT/Apache-2.0 |
| foreign-types-shared | 0.3.1 | MIT/Apache-2.0 |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-io | 0.3.34 | MIT OR Apache-2.0 |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| generic-array | 0.14.7 | MIT |
| gethostname | 1.1.0 | Apache-2.0 |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| gimli | 0.32.3 | MIT OR Apache-2.0 |
| gl_generator | 0.14.0 | Apache-2.0 |
| glifo | 0.3.0 | Apache-2.0 OR MIT |
| glob | 0.3.4 | MIT OR Apache-2.0 |
| glow | 0.17.0 | MIT OR Apache-2.0 OR Zlib |
| glutin | 0.32.3 | Apache-2.0 |
| glutin-winit | 0.5.0 | MIT |
| glutin_egl_sys | 0.7.1 | Apache-2.0 |
| glutin_glx_sys | 0.6.1 | Apache-2.0 |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 |
| gpu-allocator | 0.28.0 | MIT OR Apache-2.0 |
| guillotiere | 0.7.0 | MIT/Apache-2.0 |
| half | 2.7.1 | MIT OR Apache-2.0 |
| harfrust | 0.12.0 | MIT |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| hermit-abi | 0.5.3 | MIT OR Apache-2.0 |
| hex | 0.4.3 | MIT OR Apache-2.0 |
| http | 1.5.0 | MIT OR Apache-2.0 |
| httparse | 1.10.1 | MIT OR Apache-2.0 |
| human-panic | 2.0.8 | MIT OR Apache-2.0 |
| icu_collections | 2.3.0 | Unicode-3.0 |
| icu_locale_core | 2.3.0 | Unicode-3.0 |
| icu_normalizer | 2.3.0 | Unicode-3.0 |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 |
| icu_properties | 2.3.0 | Unicode-3.0 |
| icu_properties_data | 2.3.0 | Unicode-3.0 |
| icu_provider | 2.3.1 | Unicode-3.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT |
| image | 0.25.10 | MIT OR Apache-2.0 |
| indexmap | 2.14.2 | Apache-2.0 OR MIT |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| itertools | 0.13.0 | MIT OR Apache-2.0 |
| itertools | 0.15.0 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| java-locator | 0.1.9 | MIT/Apache-2.0 |
| jni | 0.21.1 | MIT/Apache-2.0 |
| jni | 0.22.4 | MIT OR Apache-2.0 |
| jni-macros | 0.22.4 | MIT OR Apache-2.0 |
| jni-min-helper | 0.3.4 | MIT OR Apache-2.0 |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 |
| jobserver | 0.1.35 | MIT OR Apache-2.0 |
| js-sys | 0.3.106 | MIT OR Apache-2.0 |
| keyboard-types | 0.8.3 | MIT OR Apache-2.0 |
| khronos-egl | 6.0.0 | MIT/Apache-2.0 |
| khronos_api | 3.1.0 | Apache-2.0 |
| kurbo | 0.13.1 | Apache-2.0 OR MIT |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 |
| libc | 0.2.191 | MIT OR Apache-2.0 |
| libloading | 0.7.4 | ISC |
| libloading | 0.8.9 | ISC |
| libm | 0.2.16 | MIT |
| libredox | 0.1.25 | MIT |
| linebender_resource_handle | 0.1.1 | Apache-2.0 OR MIT |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| litemap | 0.8.3 | Unicode-3.0 |
| litrs | 1.0.1 | MIT OR Apache-2.0 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 |
| log | 0.4.34 | MIT OR Apache-2.0 |
| mach2 | 0.4.3 | BSD-2-Clause OR MIT OR Apache-2.0 |
| mach2 | 0.6.0 | BSD-2-Clause OR MIT OR Apache-2.0 |
| matchers | 0.2.0 | MIT |
| memchr | 2.8.3 | Unlicense OR MIT |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 |
| memoffset | 0.9.1 | MIT |
| midir | 0.11.1 | MIT |
| minimal-lexical | 0.2.1 | MIT/Apache-2.0 |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 |
| muda | 0.21.0 | Apache-2.0 OR MIT |
| multiversion_no_op | 1.0.0 | Apache-2.0 OR MIT |
| naga | 30.0.1 | MIT OR Apache-2.0 |
| naga-types | 30.0.1 | MIT OR Apache-2.0 |
| ndk | 0.9.0 | MIT OR Apache-2.0 |
| ndk-context | 0.1.1 | MIT OR Apache-2.0 |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 |
| nix | 0.31.3 | MIT |
| nohash-hasher | 0.2.0 | Apache-2.0 OR MIT |
| nom | 7.1.3 | MIT |
| nom | 8.0.0 | MIT |
| ntapi | 0.4.3 | Apache-2.0 OR MIT |
| nu-ansi-term | 0.50.3 | MIT |
| num-derive | 0.4.2 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| num_enum | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 |
| num_enum_derive | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 |
| objc-sys | 0.3.5 | MIT |
| objc2 | 0.5.3 | MIT |
| objc2 | 0.6.5 | MIT |
| objc2-app-kit | 0.2.2 | MIT |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-audio-toolbox | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-avf-audio | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-cloud-kit | 0.2.2 | MIT |
| objc2-contacts | 0.2.2 | MIT |
| objc2-core-audio | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-audio-types | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-data | 0.2.2 | MIT |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-image | 0.2.2 | MIT |
| objc2-core-location | 0.2.2 | MIT |
| objc2-core-text | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-encode | 4.1.0 | MIT |
| objc2-foundation | 0.2.2 | MIT |
| objc2-foundation | 0.3.2 | MIT |
| objc2-io-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-link-presentation | 0.2.2 | MIT |
| objc2-metal | 0.2.2 | MIT |
| objc2-metal | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-quartz-core | 0.2.2 | MIT |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-symbols | 0.2.2 | MIT |
| objc2-ui-kit | 0.2.2 | MIT |
| objc2-ui-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-uniform-type-identifiers | 0.2.2 | MIT |
| objc2-user-notifications | 0.2.2 | MIT |
| object | 0.37.3 | Apache-2.0 OR MIT |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| openssl-probe | 0.2.1 | MIT OR Apache-2.0 |
| orbclient | 0.3.55 | MIT |
| ordered-float | 5.5.0 | MIT |
| ordered-stream | 0.2.0 | MIT OR Apache-2.0 |
| os_pipe | 1.2.3 | MIT |
| owned_ttf_parser | 0.25.1 | Apache-2.0 |
| parking | 2.2.1 | Apache-2.0 OR MIT |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 |
| parlance | 0.1.1 | Apache-2.0 OR MIT |
| peniko | 0.6.1 | Apache-2.0 OR MIT |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| petgraph | 0.8.3 | MIT OR Apache-2.0 |
| phf | 0.14.0 | MIT |
| phf_generator | 0.14.0 | MIT |
| phf_macros | 0.14.0 | MIT |
| phf_shared | 0.14.0 | MIT |
| pin-project | 1.1.13 | Apache-2.0 OR MIT |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| piper | 0.2.5 | MIT OR Apache-2.0 |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| plain | 0.2.3 | MIT/Apache-2.0 |
| png | 0.18.1 | MIT OR Apache-2.0 |
| poll-promise | 0.3.0 | MIT OR Apache-2.0 |
| polling | 3.11.0 | Apache-2.0 OR MIT |
| pollster | 0.4.0 | Apache-2.0/MIT |
| pollster | 1.0.1 | Apache-2.0/MIT |
| polycool | 0.4.0 | MIT OR Apache-2.0 |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT |
| potential_utf | 0.1.6 | Unicode-3.0 |
| presser | 0.3.1 | MIT OR Apache-2.0 |
| prettyplease | 0.2.37 | MIT OR Apache-2.0 |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| process_path | 0.1.4 | MIT/Apache-2.0 |
| profiling | 1.0.18 | MIT OR Apache-2.0 |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 |
| quick-error | 2.0.1 | MIT/Apache-2.0 |
| quick-xml | 0.41.0 | MIT |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| range-alloc | 0.1.5 | MIT OR Apache-2.0 |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib |
| raw-window-metal | 1.1.0 | MIT OR Apache-2.0 |
| read-fonts | 0.41.0 | MIT OR Apache-2.0 |
| redox_syscall | 0.4.1 | MIT |
| redox_syscall | 0.5.18 | MIT |
| redox_syscall | 0.9.4 | MIT |
| regex | 1.13.1 | MIT OR Apache-2.0 |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 |
| rfd | 0.17.2 | MIT |
| rhai | 1.27.0 | MIT OR Apache-2.0 |
| rhai_codegen | 3.3.0 | MIT OR Apache-2.0 |
| ring | 0.17.14 | Apache-2.0 AND ISC |
| roxmltree | 0.21.1 | MIT OR Apache-2.0 |
| rtrb | 0.4.0 | MIT OR Apache-2.0 |
| rustc-demangle | 0.1.28 | MIT/Apache-2.0 |
| rustc-hash | 1.1.0 | Apache-2.0/MIT |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT |
| rustls-native-certs | 0.8.4 | Apache-2.0 OR ISC OR MIT |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 |
| rustls-platform-verifier | 0.7.1 | MIT OR Apache-2.0 |
| rustls-platform-verifier-android | 0.2.0 | MIT OR Apache-2.0 |
| rustls-webpki | 0.103.16 | ISC |
| rustversion | 1.0.23 | MIT OR Apache-2.0 |
| rustysynth | 1.3.8 | MIT |
| same-file | 1.0.6 | Unlicense/MIT |
| schannel | 0.1.29 | MIT |
| scoped-tls | 1.0.1 | MIT/Apache-2.0 |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| sctk-adwaita | 0.10.1 | MIT |
| security-framework | 3.7.0 | MIT OR Apache-2.0 |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 |
| self_cell | 1.3.0 | Apache-2.0 OR GPL-2.0-only |
| semver | 1.0.28 | MIT OR Apache-2.0 |
| send_wrapper | 0.6.0 | MIT/Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 |
| serde_spanned | 1.1.2 | MIT OR Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| sharded-slab | 0.1.7 | MIT |
| shlex | 1.3.0 | MIT OR Apache-2.0 |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 |
| signalsmith-stretch | 0.1.3 | MIT |
| simd-adler32 | 0.3.10 | MIT |
| simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 |
| siphasher | 1.0.4 | MIT OR Apache-2.0 |
| skrifa | 0.44.0 | MIT OR Apache-2.0 |
| slab | 0.4.12 | MIT |
| slotmap | 1.1.1 | Zlib |
| smallvec | 1.16.3 | MIT OR Apache-2.0 |
| smartstring | 1.0.1 | MPL-2.0+ |
| smithay-client-toolkit | 0.19.2 | MIT |
| smithay-client-toolkit | 0.20.0 | MIT |
| smithay-clipboard | 0.7.3 | MIT |
| smol_str | 0.2.2 | MIT OR Apache-2.0 |
| spirv | 0.4.0+sdk-1.4.341.0 | Apache-2.0 |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 |
| strict-num | 0.1.1 | MIT |
| subtle | 2.6.1 | BSD-3-Clause |
| symphonia | 0.5.5 | MPL-2.0 |
| symphonia-bundle-flac | 0.5.5 | MPL-2.0 |
| symphonia-bundle-mp3 | 0.5.5 | MPL-2.0 |
| symphonia-codec-aac | 0.5.5 | MPL-2.0 |
| symphonia-codec-adpcm | 0.5.5 | MPL-2.0 |
| symphonia-codec-alac | 0.5.5 | MPL-2.0 |
| symphonia-codec-pcm | 0.5.5 | MPL-2.0 |
| symphonia-codec-vorbis | 0.5.5 | MPL-2.0 |
| symphonia-core | 0.5.5 | MPL-2.0 |
| symphonia-format-caf | 0.5.5 | MPL-2.0 |
| symphonia-format-isomp4 | 0.5.5 | MPL-2.0 |
| symphonia-format-ogg | 0.5.5 | MPL-2.0 |
| symphonia-format-riff | 0.5.5 | MPL-2.0 |
| symphonia-metadata | 0.5.5 | MPL-2.0 |
| symphonia-utils-xiph | 0.5.5 | MPL-2.0 |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.9 | MIT OR Apache-2.0 |
| synstructure | 0.14.0 | MIT |
| sys-locale | 0.3.2 | MIT OR Apache-2.0 |
| sysinfo | 0.38.4 | MIT |
| tar | 0.4.46 | MIT OR Apache-2.0 |
| tempfile | 3.27.0 | MIT OR Apache-2.0 |
| termcolor | 1.4.1 | Unlicense OR MIT |
| thin-vec | 0.2.21 | MIT OR Apache-2.0 |
| thiserror | 1.0.69 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| thread_local | 1.1.10 | MIT OR Apache-2.0 |
| tiff | 0.11.3 | MIT |
| tiny-keccak | 2.0.2 | CC0-1.0 |
| tiny-skia | 0.11.4 | BSD-3-Clause |
| tiny-skia-path | 0.11.4 | BSD-3-Clause |
| tinystr | 0.8.4 | Unicode-3.0 |
| tokio | 1.53.3 | MIT |
| toml | 1.1.8+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_datetime | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_edit | 0.25.17+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_writer | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 |
| tracing | 0.1.44 | MIT |
| tracing-attributes | 0.1.31 | MIT |
| tracing-core | 0.1.36 | MIT |
| tracing-log | 0.2.0 | MIT |
| tracing-subscriber | 0.3.23 | MIT |
| tree_magic_mini | 3.2.2 | MIT |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 |
| type-map | 0.5.1 | MIT/Apache-2.0 |
| typed-path | 0.12.3 | MIT OR Apache-2.0 |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| uds_windows | 1.2.1 | MIT |
| unicode-general-category | 1.1.0 | Apache-2.0 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-properties | 0.1.4 | MIT/Apache-2.0 |
| unicode-script | 0.5.8 | MIT OR Apache-2.0 |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 |
| untrusted | 0.9.0 | ISC |
| ureq | 3.4.2 | MIT OR Apache-2.0 |
| ureq-proto | 0.6.5 | MIT OR Apache-2.0 |
| url | 2.5.8 | MIT OR Apache-2.0 |
| utf8-zero | 0.8.1 | MIT OR Apache-2.0 |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT |
| uuid | 1.28.0 | Apache-2.0 OR MIT |
| valuable | 0.1.1 | MIT |
| vello_common | 0.2.0 | Apache-2.0 OR MIT |
| vello_cpu | 0.2.0 | Apache-2.0 OR MIT |
| version_check | 0.9.5 | MIT/Apache-2.0 |
| vst3 | 0.3.0 | MIT OR Apache-2.0 |
| walkdir | 2.5.0 | Unlicense/MIT |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 |
| wayland-backend | 0.3.17 | MIT |
| wayland-client | 0.31.15 | MIT |
| wayland-csd-frame | 0.3.0 | MIT |
| wayland-cursor | 0.31.14 | MIT |
| wayland-protocols | 0.32.13 | MIT |
| wayland-protocols-experimental | 20250721.0.1 | MIT |
| wayland-protocols-misc | 0.3.12 | MIT |
| wayland-protocols-plasma | 0.3.12 | MIT |
| wayland-protocols-wlr | 0.3.12 | MIT |
| wayland-scanner | 0.31.11 | MIT |
| wayland-sys | 0.31.11 | MIT |
| web-sys | 0.3.106 | MIT OR Apache-2.0 |
| web-time | 1.1.0 | MIT OR Apache-2.0 |
| webbrowser | 1.2.4 | MIT OR Apache-2.0 |
| webpki-root-certs | 1.0.10 | CDLA-Permissive-2.0 |
| webpki-roots | 1.0.10 | CDLA-Permissive-2.0 |
| weezl | 0.1.12 | MIT OR Apache-2.0 |
| wgpu | 30.0.1 | MIT OR Apache-2.0 |
| wgpu-core | 30.0.1 | MIT OR Apache-2.0 |
| wgpu-core-deps-apple | 30.0.1 | MIT OR Apache-2.0 |
| wgpu-core-deps-emscripten | 30.0.1 | MIT OR Apache-2.0 |
| wgpu-core-deps-wasm | 30.0.1 | MIT OR Apache-2.0 |
| wgpu-core-deps-windows-linux-android | 30.0.1 | MIT OR Apache-2.0 |
| wgpu-hal | 30.0.1 | MIT OR Apache-2.0 |
| wgpu-naga-bridge | 30.0.1 | MIT OR Apache-2.0 |
| wgpu-types | 30.0.1 | MIT OR Apache-2.0 |
| winapi | 0.3.9 | MIT/Apache-2.0 |
| winapi-i686-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 |
| winapi-util | 0.1.11 | Unlicense OR MIT |
| winapi-x86_64-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 |
| windows | 0.62.2 | MIT OR Apache-2.0 |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 |
| windows-core | 0.62.2 | MIT OR Apache-2.0 |
| windows-future | 0.3.2 | MIT OR Apache-2.0 |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 |
| windows-sys | 0.45.0 | MIT OR Apache-2.0 |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| windows-targets | 0.42.2 | MIT OR Apache-2.0 |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 |
| windows_aarch64_gnullvm | 0.42.2 | MIT OR Apache-2.0 |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | 0.42.2 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnu | 0.42.2 | MIT OR Apache-2.0 |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_msvc | 0.42.2 | MIT OR Apache-2.0 |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.42.2 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | 0.42.2 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.42.2 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| winit | 0.30.13 | Apache-2.0 |
| winnow | 1.0.4 | MIT |
| winresource | 0.1.34 | MIT |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wl-clipboard-rs | 0.9.4 | MIT OR Apache-2.0 |
| writeable | 0.6.4 | Unicode-3.0 |
| x11-dl | 2.21.0 | MIT |
| x11rb | 0.13.2 | MIT OR Apache-2.0 |
| x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 |
| xcursor | 0.3.11 | MIT |
| xkbcommon-dl | 0.4.2 | MIT |
| xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib |
| xml-rs | 0.8.29 | MIT |
| yeslogic-fontconfig-sys | 6.0.1 | MIT |
| yoke | 0.8.3 | Unicode-3.0 |
| yoke-derive | 0.8.4 | Unicode-3.0 |
| zbus | 5.19.0 | MIT |
| zbus-lockstep | 0.5.2 | MIT |
| zbus-lockstep-macros | 0.5.2 | MIT |
| zbus_macros | 5.19.0 | MIT |
| zbus_names | 4.3.5 | MIT |
| zbus_xml | 5.2.1 | MIT |
| zcheapstr | 1.1.0 | MIT |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerofrom | 0.1.8 | Unicode-3.0 |
| zerofrom-derive | 0.1.8 | Unicode-3.0 |
| zeroize | 1.9.1 | Apache-2.0 OR MIT |
| zerotrie | 0.2.5 | Unicode-3.0 |
| zerovec | 0.11.8 | Unicode-3.0 |
| zerovec-derive | 0.11.6 | Unicode-3.0 |
| zip | 9.0.5 | MIT |
| zlib-rs | 0.6.8 | Zlib |
| zmij | 1.0.23 | MIT |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib |
| zvariant | 5.15.1 | MIT |
| zvariant_derive | 5.15.1 | MIT |
| zvariant_utils | 4.2.0 | MIT |
