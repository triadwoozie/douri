//! The C interface (shim/pw_mi.h) of the vendored resonator, spectral processor and digital
//! oscillator, and safe owners of its objects. Each object is made and dropped off the audio
//! thread; processing does not allocate, lock or unwind.
//!
//! The C++ keeps one random-number state for the whole process (`stmlib::Random`), read and
//! written without a lock by every object: two objects on two threads make each other's
//! noise sequences different from run to run. Nothing else is shared.

pub const RESONATOR_RATE: u32 = 48_000;
pub const RESONATOR_BLOCK: usize = 24;
pub const RESONATOR_MODELS: usize = 6;
pub const RESONATOR_CHORDS: usize = 11;
pub const SPECTRAL_RATE: u32 = 32_000;
pub const SPECTRAL_BLOCK: usize = 32;
pub const OSCILLATOR_RATE: u32 = 96_000;
pub const OSCILLATOR_BLOCK: usize = 24;
pub const OSCILLATOR_SHAPES: usize = 47;

/// One block's settings for the resonator (shim/pw_mi.h `PwResonatorBlock`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ResonatorBlock {
    pub model: i32,
    pub polyphony: i32,
    pub structure: f32,
    pub brightness: f32,
    pub damping: f32,
    pub position: f32,
    pub note: f32,
    pub fm: f32,
    pub chord: i32,
    pub strum: i32,
    pub external: i32,
}

/// One block's settings for the spectral processor (`PwSpectralBlock`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpectralBlock {
    pub position: f32,
    pub size: f32,
    pub pitch: f32,
    pub density: f32,
    pub texture: f32,
    pub dry_wet: f32,
    pub feedback: f32,
    pub reverb: f32,
    pub freeze: i32,
}

#[repr(C)]
struct PwResonator {
    _private: [u8; 0],
}
#[repr(C)]
struct PwSpectral {
    _private: [u8; 0],
}
#[repr(C)]
struct PwOscillator {
    _private: [u8; 0],
}

extern "C" {
    fn pw_resonator_new() -> *mut PwResonator;
    fn pw_resonator_free(r: *mut PwResonator);
    fn pw_resonator_reset(r: *mut PwResonator);
    fn pw_resonator_process(
        r: *mut PwResonator,
        block: *const ResonatorBlock,
        input: *const f32,
        out: *mut f32,
        aux: *mut f32,
        size: usize,
    );
    fn pw_spectral_new() -> *mut PwSpectral;
    fn pw_spectral_free(s: *mut PwSpectral);
    fn pw_spectral_process(
        s: *mut PwSpectral,
        block: *const SpectralBlock,
        input: *const i16,
        out: *mut i16,
    );
    fn pw_oscillator_new() -> *mut PwOscillator;
    fn pw_oscillator_free(o: *mut PwOscillator);
    fn pw_oscillator_render(
        o: *mut PwOscillator,
        shape: i32,
        pitch: i32,
        timbre: i32,
        colour: i32,
        strike: i32,
        out: *mut i16,
    );
}

/// The resonator: 48 kHz, up to [`RESONATOR_BLOCK`] frames a call.
pub struct Resonator(std::ptr::NonNull<PwResonator>);

// SAFETY: the object is plain memory owned by this handle, used by one thread at a time
// (`&mut self`); see the module note on the shared random-number state.
unsafe impl Send for Resonator {}

impl Resonator {
    /// `None` when memory runs out. Allocates.
    pub fn new() -> Option<Self> {
        // SAFETY: returns a new object or null.
        std::ptr::NonNull::new(unsafe { pw_resonator_new() }).map(Resonator)
    }

    /// Back to silence. No allocation.
    pub fn reset(&mut self) {
        // SAFETY: the pointer is the live object this handle owns.
        unsafe { pw_resonator_reset(self.0.as_ptr()) }
    }

    /// The next frames into `out` and `aux`: as many as the shortest of the slices, at most
    /// [`RESONATOR_BLOCK`]. `input` excites the resonator when `block.external` is set.
    /// Returns the frames written.
    pub fn process(
        &mut self,
        block: &ResonatorBlock,
        input: Option<&[f32]>,
        out: &mut [f32],
        aux: &mut [f32],
    ) -> usize {
        let mut n = out.len().min(aux.len()).min(RESONATOR_BLOCK);
        if let Some(i) = input {
            n = n.min(i.len());
        }
        let ptr = input.map_or(std::ptr::null(), <[f32]>::as_ptr);
        // SAFETY: the object is live; every slice holds at least `n` frames, and the C side
        // reads and writes no more than `n`.
        unsafe {
            pw_resonator_process(self.0.as_ptr(), block, ptr, out.as_mut_ptr(), aux.as_mut_ptr(), n)
        }
        n
    }
}

impl Drop for Resonator {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `pw_resonator_new` and is freed once.
        unsafe { pw_resonator_free(self.0.as_ptr()) }
    }
}

/// The spectral processor: 32 kHz, exactly [`SPECTRAL_BLOCK`] stereo frames a call.
pub struct Spectral(std::ptr::NonNull<PwSpectral>);

// SAFETY: as `Resonator`.
unsafe impl Send for Spectral {}

impl Spectral {
    /// `None` when memory runs out. Allocates.
    pub fn new() -> Option<Self> {
        // SAFETY: returns a new object or null.
        std::ptr::NonNull::new(unsafe { pw_spectral_new() }).map(Spectral)
    }

    /// One block: interleaved stereo in, interleaved stereo out.
    pub fn process(
        &mut self,
        block: &SpectralBlock,
        input: &[i16; SPECTRAL_BLOCK * 2],
        out: &mut [i16; SPECTRAL_BLOCK * 2],
    ) {
        // SAFETY: the object is live and both arrays hold one block of stereo frames.
        unsafe { pw_spectral_process(self.0.as_ptr(), block, input.as_ptr(), out.as_mut_ptr()) }
    }
}

impl Drop for Spectral {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `pw_spectral_new` and is freed once.
        unsafe { pw_spectral_free(self.0.as_ptr()) }
    }
}

/// The digital oscillator: 96 kHz, exactly [`OSCILLATOR_BLOCK`] frames a call.
pub struct Oscillator(std::ptr::NonNull<PwOscillator>);

// SAFETY: as `Resonator`.
unsafe impl Send for Oscillator {}

impl Oscillator {
    /// `None` when memory runs out. Allocates.
    pub fn new() -> Option<Self> {
        // SAFETY: returns a new object or null.
        std::ptr::NonNull::new(unsafe { pw_oscillator_new() }).map(Oscillator)
    }

    /// One block. `pitch` is the MIDI note times 128; `timbre` and `colour` are 0 to 32767;
    /// `strike` starts a note.
    pub fn render(
        &mut self,
        shape: i32,
        pitch: i32,
        timbre: i32,
        colour: i32,
        strike: bool,
        out: &mut [i16; OSCILLATOR_BLOCK],
    ) {
        // SAFETY: the object is live and `out` holds one block.
        unsafe {
            pw_oscillator_render(
                self.0.as_ptr(),
                shape,
                pitch,
                timbre,
                colour,
                i32::from(strike),
                out.as_mut_ptr(),
            )
        }
    }
}

impl Drop for Oscillator {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `pw_oscillator_new` and is freed once.
        unsafe { pw_oscillator_free(self.0.as_ptr()) }
    }
}
