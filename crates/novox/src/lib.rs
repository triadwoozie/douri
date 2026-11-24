//! Novox The Robot (vendored C++ engine, MIT) through Pro Wave's C interface, `shim/pw_novox.h`:
//! the raw declarations only. Pro Wave's safe wrapper is `prowave-app/src/novox_singer.rs`.
#![allow(non_camel_case_types)]
use std::ffi::c_char;

/// The CMU Pronouncing Dictionary as upstream ships it (`data/cmudict.dict`), gzipped here.
pub static CMUDICT_GZ: &[u8] = include_bytes!("../data/cmudict.dict.gz");

#[repr(C)]
pub struct pw_novox {
    _private: [u8; 0],
}

#[repr(C)]
pub struct pw_novox_stream {
    _private: [u8; 0],
}

/// The knobs of a voice, in the engine's units (see `shim/pw_novox.h`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct pw_novox_knobs {
    pub formant_scale: f32,
    pub breathiness: f32,
    pub brightness: f32,
    pub grit: f32,
    pub vibrato_depth: f32,
    pub vibrato_rate: f32,
    pub scoop: f32,
    pub portamento: f32,
    pub choir_size: i32,
}

/// One sung note (see `shim/pw_novox.h`).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct pw_novox_note {
    pub start: f64,
    pub dur: f64,
    pub pitch: f32,
    pub lyric: *const c_char,
}

unsafe extern "C" {
    pub fn pw_novox_new(dict: *const c_char, len: usize) -> *mut pw_novox;
    pub fn pw_novox_free(engine: *mut pw_novox);
    pub fn pw_novox_words(engine: *const pw_novox) -> usize;
    pub fn pw_novox_preset_count() -> usize;
    pub fn pw_novox_preset_name(index: usize) -> *const c_char;
    pub fn pw_novox_preset(name: *const c_char, out: *mut pw_novox_knobs) -> i32;
    pub fn pw_novox_stream_new(
        engine: *const pw_novox,
        preset: *const c_char,
        knobs: *const pw_novox_knobs,
        notes: *const pw_novox_note,
        count: usize,
        sample_rate: f64,
    ) -> *mut pw_novox_stream;
    pub fn pw_novox_stream_free(stream: *mut pw_novox_stream);
    pub fn pw_novox_stream_length(stream: *const pw_novox_stream) -> usize;
    pub fn pw_novox_stream_lead(stream: *const pw_novox_stream) -> f64;
    pub fn pw_novox_stream_process(stream: *mut pw_novox_stream, out: *mut f32, frames: usize)
        -> usize;
    pub fn pw_novox_knobs_size() -> usize;
    pub fn pw_novox_note_size() -> usize;
}
