#!/usr/bin/env python3
"""Cuts a song into a pad set of loops from its stems (see separate_all.py).

    make_chops.py SONG.mp3 --stems DIR --out DIR      one song
    make_chops.py --folder DIR --stems ROOT --out ROOT [--jobs N] [--force]

Writes OUT/<song file stem>/set.json (a pad-set format, `"format": "prowave-pad-set"`), one
FLAC per loop, and analysis.json (tempo, bar grid, sections, and the quality numbers of every
loop). Python, numpy, scipy, librosa (ISC), soundfile (BSD-3-Clause), essentia (AGPL-3.0: a
program that uses it is bound by its terms).

Only cut songs you have the rights to: the loops are copies of the recording.

Method
- Tempo: Essentia RhythmExtractor2013 (multifeature) on the mix: global BPM and beat ticks
  that follow the tempo as it drifts (beat_this
  overestimates and librosa's beat_track puts beat 1 on the snare, so neither is used).
- Downbeats: kick-dominant scoring (kick band minus snare band), but on
  the isolated kick and snare stems, plus bass onsets; a 4-state bar-position Viterbi over the
  ticks lets the bar phase recover where a tick is missed or added.
- Drift: local tempo of every 8-bar window from the ticks. Each loop is cut on its own local
  grid (a straight line fitted through its own ticks, refined by phase search on the onset
  envelope, edges moved to the leading edge of the hit), never on one grid for the song.
- Sections: beat-synchronous chroma + MFCC per bar, a self-similarity matrix, Foote novelty
  for boundaries (snapped to bars, favouring 4-bar phrases and vocal entries), segments
  clustered by repetition, then labelled with stem energy: chorus = the most repeated loud
  vocal section with the most backing vocals; breakdown = drums or bass drop out; vocal
  break = vocals over little music; instrumental break = music with no vocals.
- Loops: 4 and 8 bars from the downbeat that starts the section; 8 ms equal-power crossfade
  of the loop's tail into the audio before its start, so the wrap is the original continuity.
"""
import argparse
import json
import math
import os
import re
import shutil
import sys
import time
from concurrent.futures import ProcessPoolExecutor

import numpy as np
import soundfile as sf

HOP = 256
XFADE_S = 0.008
SECTION_NAMES = {
    "intro": "Intro", "verse": "Verse", "chorus": "Chorus", "bridge": "Bridge",
    "breakdown": "Breakdown", "vocal_break": "Vocal Break", "inst_break": "Instrumental Break",
    "outro": "Outro",
}
# Stem pads: (stem file, pad name prefix).
STEM_PADS = [
    ("drums", "Drums"), ("rhythm", "Rhythm Section"), ("horns", "Horns"),
    ("orchestra", "Orchestra"), ("backing", "Backing Vox"), ("bass", "Bass"),
]
SHORT = {"inst_break": "Break", "vocal_break": "Vocal Break", "breakdown": "Breakdown"}
PRESENT_DB = -18.0      # stem energy in the loop relative to the mix: below this, absent
DOUBT_DB = -12.0        # between PRESENT_DB and this: "?"
FLOOR_DB = 6.0          # quieter than DOUBT_DB: must stand this far above its own quiet bars (bleed)
DRIFT_RES_MS = 25.0     # onsets' RMS offset from the loop's straight grid: above, "large drift"
DRIFT_HALF_PCT = 2.0    # tempo of the loop's first and second half differing more: "large drift"
AUDIO = (".mp3", ".wav", ".flac", ".aiff", ".aif", ".m4a")


# ------------------------------------------------------------------ audio

def load(path, sr=None):
    x, r = sf.read(path, dtype="float32", always_2d=True)
    if x.shape[1] == 1:
        x = np.repeat(x, 2, axis=1)
    return x[:, :2], r


def mono(x):
    return x.mean(axis=1)


def rms(x):
    return float(np.sqrt(np.mean(np.square(x, dtype=np.float64)))) if len(x) else 0.0


def db(a, b):
    return 20 * math.log10(max(a, 1e-12) / max(b, 1e-12))


def band(y, sr, lo, hi):
    from scipy.signal import butter, sosfiltfilt
    if lo is None:
        sos = butter(4, hi / (sr / 2), btype="low", output="sos")
    elif hi is None:
        sos = butter(4, lo / (sr / 2), btype="high", output="sos")
    else:
        sos = butter(4, [lo / (sr / 2), hi / (sr / 2)], btype="band", output="sos")
    return sosfiltfilt(sos, y).astype(np.float32)


def onset_env(y, sr):
    import librosa
    return librosa.onset.onset_strength(y=y, sr=sr, hop_length=HOP)


def zs(x):
    x = np.asarray(x, dtype=np.float64)
    s = x.std()
    return (x - x.mean()) / s if s > 1e-12 else x * 0


def at(env, t, sr):
    """The envelope's peak within ±12 ms of time `t`."""
    f = t * sr / HOP
    w = max(1, int(0.012 * sr / HOP))
    lo, hi = max(0, int(f) - w), min(len(env), int(f) + w + 1)
    return float(env[lo:hi].max()) if hi > lo else 0.0


# ------------------------------------------------------------------ tempo and bars

def rhythm(y44):
    import essentia.standard as es
    bpm, ticks, conf, _, _ = es.RhythmExtractor2013(method="multifeature")(y44.astype(np.float32))
    return float(bpm), np.asarray(ticks, dtype=np.float64), float(conf)


def tick_features(ticks, kick_e, snare_e, bass_e, sr):
    k = zs([at(kick_e, t, sr) for t in ticks]).clip(-3, 3)
    sn = zs([at(snare_e, t, sr) for t in ticks]).clip(-3, 3)
    b = zs([at(bass_e, t, sr) for t in ticks]).clip(-3, 3)
    return k, sn, b


