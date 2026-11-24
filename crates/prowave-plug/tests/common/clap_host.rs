// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! A minimal CLAP host: drives a plug-in through nih-plug's CLAP wrapper (`clap_entry` →
//! factory → plugin → activate → process), the path a DAW takes. Used by the tests (the plug-in
//! linked in) and by the `check_bundle` example (a built `.clap` opened from disk). In debug
//! builds the wrapper runs each `process` inside the allocation checker.
#![allow(dead_code)]

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::events::*;
use clap_sys::ext::audio_ports::{
    clap_audio_port_info, clap_plugin_audio_ports, CLAP_EXT_AUDIO_PORTS,
};
use clap_sys::ext::latency::{clap_plugin_latency, CLAP_EXT_LATENCY};
use clap_sys::ext::params::{clap_param_info, clap_plugin_params, CLAP_EXT_PARAMS};
use clap_sys::ext::state::{clap_plugin_state, CLAP_EXT_STATE};
use clap_sys::factory::plugin_factory::{clap_plugin_factory, CLAP_PLUGIN_FACTORY_ID};
use clap_sys::host::clap_host;
use clap_sys::plugin::clap_plugin;
use clap_sys::process::clap_process;
use clap_sys::stream::{clap_istream, clap_ostream};
use clap_sys::version::CLAP_VERSION;
use std::ffi::{c_char, c_void, CStr};
use std::ptr::null;

#[derive(Clone, Copy, Debug)]
pub enum Ev {
    On(u8, f64),
    Off(u8),
    /// pitch bend, -1..1
    Bend(f64),
    /// parameter by index, plain value
    Param(usize, f64),
}

#[repr(C)]
#[derive(Clone, Copy)]
union Raw {
    header: clap_event_header,
    note: clap_event_note,
    midi: clap_event_midi,
    param: clap_event_param_value,
}

unsafe extern "C" fn host_ext(_h: *const clap_host, _id: *const c_char) -> *const c_void {
    null()
}
unsafe extern "C" fn host_noop(_h: *const clap_host) {}

unsafe extern "C" fn ev_size(list: *const clap_input_events) -> u32 {
    let v = &*((*list).ctx as *const Vec<Raw>);
    v.len() as u32
}
unsafe extern "C" fn ev_get(list: *const clap_input_events, i: u32) -> *const clap_event_header {
    let v = &*((*list).ctx as *const Vec<Raw>);
    &v[i as usize].header
}
unsafe extern "C" fn out_push(_l: *const clap_output_events, _e: *const clap_event_header) -> bool {
    true
}

/// A parameter as the plug-in describes it.
#[derive(Clone, Debug)]
pub struct ParamInfo {
    pub id: u32,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

pub struct Host {
    _host: Box<clap_host>,
    plugin: *const clap_plugin,
    pub rate: f64,
    pub max_block: u32,
    pub params: Vec<ParamInfo>,
    pub plugin_id: String,
    pub plugin_name: String,
    /// Channels of the main input (0 for an instrument).
    pub inputs: u32,
    events: Vec<Raw>,
    steady: i64,
}

impl Host {
    /// The first plug-in of `entry` (initialised with `path`), activated at `rate`.
    pub fn new(entry: &clap_plugin_entry, path: &CStr, rate: f64, max_block: u32) -> Host {
        unsafe {
            assert!((entry.init.unwrap())(path.as_ptr()));
            let f = (entry.get_factory.unwrap())(CLAP_PLUGIN_FACTORY_ID.as_ptr())
                as *const clap_plugin_factory;
            assert!(!f.is_null());
            assert_eq!(
                ((*f).get_plugin_count.unwrap())(f),
                1,
                "one plug-in per bundle"
            );
            let desc = ((*f).get_plugin_descriptor.unwrap())(f, 0);
            let plugin_id = CStr::from_ptr((*desc).id).to_string_lossy().into_owned();
            let plugin_name = CStr::from_ptr((*desc).name).to_string_lossy().into_owned();
            let host = Box::new(clap_host {
                clap_version: CLAP_VERSION,
                host_data: std::ptr::null_mut(),
                name: c"test host".as_ptr(),
                vendor: c"test".as_ptr(),
                url: c"".as_ptr(),
                version: c"1".as_ptr(),
                get_extension: Some(host_ext),
                request_restart: Some(host_noop),
                request_process: Some(host_noop),
                request_callback: Some(host_noop),
            });
            let plugin = ((*f).create_plugin.unwrap())(f, &*host, (*desc).id);
            assert!(!plugin.is_null());
            assert!(((*plugin).init.unwrap())(plugin));
            let mut h = Host {
                _host: host,
                plugin,
                rate,
                max_block,
                params: Vec::new(),
                plugin_id,
                plugin_name,
                inputs: 0,
                events: Vec::with_capacity(4096),
                steady: 0,
            };
            h.read_params();
            h.inputs = h.input_channels();
            assert!(((*plugin).activate.unwrap())(plugin, rate, 1, max_block));
            assert!(((*plugin).start_processing.unwrap())(plugin));
            h
        }
    }

