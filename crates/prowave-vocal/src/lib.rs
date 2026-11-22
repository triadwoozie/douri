//! `prowave-vocal`: Pure-Rust OpenUtau & UTAU vocal synthesis engine for Dōri.
//!
//! Provides:
//! - Shift-JIS & UTF-8 Voicebank and `oto.ini` loading (`Voicebank`, `Oto`, `OtoSet`, `PrefixMap`)
//! - `.ust` and `.ustx` project import and export
//! - Japanese CV/VCV phonemizer (`JapaneseVcvPhonemizer`)
//! - UTAU resamplers (external CLI invocation & pure-Rust built-in stretcher fallback)
//! - Equal-power phrase crossfader and stitcher
//! - Background phrase caching and `SingerScore` generation for real-time DAW playback

pub mod autotune;
pub mod cache;
pub mod crossfade;
pub mod engine;
pub mod phonemizer;
pub mod resampler;
pub mod ust;
pub mod ustx;
pub mod voicebank;

pub use autotune::{
    find_pitch_model, merge_curve_span, predict_contours, predict_parametric_contours,
    CurvePoint, PredictedContours,
};
pub use cache::PhraseCache;
pub use crossfade::{crossfade_phrase, RenderedSegment};
pub use engine::VocalEngine;
pub use phonemizer::{JapaneseVcvPhonemizer, JapaneseVowel, ResolvedPhoneme};
pub use resampler::{
    encode_utau_pitch_bends, read_mono_wav, run_builtin_resampler, run_external_resampler,
    ResamplerItem,
};
pub use ust::{UstNote, UstPitchBend, UstProject, UstVibrato, TICKS_PER_BEAT};
pub use ustx::{UstxNote, UstxPitch, UstxProject, UstxTrack, UstxVibrato, UstxVoicePart};
pub use voicebank::{
    default_singers_dir, discover_singers, import_voicebank_folder, import_voicebank_zip,
    load_singer_info, parse_oto_ini, parse_tone_name, read_file_detect_encoding, remove_singer,
    tone_to_name, Oto, OtoSet, PrefixMap, SingerInfo, Voicebank, VoicebankType,
};
