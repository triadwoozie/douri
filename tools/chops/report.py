#!/usr/bin/env python3
"""Sums up the pad sets' analysis.json files: per song tempo, sections and pads, and the
quality numbers across all loops.

    report.py CHOPS_ROOT [--bleed BLEED.json] [--tsv OUT.tsv]
"""
import argparse
import collections
import json
import os

import numpy as np

SEAM = 0.5      # |seam step| in loop-RMS units (make_chops.SEAM_DB)
CLICK = 1.0     # click score (make_chops.CLICK)
KICK_MS = 15.0  # a kick within this of the loop start counts as on the downbeat


def pct(x, q):
    return round(float(np.percentile(x, q)), 2) if len(x) else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("chops")
    ap.add_argument("--bleed")
    ap.add_argument("--tsv")
    a = ap.parse_args()
    songs, rows = [], []
    for d in sorted(os.listdir(a.chops)):
        f = os.path.join(a.chops, d, "analysis.json")
        if os.path.exists(f):
            songs.append(json.load(open(f)))
    sections = collections.Counter()
    pad_kinds = collections.Counter()
    seams, clicks, kicks, vocal_ratio, drifted, low = [], [], [], [], 0, 0
    loops = 0
    meters = collections.Counter()
    for s in songs:
        kinds = sorted({x["kind"] for x in s["sections"]})
        sections.update(kinds)
        meters[s.get("beats_per_bar")] += 1
        for p in s["pads"]:
            if p.get("section") == "full":
                continue
            loops += 1
            pad_kinds[p["section"]] += 1
            if p.get("seam_step") is not None:
                seams.append(abs(p["seam_step"]))
            if p.get("click") is not None:
                clicks.append(p["click"])
            if p.get("kick_offset_ms") is not None and p.get("stem") == "mix":
                kicks.append(p["kick_offset_ms"])
            if p.get("vocal_ratio_db") is not None and p["name"].startswith("Instrumental Chorus"):
                vocal_ratio.append(p["vocal_ratio_db"])
            drifted += bool(p.get("drift"))
            low += p.get("confidence", 1) < 0.5
        rows.append((s["song"], s["bpm"], s.get("beats_per_bar"), s["drift_pct"], s["pad_count"],
                     " ".join(f"{x['kind']}@{x['start_bar']}" for x in s["sections"])))
    seams, clicks, kicks = np.array(seams), np.array(clicks), np.array(kicks)
    out = {
        "songs": len(songs),
        "pads": sum(s["pad_count"] for s in songs),
        "loops": loops,
        "pads_per_song": {"min": min((s["pad_count"] for s in songs), default=0),
                          "median": pct([s["pad_count"] for s in songs], 50),
                          "max": max((s["pad_count"] for s in songs), default=0)},
        "songs_with_section": dict(sections),
        "loops_by_section": dict(pad_kinds),
        "beats_per_bar": {str(k): v for k, v in meters.items()},
        "bpm": {"min": pct([s["bpm"] for s in songs], 0), "median": pct([s["bpm"] for s in songs], 50),
                "max": pct([s["bpm"] for s in songs], 100)},
        "song_drift_pct": {"median": pct([s["drift_pct"] for s in songs], 50),
                           "p90": pct([s["drift_pct"] for s in songs], 90),
                           "max": pct([s["drift_pct"] for s in songs], 100)},
        "loops_marked_drift": drifted,
        "loops_low_confidence": low,
        "seam_step": {"median": pct(seams, 50), "p95": pct(seams, 95), "max": pct(seams, 100),
                      "pass": f"{int((seams <= SEAM).sum())}/{len(seams)}"},
        "click": {"median": pct(clicks, 50), "p95": pct(clicks, 95), "max": pct(clicks, 100),
                  "pass": f"{int((clicks <= CLICK).sum())}/{len(clicks)}"},
        "kick_at_start_ms": {"measured": len(kicks), "median_abs": pct(np.abs(kicks), 50),
                             "within_15ms": f"{int((np.abs(kicks) <= KICK_MS).sum())}/{len(kicks)}"},
        "inst_chorus_vocal_stem_vs_mix_db": {"median": pct(vocal_ratio, 50), "min": pct(vocal_ratio, 0),
                                             "max": pct(vocal_ratio, 100)},
    }
    if a.bleed and os.path.exists(a.bleed):
        b = json.load(open(a.bleed))
        out["bleed_by_reseparation"] = {k: {q: v.get(q) for q in ("loops", "median_db", "p90_db", "worst_db", "below_16bit")}
                                        for k, v in b.items()}
    print(json.dumps(out, indent=1))
    if a.tsv:
        with open(a.tsv, "w") as f:
            f.write("song\tbpm\tbeats_per_bar_ticks\tdrift_pct\tpads\tsections\n")
            for r in rows:
                f.write("\t".join(str(x) for x in r) + "\n")


if __name__ == "__main__":
    main()
