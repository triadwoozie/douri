// Pro Wave's C interface to the Novox The Robot engine; see pw_novox.h. MIT, as the engine.
#include "pw_novox.h"

#include <algorithm>
#include <memory>
#include <new>
#include <string>
#include <utility>
#include <vector>

#include "lexicon.hpp"
#include "planner.hpp"
#include "score.hpp"
#include "synth.hpp"
#include "voice.hpp"

struct pw_novox {
  novox::Lexicon lexicon;
};

struct pw_novox_stream {
  novox::Stream stream;
};

namespace {

const std::vector<std::string>& presetNames() {
  static const std::vector<std::string> names = novox::voicePresetNames();
  return names;
}

void readKnobs(const novox::Voice& v, pw_novox_knobs* out) {
  out->formant_scale = v.formantScale;
  out->breathiness = v.breathiness;
  out->brightness = v.brightness;
  out->grit = v.grit;
  out->vibrato_depth = v.vibratoDepth;
  out->vibrato_rate = v.vibratoRate;
  out->scoop = v.scoop;
  out->portamento = v.portamento;
  out->choir_size = v.choirSize;
}

void writeKnobs(const pw_novox_knobs& k, novox::Voice& v) {
  v.formantScale = k.formant_scale;
  v.breathiness = k.breathiness;
  v.brightness = k.brightness;
  v.grit = k.grit;
  v.vibratoDepth = k.vibrato_depth;
  v.vibratoRate = k.vibrato_rate;
  v.scoop = k.scoop;
  v.portamento = k.portamento;
  v.choirSize = k.choir_size;
}

}  // namespace

extern "C" {

pw_novox* pw_novox_new(const char* dict, size_t len) {
  try {
    auto e = std::make_unique<pw_novox>();
    if (dict && len > 0) e->lexicon.loadCmudictText(std::string(dict, len));
    return e.release();
  } catch (...) {
    return nullptr;
  }
}

void pw_novox_free(pw_novox* engine) { delete engine; }

size_t pw_novox_words(const pw_novox* engine) { return engine ? engine->lexicon.size() : 0; }

size_t pw_novox_preset_count(void) {
  try {
    return presetNames().size();
  } catch (...) {
    return 0;
  }
}

const char* pw_novox_preset_name(size_t index) {
  try {
    const auto& names = presetNames();
    return index < names.size() ? names[index].c_str() : nullptr;
  } catch (...) {
    return nullptr;
  }
}

int pw_novox_preset(const char* name, pw_novox_knobs* out) {
  if (!name || !out) return -1;
  try {
    novox::Voice v;
    if (!novox::voicePreset(name, v)) return -1;
    readKnobs(v, out);
    return 0;
  } catch (...) {
    return -1;
  }
}

pw_novox_stream* pw_novox_stream_new(const pw_novox* engine, const char* preset,
                                     const pw_novox_knobs* knobs, const pw_novox_note* notes,
                                     size_t count, double sample_rate) {
  if (!engine || !knobs || (!notes && count > 0) || !(sample_rate > 0)) return nullptr;
  try {
    novox::Score score;
    if (!novox::voicePreset(preset ? preset : "std", score.voice)) novox::voicePreset("std", score.voice);
    writeKnobs(*knobs, score.voice);
    score.notes.reserve(count);
    for (size_t i = 0; i < count; ++i) {
      const pw_novox_note& n = notes[i];
      if (!(n.dur > 0)) continue;
      novox::Note note;
      note.start = n.start;
      note.dur = n.dur;
      note.pitch = n.pitch;
      note.lyric = n.lyric ? n.lyric : "";
      score.notes.push_back(std::move(note));
    }
    std::stable_sort(score.notes.begin(), score.notes.end(),
                     [](const novox::Note& a, const novox::Note& b) { return a.start < b.start; });
    novox::Plan plan = novox::planScore(score, engine->lexicon);
    return new pw_novox_stream{novox::Stream(std::move(plan), score.voice, sample_rate)};
  } catch (...) {
    return nullptr;
  }
}

void pw_novox_stream_free(pw_novox_stream* stream) { delete stream; }

size_t pw_novox_stream_length(const pw_novox_stream* stream) { return stream ? stream->stream.length() : 0; }

double pw_novox_stream_lead(const pw_novox_stream* stream) { return stream ? stream->stream.plan().lead : 0.0; }

size_t pw_novox_stream_process(pw_novox_stream* stream, float* out, size_t frames) {
  if (!stream || !out) return 0;
  return stream->stream.process(out, frames);
}

size_t pw_novox_knobs_size(void) { return sizeof(pw_novox_knobs); }

size_t pw_novox_note_size(void) { return sizeof(pw_novox_note); }

}  // extern "C"
