//! The C interface (shim/pw_chip.h) of the vendored sound-chip and FM cores, and safe owners
//! of its objects. Each object is made and dropped off the audio thread; playing and
//! rendering do not allocate, lock or unwind.

pub const FM6_RATE: u32 = 44_100;
pub const FM6_BLOCK: usize = 64;
pub const FM6_SLOTS: usize = 16;
/// A voice: 155 unpacked parameter bytes, in the order of the DX7's voice dump.
pub const FM6_VOICE: usize = 155;
/// A voice as it is packed in a 32-voice bank.
pub const FM6_PACKED: usize = 128;

#[repr(C)]
struct PwFm6 {
    _private: [u8; 0],
}

extern "C" {
    fn pw_fm6_new() -> *mut PwFm6;
    fn pw_fm6_free(fm: *mut PwFm6);
    fn pw_fm6_set_voice(fm: *mut PwFm6, voice: *const u8);
    fn pw_fm6_note_on(fm: *mut PwFm6, slot: i32, note: i32, velocity: i32);
    fn pw_fm6_note_off(fm: *mut PwFm6, slot: i32);
    fn pw_fm6_render(fm: *mut PwFm6, mask: u32, bend: f32, out: *mut f32, peaks: *mut f32);
    fn pw_fm6_unpack(packed: *const u8, voice: *mut u8);
}

/// The six-operator FM synthesizer: 44.1 kHz, [`FM6_BLOCK`] frames a call, [`FM6_SLOTS`]
/// notes at once. The caller says which slot plays which note.
pub struct Fm6(std::ptr::NonNull<PwFm6>);

// SAFETY: the object is plain memory owned by this handle, used by one thread at a time
// (`&mut self`). The core's tables are shared and only read once made (in `new`, under a
// lock).
unsafe impl Send for Fm6 {}

impl Fm6 {
    /// `None` when memory runs out. Allocates, and the first call takes a lock.
    pub fn new() -> Option<Self> {
        // SAFETY: returns a new object or null.
        std::ptr::NonNull::new(unsafe { pw_fm6_new() }).map(Fm6)
    }

    /// The voice the next notes play. Each byte must be within its parameter's range.
    pub fn set_voice(&mut self, voice: &[u8; FM6_VOICE]) {
        // SAFETY: the object is live and `voice` holds the 155 bytes read.
        unsafe { pw_fm6_set_voice(self.0.as_ptr(), voice.as_ptr()) }
    }

    pub fn note_on(&mut self, slot: usize, note: u8, velocity: u8) {
        // SAFETY: the object is live; the C side ignores a slot out of range.
        unsafe { pw_fm6_note_on(self.0.as_ptr(), slot as i32, i32::from(note), i32::from(velocity)) }
    }

    pub fn note_off(&mut self, slot: usize) {
        // SAFETY: as `note_on`.
        unsafe { pw_fm6_note_off(self.0.as_ptr(), slot as i32) }
    }

    /// One block of the slots in `mask`, summed into `out`; `peaks` gets each slot's own
    /// peak. `bend` is in semitones.
    pub fn render(
        &mut self,
        mask: u32,
        bend: f32,
        out: &mut [f32; FM6_BLOCK],
        peaks: &mut [f32; FM6_SLOTS],
    ) {
        // SAFETY: the object is live and both arrays have the sizes the C side writes.
        unsafe { pw_fm6_render(self.0.as_ptr(), mask, bend, out.as_mut_ptr(), peaks.as_mut_ptr()) }
    }
}

impl Drop for Fm6 {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `pw_fm6_new` and is freed once.
        unsafe { pw_fm6_free(self.0.as_ptr()) }
    }
}

/// A bank's packed voice as unpacked parameter bytes (the core's own unpacking).
pub fn fm6_unpack(packed: &[u8; FM6_PACKED]) -> [u8; FM6_VOICE] {
    let mut voice = [0u8; FM6_VOICE];
    // SAFETY: both arrays have the sizes the C side reads and writes.
    unsafe { pw_fm6_unpack(packed.as_ptr(), voice.as_mut_ptr()) }
    voice
}

