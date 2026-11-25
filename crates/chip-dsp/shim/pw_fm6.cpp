// Written for Pro Wave (Apache-2.0, as this crate): the FM synthesizer's part of pw_chip.h.
// The order of calls and the output scaling follow msfa's SynthUnit (synth_unit.cc of the
// same source, Apache-2.0, Copyright 2012 Google Inc.), which is not vendored: Pro Wave
// allocates the voices itself.
#include "pw_chip.h"

#include <cstring>
#include <mutex>
#include <new>

#include "synth.h"

#include "aligned_buf.h"
#include "controllers.h"
#include "dx7note.h"
#include "exp2.h"
#include "freqlut.h"
#include "lfo.h"
#include "patch.h"
#include "pitchenv.h"
#include "sin.h"

namespace {

// The core's tables are shared by every instance and depend on the rate: one rate for all.
void InitTables() {
  static std::once_flag once;
  std::call_once(once, [] {
    Freqlut::init(PW_FM6_RATE);
    Exp2::init();
    Tanh::init();
    Sin::init();
    Lfo::init(PW_FM6_RATE);
    PitchEnv::init(PW_FM6_RATE);
  });
}

}  // namespace

struct PwFm6 {
  Dx7Note notes[PW_FM6_SLOTS];
  // Slots that have had a note: only those hold a defined state.
  uint32_t started;
  Lfo lfo;
  Controllers controllers;
  char patch[156];
};

extern "C" {

PwFm6* pw_fm6_new(void) {
  // Not on the audio thread: the first call fills the tables under a lock.
  InitTables();
  PwFm6* fm = new (std::nothrow) PwFm6;
  if (!fm) return NULL;
  std::memset(static_cast<void*>(fm), 0, sizeof(*fm));
  fm->controllers.values_[kControllerPitch] = 0x2000;
  fm->patch[155] = 0x3f;
  fm->lfo.reset(fm->patch + 137);
  return fm;
}

void pw_fm6_free(PwFm6* fm) { delete fm; }

void pw_fm6_set_voice(PwFm6* fm, const uint8_t* voice) {
  if (!fm || !voice) return;
  std::memcpy(fm->patch, voice, PW_FM6_VOICE);
  // Every operator on.
  fm->patch[155] = 0x3f;
  fm->lfo.reset(fm->patch + 137);
}

void pw_fm6_note_on(PwFm6* fm, int32_t slot, int32_t note, int32_t velocity) {
  if (!fm || slot < 0 || slot >= PW_FM6_SLOTS) return;
  note = note < 0 ? 0 : (note > 127 ? 127 : note);
  velocity = velocity < 0 ? 0 : (velocity > 127 ? 127 : velocity);
  fm->lfo.keydown();
  fm->notes[slot].init(fm->patch, note, velocity);
  fm->started |= 1u << slot;
}

void pw_fm6_note_off(PwFm6* fm, int32_t slot) {
  if (!fm || slot < 0 || slot >= PW_FM6_SLOTS) return;
  if (fm->started & (1u << slot)) fm->notes[slot].keyup();
}

void pw_fm6_render(PwFm6* fm, uint32_t mask, float bend, float* out, float* peaks) {
  if (!fm || !out || !peaks) return;
  // The core's wheel: 0x2000 is the centre and 0x2000 either way is 3 semitones.
  if (!(bend > -48.0f)) bend = -48.0f;
  if (bend > 48.0f) bend = 48.0f;
  fm->controllers.values_[kControllerPitch] =
      0x2000 + static_cast<int>(bend * (8192.0f / 3.0f));
  const int32_t lfovalue = fm->lfo.getsample();
  const int32_t lfodelay = fm->lfo.getdelay();
  for (int j = 0; j < PW_FM6_BLOCK; ++j) out[j] = 0.0f;
  AlignedBuf<int32_t, N> audiobuf;
  mask &= fm->started;
  for (int slot = 0; slot < PW_FM6_SLOTS; ++slot) {
    peaks[slot] = 0.0f;
    if (!(mask & (1u << slot))) continue;
    int32_t* buf = audiobuf.get();
    for (int j = 0; j < N; ++j) buf[j] = 0;
    fm->notes[slot].compute(buf, lfovalue, lfodelay, &fm->controllers);
    float peak = 0.0f;
    for (int j = 0; j < N; ++j) {
      // SynthUnit's scaling: 1 << 28 is full scale.
      const float y = static_cast<float>(buf[j] >> 4) * (1.0f / 16777216.0f);
      out[j] += y;
      const float a = y < 0.0f ? -y : y;
      if (a > peak) peak = a;
    }
    peaks[slot] = peak;
  }
}

void pw_fm6_unpack(const uint8_t* packed, uint8_t* voice) {
  if (!packed || !voice) return;
  char unpacked[156];
  UnpackPatch(reinterpret_cast<const char*>(packed), unpacked);
  std::memcpy(voice, unpacked, PW_FM6_VOICE);
}

}  // extern "C"
