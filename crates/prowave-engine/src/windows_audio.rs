// SPDX-License-Identifier: GPL-3.0-or-later
// Portions adapted from transit-audio/transit (MIT OR Apache-2.0, Copyright (c) 2024 transit contributors).

//! Windows native audio backends (SDK-free ASIO and low-latency WASAPI).
//!
//! Provides protocol-level Windows ASIO COM vtable and structures without requiring
//! the proprietary Steinberg ASIO SDK headers, alongside low-latency WASAPI shared/exclusive
//! period helpers.

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;

/// 32-bit signed integer matching Win32 `LONG`.
pub type AsioLong = i32;
/// ASIO boolean (`0` = false, `1` = true).
pub type AsioBool = i32;
pub const ASIO_TRUE: AsioBool = 1;
pub const ASIO_FALSE: AsioBool = 0;

/// ASIO error codes.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsioError {
    Ok = 0,
    Success = 0x3f4847a0,
    NotPresent = -1000,
    HWMalfunction = -999,
    InvalidParameter = -998,
    InvalidMode = -997,
    SPNotAdvancing = -996,
    NoClock = -995,
    NoMemory = -994,
}

/// ASIO sample formats.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsioSampleType {
    Int16MSB = 0,
    Int24MSB = 1,
    Int32MSB = 2,
    Float32MSB = 3,
    Float64MSB = 4,
    Int32MSB16 = 8,
    Int32MSB18 = 9,
    Int32MSB20 = 10,
    Int32MSB24 = 11,
    Int16LSB = 16,
    Int24LSB = 17,
    Int32LSB = 18,
    Float32LSB = 19,
    Float64LSB = 20,
    Int32LSB16 = 24,
    Int32LSB18 = 25,
    Int32LSB20 = 26,
    Int32LSB24 = 27,
    DSDInt8LSB1 = 32,
    DSDInt8MSB1 = 33,
    DSDInt8NER8 = 40,
}

/// 64-bit samples counter representation.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AsioSamples {
    pub lo: u32,
    pub hi: u32,
}

impl AsioSamples {
    pub const fn from_u64(val: u64) -> Self {
        Self {
            lo: val as u32,
            hi: (val >> 32) as u32,
        }
    }

    pub const fn as_u64(&self) -> u64 {
        ((self.hi as u64) << 32) | (self.lo as u64)
    }
}

/// Driver buffer information for inputs and outputs.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AsioBufferInfo {
    pub isInput: AsioBool,
    pub channelNum: AsioLong,
    pub buffers: [*mut c_void; 2],
}

/// Channel metadata from driver.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct AsioChannelInfo {
    pub channel: AsioLong,
    pub isInput: AsioBool,
    pub isActive: AsioBool,
    pub channelGroup: AsioLong,
    pub type_: AsioSampleType,
    pub name: [u8; 32],
}

/// Driver callbacks structure.
#[repr(C)]
pub struct AsioCallbacks {
    pub bufferSwitch: extern "C" fn(doubleBufferIndex: AsioLong, directProcess: AsioBool),
    pub sampleRateDidChange: extern "C" fn(sRate: f64),
    pub asioMessage: extern "C" fn(
        selector: AsioLong,
        value: AsioLong,
        message: *mut c_void,
        opt: *mut f64,
    ) -> AsioLong,
    pub bufferSwitchTimeInfo: extern "C" fn(
        params: *mut c_void,
        doubleBufferIndex: AsioLong,
        directProcess: AsioBool,
    ) -> *mut c_void,
}

/// Hand-written protocol-level `IASIO` virtual method table.
#[repr(C)]
pub struct IAsioVtbl {
    // IUnknown methods
    pub QueryInterface: unsafe extern "system" fn(
        this: *mut c_void,
        riid: *const [u8; 16],
        ppv: *mut *mut c_void,
    ) -> i32,
    pub AddRef: unsafe extern "system" fn(this: *mut c_void) -> u32,
    pub Release: unsafe extern "system" fn(this: *mut c_void) -> u32,

    // IASIO methods
    pub init: unsafe extern "C" fn(this: *mut c_void, sysHandle: *mut c_void) -> AsioBool,
    pub getDriverName: unsafe extern "C" fn(this: *mut c_void, name: *mut u8),
    pub getDriverVersion: unsafe extern "C" fn(this: *mut c_void) -> AsioLong,
    pub getErrorMessage: unsafe extern "C" fn(this: *mut c_void, string: *mut u8),
    pub start: unsafe extern "C" fn(this: *mut c_void) -> AsioLong,
    pub stop: unsafe extern "C" fn(this: *mut c_void) -> AsioLong,
    pub getChannels: unsafe extern "C" fn(
        this: *mut c_void,
        numInputChannels: *mut AsioLong,
        numOutputChannels: *mut AsioLong,
    ) -> AsioLong,
    pub getLatencies: unsafe extern "C" fn(
        this: *mut c_void,
        inputLatency: *mut AsioLong,
        outputLatency: *mut AsioLong,
    ) -> AsioLong,
    pub getBufferSize: unsafe extern "C" fn(
        this: *mut c_void,
        minSize: *mut AsioLong,
        maxSize: *mut AsioLong,
        preferredSize: *mut AsioLong,
        granularity: *mut AsioLong,
    ) -> AsioLong,
    pub canSampleRate: unsafe extern "C" fn(this: *mut c_void, sampleRate: f64) -> AsioLong,
    pub getSampleRate: unsafe extern "C" fn(this: *mut c_void, sampleRate: *mut f64) -> AsioLong,
    pub setSampleRate: unsafe extern "C" fn(this: *mut c_void, sampleRate: f64) -> AsioLong,
    pub getClockSources: unsafe extern "C" fn(
        this: *mut c_void,
        clocks: *mut c_void,
        numSources: *mut AsioLong,
    ) -> AsioLong,
    pub setClockSource: unsafe extern "C" fn(this: *mut c_void, reference: AsioLong) -> AsioLong,
    pub getSamplePosition: unsafe extern "C" fn(
        this: *mut c_void,
        sPos: *mut AsioSamples,
        tStamp: *mut c_void,
    ) -> AsioLong,
    pub getChannelInfo:
        unsafe extern "C" fn(this: *mut c_void, info: *mut AsioChannelInfo) -> AsioLong,
    pub createBuffers: unsafe extern "C" fn(
        this: *mut c_void,
        bufferInfos: *mut AsioBufferInfo,
        numChannels: AsioLong,
        bufferSize: AsioLong,
        callbacks: *const AsioCallbacks,
    ) -> AsioLong,
    pub disposeBuffers: unsafe extern "C" fn(this: *mut c_void) -> AsioLong,
    pub controlPanel: unsafe extern "C" fn(this: *mut c_void) -> AsioLong,
    pub future:
        unsafe extern "C" fn(this: *mut c_void, selector: AsioLong, opt: *mut c_void) -> AsioLong,
    pub outputReady: unsafe extern "C" fn(this: *mut c_void) -> AsioLong,
}

