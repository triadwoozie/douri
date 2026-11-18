#!/usr/bin/env python3
"""Measures how clean the stem loops of the pad sets are, by separating each loop again.

    measure_bleed.py CHOPS_ROOT OUT.json [--models DIR]

For each song's first loop of a kind, the model that should find nothing (or only that part)
is run on the loop and the energy it puts elsewhere is reported, in dB under the loop:
  instrumental chorus -> vocal model: vocals left in it
  vocal chorus        -> vocal model: music left in it
  drums, bass         -> 6-stem model: everything that is not drums / bass
  horns               -> wind model: what is not wind instruments
  backing vox         -> karaoke model: lead vocal in it
  every stem loop     -> vocal model: vocals in it
This is a self-consistency check (a model's own view of its residue), not a ground truth: no
clean multitracks of the songs exist. The models and their licences: see separate_all.py.
"""
import argparse
import json
import os
import shutil
import sys
import tempfile

import numpy as np
import soundfile as sf

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from separate_all import KARAOKE, SIX, VOCALS, WIND, Stage  # noqa: E402

KINDS = {
    "Instrumental Chorus": "instrumental", "Vocal Chorus": "vocals", "Drums": "drums",
    "Bass": "bass", "Horns": "horns", "Backing Vox": "backing", "Rhythm Section": "rhythm",
    "Orchestra": "orchestra",
}


def energy(path):
    x, _ = sf.read(path, dtype="float64", always_2d=True)
    return float(np.mean(x ** 2))


# The models write 16-bit WAV: a residue under one step reads back as exact silence. Such a
# result is reported at this floor (one step's energy, relative to the loop) and counted.
STEP_E = (1 / 32768) ** 2


def ratio_db(part, whole):
    return round(10 * np.log10(max(part, STEP_E) / max(whole, 1e-20)), 2)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("chops")
    ap.add_argument("out")
    ap.add_argument("--models", help="where audio-separator keeps its models")
    a = ap.parse_args()
    if not a.models:
        sys.exit("--models DIR is needed")
    loops = {k: [] for k in KINDS.values()}
    for song in sorted(os.listdir(a.chops)):
        f = os.path.join(a.chops, song, "set.json")
        if not os.path.exists(f):
            continue
        pads = json.load(open(f))["pads"]
        seen = set()
        for p in pads:
            for prefix, kind in KINDS.items():
                if p["name"].startswith(prefix + " ") and kind not in seen and p.get("loop"):
                    seen.add(kind)
                    loops[kind].append((song, os.path.join(a.chops, song, p["file"])))
    work = tempfile.mkdtemp(prefix="bleed-")
    st = {k: Stage(m, a.models, work) for k, m in
          (("vocals", VOCALS), ("six", SIX), ("wind", WIND), ("karaoke", KARAOKE))}
    plan = {
        "instrumental": [("vocals", "vocals", "vocals in the instrumental chorus")],
        "vocals": [("vocals", "instrumental", "music in the vocal chorus")],
        "drums": [("six", "!drums", "not drums, in the drum loop"), ("vocals", "vocals", "vocals in the drum loop")],
        "bass": [("six", "!bass", "not bass, in the bass loop"), ("vocals", "vocals", "vocals in the bass loop")],
        "horns": [("wind", "no woodwinds", "not wind instruments, in the horn loop"),
                  ("vocals", "vocals", "vocals in the horn loop")],
        "backing": [("karaoke", "vocals", "lead vocal in the backing-vocal loop")],
        "rhythm": [("vocals", "vocals", "vocals in the rhythm-section loop")],
        "orchestra": [("vocals", "vocals", "vocals in the orchestra loop")],
    }
    results = {}
    for kind, items in loops.items():
        for stage, want, label in plan[kind]:
            vals = []
            for song, path in items:
                d = tempfile.mkdtemp(dir=work)
                try:
                    out = st[stage].run(path, d)
                    whole = energy(path)
                    if want.startswith("!"):
                        keep = want[1:]
                        part = sum(energy(p) for n, p in out.items() if n != keep)
                    else:
                        part = energy(out[want]) if want in out else 0.0
                    vals.append({"song": song, "db": ratio_db(part, whole),
                                 "below_16bit": part < STEP_E})
                except Exception as e:  # a failed loop is reported, not fatal
                    vals.append({"song": song, "error": repr(e)})
                shutil.rmtree(d, ignore_errors=True)
            dbs = np.array([v["db"] for v in vals if "db" in v])
            results[label] = {
                "loops": len(dbs),
                "median_db": round(float(np.median(dbs)), 1) if len(dbs) else None,
                "p10_db": round(float(np.percentile(dbs, 10)), 1) if len(dbs) else None,
                "p90_db": round(float(np.percentile(dbs, 90)), 1) if len(dbs) else None,
                "worst_db": round(float(dbs.max()), 1) if len(dbs) else None,
                "below_16bit": sum(bool(v.get("below_16bit")) for v in vals),
                "per_song": vals,
            }
            print(label, {k: v for k, v in results[label].items() if k != "per_song"}, flush=True)
    shutil.rmtree(work, ignore_errors=True)
    json.dump(results, open(a.out, "w"), indent=1)


if __name__ == "__main__":
    main()
