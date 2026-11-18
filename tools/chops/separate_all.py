#!/usr/bin/env python3
"""Full stem separation of a folder of songs, resumable, one song at a time.

    separate_all.py IN_DIR OUT_DIR [--models DIR] [--limit N] [--only NAME] [--shard I/N]

Per song (OUT_DIR/<file stem>/), every model loaded once for the whole run:
  1. vocals_mel_band_roformer.ckpt                            mix          -> vocals, instrumental
  2. mel_band_roformer_karaoke_aufr33_viperx_sdr_10.1956.ckpt vocals       -> lead, backing
  3. BS-Roformer-SW.ckpt (6 stems)                            instrumental -> drums, bass, guitar, piano, other
  4. 17_HP-Wind_Inst-UVR.pth                                  other        -> horns (wind instruments), other_rest
  5. MDX23C-DrumSep-aufr33-jarredou.ckpt                      drums        -> kick, snare, toms, hh, ride, crash
  Groups, summed sample-aligned: rhythm = drums+bass+guitar+piano, orchestra = horns+other_rest.
  No strings model exists in audio-separator 0.48's list, so there is no strings stem.

Writes FLAC 16-bit stems, meta.json (models, seconds, null residuals, levels) and a DONE marker.
A song with DONE is skipped. One JSON line per song is appended to OUT_DIR/log.jsonl.

Model licences (the models are downloaded by audio-separator, MIT, into --models; none is
shipped here; check each before using the stems):
  vocals_mel_band_roformer.ckpt (Kimberley Jensen): no licence stated by its maker
  mel_band_roformer_karaoke_aufr33_viperx_sdr_10.1956.ckpt (aufr33, viperx): no licence found
  BS-Roformer-SW.ckpt: no licence found
  17_HP-Wind_Inst-UVR.pth (Ultimate Vocal Remover models): no licence found for the weights
  MDX23C-DrumSep-aufr33-jarredou.ckpt (aufr33, jarredou): no licence stated by its makers
"""
import argparse
import json
import os
import re
import shutil
import sys
import tempfile
import time

import numpy as np
import soundfile as sf

VOCALS = "vocals_mel_band_roformer.ckpt"
KARAOKE = "mel_band_roformer_karaoke_aufr33_viperx_sdr_10.1956.ckpt"
SIX = os.environ.get("SIX_MODEL", "BS-Roformer-SW.ckpt")
WIND = "17_HP-Wind_Inst-UVR.pth"
DRUMS = "MDX23C-DrumSep-aufr33-jarredou.ckpt"
AUDIO = (".mp3", ".wav", ".flac", ".aiff", ".aif", ".m4a", ".ogg")
SIX_NEED = ("drums", "bass", "guitar", "piano", "other")


def stem_of(path):
    m = re.findall(r"\(([^)]+)\)", os.path.basename(path))
    return m[-1].strip().lower() if m else None


class Stage:
    """One model, loaded once."""

    def __init__(self, model, models, work):
        from audio_separator.separator import Separator

        self.model = model
        self.sep = Separator(
            output_dir=work,
            output_format="WAV",
            model_file_dir=models,
            normalization_threshold=1.0,
            log_level=30,
        )
        self.sep.load_model(model_filename=model)

    def run(self, path, work):
        self.sep.output_dir = work
        inst = getattr(self.sep, "model_instance", None)
        if inst is not None:
            inst.output_dir = work
        out = {}
        for f in self.sep.separate(path):
            full = f if os.path.isabs(f) else os.path.join(work, f)
            name = stem_of(full)
            if name:
                out[name] = full
        return out


def read(path):
    x, sr = sf.read(path, dtype="float32", always_2d=True)
    if x.shape[1] == 1:
        x = np.repeat(x, 2, axis=1)
    return x, sr


def db(a, ref):
    e = float(np.mean(a.astype(np.float64) ** 2))
    r = float(np.mean(ref.astype(np.float64) ** 2))
    if r <= 0:
        return None
    return round(10 * np.log10(max(e, 1e-20) / r), 2)


def fit(x, n):
    if len(x) >= n:
        return x[:n]
    return np.pad(x, ((0, n - len(x)), (0, 0)))