def meter(ticks, k, sn):
    """Ticks per bar: the drum pattern's period (autocorrelation of kick and snare at the
    ticks), kept to a bar of 1.2 to 4.2 seconds. 4 for 4/4, 6 for a 6/8 or 12/8 feel."""
    def ac(x, lag):
        x = x - x.mean()
        v = np.var(x)
        return float(np.mean(x[:-lag] * x[lag:]) / v) if v > 0 and lag < len(x) else 0.0
    score = {P: ac(k, P) + ac(sn, P) for P in (3, 4, 6, 8)}
    # A 4/4 pattern also repeats at 8, and 6/8 at 3 only weakly: prefer the shorter one unless
    # the longer is clearly stronger.
    if score[8] < score[4] + 0.15:
        score.pop(8)
    if score[3] < score[6] + 0.15:
        score.pop(3)
    P = max(score, key=score.get)
    beat = float(np.median(np.diff(ticks)))
    while P * beat > 4.2 and P % 2 == 0:
        P //= 2
    while P * beat < 1.2:
        P *= 2
    return P, {str(q): round(v, 2) for q, v in score.items()}


def downbeats(k, sn, b, P):
    """Bar positions of the ticks (0 = beat 1). A first pass finds the phase as a kick-dominant score
    does (among phases with at least half the loudest kick sum, the most kick-over-snare,
    here with bass onsets added); each position's average drum pattern is then a template, and
    a Viterbi over the P positions follows the pattern where a tick is missed or added."""
    n = len(k)
    kick_sums = np.array([k[p::P].sum() for p in range(P)])
    diff = np.array([(k - sn + 0.5 * b)[p::P].sum() for p in range(P)])
    ok = np.where(kick_sums >= kick_sums.min() + 0.5 * (kick_sums.max() - kick_sums.min()))[0]
    phase = int(ok[np.argmax(diff[ok])]) if len(ok) else int(np.argmax(diff))
    feats = np.stack([k, sn, b], axis=1)
    tmpl = np.stack([feats[(phase + j) % P::P].mean(axis=0) for j in range(P)])
    emit = feats @ tmpl.T
    stay, jump = math.log(1 - 2e-3), math.log(2e-3 / max(1, P - 1))
    trans = np.full((P, P), jump)
    for i in range(P):
        trans[i, (i + 1) % P] = stay
    score = emit[0].copy()
    back = np.zeros((n, P), dtype=np.int64)
    for i in range(1, n):
        cand = score[:, None] + trans
        back[i] = cand.argmax(axis=0)
        score = cand.max(axis=0) + emit[i]
    state = np.zeros(n, dtype=np.int64)
    state[-1] = int(score.argmax())
    for i in range(n - 1, 0, -1):
        state[i - 1] = back[i, state[i]]
    return state, phase


def local_tempos(ticks, idx, window=32):
    out = []
    for i in range(0, max(1, len(idx)), 8):
        a = idx[i] if i < len(idx) else None
        if a is None or a + window >= len(ticks):
            break
        seg = ticks[a:a + window + 1]
        slope = np.polyfit(np.arange(len(seg)), seg, 1)[0]
        out.append((float(ticks[a]), 60.0 / slope))
    return out


def onsets(y, sr):
    """Attack starts (s): librosa's onsets, moved back to the start of each rise."""
    import librosa
    return librosa.onset.onset_detect(y=y, sr=sr, hop_length=128, units="time", backtrack=True)


def attack_offset(ticks, y, sr):
    on = onsets(y, sr)
    offs = []
    for t in ticks:
        near = on[np.abs(on - t) < 0.08] if len(on) else on
        if len(near):
            offs.append(near[np.argmin(np.abs(near - t))] - t)
    return float(np.median(offs)) if len(offs) > 16 else 0.0


def envelope(y, sr, lo, hi):
    """|y| through a 1 ms running maximum, samples lo..hi (the hit's shape, not its cycles)."""
    from scipy.ndimage import maximum_filter1d
    lo, hi = max(0, lo), min(len(y), hi)
    return maximum_filter1d(np.abs(y[lo:hi]), size=max(1, int(0.001 * sr))), lo


def attack(y, sr, t, search=0.020, back=0.040, floor=None):
    """The attack start of the hit nearest `t`: the strongest envelope peak within ±search,
    then back to where the envelope first rises past 10 % of that peak (the
    refine_transient_start, on the envelope rather than raw samples, which cross zero every
    few ms in a kick). Returns (sample, peak), or (None, 0) without a hit above `floor`."""
    c = int(t * sr)
    env, base = envelope(y, sr, c - int((search + back) * sr), c + int(search * sr))
    if not len(env):
        return None, 0.0
    lo = c - int(search * sr) - base
    pk = max(0, lo) + int(np.argmax(env[max(0, lo):]))
    amp = float(env[pk])
    if amp < 1e-4 or (floor is not None and amp < floor):
        return None, amp
    i = pk
    while i > 0 and env[i - 1] >= amp * 0.10:
        i -= 1
    if i == 0:
        # Never quiet before it (a wash, or bleed): no clear attack.
        return None, amp
    return base + i, amp


def refine_edge(y, sr, t, kick=None, kick_floor=None):
    """The loop's edge at grid time `t`: the attack start of the hit there; where that is
    unclear (a cymbal wash), the kick's attack if a kick is there; else `t`."""
    i, _ = attack(y, sr, t, search=0.045)
    if i is None and kick is not None:
        i, _ = attack(kick, sr, t, search=0.045, floor=kick_floor)
    return i if i is not None else int(t * sr)


def peak_offset(env, t, sr, win=0.06):
    """Where the envelope's strongest peak within ±win of `t` is, relative to `t` (s)."""
    f = t * sr / HOP
    w = int(win * sr / HOP)
    lo, hi = max(0, int(f) - w), min(len(env), int(f) + w + 1)
    if hi <= lo:
        return None, 0.0
    i = lo + int(np.argmax(env[lo:hi]))
    return i * HOP / sr - t, float(env[i])