    unsafe fn ext<T>(&self, id: &CStr) -> *const T {
        ((*self.plugin).get_extension.unwrap())(self.plugin, id.as_ptr()) as *const T
    }

    fn read_params(&mut self) {
        unsafe {
            let p = self.ext::<clap_plugin_params>(CLAP_EXT_PARAMS);
            let n = ((*p).count.unwrap())(self.plugin);
            for i in 0..n {
                let mut info: clap_param_info = std::mem::zeroed();
                assert!(((*p).get_info.unwrap())(self.plugin, i, &mut info));
                let name = CStr::from_ptr(info.name.as_ptr())
                    .to_string_lossy()
                    .into_owned();
                self.params.push(ParamInfo {
                    id: info.id,
                    name,
                    min: info.min_value,
                    max: info.max_value,
                    default: info.default_value,
                });
            }
        }
    }

    fn input_channels(&self) -> u32 {
        unsafe {
            let a = self.ext::<clap_plugin_audio_ports>(CLAP_EXT_AUDIO_PORTS);
            if a.is_null() || ((*a).count.unwrap())(self.plugin, true) == 0 {
                return 0;
            }
            let mut info: clap_audio_port_info = std::mem::zeroed();
            assert!(((*a).get.unwrap())(self.plugin, 0, true, &mut info));
            info.channel_count
        }
    }

    /// The text the plug-in shows for parameter `index` at `value`.
    pub fn value_text(&self, index: usize, value: f64) -> String {
        unsafe {
            let p = self.ext::<clap_plugin_params>(CLAP_EXT_PARAMS);
            let mut buf = [0 as c_char; 256];
            let ok = ((*p).value_to_text.unwrap())(
                self.plugin,
                self.params[index].id,
                value,
                buf.as_mut_ptr(),
                buf.len() as u32,
            );
            if !ok {
                return String::new();
            }
            CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
        }
    }

    /// Parameter `index`'s value now.
    pub fn param_value(&self, index: usize) -> f64 {
        unsafe {
            let p = self.ext::<clap_plugin_params>(CLAP_EXT_PARAMS);
            let mut v = 0.0;
            assert!(((*p).get_value.unwrap())(
                self.plugin,
                self.params[index].id,
                &mut v
            ));
            v
        }
    }

    pub fn latency(&self) -> u32 {
        unsafe {
            let l = self.ext::<clap_plugin_latency>(CLAP_EXT_LATENCY);
            if l.is_null() {
                return 0;
            }
            ((*l).get.unwrap())(self.plugin)
        }
    }