/// The chips [`Chip`] can be: `Chip::new`'s `kind`.
pub const CHIP_OPL2: i32 = 0;
pub const CHIP_OPN2: i32 = 1;
pub const CHIP_OPLL: i32 = 2;
pub const CHIP_SID: i32 = 3;
pub const CHIP_PSG: i32 = 4;

#[repr(C)]
struct PwChip {
    _private: [u8; 0],
}

extern "C" {
    fn pw_chip_new(kind: i32) -> *mut PwChip;
    fn pw_chip_free(chip: *mut PwChip);
    fn pw_chip_rate(chip: *const PwChip) -> u32;
    fn pw_chip_reset(chip: *mut PwChip);
    fn pw_chip_write(chip: *mut PwChip, reg: u32, data: u8);
    fn pw_chip_render(chip: *mut PwChip, left: *mut f32, right: *mut f32, frames: u32);
}

/// A sound chip driven by register writes.
pub struct Chip(std::ptr::NonNull<PwChip>);

// SAFETY: the object is plain memory owned by this handle, used by one thread at a time
// (`&mut self`); the cores keep no state outside it but constant tables.
unsafe impl Send for Chip {}

impl Chip {
    /// `None` for an unknown kind or when memory runs out. Allocates.
    pub fn new(kind: i32) -> Option<Self> {
        // SAFETY: returns a new object or null.
        std::ptr::NonNull::new(unsafe { pw_chip_new(kind) }).map(Chip)
    }

    /// The rate of its output, in Hz.
    pub fn rate(&self) -> u32 {
        // SAFETY: the object is live.
        unsafe { pw_chip_rate(self.0.as_ptr()) }
    }

    /// As after power-on. No allocation.
    pub fn reset(&mut self) {
        // SAFETY: the object is live.
        unsafe { pw_chip_reset(self.0.as_ptr()) }
    }

    pub fn write(&mut self, reg: u32, data: u8) {
        // SAFETY: the object is live; the C side takes any register number.
        unsafe { pw_chip_write(self.0.as_ptr(), reg, data) }
    }

    /// The next frames into `left` and `right` (as many as the shorter holds).
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        // SAFETY: the object is live and both slices hold `n` frames.
        unsafe { pw_chip_render(self.0.as_ptr(), left.as_mut_ptr(), right.as_mut_ptr(), n as u32) }
    }
}

impl Drop for Chip {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `pw_chip_new` and is freed once.
        unsafe { pw_chip_free(self.0.as_ptr()) }
    }
}

/// Why a SMAF file could not be rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmafError {
    /// The bytes are not a SMAF file.
    NotSmaf,
    /// The file has no track the player can play.
    NothingToPlay,
    /// Memory ran out, or the player failed.
    Failed,
}

extern "C" {
    fn pw_smaf_render(
        data: *const u8,
        size: usize,
        rate: u32,
        max_seconds: u32,
        out: *mut *mut f32,
        frames: *mut usize,
    ) -> i32;
    fn pw_smaf_free(samples: *mut f32);
}

/// Renders a SMAF file (.mmf) at `rate` Hz, at most `max_seconds` of it, to interleaved
/// stereo. Allocates and takes as long as the file: not for the audio thread.
pub fn smaf_render(data: &[u8], rate: u32, max_seconds: u32) -> Result<Vec<f32>, SmafError> {
    let mut out: *mut f32 = std::ptr::null_mut();
    let mut frames = 0usize;
    // SAFETY: `data` is valid for `data.len()` bytes; `out` and `frames` are valid to write.
    let code =
        unsafe { pw_smaf_render(data.as_ptr(), data.len(), rate, max_seconds, &mut out, &mut frames) };
    match code {
        0 if !out.is_null() => {
            // SAFETY: on success `out` holds `frames` stereo frames, owned by us until freed.
            let samples = unsafe { std::slice::from_raw_parts(out, frames * 2) }.to_vec();
            // SAFETY: `out` came from `pw_smaf_render` and is freed once.
            unsafe { pw_smaf_free(out) };
            Ok(samples)
        }
        1 => Err(SmafError::NotSmaf),
        2 => Err(SmafError::NothingToPlay),
        _ => Err(SmafError::Failed),
    }
}