def loop_grid(ticks, first_tick, beats, env, sr):
    """The loop's own straight grid: its ticks fitted by a line, then tempo and phase searched
    on the onset envelope `env`. Returns the grid and how far the music strays from it: the
    RMS offset of each beat's onset from the grid (strong onsets only), and the tempo of the
    loop's first half against its second."""
    seg = ticks[first_tick:first_tick + beats + 1]
    if len(seg) < beats + 1:
        return None
    n = np.arange(len(seg))
    slope, icpt = np.polyfit(n, seg, 1)
    best = (-1.0, icpt, slope)
    for dp in np.linspace(-0.02, 0.02, 17):
        T = slope * (1 + dp)
        for dt in np.linspace(-0.02, 0.02, 17):
            t0 = icpt + dt
            sc = sum(at(env, t, sr) for t in t0 + T * n)
            if sc > best[0]:
                best = (sc, t0, T)
    _, t0, T = best
    offs, strength = [], []
    for t in t0 + T * n:
        o, e = peak_offset(env, t, sr)
        if o is not None:
            offs.append(o)
            strength.append(e)
    offs, strength = np.array(offs), np.array(strength)
    strong = strength >= np.percentile(strength, 40) if len(strength) else strength
    res_ms = float(np.sqrt(np.mean(offs[strong] ** 2)) * 1000) if strong.any() else 0.0
    # The beats' real places (grid + offset where the onset is strong): a tempo per half.
    real = t0 + T * n + np.where(strong, offs, 0.0)
    h = beats // 2
    s1 = np.polyfit(n[:h + 1], real[:h + 1], 1)[0]
    s2 = np.polyfit(n[h:], real[h:], 1)[0]
    half_pct = float(abs(s1 - s2) / T * 100)
    return {"t0": t0, "T": T, "res_ms": res_ms, "half_pct": half_pct}


# ------------------------------------------------------------------ sections

def bar_features(y, sr, bars, stems):
    import librosa
    chroma = librosa.feature.chroma_cqt(y=y, sr=sr, hop_length=512)
    mfcc = librosa.feature.mfcc(y=y, sr=sr, hop_length=512, n_mfcc=13)
    # CQT and STFT framing can differ by one frame at some lengths (Disco-Tick: 20169 vs 20168).
    n = min(chroma.shape[1], mfcc.shape[1])
    chroma, mfcc = chroma[:, :n], mfcc[:, :n]
    t = librosa.frames_to_time(np.arange(n), sr=sr, hop_length=512)
    feats, energy = [], {k: [] for k in stems}
    for a, b in zip(bars[:-1], bars[1:]):
        m = (t >= a) & (t < b)
        if not m.any():
            m = np.argmin(np.abs(t - a)) == np.arange(len(t))
        feats.append(np.concatenate([chroma[:, m].mean(axis=1), mfcc[1:, m].mean(axis=1)]))
        i0, i1 = int(a * sr), int(b * sr)
        for k, x in stems.items():
            energy[k].append(rms(x[i0:i1]))
    F = np.array(feats)
    F = (F - F.mean(axis=0)) / (F.std(axis=0) + 1e-9)
    # Chroma and timbre weighted alike.
    F[:, :12] *= math.sqrt(13 / 12)
    return F, {k: np.array(v) for k, v in energy.items()}


def ssm(F):
    N = F / (np.linalg.norm(F, axis=1, keepdims=True) + 1e-9)
    return N @ N.T


def novelty(S, half=4):
    n = len(S)
    k = np.ones((2 * half, 2 * half))
    k[:half, half:] = -1
    k[half:, :half] = -1
    g = np.outer(np.hanning(2 * half), np.hanning(2 * half))
    k *= g
    out = np.zeros(n)
    P = np.pad(S, half, mode="edge")
    for i in range(n):
        out[i] = (P[i:i + 2 * half, i:i + 2 * half] * k).sum()
    return out


# Phrase lengths in bars and how much each is favoured (soul is built of 4- and 8-bar phrases).
PHRASE = {2: -0.8, 4: 0.5, 6: -0.4, 8: 0.8, 10: -0.6, 12: 0.1, 16: 0.3}


def segments(S, vocal_on, n_bars):
    """Boundaries by dynamic programming: Foote novelty at each boundary, vocal entries and
    exits, and a prior on phrase lengths; the first segment may be any length (a pickup or
    an intro), the last too (a fade)."""
    nov = novelty(S, 4)
    nov = (nov - nov.mean()) / (nov.std() + 1e-9)
    v = np.r_[False, vocal_on].astype(int)
    change = np.abs(np.diff(v))[:n_bars] * 1.2
    edge = nov + change
    best = np.full(n_bars + 1, -1e9)
    prev = np.zeros(n_bars + 1, dtype=np.int64)
    best[0] = 0.0
    for i in range(1, n_bars + 1):
        for j in range(max(0, i - 16), i):
            L = i - j
            if j == 0:
                pr = 0.0 if L <= 16 else -1e9
            elif i == n_bars:
                pr = PHRASE.get(L, -0.2)
            else:
                pr = PHRASE.get(L, -1.5)
            sc = best[j] + pr + (edge[i] if i < n_bars else 0.0)
            if sc > best[i]:
                best[i], prev[i] = sc, j
    cuts, i = [], n_bars
    while i > 0:
        cuts.append(i)
        i = int(prev[i])
    cuts = [0] + cuts[::-1]
    return [(a_, b_) for a_, b_ in zip(cuts[:-1], cuts[1:])]


def smooth_ssm(S, L=4):
    """Each cell the mean of the next L cells along its diagonal (repeated sequences stand out)."""
    n = len(S)
    out = np.zeros_like(S)
    for q in range(L):
        out[:n - q, :n - q] += S[q:, q:]
    counts = np.minimum(L, n - np.maximum.outer(np.arange(n), np.arange(n)))
    return out / np.maximum(counts, 1)


