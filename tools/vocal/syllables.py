#!/usr/bin/env python3
"""Makes crates/prowave-app/assets/syllable_exceptions.txt: the CMUdict words whose syllable
count (the vowels of their first pronunciation) differs from Pro Wave's spelling rules.

The rules are `vowel_groups` in crates/prowave-app/src/lyrics.rs; this is the same code in
Python, so change both together and run this again:

    python3 tools/vocal/syllables.py > crates/prowave-app/assets/syllable_exceptions.txt

CMUdict (Carnegie Mellon University, BSD-style licence) is read from its GitHub repository.
"""
import sys
import urllib.request

URL = "https://raw.githubusercontent.com/cmusphinx/cmudict/master/cmudict.dict"


def is_vowel(c):
    return c in "aeiouy"


def vowel_groups(w):
    groups, i, n = [], 0, len(w)
    while i < n:
        if is_vowel(w[i]) and not (w[i] == "y" and i == 0):
            s = i
            while i < n and is_vowel(w[i]) and not (w[i] == "y" and i > s):
                i += 1
            groups.append((s, i))
        else:
            i += 1
    if len(groups) > 1:
        last = groups[-1]
        if last == (n - 1, n) and w[n - 1] == "e":
            le = n >= 3 and w[n - 2] == "l" and not is_vowel(w[n - 3])
            if not le:
                groups.pop()
        elif last == (n - 2, n - 1) and (w.endswith("ed") or w.endswith("es")) and n >= 4:
            before = w[n - 3]
            if w.endswith("ed"):
                sounded = before in "td"
            else:
                sounded = before in "sxzcg" or w[n - 4:n - 2] in ("ch", "sh")
            if not sounded:
                groups.pop()
    return groups


def main():
    text = urllib.request.urlopen(URL, timeout=60).read().decode("utf-8", "replace")
    seen = {}
    for line in text.splitlines():
        parts = line.split("#")[0].split()
        if len(parts) < 2:
            continue
        word = parts[0]
        if "(" in word:
            continue  # alternative pronunciations
        letters = "".join(c for c in word.lower() if "a" <= c <= "z")
        if not letters or letters in seen:
            continue
        seen[letters] = sum(1 for p in parts[1:] if p[-1].isdigit())
    out = sys.stdout
    for w in sorted(seen):
        n = seen[w]
        if n >= 1 and n != max(1, len(vowel_groups(w))):
            out.write(f"{w} {n}\n")


if __name__ == "__main__":
    main()
