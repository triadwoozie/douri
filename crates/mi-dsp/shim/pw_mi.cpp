// Written for Pro Wave (MIT, as the rest of this crate): the C interface of pw_mi.h. The
// resonator's noise gate and the order of calls follow the modules' own main loops
// (rings/rings.cc, clouds/clouds.cc and braids/braids.cc of the same repository, MIT,
// Copyright 2012-2015 Emilie Gillet), which are hardware code and are not vendored.
#include "pw_mi.h"

#include <algorithm>
#include <cmath>
#include <cstring>
#include <new>

#include "braids/macro_oscillator.h"
#include "clouds/dsp/granular_processor.h"
#include "rings/dsp/part.h"
#include "rings/dsp/strummer.h"

namespace {

inline float Clamp(float x, float lo, float hi) {
  // Not-a-number goes to `lo`.
  return x >= lo ? (x <= hi ? x : hi) : lo;
}

inline int32_t ClampInt(int32_t x, int32_t lo, int32_t hi) {
  return x < lo ? lo : (x > hi ? hi : x);
}

const float kNoiseGateThreshold = 0.00003f;

}  // namespace

struct PwResonator {
  rings::Part part;
  rings::Strummer strummer;
  uint16_t reverb_buffer[32768];
  float in[PW_RESONATOR_BLOCK];
  float in_level;
};

struct PwSpectral {
  clouds::GranularProcessor processor;
  // The module's two memory blocks (clouds/clouds.cc).
  uint8_t block_mem[118784];
  uint8_t block_ccm[65536 - 128];
};

struct PwOscillator {
  braids::MacroOscillator osc;
  uint8_t sync[PW_OSCILLATOR_BLOCK];
};