def seg_sim(S, a, b):
    la, lb = a[1] - a[0], b[1] - b[0]
    n = min(la, lb)
    best = -1.0
    for shift in (-1, 0, 1):
        vals = [S[a[0] + k, b[0] + k + shift] for k in range(n)
                if 0 <= b[0] + k + shift < len(S) and a[0] + k < len(S)]
        if vals:
            best = max(best, float(np.mean(vals)))
    return best


def cluster(S, segs):
    """Segments that repeat each other share a label (average similarity above a threshold
    set from the song's own spread of segment similarities)."""
    n = len(segs)
    sim = np.full((n, n), np.nan)
    for i in range(n):
        for j in range(i):
            sim[i, j] = sim[j, i] = seg_sim(S, segs[i], segs[j])
    vals = sim[~np.isnan(sim)]
    thr = max(0.45, float(vals.mean() + 0.6 * vals.std())) if len(vals) else 1.0
    lab = list(range(n))
    for i in range(n):
        best, bj = thr, None
        for j in range(i):
            if sim[i, j] >= best:
                best, bj = sim[i, j], j
        if bj is not None:
            lab[i] = lab[bj]
    return lab, thr


def runs(mask, min_len):
    out, i, n = [], 0, len(mask)
    while i < n:
        if mask[i]:
            j = i
            while j < n and mask[j]:
                j += 1
            if j - i >= min_len:
                out.append((i, j))
            i = j
        else:
            i += 1
    return out


def label(S, segs, E, n_bars):
    """Sections from repetition and the stems.

    Chorus: the 8-bar block that repeats most (path-smoothed self-similarity), weighted by
    loudness and backing vocals, among bars with vocals; its instances are the other places
    that block comes back, each snapped to the nearest phrase boundary. Then the form between
    them: no vocals at the start = intro, at the end = outro; vocal stretches before the first
    chorus and between choruses = verses, unless one appears once, late, and unlike the verses
    = bridge; after the last chorus = outro."""
    voc = E["vocals"]
    act = voc[voc > np.percentile(voc, 60)]
    vref = float(np.median(act)) if len(act) else 1e-6
    vocal_on = voc > 0.25 * vref
    mix = E["mix"]
    L = 8 if n_bars >= 40 else 4
    S8 = smooth_ssm(S, L)
    off = np.abs(np.subtract.outer(np.arange(n_bars), np.arange(n_bars))) >= L
    vals = S8[off]
    thr = float(np.percentile(vals, 85)) if len(vals) else 1.0
    bounds = sorted({a for a, _ in segs} | {n_bars})

    def snap(i, reach=2):
        near = [b for b in bounds if abs(b - i) <= reach and b + L <= n_bars]
        return min(near, key=lambda b: abs(b - i)) if near else i

    def mean8(x, i):
        return float(x[i:i + L].mean())

    cand = [i for i in range(0, n_bars - L + 1) if vocal_on[i:i + L].mean() >= 0.5]
    out = []
    chorus = []
    if cand:
        R = np.array([np.clip(S8[i][off[i]] - thr, 0, None).sum() for i in cand])
        en = np.array([mean8(mix, i) for i in cand])
        bk = np.array([mean8(E["backing"], i) / (mean8(E["lead"], i) + 1e-9) for i in cand])
        score = zs(R) + 0.7 * zs(en) + 0.7 * zs(np.minimum(bk, 2.0))
        top = cand[int(np.argmax(score))]
        # Its instances: where the block comes back (local peaks of similarity, not overlapping).
        sim = S8[top]
        peaks = sorted(range(n_bars - L + 1), key=lambda j: -sim[j])
        inst = []
        for j in peaks:
            if sim[j] < thr and j != top:
                break
            if vocal_on[j:j + L].mean() < 0.4:
                continue
            if all(abs(j - k) >= L for k in inst):
                inst.append(j)
        inst = sorted({snap(j) for j in inst})
        # Keep them apart after snapping.
        kept = []
        for j in inst:
            if not kept or j - kept[-1] >= L:
                kept.append(j)
        n_inst = len(kept)
        conf = {1: 0.3, 2: 0.65}.get(n_inst, 0.85)
        if n_inst >= 2 and float(np.mean([sim[j] for j in kept if j != top] or [0])) > thr + 0.1:
            conf = min(1.0, conf + 0.1)
        chorus = [{"start": j, "end": min(n_bars, j + L), "kind": "chorus", "confidence": round(conf, 2),
                   "vocal": float(vocal_on[j:j + L].mean()), "energy": mean8(mix, j)} for j in kept]
    taken = np.zeros(n_bars, dtype=bool)
    for c in chorus:
        taken[c["start"]:c["end"]] = True
    out.extend(chorus)
    # The rest of the song in stretches between the choruses, cut further at phrase boundaries
    # where the vocals start or stop.
    stretches, i = [], 0
    while i < n_bars:
        if taken[i]:
            i += 1
            continue
        j = i
        while j < n_bars and not taken[j]:
            j += 1
        stretches.append((i, j))
        i = j
    first_ch = chorus[0]["start"] if chorus else n_bars
    last_ch = chorus[-1]["end"] if chorus else 0
    parts = []
    for a, b in stretches:
        cuts = [a] + [x for x in bounds if a < x < b] + [b]
        # Join neighbouring pieces that are both vocal or both not.
        pieces = []
        for x, y in zip(cuts[:-1], cuts[1:]):
            v = vocal_on[x:y].mean() >= 0.5
            if pieces and pieces[-1][2] == v:
                pieces[-1] = (pieces[-1][0], y, v)
            else:
                pieces.append((x, y, v))
        parts.extend(pieces)
    verses = []
    for x, y, v in parts:
        rec = {"start": x, "end": y, "vocal": float(vocal_on[x:y].mean()), "energy": float(mix[x:y].mean())}
        if not v:
            if x == 0:
                out.append({**rec, "kind": "intro", "confidence": 0.85})
            elif y >= n_bars - 1 or x >= last_ch and x > first_ch:
                out.append({**rec, "kind": "outro", "confidence": 0.75})
            else:
                out.append({**rec, "kind": "inst_break", "confidence": 0.7})
            continue
        if x >= last_ch and chorus and y >= n_bars - 2 and x > first_ch:
            out.append({**rec, "kind": "outro", "confidence": 0.55})
            continue
        verses.append(rec)
    # A vocal stretch unlike the others, once, after the first chorus: the bridge.
    for k, rec in enumerate(verses):
        others = [o for o in verses if o is not rec]
        like = max((S8[rec["start"], o["start"]] for o in others), default=-1.0)
        if chorus and rec["start"] > first_ch and like < thr and len(verses) >= 3:
            out.append({**rec, "kind": "bridge", "confidence": 0.55})
        else:
            c = 0.75 if like >= thr else 0.5
            out.append({**rec, "kind": "verse", "confidence": c})
    out.sort(key=lambda s: s["start"])
    # Bar-level parts that can sit inside any section.
    drums, bass, music = E["drums"], E["bass"], E["instrumental"]
    dmed, bmed, mmed = (float(np.median(x[x > np.percentile(x, 30)])) + 1e-9 for x in (drums, bass, music))
    extra = []
    inner = np.zeros(n_bars, dtype=bool)
    inner[2:max(2, n_bars - 2)] = True
    music_on = music > 0.3 * mmed
    bd = ((drums < 0.30 * dmed) | (bass < 0.25 * bmed)) & music_on & inner
    for a, b in runs(bd, 4):
        depth = 1 - min(drums[a:b].mean() / dmed, bass[a:b].mean() / bmed)
        extra.append({"start": a, "end": b, "kind": "breakdown", "confidence": round(float(np.clip(0.4 + depth, 0, 1)), 2)})
    vb = (voc > 0.6 * vref) & (music < 0.35 * mmed)
    for a, b in runs(vb, 4):
        extra.append({"start": a, "end": b, "kind": "vocal_break",
                      "confidence": round(float(np.clip(0.5 + (1 - music[a:b].mean() / (0.35 * mmed)), 0, 1)), 2)})
    ib = (voc < 0.08 * vref) & (music > 0.7 * mmed) & inner
    for a, b in runs(ib, 4):
        if any(s["kind"] == "inst_break" and s["start"] <= a and b <= s["end"] for s in out):
            continue
        extra.append({"start": a, "end": b, "kind": "inst_break", "confidence": 0.65})
    # A bar-level part already covered by a section of its kind (or by an earlier part of its
    # kind) is not a part of its own.
    def overlap(a_, b_):
        return max(0, min(a_["end"], b_["end"]) - max(a_["start"], b_["start"]))

    kept = []
    for e in sorted(extra, key=lambda e: e["start"]):
        same = [s for s in out + kept if s["kind"] == e["kind"]]
        if any(overlap(e, s) >= 0.5 * (e["end"] - e["start"]) for s in same):
            continue
        kept.append(e)
    return out, kept, vocal_on