    pub fn save_state(&self) -> Vec<u8> {
        unsafe extern "C" fn write(s: *const clap_ostream, buf: *const c_void, size: u64) -> i64 {
            let v = &mut *((*s).ctx as *mut Vec<u8>);
            v.extend_from_slice(std::slice::from_raw_parts(buf as *const u8, size as usize));
            size as i64
        }
        let mut out: Vec<u8> = Vec::new();
        unsafe {
            let st = self.ext::<clap_plugin_state>(CLAP_EXT_STATE);
            let stream = clap_ostream {
                ctx: &mut out as *mut _ as *mut c_void,
                write: Some(write),
            };
            assert!(((*st).save.unwrap())(self.plugin, &stream));
        }
        out
    }

    pub fn load_state(&self, data: &[u8]) {
        struct Reader<'a>(&'a [u8], usize);
        unsafe extern "C" fn read(s: *const clap_istream, buf: *mut c_void, size: u64) -> i64 {
            let r = &mut *((*s).ctx as *mut Reader);
            let n = (r.0.len() - r.1).min(size as usize);
            std::ptr::copy_nonoverlapping(r.0[r.1..].as_ptr(), buf as *mut u8, n);
            r.1 += n;
            n as i64
        }
        let mut r = Reader(data, 0);
        unsafe {
            let st = self.ext::<clap_plugin_state>(CLAP_EXT_STATE);
            let stream = clap_istream {
                ctx: &mut r as *mut _ as *mut c_void,
                read: Some(read),
            };
            assert!(((*st).load.unwrap())(self.plugin, &stream));
        }
    }

    /// One block in place: `l`/`r` are the input (for an effect) and get the output. `events`
    /// at frame offsets within it, sorted.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], events: &[(u32, Ev)]) {
        let n = l.len().min(r.len());
        assert!(n as u32 <= self.max_block);
        self.events.clear();
        for &(t, e) in events {
            let header = |size: usize, type_: u16| clap_event_header {
                size: size as u32,
                time: t,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_,
                flags: 0,
            };
            let note = |k: u8, v: f64, type_: u16| Raw {
                note: clap_event_note {
                    header: header(size_of::<clap_event_note>(), type_),
                    note_id: -1,
                    port_index: 0,
                    channel: 0,
                    key: k as i16,
                    velocity: v,
                },
            };
            let raw = match e {
                Ev::On(k, v) => note(k, v, CLAP_EVENT_NOTE_ON),
                Ev::Off(k) => note(k, 0.0, CLAP_EVENT_NOTE_OFF),
                Ev::Bend(b) => {
                    let v =
                        (((b.clamp(-1.0, 1.0) + 1.0) * 0.5 * 16383.0).round() as u16).min(16383);
                    Raw {
                        midi: clap_event_midi {
                            header: header(size_of::<clap_event_midi>(), CLAP_EVENT_MIDI),
                            port_index: 0,
                            data: [0xE0, (v & 0x7f) as u8, (v >> 7) as u8],
                        },
                    }
                }
                Ev::Param(k, v) => Raw {
                    param: clap_event_param_value {
                        header: header(size_of::<clap_event_param_value>(), CLAP_EVENT_PARAM_VALUE),
                        param_id: self.params[k].id,
                        cookie: std::ptr::null_mut(),
                        note_id: -1,
                        port_index: -1,
                        channel: -1,
                        key: -1,
                        value: v,
                    },
                },
            };
            self.events.push(raw);
        }
        let in_events = clap_input_events {
            ctx: &self.events as *const _ as *mut c_void,
            size: Some(ev_size),
            get: Some(ev_get),
        };
        let out_events = clap_output_events {
            ctx: std::ptr::null_mut(),
            try_push: Some(out_push),
        };
        let mut transport: clap_event_transport = unsafe { std::mem::zeroed() };
        transport.header = clap_event_header {
            size: size_of::<clap_event_transport>() as u32,
            time: 0,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_: CLAP_EVENT_TRANSPORT,
            flags: 0,
        };
        transport.flags = CLAP_TRANSPORT_HAS_TEMPO | CLAP_TRANSPORT_IS_PLAYING;
        transport.tempo = 120.0;
        transport.tsig_num = 4;
        transport.tsig_denom = 4;
        // nih-plug processes in place when the input and output buffers are the same.
        let mut ptrs = [l.as_mut_ptr(), r.as_mut_ptr()];
        let channels = if self.inputs == 1 { 1 } else { 2 };
        let mut out = clap_audio_buffer {
            data32: ptrs.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: channels,
            latency: 0,
            constant_mask: 0,
        };
        let input = clap_audio_buffer {
            data32: ptrs.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: channels,
            latency: 0,
            constant_mask: 0,
        };
        let proc_ = clap_process {
            steady_time: self.steady,
            frames_count: n as u32,
            transport: &transport,
            audio_inputs: if self.inputs > 0 { &input } else { null() },
            audio_outputs: &mut out,
            audio_inputs_count: u32::from(self.inputs > 0),
            audio_outputs_count: 1,
            in_events: &in_events,
            out_events: &out_events,
        };
        unsafe {
            ((*self.plugin).process.unwrap())(self.plugin, &proc_);
        }
        self.steady += n as i64;
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        unsafe {
            ((*self.plugin).stop_processing.unwrap())(self.plugin);
            ((*self.plugin).deactivate.unwrap())(self.plugin);
            ((*self.plugin).destroy.unwrap())(self.plugin);
        }
    }
}

