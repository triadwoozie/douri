// Written for Pro Wave (Apache-2.0, as this crate): the register-driven chips of pw_chip.h.
#include "pw_chip.h"

#include <cmath>
#include <new>

#include "ymfm_opl.h"
#include "ymfm_opn.h"

// The 6581 is one header: this file holds its implementation. Its checks of its own
// arguments are left out (they would stop the program).
#define CHIPS_IMPL
#define CHIPS_ASSERT(c) ((void)0)
#include "m6581.h"

#include "segapsg.h"

namespace {

inline float Scale(int32_t x) {
  const float y = static_cast<float>(x) * (1.0f / 32768.0f);
  return y < -1.0f ? -1.0f : (y > 1.0f ? 1.0f : y);
}

// The cores ask their host for timers and interrupts; an instrument has neither.
class NoHost : public ymfm::ymfm_interface {};

// The colour-burst crystal most of these chips were clocked from.
const uint32_t kNtscClock = 3579545;
// The YM2612's clock in the console it is known from.
const uint32_t kMegaDriveClock = 7670453;
// The 6581's clock in a PAL machine, and an output rate it divides without a remainder (the
// core counts the clock in sixteenths: 985248 * 16 / 44784 is 352 exactly).
const int kSidClock = 985248;
const int kSidRate = 44784;
// The PSG renders at whatever rate it is given.
const uint32_t kPsgRate = 48000;

}  // namespace

// One chip of each kind is held; only the one of `kind` is used.
struct PwChip {
  explicit PwChip(int32_t k) : kind(k), opl2(host), opn2(host), opll(host), psg(kPsgRate, kNtscClock) {}
  int32_t kind;
  NoHost host;
  ymfm::ym3812 opl2;
  ymfm::ym2612 opn2;
  ymfm::ym2413 opll;
  m6581_t sid;
  SegaPSG psg;
};

extern "C" {

PwChip* pw_chip_new(int32_t kind) {
  if (kind < PW_CHIP_OPL2 || kind > PW_CHIP_PSG) return NULL;
  PwChip* chip = new (std::nothrow) PwChip(kind);
  if (chip) pw_chip_reset(chip);
  return chip;
}

void pw_chip_free(PwChip* chip) { delete chip; }

uint32_t pw_chip_rate(const PwChip* chip) {
  if (!chip) return 0;
  if (chip->kind == PW_CHIP_OPL2) return chip->opl2.sample_rate(kNtscClock);
  if (chip->kind == PW_CHIP_OPN2) return chip->opn2.sample_rate(kMegaDriveClock);
  if (chip->kind == PW_CHIP_OPLL) return chip->opll.sample_rate(kNtscClock);
  if (chip->kind == PW_CHIP_SID) return kSidRate;
  if (chip->kind == PW_CHIP_PSG) return kPsgRate;
  return 0;
}

void pw_chip_reset(PwChip* chip) {
  if (!chip) return;
  if (chip->kind == PW_CHIP_OPL2) chip->opl2.reset();
  if (chip->kind == PW_CHIP_OPN2) chip->opn2.reset();
  if (chip->kind == PW_CHIP_OPLL) chip->opll.reset();
  if (chip->kind == PW_CHIP_SID) {
    m6581_desc_t desc;
    desc.tick_hz = kSidClock;
    desc.sound_hz = kSidRate;
    desc.magnitude = 1.0f;
    m6581_init(&chip->sid, &desc);
  }
  if (chip->kind == PW_CHIP_PSG) chip->psg.reset();
}

void pw_chip_write(PwChip* chip, uint32_t reg, uint8_t data) {
  if (!chip) return;
  if (chip->kind == PW_CHIP_OPL2) {
    chip->opl2.write_address(static_cast<uint8_t>(reg));
    chip->opl2.write_data(data);
  } else if (chip->kind == PW_CHIP_OPN2) {
    if (reg & 0x100) {
      chip->opn2.write_address_hi(static_cast<uint8_t>(reg));
      chip->opn2.write_data_hi(data);
    } else {
      chip->opn2.write_address(static_cast<uint8_t>(reg));
      chip->opn2.write_data(data);
    }
  } else if (chip->kind == PW_CHIP_OPLL) {
    chip->opll.write_address(static_cast<uint8_t>(reg));
    chip->opll.write_data(data);
  } else if (chip->kind == PW_CHIP_SID) {
    // Chip selected, write, address and data on the pins; no clock passes.
    uint64_t pins = M6581_CS | (reg & M6581_ADDR_MASK);
    M6581_SET_DATA(pins, static_cast<uint64_t>(data));
    _m6581_write(&chip->sid, pins);
  } else if (chip->kind == PW_CHIP_PSG) {
    chip->psg.write(data);
  }
}

void pw_chip_render(PwChip* chip, float* left, float* right, uint32_t frames) {
  if (!chip || !left || !right) return;
  if (chip->kind == PW_CHIP_OPL2) {
    for (uint32_t i = 0; i < frames; ++i) {
      ymfm::ym3812::output_data out;
      chip->opl2.generate(&out, 1);
      left[i] = right[i] = Scale(out.data[0]);
    }
  } else if (chip->kind == PW_CHIP_OPN2) {
    for (uint32_t i = 0; i < frames; ++i) {
      ymfm::ym2612::output_data out;
      chip->opn2.generate(&out, 1);
      left[i] = Scale(out.data[0]);
      right[i] = Scale(out.data[1]);
    }
  } else if (chip->kind == PW_CHIP_OPLL) {
    for (uint32_t i = 0; i < frames; ++i) {
      ymfm::ym2413::output_data out;
      chip->opll.generate(&out, 1);
      // Its melody and rhythm outputs, which a board mixes.
      left[i] = right[i] = Scale(out.data[0] + out.data[1]);
    }
  } else if (chip->kind == PW_CHIP_SID) {
    for (uint32_t i = 0; i < frames; ++i) {
      // Clock the chip until it has the next sample (352 sixteenths of a clock each).
      for (int guard = 0; guard < 64; ++guard) {
        if (m6581_tick(&chip->sid, 0) & M6581_SAMPLE) break;
      }
      left[i] = right[i] = chip->sid.sample;
    }
  } else if (chip->kind == PW_CHIP_PSG) {
    chip->psg.generate(left, right, frames);
  } else {
    for (uint32_t i = 0; i < frames; ++i) left[i] = right[i] = 0.0f;
  }
}

}  // extern "C"
