#!/usr/bin/env python3
"""Makes crates/prowave-engine/tests/data/dadaspec-golden.bin: the picture dadaspec itself draws
of the colour spectrogram test signal, before its final resize, for the test
`colour_spectrum::tests::the_picture_matches_dadaspec_itself`.

    python3 tools/dadaspec_golden.py /path/to/dadaspec.py OUT.bin

Needs numpy (and dadaspec.py from https://github.com/dada-bots/dadaspec, MIT). The signal
here must stay the same as `golden_signal` in crates/prowave-engine/src/colour_spectrum.rs.
"""
import importlib.util
import struct
import sys
import types

import numpy as np

# dadaspec imports Pillow at the top for its JPEG output, which is not used here.
if "PIL" not in sys.modules:
    try:
        import PIL  # noqa: F401
    except ImportError:
        pil = types.ModuleType("PIL")
        pil.Image = None
        sys.modules["PIL"] = pil

spec = importlib.util.spec_from_file_location("dadaspec", sys.argv[1])
dada = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dada)

RATE = 44100
n = int(8.0 * RATE)
t = np.arange(n, dtype=np.float64) / RATE


def sine(hz):
    return np.sin(2 * np.pi * hz * t)


env = 0.6 + 0.4 * np.abs(sine(2.0))
first = t < 4.0
left = np.where(first,
                env * (0.3 * sine(110.0) + 0.1 * sine(440.0)),
                env * (0.2 * sine(2500.0) + 0.1 * sine(6000.0) + 0.02 * sine(90.0)))
right = np.where(first,
                 env * (0.2 * sine(220.0) + 0.05 * sine(3000.0)),
                 env * (0.15 * sine(5000.0) + 0.1 * sine(800.0)))
y = np.stack([left, right]).astype(np.float32)

# render_to_jpg up to the resize, as dadaspec has it.
y = dada.resample_linear(y, RATE, dada.TARGET_SR)
S_L, rgb_L = dada._mel_channel(y[0], dada.TARGET_SR, dada.SPEC_MELS)
S_R, rgb_R = dada._mel_channel(y[1], dada.TARGET_SR, dada.SPEC_MELS)
nf = min(S_L.shape[1], S_R.shape[1])
S_L, S_R = S_L[:, :nf], S_R[:, :nf]
rgb_L, rgb_R = rgb_L[:nf], rgb_R[:nf]
nm = S_L.shape[0]
S_L = S_L[::-1]
img = np.empty((nm * 2, nf, 3), dtype=np.float32)
img[:nm] = S_L[:, :, np.newaxis] * rgb_L[np.newaxis, :, :]
img[nm:] = S_R[:, :, np.newaxis] * rgb_R[np.newaxis, :, :]
np.clip(img, 0, 1, out=img)
img *= 255
out = img.astype(np.uint8)

with open(sys.argv[2], "wb") as f:
    f.write(struct.pack("<II", out.shape[0], out.shape[1]))
    f.write(out.tobytes())
print(out.shape)
