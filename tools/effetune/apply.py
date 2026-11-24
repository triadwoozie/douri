#!/usr/bin/env python3
"""Runs an audio file through a chain of EffeTune effects, offline.

    python apply.py INPUT OUTPUT.wav CHAIN.json

Pro Wave's File > Bounce > Region through EffeTune Chain… runs this with the Python that has
EffeTune installed (`pip install effetune soundfile`; https://github.com/Frieve-A/effetune,
MIT). EffeTune itself is not part of Pro Wave: it is whatever version that Python has.

CHAIN.json is a list of effects in order, each {"type": "Compressor", "parameters":
{"threshold": -24, "ratio": 4}}: the type is an EffeTune effect class, the parameters its
constructor's keywords. An object with a "chain" list holds the same list.

The output is 32-bit float WAV at the input's sample rate, with the input's channels and
length: EffeTune does not resample, and a tail longer than the input is cut.
"""
import json
import sys


def fail(message):
    print(message, file=sys.stderr)
    sys.exit(1)


def main():
    if len(sys.argv) != 4:
        fail("usage: apply.py INPUT OUTPUT.wav CHAIN.json")
    source, target, chain_file = sys.argv[1:4]
    try:
        import numpy as np
        import soundfile as sf
        import effetune as et
    except ImportError as e:
        fail(f"this Python lacks a package ({e.name}): pip install effetune soundfile")
    try:
        with open(chain_file, encoding="utf-8") as f:
            document = json.load(f)
    except (OSError, ValueError) as e:
        fail(f"cannot read the chain: {e}")
    entries = document.get("chain") if isinstance(document, dict) else document
    if not isinstance(entries, list) or not entries:
        fail("the chain is not a list of effects")
    effects = []
    for n, entry in enumerate(entries, 1):
        if not isinstance(entry, dict) or not isinstance(entry.get("type"), str):
            fail(f"effect {n} has no type")
        parameters = entry.get("parameters", {})
        if not isinstance(parameters, dict):
            fail(f"effect {n}: parameters is not an object")
        try:
            effects.append(et.create_effect(entry["type"], **parameters))
        except Exception as e:  # EffeTune's own errors say what is wrong with the entry.
            fail(f"effect {n} ({entry['type']}): {e}")
    try:
        decoded, rate = sf.read(source, dtype="float32", always_2d=True)
    except Exception as e:
        fail(f"cannot read {source}: {e}")
    # EffeTune takes (channels, frames); SoundFile gives (frames, channels).
    audio = np.ascontiguousarray(decoded.T, dtype=np.float32)
    try:
        output = et.Chain(effects).process(audio, sample_rate=rate)
    except Exception as e:
        fail(f"the chain failed: {e}")
    frames = audio.shape[1]
    output = np.asarray(output, dtype=np.float32)[:, :frames]
    if output.shape[1] < frames:
        output = np.pad(output, ((0, 0), (0, frames - output.shape[1])))
    sf.write(target, output.T, rate, subtype="FLOAT")
    print(json.dumps({"frames": int(frames), "sample_rate": int(rate), "effects": len(effects)}))


if __name__ == "__main__":
    main()
