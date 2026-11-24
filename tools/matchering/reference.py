"""Runs Matchering 2.0.6 (the Python original, GPL-3.0) on a target and a reference and dumps
what Pro Wave's port is compared with (crates/prowave-engine/tests/reference_match_parity.rs):

    python reference.py TARGET REFERENCE OUT_DIR

OUT_DIR gets, as little-endian float64: limited.f64, nolimiter.f64, normalized.f64 (interleaved
stereo at 44100 Hz: Matchering's three finishes), mid_fir.f64, side_fir.f64, and
scalars.f64 (the final amplitude coefficient, then every RMS coefficient in order).
Run it with `uv run --with matchering==2.0.6 python reference.py ...`.
"""
import json
import os
import sys
import tempfile

import numpy as np
import soundfile as sf
import matchering as mg
import matchering.stages as stages

target, reference, out = sys.argv[1], sys.argv[2], sys.argv[3]
os.makedirs(out, exist_ok=True)
seen = {"firs": [], "rms": [], "final": None}

_get_fir = stages.get_fir
_rms = stages.get_rms_c_and_amplify_pair
_norm = stages.normalize_reference


def get_fir(*a, **k):
    fir = _get_fir(*a, **k)
    seen["firs"].append(fir)
    return fir


def rms(*a, **k):
    r = _rms(*a, **k)
    seen["rms"].append(float(r[0]))
    return r


def norm(*a, **k):
    r = _norm(*a, **k)
    seen["final"] = float(r[1])
    return r


stages.get_fir, stages.get_rms_c_and_amplify_pair, stages.normalize_reference = get_fir, rms, norm

with tempfile.TemporaryDirectory() as tmp:
    files = {n: os.path.join(tmp, n + ".wav") for n in ("limited", "nolimiter", "normalized")}
    mg.process(
        target=target,
        reference=reference,
        results=[
            mg.Result(files["limited"], "DOUBLE"),
            mg.Result(files["nolimiter"], "DOUBLE", use_limiter=False, normalize=False),
            mg.Result(files["normalized"], "DOUBLE", use_limiter=False, normalize=True),
        ],
    )
    for name, path in files.items():
        data, rate = sf.read(path, dtype="float64", always_2d=True)
        assert rate == 44100 and data.shape[1] == 2
        np.ascontiguousarray(data).astype("<f8").tofile(os.path.join(out, name + ".f64"))

# stages.main runs the limited finish first; the FIRs are computed once (mid, then side).
seen["firs"][0].astype("<f8").tofile(os.path.join(out, "mid_fir.f64"))
seen["firs"][1].astype("<f8").tofile(os.path.join(out, "side_fir.f64"))
np.array([seen["final"]] + seen["rms"], dtype="<f8").tofile(os.path.join(out, "scalars.f64"))
print(json.dumps({"ok": True, "out": out}))
