#!/usr/bin/env python3
"""Separates a song into stems with audio-separator (python-audio-separator), on this machine.

    separate.py INPUT OUT_DIR [--models DIR] [--no-drums]

1. Vocals: MelBand RoFormer (Kimberley Jensen)  -> vocals, instrumental
2. Instrumental: Demucs v4 htdemucs_ft          -> drums, bass, other
3. Drums: MDX23C DrumSep (aufr33, jarredou)    -> kick, snare, toms, hh, ride, crash

Licences: audio-separator is MIT. The models are downloaded by it, not shipped: MelBand
RoFormer (Kimberley Jensen) states no licence; Demucs (Meta) is MIT; MDX23C DrumSep (aufr33,
jarredou) states no licence. Their terms are their makers'.

Writes OUT_DIR/<stem>.wav and prints one JSON line {"stems": {name: path}, "seconds": t} at the end.
Progress lines start with "STEP ".
"""
import argparse
import json
import os
import re
import shutil
import sys
import tempfile
import time

VOCALS = "vocals_mel_band_roformer.ckpt"
SPLIT = "htdemucs_ft.yaml"
DRUMS = "MDX23C-DrumSep-aufr33-jarredou.ckpt"


def stem_of(path):
    """The stem name audio-separator put in parentheses in an output file name."""
    m = re.findall(r"\(([^)]+)\)", os.path.basename(path))
    return m[-1].strip().lower() if m else None


def run(sep, model, path, work):
    print(f"STEP {model}", flush=True)
    sep.load_model(model_filename=model)
    out = {}
    for f in sep.separate(path):
        full = f if os.path.isabs(f) else os.path.join(work, f)
        name = stem_of(full)
        if name:
            out[name] = full
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("input")
    ap.add_argument("out_dir")
    ap.add_argument("--models")
    ap.add_argument("--no-drums", action="store_true")
    a = ap.parse_args()
    models = a.models
    if not models:
        sys.exit("--models DIR is needed")

    from audio_separator.separator import Separator

    t0 = time.time()
    os.makedirs(a.out_dir, exist_ok=True)
    work = tempfile.mkdtemp(prefix="stems-")
    sep = Separator(output_dir=work, output_format="WAV", model_file_dir=models)
    stems = {}

    first = run(sep, VOCALS, a.input, work)
    vocals = first.get("vocals")
    rest = first.get("other") or first.get("instrumental")
    if not vocals or not rest:
        sys.exit(f"vocal model gave {sorted(first)}")
    stems["vocals"] = vocals

    split = run(sep, SPLIT, rest, work)
    for name in ("drums", "bass", "other"):
        if name in split:
            stems[name] = split[name]

    if not a.no_drums and "drums" in split:
        kit = run(sep, DRUMS, split["drums"], work)
        for name, path in kit.items():
            stems[name] = path

    final = {}
    for name, path in stems.items():
        dest = os.path.join(a.out_dir, f"{name}.wav")
        shutil.move(path, dest)
        final[name] = dest
    shutil.rmtree(work, ignore_errors=True)
    print(json.dumps({"stems": final, "seconds": round(time.time() - t0, 1)}), flush=True)


if __name__ == "__main__":
    main()
