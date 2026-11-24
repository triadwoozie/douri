// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! One of Pro Wave's built-in instruments or effects as a VST3 and CLAP plug-in (and, wrapped
//! from the CLAP, an Audio Unit; see README.md). Which one is chosen when it is built
//! (`PROWAVE_PLUG_ID`, see build.rs): nih-plug makes one plug-in per library.
//!
//! - [`params`]: the host's parameters, made from the registry's table.
//! - [`core`]: the engine and the audio thread's work.
//! - [`editor`]: the window, in Pro Wave's Hardware look.

use nih_plug::prelude::*;
use prowave_engine::{BlockTransport, NativeFxInfo};
use std::sync::Arc;

pub mod codes;
pub mod core;
pub mod editor;
pub mod pages;
pub mod params;

/// What this build wraps (written by build.rs).
pub mod selected {
    include!(concat!(env!("OUT_DIR"), "/selected.rs"));
}

use crate::core::PlugCore;
use crate::params::PlugParams;

/// The registry entry of this build.
pub fn selected_info() -> &'static NativeFxInfo {
    let found = if selected::INSTRUMENT {
        prowave_engine::native_instrument_info(selected::ID)
    } else {
        prowave_engine::native_info(selected::ID)
    };
    // build.rs read the same table, so the entry is there.
    found.unwrap_or_else(|| unreachable!("{} is not in the engine's tables", selected::ID))
}

pub struct ProWavePlug {
    params: Arc<PlugParams>,
    core: Option<PlugCore>,
    /// The latency the host was told.
    latency: u32,
}

impl Default for ProWavePlug {
    fn default() -> Self {
        Self {
            params: Arc::new(PlugParams::new(selected_info(), selected::INSTRUMENT)),
            core: None,
            latency: 0,
        }
    }
}

const INSTRUMENT_LAYOUTS: &[AudioIOLayout] = &[
    AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    },
    AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(1),
        ..AudioIOLayout::const_default()
    },
];

const EFFECT_LAYOUTS: &[AudioIOLayout] = &[
    AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    },
    AudioIOLayout {
        main_input_channels: NonZeroU32::new(1),
        main_output_channels: NonZeroU32::new(1),
        ..AudioIOLayout::const_default()
    },
];

/// The status byte of a channel message.
fn status(kind: u8, channel: u8) -> u8 {
    kind | (channel & 0x0f)
}

/// A 0..1 value as a 7-bit MIDI value.
fn seven(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 127.0).round() as u8
}

