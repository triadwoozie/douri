//! High-level vocal synthesis engine orchestrator for OpenUtau & UTAU voicebanks.
//!
//! Generates `SingerClip` and `SingerScore` objects for `prowave-engine` from
//! track notes and lyrics.

use std::path::PathBuf;
use std::sync::Arc;
use prowave_engine::{SingerClip, SingerScore, TempoMap};

use crate::cache::PhraseCache;
use crate::crossfade::{crossfade_phrase, RenderedSegment};
use crate::phonemizer::JapaneseVcvPhonemizer;
use crate::resampler::{run_builtin_resampler, run_external_resampler, ResamplerItem};
use crate::voicebank::Voicebank;

/// Configuration and state of the OpenUtau vocal synthesis engine.
pub struct VocalEngine {
    pub phonemizer: JapaneseVcvPhonemizer,
    pub cache: PhraseCache,
    pub external_resampler_path: Option<PathBuf>,
}

impl Default for VocalEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl VocalEngine {
    pub fn new() -> Self {
        Self {
            phonemizer: JapaneseVcvPhonemizer::new(),
            cache: PhraseCache::default(),
            external_resampler_path: None,
        }
    }

    /// Renders a single phrase (a contiguous group of notes) into a `SingerClip`.
    ///
    /// Notes are tuples of `(beat, duration_beats, tone, lyric)`.
    pub fn render_phrase(
        &mut self,
        voicebank: &Voicebank,
        notes: &[(f64, f64, u8, String)],
        tempo_map: &TempoMap,
        sample_rate: u32,
    ) -> Option<SingerClip> {
        if notes.is_empty() {
            return None;
        }

        let first_beat = notes.first().map(|n| n.0).unwrap_or(0.0);
        let first_second = tempo_map.seconds_at(first_beat);

        // Check phrase cache
        let cache_key = PhraseCache::compute_key(
            &voicebank.name,
            tempo_map.tempo_at(first_beat) as f64,
            notes,
        );
        if let Some(sound) = self.cache.get(cache_key) {
            return Some(SingerClip {
                origin: first_second,
                sound,
            });
        }

        // 1. Phonemize lyric sequence
        let phonemes = self.phonemizer.phonemize(notes, voicebank);
        if phonemes.is_empty() {
            return None;
        }

        // 2. Render each phoneme segment
        let mut segments = Vec::with_capacity(phonemes.len());

        for ph in &phonemes {
            let note_sec = tempo_map.seconds_at(ph.beat);
            let rel_sec = note_sec - first_second;
            let dur_sec = tempo_map.seconds_at(ph.beat + ph.duration_beats) - note_sec;
            let dur_ms = dur_sec * 1000.0;
            let bpm = tempo_map.tempo_at(ph.beat) as f64;

            // Look up Oto in Voicebank
            let (oto, wav_path) = match voicebank.find_oto(&ph.alias, Some(ph.tone)) {
                Some((o, dir)) => (o.clone(), dir.join(&o.wav)),
                None => {
                    // Fallback to sine tone or silence
                    continue;
                }
            };

            let preutter_ms = oto.preutterance;
            let overlap_ms = oto.overlap;

            let item = ResamplerItem {
                input_file: wav_path,
                output_file: PathBuf::new(),
                tone: ph.tone,
                velocity: 100,
                flags: String::new(),
                offset: oto.offset,
                dur_required: dur_ms + preutter_ms,
                consonant: oto.consonant,
                cutoff: oto.cutoff,
                volume: 100.0,
                modulation: 0.0,
                tempo: bpm,
                pitch_bends: Vec::new(),
            };

            // Invoke resampler (external if available, otherwise pure-Rust builtin)
            let samples = if let Some(bin) = &self.external_resampler_path {
                if bin.is_file() {
                    let mut temp_item = item.clone();
                    let temp_out = std::env::temp_dir().join(format!("resamp_{}.wav", ph.tone));
                    temp_item.output_file = temp_out;
                    run_external_resampler(bin, &temp_item)
                        .or_else(|_| run_builtin_resampler(&item, ph.tone, sample_rate))
                        .unwrap_or_default()
                } else {
                    run_builtin_resampler(&item, ph.tone, sample_rate).unwrap_or_default()
                }
            } else {
                run_builtin_resampler(&item, ph.tone, sample_rate).unwrap_or_default()
            };

            if !samples.is_empty() {
                segments.push(RenderedSegment {
                    samples,
                    start_time_secs: rel_sec,
                    preutterance_secs: preutter_ms / 1000.0,
                    overlap_secs: overlap_ms / 1000.0,
                });
            }
        }

        if segments.is_empty() {
            return None;
        }

        // 3. Crossfade and stitch all segments together
        let phrase_audio = crossfade_phrase(&segments, sample_rate);
        let sound: Arc<[f32]> = phrase_audio.into();

        // Store in cache
        self.cache.insert(cache_key, sound.clone());

        Some(SingerClip {
            origin: first_second,
            sound,
        })
    }

    /// Renders a full project / track score of phrases into a `SingerScore`.
    pub fn render_score(
        &mut self,
        score_id: u64,
        voicebank: &Voicebank,
        phrases: &[Vec<(f64, f64, u8, String)>],
        tempo_map: &TempoMap,
        sample_rate: u32,
    ) -> SingerScore {
        let mut clips = Vec::new();
        for phrase in phrases {
            if let Some(clip) = self.render_phrase(voicebank, phrase, tempo_map, sample_rate) {
                clips.push(clip);
            }
        }
        SingerScore::new(score_id, tempo_map.clone(), clips, sample_rate)
    }
}
