// Written for Pro Wave (Apache-2.0, as this crate): renders a SMAF file to audio with the
// vendored player. Not for the audio thread: it allocates, and takes as long as the file.
#include "pw_chip.h"

#include <cstdlib>
#include <cstring>
#include <vector>

#include "ma_player.h"
#include "smaf_file.h"

extern "C" {

int32_t pw_smaf_render(const uint8_t* data, size_t size, uint32_t rate, uint32_t max_seconds,
                       float** out, size_t* frames) {
  if (!data || !out || !frames || rate == 0) return PW_SMAF_NOT_SMAF;
  *out = NULL;
  *frames = 0;
  try {
    fxchain::smaf::SmafFile file;
    if (!file.parse(data, size)) return PW_SMAF_NOT_SMAF;
    fxchain::smaf::MaPlayer player;
    if (!player.init(file, rate)) return PW_SMAF_NOTHING_TO_PLAY;
    const size_t limit = static_cast<size_t>(rate) * max_seconds;
    const int kChunk = 4096;
    std::vector<float> all;
    std::vector<float> chunk(static_cast<size_t>(kChunk) * 2);
    while (!player.finished() && all.size() / 2 < limit) {
      const int got = player.render(chunk.data(), kChunk);
      if (got <= 0) break;
      all.insert(all.end(), chunk.begin(), chunk.begin() + static_cast<size_t>(got) * 2);
    }
    if (all.size() / 2 > limit) all.resize(limit * 2);
    if (all.empty()) return PW_SMAF_NOTHING_TO_PLAY;
    float* copy = static_cast<float*>(std::malloc(all.size() * sizeof(float)));
    if (!copy) return PW_SMAF_FAILED;
    std::memcpy(copy, all.data(), all.size() * sizeof(float));
    *out = copy;
    *frames = all.size() / 2;
    return PW_SMAF_OK;
  } catch (...) {
    return PW_SMAF_FAILED;
  }
}

void pw_smaf_free(float* samples) { std::free(samples); }

}  // extern "C"
