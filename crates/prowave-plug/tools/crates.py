#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Pro Wave contributors.
"""Reads `cargo metadata` (JSON, on standard input) of the plug-in crate and prints the crates
a plug-in is built from (normal and build dependencies, not the tests'), one a line: name,
version and declared licence, tab-separated, sorted. Pro Wave's own crates are left out."""
import json
import sys

OURS = {'prowave-plug', 'prowave-engine'}


def main():
    meta = json.load(sys.stdin)
    pkgs = {p['id']: p for p in meta['packages']}
    nodes = {n['id']: n for n in meta['resolve']['nodes']}
    root = meta['resolve']['root'] or next(i for i, p in pkgs.items() if p['name'] == 'prowave-plug')
    seen, todo = set(), [root]
    while todo:
        i = todo.pop()
        if i in seen:
            continue
        seen.add(i)
        for d in nodes[i]['deps']:
            if any(k['kind'] != 'dev' for k in d['dep_kinds']):
                todo.append(d['pkg'])
    rows = sorted({(pkgs[i]['name'], pkgs[i]['version'], pkgs[i].get('license') or 'see its source')
                   for i in seen if pkgs[i]['name'] not in OURS})
    for r in rows:
        print('\t'.join(r))


if __name__ == '__main__':
    main()
