// Written for Pro Wave (MIT, as the rest of this crate): a C interface over the vendored DSP
// classes. Every object is made and freed off the audio thread; the process functions do not
// allocate, lock or throw.
#ifndef PW_MI_H_
#define PW_MI_H_

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// ---- The resonator (rings::Part). 48 kHz, blocks of at most PW_RESONATOR_BLOCK frames.

#define PW_RESONATOR_RATE 48000
#define PW_RESONATOR_BLOCK 24
#define PW_RESONATOR_MODELS 6
#define PW_RESONATOR_CHORDS 11

typedef struct PwResonator PwResonator;

typedef struct PwResonatorBlock {
  // 0 modal, 1 sympathetic strings, 2 string, 3 FM voice, 4 quantized sympathetic strings,
  // 5 string and reverb.
  int32_t model;
  // 1, 2 or 4.
  int32_t polyphony;
  // 0 to 1.
  float structure;
  float brightness;
  float damping;
  float position;
  // The MIDI note.
  float note;
  // Semitones added to it (tuning, the pitch wheel).
  float fm;
  int32_t chord;
  // A note begins in this block (ignored when `external`).
  int32_t strum;
  // The input excites the resonator, and its onsets strum it; otherwise `in` is ignored
  // and the resonator's own exciter plays.
  int32_t external;
} PwResonatorBlock;

// NULL when memory runs out.
PwResonator* pw_resonator_new(void);
void pw_resonator_free(PwResonator* r);
// Back to silence.
void pw_resonator_reset(PwResonator* r);
// `size` frames (at most PW_RESONATOR_BLOCK) from `in` (may be NULL) into `out` and `aux`.
void pw_resonator_process(PwResonator* r, const PwResonatorBlock* block, const float* in,
                          float* out, float* aux, size_t size);

// ---- The spectral processor (clouds::GranularProcessor in its spectral mode). 32 kHz,
// blocks of exactly PW_SPECTRAL_BLOCK frames.

#define PW_SPECTRAL_RATE 32000
#define PW_SPECTRAL_BLOCK 32

typedef struct PwSpectral PwSpectral;

typedef struct PwSpectralBlock {
  // 0 to 1.
  float position;
  float size;
  // Semitones, -48 to 48.
  float pitch;
  float density;
  float texture;
  float dry_wet;
  float feedback;
  float reverb;
  int32_t freeze;
} PwSpectralBlock;

PwSpectral* pw_spectral_new(void);
void pw_spectral_free(PwSpectral* s);
// PW_SPECTRAL_BLOCK interleaved stereo frames from `in` into `out`.
void pw_spectral_process(PwSpectral* s, const PwSpectralBlock* block, const int16_t* in,
                         int16_t* out);

// ---- The digital oscillator (braids::MacroOscillator). 96 kHz, blocks of exactly
// PW_OSCILLATOR_BLOCK frames.

#define PW_OSCILLATOR_RATE 96000
#define PW_OSCILLATOR_BLOCK 24
#define PW_OSCILLATOR_SHAPES 47

typedef struct PwOscillator PwOscillator;

PwOscillator* pw_oscillator_new(void);
void pw_oscillator_free(PwOscillator* o);
// One block into `out`. `pitch` is the MIDI note times 128; `timbre` and `colour` are 0 to
// 32767; `strike` starts a note (the struck and plucked shapes).
void pw_oscillator_render(PwOscillator* o, int32_t shape, int32_t pitch, int32_t timbre,
                          int32_t colour, int32_t strike, int16_t* out);

#ifdef __cplusplus
}  // extern "C"
#endif

#endif  // PW_MI_H_