/// A timed score: (seconds, event).
pub type Score = Vec<(f64, Ev)>;

/// Renders `secs` seconds of `score` in blocks of the given sizes (cycled). `input` makes each
/// input frame (an effect's test signal; ignored by instruments). Returns left and right.
pub fn render(
    h: &mut Host,
    score: &Score,
    secs: f64,
    blocks: &[usize],
    input: &mut dyn FnMut(usize) -> (f32, f32),
) -> (Vec<f32>, Vec<f32>) {
    let total = (secs * h.rate) as usize;
    let mut left = vec![0f32; total];
    let mut right = vec![0f32; total];
    if h.inputs > 0 {
        for i in 0..total {
            let (a, b) = input(i);
            left[i] = a;
            right[i] = b;
        }
    }
    let mut at = 0usize;
    let mut k = 0usize;
    let mut sorted = score.clone();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut next = 0usize;
    let mut evs: Vec<(u32, Ev)> = Vec::with_capacity(256);
    while at < total {
        let n = blocks[k % blocks.len()].min(total - at);
        k += 1;
        evs.clear();
        while next < sorted.len() && ((sorted[next].0 * h.rate) as usize) < at + n {
            let t = ((sorted[next].0 * h.rate) as usize).max(at) - at;
            evs.push((t as u32, sorted[next].1));
            next += 1;
        }
        let (l, r) = (&mut left[at..at + n], &mut right[at..at + n]);
        h.process(l, r, &evs);
        at += n;
    }
    (left, right)
}

pub fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

pub fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// A deterministic test signal: a 220 Hz sine with a little noise, -12 dBFS, and the right
/// channel a fifth above.
pub fn test_signal(rate: f64) -> impl FnMut(usize) -> (f32, f32) {
    let mut seed = 0x1234_5678u32;
    move |i| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let n = (seed as f32 / u32::MAX as f32 - 0.5) * 0.02;
        let t = i as f64 / rate;
        let a = 0.25 * (t * 220.0 * std::f64::consts::TAU).sin() as f32;
        let b = 0.25 * (t * 330.0 * std::f64::consts::TAU).sin() as f32;
        (a + n, b + n)
    }
}

/// The score every check plays an instrument: a chord (with 36, the Drum Synth's first pad),
/// a bend, and the notes let go.
pub fn instrument_score() -> Score {
    vec![
        (0.05, Ev::On(36, 1.0)),
        (0.05, Ev::On(48, 0.9)),
        (0.05, Ev::On(60, 0.8)),
        (0.30, Ev::On(64, 0.7)),
        (0.60, Ev::Bend(0.5)),
        (0.90, Ev::Bend(0.0)),
        (1.20, Ev::Off(36)),
        (1.20, Ev::Off(48)),
        (1.20, Ev::Off(60)),
        (1.20, Ev::Off(64)),
    ]
}
