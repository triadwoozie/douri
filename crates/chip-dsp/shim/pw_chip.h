// Written for Pro Wave (Apache-2.0, as this crate): a C interface over the vendored cores.
// Every object is made and freed off the audio thread; the note and render functions do not
// allocate, lock or throw.
#ifndef PW_CHIP_H_
#define PW_CHIP_H_

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// ---- The six-operator FM synthesizer (msfa's Dx7Note). 44.1 kHz, blocks of exactly
// PW_FM6_BLOCK frames, PW_FM6_SLOTS notes at once.

#define PW_FM6_RATE 44100
#define PW_FM6_BLOCK 64
#define PW_FM6_SLOTS 16
// A voice: 155 unpacked parameter bytes, in the order of the DX7's voice dump.
#define PW_FM6_VOICE 155
// A voice as it is packed in a 32-voice bank.
#define PW_FM6_PACKED 128

typedef struct PwFm6 PwFm6;

// NULL when memory runs out.
PwFm6* pw_fm6_new(void);
void pw_fm6_free(PwFm6* fm);
// The voice the next notes play. Every byte must be in its parameter's range.
void pw_fm6_set_voice(PwFm6* fm, const uint8_t* voice);
// Starts a note in `slot` (0 to PW_FM6_SLOTS - 1).
void pw_fm6_note_on(PwFm6* fm, int32_t slot, int32_t note, int32_t velocity);
// Lets the note in `slot` go (a slot that never had a note is ignored).
void pw_fm6_note_off(PwFm6* fm, int32_t slot);
// One block of the slots in `mask` (bit k: slot k) that have had a note, summed into `out`
// (full scale is 1). `bend` is the pitch offset in semitones. `peaks[k]` is slot k's own
// peak in the block (0 for slots not rendered).
void pw_fm6_render(PwFm6* fm, uint32_t mask, float bend, float* out, float* peaks);
// A bank's packed voice (PW_FM6_PACKED bytes) as PW_FM6_VOICE unpacked bytes.
void pw_fm6_unpack(const uint8_t* packed, uint8_t* voice);

// ---- Sound chips driven by register writes.

// ymfm's YM3812: nine two-operator FM voices. Registers 0x00 to 0xff.
#define PW_CHIP_OPL2 0

// ymfm's YM2612: six four-operator FM voices. Registers 0x000 to 0x0ff are its first port
// (voices 1 to 3 and the global ones), 0x100 to 0x1ff its second (voices 4 to 6).
#define PW_CHIP_OPN2 1

// ymfm's YM2413: nine FM voices that play the chip's fifteen built-in instruments.
// Registers 0x00 to 0x3f.
#define PW_CHIP_OPLL 2

// The chips project's MOS 6581: three voices and a filter. Registers 0x00 to 0x18.
#define PW_CHIP_SID 3

// Tetorica's Sega PSG (an SN76489): three square-wave voices and noise. It has one port: the
// register number is ignored and `data` is the byte written to it.
#define PW_CHIP_PSG 4

typedef struct PwChip PwChip;

// NULL for an unknown kind or when memory runs out.
PwChip* pw_chip_new(int32_t kind);
void pw_chip_free(PwChip* chip);
// The rate of its output, in Hz, at the clock it is run at here.
uint32_t pw_chip_rate(const PwChip* chip);
// As after power-on.
void pw_chip_reset(PwChip* chip);
// Writes `data` to register `reg`.
void pw_chip_write(PwChip* chip, uint32_t reg, uint8_t data);
// The next `frames` frames into `left` and `right` (full scale is 1).
void pw_chip_render(PwChip* chip, float* left, float* right, uint32_t frames);

// ---- SMAF files (Yamaha's mobile-phone ringtone format, .mmf), rendered whole. Not for
// the audio thread.

#define PW_SMAF_OK 0
#define PW_SMAF_NOT_SMAF 1
#define PW_SMAF_NOTHING_TO_PLAY 2
#define PW_SMAF_FAILED 3

// Renders the file in `data` at `rate` Hz, at most `max_seconds` of it, to interleaved stereo.
// On PW_SMAF_OK `*out` holds `*frames` frames and is freed with pw_smaf_free.
int32_t pw_smaf_render(const uint8_t* data, size_t size, uint32_t rate, uint32_t max_seconds,
                       float** out, size_t* frames);
void pw_smaf_free(float* samples);

#ifdef __cplusplus
}  // extern "C"
#endif

#endif  // PW_CHIP_H_