def one(stages, src, work):
    times = {}

    def go(key, path):
        s = time.time()
        out = stages[key].run(path, work)
        times[key] = round(time.time() - s, 1)
        return out

    v = go("vocals", src)
    vocals, inst = v.get("vocals"), v.get("instrumental") or v.get("other")
    if not vocals or not inst:
        raise RuntimeError(f"vocal model gave {sorted(v)}")
    k = go("karaoke", vocals)
    lead, backing = k.get("vocals"), k.get("instrumental")
    if not lead or not backing:
        raise RuntimeError(f"karaoke model gave {sorted(k)}")
    six = go("six", inst)
    if any(n not in six for n in SIX_NEED):
        raise RuntimeError(f"6-stem model gave {sorted(six)}")
    w = go("wind", six["other"])
    horns, rest = w.get("woodwinds"), w.get("no woodwinds")
    if not horns or not rest:
        raise RuntimeError(f"wind model gave {sorted(w)}")
    kit = go("drums", six["drums"])
    files = {"vocals": vocals, "instrumental": inst, "lead": lead, "backing": backing,
             "horns": horns, "other_rest": rest}
    files.update({n: six[n] for n in SIX_NEED})
    files.update(kit)
    return files, sorted(kit), times


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("in_dir")
    ap.add_argument("out_dir")
    ap.add_argument("--models", help="where audio-separator keeps its models")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--only", default="")
    ap.add_argument("--shard", default="0/1", help="i/n: this worker takes every n-th song from i")
    a = ap.parse_args()
    if not a.models:
        sys.exit("--models DIR is needed")
    shard, shards = (int(x) for x in a.shard.split("/"))

    songs = sorted(f for f in os.listdir(a.in_dir) if f.lower().endswith(AUDIO))
    if a.only:
        songs = [s for s in songs if a.only in s]
    songs = songs[shard::shards]
    todo = [s for s in songs
            if not os.path.exists(os.path.join(a.out_dir, os.path.splitext(s)[0], "DONE"))]
    if a.limit:
        todo = todo[: a.limit]
    print(f"{len(songs)} songs, {len(todo)} to do", flush=True)
    if not todo:
        return 0
    os.makedirs(a.out_dir, exist_ok=True)
    root = tempfile.mkdtemp(prefix="chops-sep-")
    t0 = time.time()
    stages = {k: Stage(m, a.models, root) for k, m in
              (("vocals", VOCALS), ("karaoke", KARAOKE), ("six", SIX), ("wind", WIND), ("drums", DRUMS))}
    print(f"models loaded in {time.time() - t0:.1f}s", flush=True)
    log = open(os.path.join(a.out_dir, f"log-{shard}.jsonl"), "a")

    for song in todo:
        name = os.path.splitext(song)[0]
        dest = os.path.join(a.out_dir, name)
        work = tempfile.mkdtemp(dir=root)
        t = time.time()
        rec = {"song": name}
        try:
            src = os.path.join(a.in_dir, song)
            files, kit, times = one(stages, src, work)
            mix, sr = read(src)
            n = len(mix)
            data = {}
            for key, p in files.items():
                x, r = read(p)
                if r != sr:
                    raise RuntimeError(f"{key} at {r} Hz, mix at {sr}")
                data[key] = fit(x, n)
            data["rhythm"] = data["drums"] + data["bass"] + data["guitar"] + data["piano"]
            data["orchestra"] = data["horns"] + data["other_rest"]
            ix = data["instrumental"]
            rec["null_db"] = {
                "groups_vs_instrumental": db(ix - data["rhythm"] - data["orchestra"], ix),
                "six_vs_instrumental": db(ix - sum(data[q] for q in SIX_NEED), ix),
                "vocals_plus_inst_vs_mix": db(mix - data["vocals"] - ix, mix),
                "lead_plus_backing_vs_vocals": db(data["vocals"] - data["lead"] - data["backing"], data["vocals"]),
                "horns_plus_rest_vs_other": db(data["other"] - data["horns"] - data["other_rest"], data["other"]),
                "kit_vs_drums": db(data["drums"] - sum(data[q] for q in kit), data["drums"]),
            }
            rec["level_db"] = {key: db(x, mix) for key, x in data.items()}
            tmp = dest + ".part"
            shutil.rmtree(tmp, ignore_errors=True)
            os.makedirs(tmp)
            for key, x in data.items():
                sf.write(os.path.join(tmp, f"{key}.flac"), np.clip(x, -1, 1), sr, subtype="PCM_16")
            rec.update({
                "seconds": round(time.time() - t, 1), "stage_seconds": times,
                "duration": round(n / sr, 2), "sr": sr,
                "models": {"vocals": VOCALS, "karaoke": KARAOKE, "six": SIX, "wind": WIND, "drums": DRUMS},
                "stems": sorted(data),
            })
            with open(os.path.join(tmp, "meta.json"), "w") as f:
                json.dump(rec, f, indent=1)
            open(os.path.join(tmp, "DONE"), "w").close()
            shutil.rmtree(dest, ignore_errors=True)
            os.rename(tmp, dest)
            rec["ok"] = True
        except Exception as e:  # keep going; the song is retried on the next run
            rec.update({"ok": False, "error": repr(e), "seconds": round(time.time() - t, 1)})
        shutil.rmtree(work, ignore_errors=True)
        log.write(json.dumps(rec) + "\n")
        log.flush()
        print(json.dumps({q: rec.get(q) for q in ("song", "ok", "seconds", "stage_seconds", "error")}),
              flush=True)
    shutil.rmtree(root, ignore_errors=True)
    print(f"total {time.time() - t0:.1f}s", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
