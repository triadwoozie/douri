/* Pro Wave's C interface to the Novox The Robot engine (engine/src, vendored, MIT).
 *
 * Written for Pro Wave, under the same MIT licence as the engine (see ../LICENSE and ../NOTICE).
 * It is smaller than upstream's own C API (engine/include/novox.h, not vendored): only what the
 * Singer needs, notes passed as structs instead of JSON, and no C++ exception ever crosses it.
 *
 * Threads: an engine (its dictionary) is only read after pw_novox_new, so one engine may make
 * streams on several threads at once. A stream belongs to one thread at a time.
 * Allocation: pw_novox_stream_new allocates; pw_novox_stream_process never does. */
#ifndef PW_NOVOX_H
#define PW_NOVOX_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct pw_novox pw_novox;
typedef struct pw_novox_stream pw_novox_stream;

/* The knobs of a voice the Singer shows, in the engine's own units. */
typedef struct pw_novox_knobs {
  float formant_scale;  /* vocal tract size: 1 = adult male, ~1.17 adult female, ~1.3 child */
  float breathiness;    /* aspiration mixed into the voiced sound, 0..1 */
  float brightness;     /* spectral tilt, -1 dark .. +1 bright */
  float grit;           /* saturation, 0..1 */
  float vibrato_depth;  /* semitones (peak) */
  float vibrato_rate;   /* Hz */
  float scoop;          /* semitones a note's attack starts from (negative = below) */
  float portamento;     /* legato glide time, seconds */
  int32_t choir_size;   /* voices, 1..8 */
} pw_novox_knobs;

/* One sung note: seconds from the score's time 0, a MIDI pitch, and a lyric token in Novox's
 * own notation ("word", "hel-" continues a word on the next note, "_" holds the vowel before,
 * "" ends a word continued from the note before). */
typedef struct pw_novox_note {
  double start;
  double dur;
  float pitch;
  const char* lyric;
} pw_novox_note;

/* An engine with the CMU-format dictionary `dict` (`len` bytes of text, may be empty: the
 * engine's spelling rules then guess every word). NULL if it could not be made. */
pw_novox* pw_novox_new(const char* dict, size_t len);
void pw_novox_free(pw_novox* engine);
/* Dictionary entries loaded. */
size_t pw_novox_words(const pw_novox* engine);

/* The built-in voice presets: their count, and the name of each (static, NUL-terminated). */
size_t pw_novox_preset_count(void);
const char* pw_novox_preset_name(size_t index);
/* The knobs of preset `name`. Returns 0, or -1 for a name that is not a preset. */
int pw_novox_preset(const char* name, pw_novox_knobs* out);

/* A stream singing `notes` (any order) with preset `preset` and its knobs replaced by `knobs`,
 * at `sample_rate`. NULL on failure. */
pw_novox_stream* pw_novox_stream_new(const pw_novox* engine, const char* preset,
                                     const pw_novox_knobs* knobs, const pw_novox_note* notes,
                                     size_t count, double sample_rate);
void pw_novox_stream_free(pw_novox_stream* stream);
/* Samples the stream holds in all (its tail included). */
size_t pw_novox_stream_length(const pw_novox_stream* stream);
/* Seconds before score time 0 that its sample 0 is (onset consonants start before the beat). */
double pw_novox_stream_lead(const pw_novox_stream* stream);
/* Writes the next `frames` samples to `out` (zeros past the end). Returns how many had sound.
 * Never allocates. */
size_t pw_novox_stream_process(pw_novox_stream* stream, float* out, size_t frames);

/* sizeof(pw_novox_knobs) and sizeof(pw_novox_note), for a layout check from Rust. */
size_t pw_novox_knobs_size(void);
size_t pw_novox_note_size(void);

#ifdef __cplusplus
}
#endif

#endif /* PW_NOVOX_H */