# ------------------------------------------------------------------ loops

def render(x, sr, s0, s1):
    """Samples s0..s1 of `x`, the tail crossfaded into what precedes s0 (equal power)."""
    loop = x[s0:s1].copy()
    n = int(XFADE_S * sr)
    if s0 >= n and len(loop) > 4 * n:
        w = np.sin(np.linspace(0, np.pi / 2, n))[:, None] ** 2
        pre = x[s0 - n:s0]
        loop[-n:] = loop[-n:] * np.sqrt(1 - w) + pre * np.sqrt(w)
    return loop


def seam_quality(loop, x, s0, sr, length=None):
    """Loudness step across the wrap compared with the same step in the original audio, and a
    click score (the jump at the wrap over the loop's 99th-percentile sample step). `length`:
    the loop is the first `length` samples of `loop`."""
    w = int(0.002 * sr)
    m = mono(loop if length is None else loop[:length])
    xm = mono(x)
    if len(m) < 4 * w or s0 < w:
        return None, None
    # Steps in units of the loop's own RMS (so a quiet edge next to silence is a small step).
    ref = rms(m) + 1e-9
    seam = (rms(m[:w]) - rms(m[-w:])) / ref
    orig = (rms(xm[s0:s0 + w]) - rms(xm[s0 - w:s0])) / ref
    d = np.abs(np.diff(m))
    click = abs(float(m[0] - m[-1])) / (float(np.percentile(d, 99)) + 1e-9)
    return round(seam - orig, 2), round(click, 3)


SEAM_DB = 0.5    # a loop's loudness step at the wrap may differ from the original's by this
                 # much, in units of the loop's RMS
CLICK = 1.0      # and its jump at the wrap may be at most the loop's 99th-percentile sample step


def safe(name):
    name = name.replace("?", "").replace("~", "")
    return re.sub(r"[^A-Za-z0-9 ._-]", "", name).strip()


def share_end(longer, n, sr):
    """Where to end a loop of about `n` samples cut from the start of `longer`: within 1 ms,
    the sample whose value and slope best match the loop's first sample (so the wrap goes on
    as the audio did)."""
    w = int(0.001 * sr)
    best, be = None, n
    a0, d0 = longer[0], longer[1] - longer[0]
    for e in range(max(2, n - w), min(len(longer) - 1, n + w + 1)):
        c = np.abs(longer[e] - a0).sum() + 0.5 * np.abs((longer[e + 1] - longer[e]) - d0).sum()
        if best is None or c < best:
            best, be = c, e
    return be


