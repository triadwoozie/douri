// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.
//
// Runs Mutable Instruments' Beads processor (the original C++ in eurorack/, MIT) on the
// cases below and writes, for each, the input, the settings of every block and the output,
// so tests/texture_parity.rs can run the Rust port on the same blocks and compare.
//
// Built by the test with: c++ -std=c++14 -O2 -DTEST -ffp-contract=off -I eurorack
//
// File format (little-endian), one file per case, <name>.bin:
//   "PWTX", u32 media (Beads' enum), u32 block size, u32 blocks,
//   then per block: 13 f32 settings (density, time, size, shape, pitch, the four amounts,
//   feedback, dry/wet, reverb, freeze), block x 2 f32 input, block x 2 f32 output.

#include <cstdio>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

#include "beads/dsp/processor.h"
#include "stmlib/utils/random.h"

using namespace beads;
using namespace stmlib;

// Same memory layout as on the hardware.
static uint8_t block_1[512 * 1024];
static uint8_t block_2[256 * 1024];
static uint16_t reverb_buffer[32768];

struct Block {
  float v[13];
};

typedef void (*SettingsFn)(int block, int blocks, Block* b);

struct Case {
  const char* name;
  Processor::Media media;
  float seconds;
  SettingsFn settings;
};

static void Base(Block* b) {
  float d[13] = {
    0.3f,                     // density
    0.3f, 0.7f, 0.5f, 0.0f,   // time, size, shape, pitch (semitones)
    0.0f, 0.0f, 0.0f, 0.0f,   // amounts
    0.2f, 0.7f, 0.3f,         // feedback, dry/wet, reverb
    0.0f,                     // freeze
  };
  memcpy(b->v, d, sizeof(d));
}

static void Regular(int, int, Block* b) {
  Base(b);
}

static void RandomSettings(int, int, Block* b) {
  Base(b);
  b->v[0] = 0.8f;
  b->v[4] = 7.0f;
  b->v[5] = 0.5f;
  b->v[6] = 0.3f;
  b->v[7] = -0.4f;
  b->v[8] = -0.6f;
  b->v[9] = 0.5f;
  b->v[11] = 0.6f;
}

static void Down(int, int, Block* b) {
  Base(b);
  b->v[4] = -12.0f;
  b->v[2] = 0.2f;  // reversed grains
}

static void Delay(int, int, Block* b) {
  Base(b);
  b->v[0] = 0.25f;
  b->v[1] = 0.2f;
  b->v[2] = 1.0f;
  b->v[6] = 0.6f;  // size amount: the delay's filter
  b->v[9] = 0.4f;
}

static void DelayPitched(int, int, Block* b) {
  Delay(0, 0, b);
  b->v[0] = 0.7f;
  b->v[4] = 5.0f;
  b->v[8] = 0.3f;
}

static void Freeze(int block, int blocks, Block* b) {
  RandomSettings(block, blocks, b);
  b->v[12] = (block > blocks * 3 / 8 && block < blocks * 6 / 8) ? 1.0f : 0.0f;
}

static void DelayFreeze(int block, int blocks, Block* b) {
  Delay(block, blocks, b);
  b->v[12] = block > blocks / 2 ? 1.0f : 0.0f;
  b->v[1] = float(block) / float(blocks);
}

// Every setting moves.
static void Sweep(int block, int blocks, Block* b) {
  const float x = float(block) / float(blocks);
  b->v[0] = x;
  b->v[1] = 1.0f - x;
  b->v[2] = 0.05f + 0.9f * x;
  b->v[3] = x;
  b->v[4] = -24.0f + 48.0f * x;
  b->v[5] = 2.0f * x - 1.0f;
  b->v[6] = 1.0f - 2.0f * x;
  b->v[7] = x - 0.5f;
  b->v[8] = 0.5f - x;
  b->v[9] = x;
  b->v[10] = 1.0f - 0.5f * x;
  b->v[11] = x;
  b->v[12] = (block / 2000) % 3 == 2 ? 1.0f : 0.0f;
}

