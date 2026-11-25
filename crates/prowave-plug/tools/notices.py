#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Pro Wave contributors.
"""Writes one plug-in's THIRD-PARTY-NOTICES.txt from the program's notices.

  python3 notices.py ID NAME VERSION NOTICES_MD CRATES_TSV OUT

NOTICES_MD is the program's THIRD-PARTY-NOTICES.md (public/ in the private source, the top of
the public source). The plug-in's own sound engine comes first (SECTIONS below), then every
other work of the engine library (each plug-in links the whole library), then the fonts of the
window and the Rust crates (CRATES_TSV: name, version, licence a line). Sections of works that
are only in the program (its singers, its MIDI effects, the optional installer) are left out.
"""
import re
import sys

# The notices of each plug-in's own engine: headings of the program's notices, by their start.
SECTIONS = {
    'prowave:macro-osc': ['mi-plaits-dsp'],
    'prowave:kick': ['LMMS'],
    'prowave:bass-line': ['LMMS'],
    'prowave:triple-osc': ['LMMS'],
    'prowave:wave-draw': ['LMMS'],
    'prowave:monstro': ['LMMS'],
    'prowave:pluck': ['Bespoke Synth'],
    'prowave:drum-synth': ['Bespoke Synth'],
    'prowave:engine': ['REDLINE'],
    'prowave:resonator': ['Resonator, spectral processor'],
    'prowave:digital-osc': ['Resonator, spectral processor'],
    'prowave:fm6': ['music-synthesizer-for-android'],
    'prowave:chip-opl': ['ymfm'],
    'prowave:chip-opn': ['ymfm'],
    'prowave:chip-opll': ['ymfm'],
    'prowave:chip-sid': ['chips m6581'],
    'prowave:chip-psg': ['Tetorica Sega PSG'],
    'prowave:lemon-limiter': ['Lemon Studio'],
    'prowave:press': ['Lemon Studio'],
    'prowave:lemon-transient-processor': ['Lemon Studio'],
    'prowave:lemon-soft-clipper': ['Lemon Studio'],
    'prowave:lemon-fast-distortion': ['Lemon Studio'],
    # Pro Wave's own.
    'prowave:tune': [],
    'prowave:texture': ['Mutable Instruments Beads'],
    'prowave:circuit-overdrive': ['Circuit model DSP'],
    'prowave:resonator-fx': ['Resonator, spectral processor'],
    'prowave:spectral': ['Resonator, spectral processor'],
}

# Sections of works only the program has, and the two written here instead.
LEFT_OUT = ['CHIMERA voice engine', 'Mandarin readings', 'Novox The Robot',
            'CMU Pronouncing Dictionary', 'DiffSinger', 'Hydrogen', 'Fonts', 'Rust crates']

FONTS = """Fonts

The window uses Barlow Condensed and JetBrains Mono, under the SIL Open Font License 1.1: the
texts are OFL-BarlowCondensed.txt and OFL-JetBrainsMono.txt next to this file.
"""


def sections(md):
    """(heading, text) of each '## ' section (its '### ' subsections included)."""
    out = []
    for part in re.split(r'(?m)^## ', md)[1:]:
        head, _, body = part.partition('\n')
        out.append((head.strip(), '## ' + part.rstrip() + '\n'))
    return out


def main():
    if len(sys.argv) != 7:
        sys.exit(__doc__)
    pid, name, version, md_path, crates_path, out_path = sys.argv[1:]
    if pid not in SECTIONS:
        sys.exit(f'notices.py: no notices listed for {pid}: add it to SECTIONS')
    with open(md_path, encoding='utf-8') as f:
        all_sections = sections(f.read())
    own = [s for s in all_sections if any(s[0].startswith(h) for h in SECTIONS[pid])]
    if len(own) < len(SECTIONS[pid]):
        sys.exit(f'notices.py: a section for {pid} is missing from {md_path}')
    rest = [s for s in all_sections
            if s not in own and not any(s[0].startswith(h) for h in LEFT_OUT)]
    rows = []
    with open(crates_path, encoding='utf-8') as f:
        for line in f:
            parts = line.rstrip('\n').split('\t')
            if len(parts) == 3:
                rows.append(parts)
    lines = [
        f'Third-party notices: {name} {version}',
        '',
        f'{name} is a plug-in made from Pro Wave (Yuri\'s Version), GPL-3.0-or-later (the text',
        'is LICENSE next to this file). It contains or is built with the works below, under their',
        'own licences; their notices are reproduced as those licences ask.',
        '',
    ]
    if own:
        lines += ['THIS PLUG-IN\'S SOUND ENGINE', '']
        lines += [text for _, text in own]
    else:
        lines += ['This plug-in\'s sound engine is Pro Wave\'s own.', '']
    lines += [
        'THE REST OF THE ENGINE LIBRARY',
        '',
        'Every Pro Wave plug-in is built from the whole engine library, so it carries these too.',
        '',
    ]
    lines += [text for _, text in rest]
    lines += [FONTS, 'Rust crates', '',
              'The Rust libraries the plug-in is built from and the licence each declares:', '']
    lines += [f'{n} {v}: {lic}' for n, v, lic in rows]
    with open(out_path, 'w', encoding='utf-8') as f:
        f.write('\n'.join(lines) + '\n')


if __name__ == '__main__':
    main()