impl Plugin for ProWavePlug {
    const NAME: &'static str = selected::NAME;
    const VENDOR: &'static str = codes::AU_MANUFACTURER_NAME;
    const URL: &'static str = "https://github.com/yuribeats/prowave";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = if selected::INSTRUMENT {
        INSTRUMENT_LAYOUTS
    } else {
        EFFECT_LAYOUTS
    };

    const MIDI_INPUT: MidiConfig = if selected::INSTRUMENT {
        MidiConfig::MidiCCs
    } else {
        MidiConfig::None
    };
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone())
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        let values: Vec<i32> = self.params.values.iter().map(|p| p.value()).collect();
        let rate = config.sample_rate.round().max(1.0) as u32;
        let max_block = config.max_buffer_size.max(1) as usize;
        self.core = PlugCore::open(
            self.params.info,
            self.params.instrument,
            rate,
            max_block,
            &values,
        );
        let Some(core) = self.core.as_ref() else {
            return false;
        };
        self.latency = core.latency_samples();
        context.set_latency_samples(self.latency);
        true
    }

    fn reset(&mut self) {
        if let Some(core) = self.core.as_mut() {
            core.silence();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let Some(core) = self.core.as_mut() else {
            for ch in buffer.as_slice() {
                ch.fill(0.0);
            }
            return ProcessStatus::Normal;
        };
        for (k, p) in self.params.values.iter().enumerate() {
            core.set_param(k, p.value());
        }
        let t = context.transport();
        core.transport(&BlockTransport {
            playing: t.playing,
            recording: t.recording,
            sample_pos: t.pos_samples().unwrap_or(0),
            beat: t.pos_beats().unwrap_or(0.0),
            bar_start_beat: t.bar_start_pos_beats().unwrap_or(0.0),
            tempo: t.tempo.unwrap_or(120.0),
            sig: (
                t.time_sig_numerator.unwrap_or(4).clamp(1, 255) as u8,
                t.time_sig_denominator.unwrap_or(4).clamp(1, 255) as u8,
            ),
            cycle: t.loop_range_beats(),
        });
        while let Some(ev) = context.next_event() {
            let at = ev.timing();
            match ev {
                NoteEvent::NoteOn {
                    channel,
                    note,
                    velocity,
                    ..
                } => {
                    // A note on at velocity 0 is a note off in MIDI: keep it sounding.
                    core.midi(status(0x90, channel), note, seven(velocity).max(1), at)
                }
                NoteEvent::NoteOff { channel, note, .. }
                | NoteEvent::Choke { channel, note, .. } => {
                    core.midi(status(0x80, channel), note, 0, at)
                }
                NoteEvent::MidiPitchBend { channel, value, .. } => {
                    let v = (value.clamp(0.0, 1.0) * 16383.0).round() as u16;
                    core.midi(status(0xe0, channel), (v & 0x7f) as u8, (v >> 7) as u8, at)
                }
                NoteEvent::MidiCC {
                    channel, cc, value, ..
                } => core.midi(status(0xb0, channel), cc, seven(value), at),
                _ => {}
            }
        }
        let n = buffer.samples();
        let chans = buffer.as_slice();
        match chans.len() {
            0 => {}
            1 => core.process_mono(&mut chans[0][..n]),
            _ => {
                let (a, b) = chans.split_at_mut(1);
                core.process(&mut a[0][..n], &mut b[0][..n]);
            }
        }
        let latency = core.latency_samples();
        if latency != self.latency {
            self.latency = latency;
            context.set_latency_samples(latency);
        }
        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for ProWavePlug {
    const CLAP_ID: &'static str = selected::CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> = Some(if selected::INSTRUMENT {
        "A Pro Wave instrument"
    } else {
        "A Pro Wave effect"
    });
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = if selected::INSTRUMENT {
        &[
            ClapFeature::Instrument,
            ClapFeature::Synthesizer,
            ClapFeature::Stereo,
            ClapFeature::Mono,
        ]
    } else {
        &[
            ClapFeature::AudioEffect,
            ClapFeature::Stereo,
            ClapFeature::Mono,
        ]
    };
}

impl Vst3Plugin for ProWavePlug {
    const VST3_CLASS_ID: [u8; 16] = selected::VST3_CLASS_ID;
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = if selected::INSTRUMENT {
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth]
    } else {
        &[Vst3SubCategory::Fx]
    };
}

nih_export_clap!(ProWavePlug);
// nih-plug's VST3 export macro ends an expression in a semicolon, which the compiler warns of.
#[allow(semicolon_in_expressions_from_non_local_macros)]
mod vst3_export {
    use super::ProWavePlug;
    nih_plug::nih_export_vst3!(ProWavePlug);
}

#[cfg(test)]
mod tests {
    use super::codes;

    /// No two plug-ins share a CLAP id, a VST3 class id or an Audio Unit subtype.
    #[test]
    fn codes_are_unique() {
        let all: Vec<_> = prowave_engine::native_instruments()
            .iter()
            .chain(prowave_engine::native_effects())
            .collect();
        for (i, a) in all.iter().enumerate() {
            let sub = codes::au_subtype(a.id);
            assert_eq!(sub.len(), 4);
            assert!(sub
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
            for b in &all[i + 1..] {
                assert_ne!(codes::clap_id(a.id), codes::clap_id(b.id));
                assert_ne!(codes::vst3_class_id(a.id), codes::vst3_class_id(b.id));
                assert_ne!(sub, codes::au_subtype(b.id), "{} and {}", a.id, b.id);
                assert_ne!(codes::bundle_name(a.name), codes::bundle_name(b.name));
            }
        }
    }
}