def make(song, stems_dir, out_root, force=False, source_dir=None):
    t_start = time.time()
    import librosa  # noqa: F401  (imported here so worker processes load it once)
    key = os.path.splitext(os.path.basename(song))[0]
    dest = os.path.join(out_root, key)
    if os.path.exists(os.path.join(dest, "set.json")) and not force:
        return {"song": key, "skipped": True}
    need = ["vocals", "instrumental", "lead", "backing", "drums", "bass", "kick", "snare",
            "rhythm", "orchestra", "horns"]
    if not all(os.path.exists(os.path.join(stems_dir, f"{k}.flac")) for k in need):
        return {"song": key, "error": "stems missing"}
    x, sr = load(song)
    st = {k: load(os.path.join(stems_dir, f"{k}.flac"))[0] for k in need}
    n = min(len(x), *(len(v) for v in st.values()))
    x = x[:n]
    st = {k: v[:n] for k, v in st.items()}
    y = mono(x)
    if sr != 44100:
        import librosa
        y44 = librosa.resample(y, orig_sr=sr, target_sr=44100)
    else:
        y44 = y
    bpm, ticks, conf = rhythm(y44)
    # Essentia's ticks sit 15-35 ms after the drums' attacks on the records it was tuned on: the whole grid
    # is moved by the song's median offset to the drum stem's onsets (attack starts).
    drum_y = mono(st["drums"])
    tick_shift = attack_offset(ticks, drum_y, sr)
    ticks = ticks + tick_shift
    kick_e = onset_env(band(mono(st["kick"]), sr, None, 120), sr)
    snare_e = onset_env(band(mono(st["snare"]), sr, 180, 600), sr)
    bass_e = onset_env(mono(st["bass"]), sr)
    mix_e = onset_env(y, sr)
    kick_y = mono(st["kick"])  # the isolated kick, unfiltered (a zero-phase filter smears it earlier)
    kick_floor = 0.25 * float(np.percentile(np.abs(kick_y), 99.5))
    drum_e = onset_env(mono(st["drums"]), sr)
    kf, sf_, bf = tick_features(ticks, kick_e, snare_e, bass_e, sr)
    P, meter_scores = meter(ticks, kf, sf_)
    state, _ = downbeats(kf, sf_, bf, P)
    # Quarter notes a bar: 4 for 4/4 (ticks on quarters, P 4, or on eighths, P 8), 3 for a
    # 6/8 or 12/8 feel counted in eighths (P 6) or 3/4 (P 3). Pad tempos are in quarter notes.
    Q = {3: 3, 4: 4, 6: 3, 8: 4}.get(P, P)
    song_bpm = bpm * Q / P
    db_idx = [i for i in range(len(ticks)) if state[i] == 0]
    # A loop counts ticks from its downbeat, so a bar must hold P ticks: bars that do not
    # (where the pattern slipped) are kept as bars but loops never span them.
    regular = [db_idx[j + 1] - db_idx[j] == P for j in range(len(db_idx) - 1)] + [True]
    bars = [float(ticks[i]) for i in db_idx]
    if len(bars) < 12:
        return {"song": key, "error": f"only {len(bars)} bars found"}
    tempos = local_tempos(ticks, db_idx, 8 * P)
    tv = np.array([b for _, b in tempos]) if tempos else np.array([bpm])
    drift_pct = float((np.percentile(tv, 95) - np.percentile(tv, 5)) / np.median(tv) * 100)
    bar_list = bars + [float(ticks[min(len(ticks) - 1, db_idx[-1] + P)])]
    stems_m = {k: mono(v) for k, v in st.items()}
    stems_m["mix"] = y
    F, E = bar_features(y, sr, bar_list, stems_m)
    S = ssm(F)
    nb = len(bar_list) - 1
    voc = E["vocals"]
    act = voc[voc > np.percentile(voc, 60)]
    vocal_on = voc > 0.25 * (float(np.median(act)) if len(act) else 1e-6)
    segs = segments(S, vocal_on, nb)
    sects, extra, vocal_on = label(S, segs, E, nb)

    def loop_at(bar, bars_n, src):
        """A loop of `bars_n` bars from bar `bar`, cut from `src` on its own grid."""
        if bar + bars_n > nb or not all(regular[bar:bar + bars_n]):
            return None
        grid_env = drum_e if rms(drum_y[int(bar_list[bar] * sr):int(bar_list[bar + bars_n] * sr)]) > \
            0.05 * rms(y[int(bar_list[bar] * sr):int(bar_list[bar + bars_n] * sr)]) else mix_e
        g = loop_grid(ticks, db_idx[bar], P * bars_n, grid_env, sr)
        if g is None:
            return None
        t0, t1 = g["t0"], g["t0"] + g["T"] * P * bars_n
        edge = drum_y if rms(drum_y[int(t0 * sr):int(t1 * sr)]) > 0.05 * rms(y[int(t0 * sr):int(t1 * sr)]) else y
        s0 = refine_edge(edge, sr, t0, kick_y, kick_floor)
        s1 = refine_edge(edge, sr, t1, kick_y, kick_floor)
        if s1 - s0 < sr or s1 > n:
            return None
        return {"s0": s0, "s1": s1, "bpm": 60.0 * Q * bars_n / ((s1 - s0) / sr), "grid": g, "src": src}

    pads, used = [], set()

    def add(name, bar, bars_n, src, conf, section, stem="mix"):
        lp = loop_at(bar, bars_n, src)
        if lp is None:
            return None
        sig = (src, lp["s0"], bars_n)
        if sig in used:
            return None
        used.add(sig)
        g = lp["grid"]
        drift = g["res_ms"] > DRIFT_RES_MS or g["half_pct"] > DRIFT_HALF_PCT
        label_ = f"{name} {bars_n}b" + ("?" if conf < 0.5 else "") + (" ~" if drift else "")
        pads.append({"name": label_, "src": src, "s0": lp["s0"], "s1": lp["s1"], "bars": bars_n,
                     "bpm": round(lp["bpm"], 3), "confidence": round(conf, 2), "section": section,
                     "stem": stem, "start_bar": bar, "drift": drift,
                     "grid_res_ms": round(g["res_ms"], 1), "half_tempo_diff_pct": round(g["half_pct"], 2)})
        return pads[-1]

    def best_instance(kind, pool):
        cand = [s for s in pool if s["kind"] == kind and s["end"] - s["start"] >= 4]
        if not cand:
            return None
        # The surest one, then the one whose 8 bars sit on the straightest grid.
        def key_(s):
            g = loop_grid(ticks, db_idx[s["start"]], P * min(8, s["end"] - s["start"]), mix_e, sr) \
                if s["start"] + min(8, s["end"] - s["start"]) <= nb else None
            res = g["res_ms"] if g else 99
            return (-(s["end"] - s["start"] >= 8), -s["confidence"], res)
        return sorted(cand, key=key_)[0]

    pool = sects + extra
    chorus = best_instance("chorus", pool)
    # Pad 1: the whole track.
    pads.append({"name": "Full Track", "src": "song", "s0": 0, "s1": 0, "bars": None,
                 "bpm": round(song_bpm, 3), "confidence": 1.0, "section": "full", "stem": "mix"})
    if chorus is None:
        # No repeated loud vocal section: the loudest vocal section stands in, marked.
        voc_s = [s for s in sects if s["vocal"] >= 0.5 and s["end"] - s["start"] >= 4]
        if voc_s:
            chorus = dict(max(voc_s, key=lambda s: s["energy"]), kind="chorus", confidence=0.3)
    ch_len = None
    if chorus:
        ch_len = 8 if chorus["end"] - chorus["start"] >= 8 else 4
        c = chorus["confidence"]
        add("Instrumental Chorus", chorus["start"], ch_len, "instrumental", c, "chorus", "instrumental")
        add("Vocal Chorus", chorus["start"], ch_len, "vocals", c, "chorus", "vocals")
        add("Chorus", chorus["start"], ch_len, "mix", c, "chorus")
        if ch_len == 8:
            add("Chorus", chorus["start"], 4, "mix", c, "chorus")
            add("Instrumental Chorus", chorus["start"], 4, "instrumental", c, "chorus", "instrumental")
            add("Vocal Chorus", chorus["start"], 4, "vocals", c, "chorus", "vocals")
    # Every other section, in song order, 8 and 4 bars.
    chosen = []
    verses = [s for s in sects if s["kind"] == "verse" and s["end"] - s["start"] >= 4]
    kinds = ["intro", "verse", "bridge", "breakdown", "vocal_break", "inst_break", "outro"]
    for kind in kinds:
        if kind == "verse":
            inst = verses[:2]
        else:
            b = best_instance(kind, pool)
            inst = [b] if b else []
        for j, s in enumerate(inst):
            chosen.append((s["start"], kind, j, s))
    chosen.sort(key=lambda c: c[0])
    for start, kind, j, s in chosen:
        name = SECTION_NAMES[kind] + (f" {j + 1}" if kind == "verse" and len(verses) > 1 else "")
        length = s["end"] - s["start"]
        if length >= 8:
            add(name, s["start"], 8, "mix", s["confidence"], kind)
        add(name, s["start"], 4, "mix", s["confidence"], kind)
    # Stem loops where the part really plays.
    floors = {k: float(np.percentile(E[k][E[k] > 0], 10)) if (E[k] > 0).any() else 1e-9 for k in E}
    secs_for_stems = ([chorus] if chorus else []) + [c[3] for c in chosen]

    def presence(stem, s, bars_n):
        a, b = s["start"], min(s["start"] + bars_n, nb)
        e = float(E[stem][a:b].mean())
        return db(e, float(E["mix"][a:b].mean())), db(e, floors[stem])

    for stem, label_name in STEM_PADS:
        cands = []
        for s in secs_for_stems:
            rel, above = presence(stem, s, 8)
            if rel >= PRESENT_DB and (above >= FLOOR_DB or rel >= DOUBT_DB):
                cands.append((rel, s))
        if not cands:
            continue
        pick = []
        ch = [c for c in cands if chorus and c[1] is chorus]
        if ch:
            pick.append(ch[0])
        rest = sorted((c for c in cands if not (chorus and c[1] is chorus)), key=lambda c: -c[0])
        if rest:
            pick.append(rest[0])
        if not pick:
            pick = sorted(cands, key=lambda c: -c[0])[:1]
        for rel, s in pick[:2]:
            kind = s["kind"]
            sec_name = SHORT.get(kind, SECTION_NAMES[kind])
            conf = min(s["confidence"], 1.0) * (1.0 if rel >= DOUBT_DB else 0.45)
            length = s["end"] - s["start"]
            if length >= 8:
                add(f"{label_name} {sec_name}", s["start"], 8, stem, conf, kind, stem)
            add(f"{label_name} {sec_name}", s["start"], 4, stem, conf, kind, stem)
    # A lead vocal phrase from the first verse.
    if verses:
        s = verses[0]
        rel, above = presence("lead", s, 4)
        if rel >= PRESENT_DB and (above >= FLOOR_DB or rel >= DOUBT_DB):
            add("Lead Vox Verse", s["start"], 4, "lead", s["confidence"], "verse", "lead")

    # Write.
    tmp = dest + ".part"
    shutil.rmtree(tmp, ignore_errors=True)
    os.makedirs(tmp)
    set_pads = []
    written = {}
    for k, pd in enumerate(pads[:64]):
        if pd["src"] == "song":
            set_pads.append({"name": pd["name"], "file": os.path.join(source_dir or os.path.dirname(os.path.abspath(song)),
                                                                      os.path.basename(song)),
                             "start": 0, "end": 0, "gate": True, "loop": False, "bpm": pd["bpm"],
                             "confidence": 1.0})
            continue
        src = x if pd["src"] == "mix" else st[pd["src"]]
        start, end = 0, 0
        shared = written.get((pd["src"], pd["s0"]))
        if shared and shared[1] > pd["s1"]:
            # The first bars of a longer loop already written: the pad plays that file up to
            # here, the end moved to the sample that best continues into the loop's start.
            fname, _, longer = shared
            e = share_end(longer, pd["s1"] - pd["s0"], sr)
            seam, click = seam_quality(np.concatenate([longer[:e], longer[:8]]), src, pd["s0"], sr, length=e)
            if seam is not None and abs(seam) <= SEAM_DB and click <= CLICK:
                end = e
                pd["shared"] = fname
            else:
                shared = None
        if not (shared and end):
            loop = render(src, sr, pd["s0"], pd["s1"])
            fname = f"{k + 1:02d} {safe(pd['name'])}.flac"
            sf.write(os.path.join(tmp, fname), np.clip(loop, -1, 1), sr, subtype="PCM_16")
            written[(pd["src"], pd["s0"])] = (fname, pd["s1"], loop)
            seam, click = seam_quality(loop, src, pd["s0"], sr)
        pd["seam_step"], pd["click"] = seam, click
        t0 = pd["s0"] / sr
        # Downbeat check on the kick stem (not the signal the edge was cut on): where the kick
        # nearest the loop's start begins, relative to the start.
        ki, _ = attack(kick_y, sr, t0, search=0.060, back=0.040, floor=kick_floor)
        pd["kick_offset_ms"] = round((ki - pd["s0"]) / sr * 1000, 1) if ki is not None else None
        if pd["src"] == "instrumental":
            pd["vocal_ratio_db"] = round(db(rms(mono(st["vocals"][pd["s0"]:pd["s1"]])), rms(y[pd["s0"]:pd["s1"]])), 2)
        set_pads.append({"name": pd["name"], "file": fname, "start": start, "end": end, "gate": False,
                         "loop": True, "bpm": pd["bpm"], "bars": pd["bars"],
                         "confidence": pd["confidence"]})
    pset = {"format": "prowave-pad-set", "version": 1, "song": key, "bpm": round(song_bpm, 3),
            "pads": set_pads}
    with open(os.path.join(tmp, "set.json"), "w") as f:
        json.dump(pset, f, indent=1)
    sec_out = [{"kind": s["kind"], "start_bar": s["start"], "end_bar": s["end"],
                "start_s": round(bar_list[s["start"]], 2), "end_s": round(bar_list[min(s["end"], nb)], 2),
                "confidence": s["confidence"]} for s in sorted(sects + extra, key=lambda s: s["start"])]
    analysis = {"song": key, "bpm": round(song_bpm, 3), "tick_bpm": round(bpm, 3), "bpm_confidence": round(conf, 2), "bars": nb,
                "beats_per_bar": P, "meter_scores": meter_scores, "tick_shift_ms": round(tick_shift * 1000, 1),
                "irregular_bars": int(len(regular) - sum(regular)),
                "drift_pct": round(drift_pct, 2), "local_tempos": [round(b, 2) for _, b in tempos],
                "sections": sec_out, "pads": [{k: v for k, v in p.items() if k not in ("src",)} for p in pads[:64]],
                "pad_count": len(set_pads), "seconds": round(time.time() - t_start, 1)}
    with open(os.path.join(tmp, "analysis.json"), "w") as f:
        json.dump(analysis, f, indent=1)
    shutil.rmtree(dest, ignore_errors=True)
    os.rename(tmp, dest)
    return {"song": key, "pads": len(set_pads), "bpm": round(song_bpm, 2), "P": P, "drift_pct": round(drift_pct, 2),
            "sections": [s["kind"] for s in sec_out], "seconds": analysis["seconds"]}