static const Case kCases[] = {
  { "digital_regular", Processor::MEDIA_DIGITAL, 4.0f, &Regular },
  { "digital_random", Processor::MEDIA_DIGITAL, 4.0f, &RandomSettings },
  { "digital_freeze", Processor::MEDIA_DIGITAL, 4.0f, &Freeze },
  { "digital_delay", Processor::MEDIA_DIGITAL, 4.0f, &Delay },
  { "digital_delay_pitched", Processor::MEDIA_DIGITAL, 4.0f, &DelayPitched },
  { "digital_delay_freeze", Processor::MEDIA_DIGITAL, 5.0f, &DelayFreeze },
  { "digital_sweep", Processor::MEDIA_DIGITAL, 6.0f, &Sweep },
  { "vintage_random", Processor::MEDIA_VINTAGE_DIGITAL, 4.0f, &RandomSettings },
  { "clean_tape_down", Processor::MEDIA_CLEAN_TAPE, 4.0f, &Down },
  { "clean_tape_delay", Processor::MEDIA_CLEAN_TAPE, 4.0f, &DelayPitched },
  { "dirty_tape_random", Processor::MEDIA_DIRTY_TAPE, 4.0f, &RandomSettings },
  { "dirty_tape_sweep", Processor::MEDIA_DIRTY_TAPE, 6.0f, &Sweep },
};

// The input: a saw on the left, noise on the right (arithmetic only, so both sides could
// make it the same way; the test reads it from the file anyway).
struct Source {
  float phase;
  uint32_t noise;
  void Next(float sample_rate, FloatFrame* f) {
    phase += 220.0f / sample_rate;
    if (phase >= 1.0f) phase -= 1.0f;
    noise = noise * 1664525u + 1013904223u;
    f->l = 0.6f * (phase - 0.5f);
    f->r = 0.4f * (float(noise >> 8) / 16777216.0f - 0.5f);
  }
};

static void Put(FILE* fp, const void* p, size_t n) {
  if (fwrite(p, 1, n, fp) != n) {
    perror("write");
    exit(1);
  }
}

int main(int argc, char** argv) {
  if (argc < 2) {
    fprintf(stderr, "usage: harness <out dir>\n");
    return 1;
  }
  uint8_t* buffers[3];
  buffers[0] = block_1;
  buffers[1] = block_1 + (256 * 1024);
  buffers[2] = block_2;

  for (const Case& c : kCases) {
    Random::Seed(0x21);
    static Processor processor;
    memset(block_1, 0, sizeof(block_1));
    memset(block_2, 0, sizeof(block_2));
    processor.Init(buffers, reverb_buffer);
    processor.Configure(c.media, false);
    const float sr = processor.sample_rate();
    const uint32_t block = uint32_t(sr) / 4000;
    const uint32_t blocks = uint32_t(c.seconds * sr) / block;

    std::string path = std::string(argv[1]) + "/" + c.name + ".bin";
    FILE* fp = fopen(path.c_str(), "wb");
    if (!fp) {
      perror(path.c_str());
      return 1;
    }
    uint32_t media = c.media;
    Put(fp, "PWTX", 4);
    Put(fp, &media, 4);
    Put(fp, &block, 4);
    Put(fp, &blocks, 4);

    Source source = { 0.0f, 12345u };
    std::vector<FloatFrame> in(block), out(block);
    std::vector<uint8_t> freeze_flags(block);
    bool freeze[12];
    for (uint32_t k = 0; k < blocks; ++k) {
      Block b;
      c.settings(int(k), int(blocks), &b);
      for (uint32_t i = 0; i < block; ++i) {
        source.Next(sr, &in[i]);
        freeze[i] = b.v[12] > 0.5f;
      }
      Parameters p;
      RandomizedParameter* rp[4] = { &p.time, &p.size, &p.shape, &p.pitch };
      for (int i = 0; i < 4; ++i) {
        rp[i]->base = b.v[1 + i];
        rp[i]->attenuverter = b.v[5 + i];
        rp[i]->cv = 0.0f;
        rp[i]->has_cv = false;
      }
      p.monophonic_mode = false;
      ModulatedParameter d;
      d.base = b.v[0];
      d.cv = 0.0f;
      Processor::MixParameters m;
      m.feedback = b.v[9];
      m.dry_wet = b.v[10];
      m.reverb_amount = b.v[11];
      m.output_gate = false;
      processor.Process(
          SEED_BUTTON_STATE_LOCKED, NULL, freeze, d, p, m, &in[0], &out[0], block);
      Put(fp, b.v, sizeof(b.v));
      Put(fp, &in[0], block * sizeof(FloatFrame));
      Put(fp, &out[0], block * sizeof(FloatFrame));
    }
    fclose(fp);
  }
  return 0;
}