/// An installed ASIO driver entry discovered in the Windows registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsioDriverDesc {
    pub name: String,
    pub clsid: String,
    pub description: String,
}

/// Discovers installed ASIO drivers from registry (`HKLM\SOFTWARE\ASIO` and `HKCU\SOFTWARE\ASIO`).
pub fn enumerate_asio_drivers() -> Vec<AsioDriverDesc> {
    #[cfg(target_os = "windows")]
    {
        use windows::core::PCWSTR;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
            HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ,
        };

        let mut drivers = Vec::new();
        for &root in &[HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
            let subkey = windows::core::w!("SOFTWARE\\ASIO");
            let mut key = HKEY::default();
            let res = unsafe { RegOpenKeyExW(root, subkey, Some(0), KEY_READ, &mut key) };
            if res.is_ok() {
                let mut index = 0u32;
                let mut name_buf = [0u16; 256];
                loop {
                    let mut name_len = name_buf.len() as u32;
                    let status = unsafe {
                        RegEnumKeyExW(
                            key,
                            index,
                            Some(windows::core::PWSTR(name_buf.as_mut_ptr())),
                            &mut name_len,
                            None,
                            None,
                            None,
                            None,
                        )
                    };
                    if status.is_err() {
                        break;
                    }
                    let driver_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);

                    // Read CLSID
                    let mut child = HKEY::default();
                    let child_w: Vec<u16> = driver_name
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();
                    if unsafe {
                        RegOpenKeyExW(key, PCWSTR(child_w.as_ptr()), Some(0), KEY_READ, &mut child)
                    }
                    .is_ok()
                    {
                        let mut clsid_buf = [0u16; 128];
                        let mut clsid_len = (clsid_buf.len() * 2) as u32;
                        let mut reg_type = REG_SZ;
                        let clsid_res = unsafe {
                            RegQueryValueExW(
                                child,
                                windows::core::w!("CLSID"),
                                None,
                                Some(&mut reg_type),
                                Some(clsid_buf.as_mut_ptr() as *mut u8),
                                Some(&mut clsid_len),
                            )
                        };
                        let clsid = if clsid_res.is_ok() {
                            let chars = (clsid_len as usize / 2).saturating_sub(1);
                            String::from_utf16_lossy(&clsid_buf[..chars])
                        } else {
                            String::new()
                        };
                        unsafe {
                            let _ = RegCloseKey(child);
                        };

                        if !drivers
                            .iter()
                            .any(|d: &AsioDriverDesc| d.name == driver_name)
                        {
                            drivers.push(AsioDriverDesc {
                                name: driver_name.clone(),
                                clsid,
                                description: driver_name,
                            });
                        }
                    }
                    index += 1;
                }
                unsafe {
                    let _ = RegCloseKey(key);
                };
            }
        }
        drivers
    }
    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }
}

/// WASAPI low-latency buffer selection calculation.
pub fn pick_wasapi_period(
    buffer_frames: Option<u32>,
    step_frames: u32,
    min_frames: u32,
    max_frames: u32,
) -> u32 {
    let requested = buffer_frames.unwrap_or(min_frames);
    let clamped = requested.clamp(min_frames, max_frames);
    if step_frames > 0 {
        let remainder = (clamped - min_frames) % step_frames;
        if remainder != 0 {
            (clamped + (step_frames - remainder)).min(max_frames)
        } else {
            clamped
        }
    } else {
        clamped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asio_samples_conversion() {
        let s = AsioSamples::from_u64(0x1234_5678_9abc_def0);
        assert_eq!(s.lo, 0x9abc_def0);
        assert_eq!(s.hi, 0x1234_5678);
        assert_eq!(s.as_u64(), 0x1234_5678_9abc_def0);
    }

    #[test]
    fn wasapi_period_selection_matches_spec() {
        assert_eq!(pick_wasapi_period(None, 16, 48, 480), 48);
        assert_eq!(pick_wasapi_period(Some(128), 16, 48, 480), 128);
        assert_eq!(pick_wasapi_period(Some(130), 16, 48, 480), 144);
        assert_eq!(pick_wasapi_period(Some(10), 16, 48, 480), 48);
        assert_eq!(pick_wasapi_period(Some(1024), 16, 48, 480), 480);
    }
}