def run_one(args):
    song, stems_root, out_root, force, source_dir = args
    key = os.path.splitext(os.path.basename(song))[0]
    if not os.path.exists(os.path.join(stems_root, key, "DONE")):
        return {"song": key, "skipped": "stems not ready"}
    try:
        return make(song, os.path.join(stems_root, key), out_root, force, source_dir)
    except Exception as e:  # one song failing does not stop the others
        import traceback
        return {"song": key, "error": repr(e), "trace": traceback.format_exc()[-800:]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("song", nargs="?")
    ap.add_argument("--folder")
    ap.add_argument("--stems", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--min-free-gb", type=float, default=8.0)
    ap.add_argument("--source-dir", help="the songs' folder as the Sampler will see it (pad 1's path)")
    a = ap.parse_args()
    free = shutil.disk_usage(os.path.dirname(os.path.abspath(a.out)) or "/").free / 1e9
    if free < a.min_free_gb:
        sys.exit(f"only {free:.1f} GB free (need {a.min_free_gb})")
    os.makedirs(a.out, exist_ok=True)
    if a.song:
        stems = a.stems if os.path.exists(os.path.join(a.stems, "vocals.flac")) else \
            os.path.join(a.stems, os.path.splitext(os.path.basename(a.song))[0])
        r = make(a.song, stems, a.out, a.force, a.source_dir)
        print(json.dumps(r))
        return
    songs = sorted(os.path.join(a.folder, f) for f in os.listdir(a.folder) if f.lower().endswith(AUDIO))
    jobs = [(s, a.stems, a.out, a.force, a.source_dir) for s in songs]
    log = open(os.path.join(a.out, "log.jsonl"), "a")
    with ProcessPoolExecutor(a.jobs) as ex:
        for r in ex.map(run_one, jobs):
            log.write(json.dumps(r) + "\n")
            log.flush()
            print(json.dumps({k: r.get(k) for k in ("song", "pads", "bpm", "drift_pct", "error", "skipped", "seconds")}), flush=True)


if __name__ == "__main__":
    main()
