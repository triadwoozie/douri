//! ProWave Studio real-time engine.
//!
//! Logic Pro equivalent: transport + software-instrument playback + metronome (Klopfgeist).
//! The UI talks to the engine only through [`Command`]s on a lock-free SPSC queue and reads
//! state back through a lock-free triple buffer ([`StateReader`]) and atomics ([`Shared`]); the audio callback never locks, allocates or does I/O.
#![deny(clippy::unwrap_used, clippy::expect_used)]

// Debug test builds abort if anything inside `assert_no_alloc(..)` allocates or frees.
#[cfg(all(test, debug_assertions))]
#[global_allocator]
static ALLOC_GUARD: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

mod analysis;
pub mod auto_pan;
pub mod bass808;
pub mod calibration;
#[cfg(not(target_arch = "wasm32"))]
mod capture;
mod clock;
mod convolver;
mod core;
mod disk;
pub mod drum_machine;
pub mod epiano;
mod flac;
mod flex;
pub mod freq_shifter;
mod fx;
mod gain_smoothing;
pub mod hydrogen;
pub mod ir_file;
#[cfg(not(target_arch = "wasm32"))]
mod mapped;
pub mod melody;
pub mod meter;
pub mod modal_voice;
pub mod modulators;
mod monitor;
mod mtc;
pub mod native_fx;
pub mod note_render;
pub mod offline;
mod output;
pub mod pitch_tracker;
pub mod plugin;
pub mod plugin_track;
pub mod prerender;
pub mod pult_eq;
mod routing;
pub mod rt_alloc;
pub mod sample_file;
pub mod sampler;
mod sf2;
mod soundfont;
pub mod spectral;
mod synth;
mod tempo_map;
pub mod tri_osc;
mod triple;
#[cfg(windows)]
mod wasapi;
pub mod wave_draw;
pub mod windows_audio;

pub use crate::analysis::{tempo_curve, Analysis, Analyzer, TempoEstimate, STRENGTH_RATE};
pub use crate::auto_pan::AutoPanNode;
pub use crate::bass808::{
    can_sustain as bass808_can_sustain, measure_root_hz as bass808_measure_root_hz,
    prepare_sample as bass808_prepare_sample, Bass808, Bass808Param, Bass808Params, Bass808Spec,
    BASS808_DECAY, BASS808_DRIVE, BASS808_GLIDE, BASS808_HIGH_PASS, BASS808_LEGATO, BASS808_LEVEL,
    BASS808_PARAMS, BASS808_PUNCH, BASS808_PUNCH_TIME, BASS808_RELEASE, BASS808_SINE,
    BASS808_SQUARE, BASS808_SUSTAIN, BASS808_TUNE,
};
pub use crate::calibration::{
    analyze as analyze_latency, click_template, Measured as LatencyMeasurement,
    ProbeError as LatencyProbeError,
};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::capture::{
    input_devices, Capture, CaptureError, InputInfo, InputSource, Recorded, MAX_INPUT_CHANNELS,
};
pub use crate::clock::{song_position, tick_index, ClockMessage, TICKS_PER_BEAT};
pub use crate::convolver::{
    ir_preview, ir_preview_with, ir_samples, ir_samples_with, ir_seconds, sd, volume_env,
    Convolver, BLOCK as CONVOLVER_BLOCK, IR_NAMES, IR_PARAMS, MAX_IR_SECONDS,
};
pub use crate::core::{
    dsp_load, AudioClip, AuditionPlayer, CaptureSink, ClipEdit, Command, ControlEvent, EngineCore,
    EngineState, FadeCurve, Garbage, LiveMidiEvent, MidiOutEvent, NoteEvent, Shared, Song,
    StateReader, TrackMix, MASTER_TRACK, MAX_TRACKS,
};
pub use crate::disk::{
    ring_blocks_for, AudioError, AudioFile, DiskThread, FileSpan, RING_BLOCKS, RING_BLOCKS_MAX,
};
pub use crate::flac::{encode_flac, write_flac};
pub use crate::flex::{
    detect_notes, hz_to_midi, pitch_track, shift_pitch, warp, write_wav, DetectedNote, FlexMode,
    PitchShift, WarpPoint, MAX_POINTS as MAX_FLEX_POINTS,
};
pub use crate::freq_shifter::FreqShifterNode;
pub use crate::fx::{
    eq_response_db, response_db, Chain, Fx, FxKind, ParamInfo, Pdc, AMP_CABS, AMP_MODELS,
    MAX_FX_PARAMS, MAX_INSERTS,
};
pub use crate::melody::{Algorithm, Blob, NoteAnalysis};
pub use crate::meter::{
    db_to_gain, gain_to_db, LoudnessMeter, PeakMeter, RmsMeter, MIN_DB as METER_MIN_DB,
};
pub use crate::modal_voice::ModalVoice;
pub use crate::modulators::{LfoController, LfoShape, PeakFollower};
pub use crate::monitor::{InputFeed, MonitorTrack, MAX_MONITORS};
pub use crate::mtc::{
    full_frame, quarter_frame_data, rate_code, rate_of_code, MtcDecoder, MtcEvent, MtcGenerator,
    MtcMessage,
};
pub use crate::native_fx::{
    lemon_fast_distortion, lemon_limiter, lemon_soft_clipper, lemon_transient_processor,
    native_effects, native_info, open_native, open_native_block, open_tune_block, press, tune,
    BlockProcessor, BlockTransport, NativeFx, NativeFxInfo, NativeParam,
};
pub use crate::note_render::{NoteChange, Resynth, SegmentCache};
pub use crate::offline::{write_wav_with_options, BitDepth, DitherRng, OfflineBounceResult};
pub use crate::output::{
    buffer_choices, buffer_range, output_devices, peak_of, Engine, EngineError, OutputDevice,
    BUFFER_CHOICES,
};
pub use crate::pitch_tracker::{track_pitch, PitchTrack, PITCH_HOP};
pub use crate::plugin::{BlockEffect, PluginInstrument, PLUGIN_BLOCK};
pub use crate::plugin_track::PluginTrack;
pub use crate::prerender::{
    plugin_host_delay, prerender_delay, PROCESS_BUFFER_LARGE, PROCESS_BUFFER_MEDIUM,
    PROCESS_BUFFER_SMALL,
};
pub use crate::routing::{
    curve_shape, lane_value, send_gain, AutoLane, AutoPoint, AutoTarget, Route, SendSpec,
    MAX_AUTO_POINTS, MAX_BUSES, MAX_SENDS,
};
pub use crate::soundfont::{
    PresetInfo, SfPlayer, SoundFontBank, MAX_SOUNDFONT_BYTES, PITCH_BEND_CONTROL,
};
pub use crate::spectral::{spectrogram, Spectrogram};
pub use crate::synth::{Kit, SynthPatch, VoiceKind, Wave};
pub use crate::tempo_map::{
    MusicalPosition, SignatureEvent, Smpte, SmpteRate, TempoCurve, TempoEvent, TempoMap, MAX_BPM,
    MIN_BPM,
};
pub use crate::tri_osc::{ModAlgo as TriModAlgo, TriOsc, TriWave};
pub use crate::wave_draw::{
    InterpolationMode as WaveDrawInterpolation, WaveDraw, WAVETABLE_SIZE as WAVEDRAW_SIZE,
};
pub use crate::windows_audio::{enumerate_asio_drivers, pick_wasapi_period, AsioDriverDesc};