extern "C" {

void pw_resonator_reset(PwResonator* r) {
  if (!r) return;
  std::memset(r->reverb_buffer, 0, sizeof(r->reverb_buffer));
  std::memset(r->in, 0, sizeof(r->in));
  r->in_level = 0.0f;
  std::memset(static_cast<void*>(&r->part), 0, sizeof(r->part));
  std::memset(static_cast<void*>(&r->strummer), 0, sizeof(r->strummer));
  r->strummer.Init(0.01f, rings::kSampleRate / rings::kMaxBlockSize);
  r->part.Init(r->reverb_buffer);
}

PwResonator* pw_resonator_new(void) {
  PwResonator* r = new (std::nothrow) PwResonator;
  if (r) pw_resonator_reset(r);
  return r;
}

void pw_resonator_free(PwResonator* r) { delete r; }

void pw_resonator_process(PwResonator* r, const PwResonatorBlock* b, const float* in,
                          float* out, float* aux, size_t size) {
  if (!r || !b || !out || !aux) return;
  size = std::min(size, static_cast<size_t>(PW_RESONATOR_BLOCK));
  if (size == 0) return;

  rings::Patch patch;
  patch.structure = Clamp(b->structure, 0.0f, 0.9995f);
  patch.brightness = Clamp(b->brightness, 0.0f, 1.0f);
  patch.damping = Clamp(b->damping, 0.0f, 1.0f);
  patch.position = Clamp(b->position, 0.0f, 1.0f);

  const bool external = b->external != 0 && in != NULL;
  rings::PerformanceState ps;
  ps.tonic = 12.0f;
  ps.note = Clamp(b->note, 0.0f, 127.0f) - 12.0f;
  ps.fm = Clamp(b->fm, -48.0f, 48.0f);
  ps.chord = ClampInt(b->chord, 0, rings::kNumChords - 1);
  ps.internal_exciter = !external;
  ps.internal_strum = external;
  ps.internal_note = external;
  ps.strum = !external && b->strum != 0;

  r->part.set_model(static_cast<rings::ResonatorModel>(
      ClampInt(b->model, 0, rings::RESONATOR_MODEL_LAST - 1)));
  const int32_t polyphony = b->polyphony >= 4 ? 4 : (b->polyphony >= 2 ? 2 : 1);
  if (polyphony != r->part.polyphony()) {
    r->part.set_polyphony(polyphony);
  }

  if (external) {
    // The module's noise gate on its input.
    for (size_t i = 0; i < size; ++i) {
      const float in_sample = std::isfinite(in[i]) ? Clamp(in[i], -1.0f, 1.0f) : 0.0f;
      const float error = in_sample * in_sample - r->in_level;
      r->in_level += error * (error > 0.0f ? 0.1f : 0.0001f);
      const float gain = r->in_level <= kNoiseGateThreshold
          ? (1.0f / kNoiseGateThreshold) * r->in_level : 1.0f;
      r->in[i] = gain * in_sample;
    }
    r->strummer.Process(r->in, size, &ps);
  } else {
    std::memset(r->in, 0, sizeof(r->in));
  }
  r->part.Process(ps, patch, r->in, out, aux, size);
}

PwSpectral* pw_spectral_new(void) {
  PwSpectral* s = new (std::nothrow) PwSpectral;
  if (!s) return NULL;
  std::memset(static_cast<void*>(s), 0, sizeof(*s));
  s->processor.Init(s->block_mem, sizeof(s->block_mem), s->block_ccm, sizeof(s->block_ccm));
  s->processor.set_playback_mode(clouds::PLAYBACK_MODE_SPECTRAL);
  // Stereo, 16 bits.
  s->processor.set_quality(0);
  s->processor.Prepare();
  return s;
}

void pw_spectral_free(PwSpectral* s) { delete s; }

void pw_spectral_process(PwSpectral* s, const PwSpectralBlock* b, const int16_t* in,
                         int16_t* out) {
  if (!s || !b || !in || !out) return;
  clouds::Parameters* p = s->processor.mutable_parameters();
  p->position = Clamp(b->position, 0.0f, 1.0f);
  p->size = Clamp(b->size, 0.0f, 1.0f);
  p->pitch = Clamp(b->pitch, -48.0f, 48.0f);
  p->density = Clamp(b->density, 0.0f, 1.0f);
  p->texture = Clamp(b->texture, 0.0f, 1.0f);
  p->dry_wet = Clamp(b->dry_wet, 0.0f, 1.0f);
  p->stereo_spread = 0.0f;
  p->feedback = Clamp(b->feedback, 0.0f, 1.0f);
  p->reverb = Clamp(b->reverb, 0.0f, 1.0f);
  p->freeze = b->freeze != 0;
  p->trigger = false;
  p->gate = false;
  // On the module this runs in the main loop, between audio interrupts: it gives the phase
  // vocoder the frames it is due. It works in the memory given to Init.
  s->processor.Prepare();
  clouds::ShortFrame input[PW_SPECTRAL_BLOCK];
  std::memcpy(input, in, sizeof(input));
  s->processor.Process(input, reinterpret_cast<clouds::ShortFrame*>(out), PW_SPECTRAL_BLOCK);
}

PwOscillator* pw_oscillator_new(void) {
  PwOscillator* o = new (std::nothrow) PwOscillator;
  if (!o) return NULL;
  std::memset(static_cast<void*>(o), 0, sizeof(*o));
  o->osc.Init();
  return o;
}

void pw_oscillator_free(PwOscillator* o) { delete o; }

void pw_oscillator_render(PwOscillator* o, int32_t shape, int32_t pitch, int32_t timbre,
                          int32_t colour, int32_t strike, int16_t* out) {
  if (!o || !out) return;
  o->osc.set_shape(static_cast<braids::MacroOscillatorShape>(
      ClampInt(shape, 0, PW_OSCILLATOR_SHAPES - 1)));
  o->osc.set_parameters(static_cast<int16_t>(ClampInt(timbre, 0, 32767)),
                        static_cast<int16_t>(ClampInt(colour, 0, 32767)));
  o->osc.set_pitch(static_cast<int16_t>(ClampInt(pitch, 0, 16383)));
  if (strike) o->osc.Strike();
  o->osc.Render(o->sync, out, PW_OSCILLATOR_BLOCK);
}

}  // extern "C"
