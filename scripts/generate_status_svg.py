#!/usr/bin/env python3
"""Generate docs/status.svg for Dōri."""

import argparse
import html
import json
import math
from pathlib import Path
import sys
import xml.etree.ElementTree as ET

DEFAULT_MENU_COVERAGE = [
    {"name": "Edit",     "done": 22, "prog": 4, "rem": 8},
    {"name": "View",     "done": 20, "prog": 3, "rem": 9},
    {"name": "Track",    "done": 19, "prog": 3, "rem": 6},
    {"name": "File",     "done": 18, "prog": 2, "rem": 4},
    {"name": "Mix",      "done": 10, "prog": 3, "rem": 9},
    {"name": "Navigate", "done": 9,  "prog": 2, "rem": 10},
    {"name": "Window",   "done": 7,  "prog": 1, "rem": 8},
    {"name": "Help",     "done": 5,  "prog": 0, "rem": 7},
]

DEFAULT_ROADMAP_PROGRESS = [
    {"name": "UI",          "done": 6, "prog": 3, "rem": 4},
    {"name": "Engine",      "done": 6, "prog": 2, "rem": 4},
    {"name": "MIDI",        "done": 4, "prog": 2, "rem": 4},
    {"name": "Mixer",       "done": 3, "prog": 2, "rem": 5},
    {"name": "Plugins",     "done": 2, "prog": 1, "rem": 6},
    {"name": "Instruments", "done": 1, "prog": 1, "rem": 7},
    {"name": "Flex",        "done": 0, "prog": 0, "rem": 7},
    {"name": "Platform",    "done": 3, "prog": 1, "rem": 3},
]


def binary_treemap(items, x, y, w, h, gap=4):
    if not items:
        return []
    if len(items) == 1:
        return [(items[0], x, y, w, h)]

    total = sum(it["weight"] for it in items)
    if total <= 0:
        total = 1.0
    half = total / 2.0
    acc = 0
    best_idx = 1
    best_diff = float("inf")
    for i in range(1, len(items)):
        acc += items[i - 1]["weight"]
        diff = abs(acc - half)
        if diff < best_diff:
            best_diff = diff
            best_idx = i

    left_items = items[:best_idx]
    right_items = items[best_idx:]
    left_ratio = sum(it["weight"] for it in left_items) / total

    if w >= h:
        lw = (w - gap) * left_ratio
        rw = (w - gap) - lw
        return (
            binary_treemap(left_items, x, y, lw, h, gap)
            + binary_treemap(right_items, x + lw + gap, y, rw, h, gap)
        )
    lh = (h - gap) * left_ratio
    rh = (h - gap) - lh
    return (
        binary_treemap(left_items, x, y, w, lh, gap)
        + binary_treemap(right_items, x, y + lh + gap, w, rh, gap)
    )


def tile_grid(n_done, n_prog, n_rem, x, y, w, h, gap=2):
    n = n_done + n_prog + n_rem
    if n <= 0:
        return []
    best_cols = 1
    best_score = float("inf")
    for cols in range(1, n + 1):
        rows = math.ceil(n / cols)
        tw = (w - gap * (cols - 1)) / cols
        th = (h - gap * (rows - 1)) / rows
        if tw <= 4 or th <= 4:
            continue
        aspect = max(tw / th, th / tw)
        empty = cols * rows - n
        score = aspect + empty * 0.35
        if score < best_score:
            best_score = score
            best_cols = cols

    cols = best_cols
    rows = math.ceil(n / cols)
    statuses = ["done"] * n_done + ["prog"] * n_prog + ["rem"] * n_rem
    tiles = []
    idx = 0
    th = (h - gap * (rows - 1)) / rows
    for r in range(rows):
        items_in_row = cols if r < rows - 1 else (n - r * cols)
        ty = y + r * (th + gap)
        tw = (w - gap * (items_in_row - 1)) / max(items_in_row, 1)
        for c in range(items_in_row):
            tx = x + c * (tw + gap)
            tiles.append(
                (statuses[idx], round(tx, 1), round(ty, 1), round(tw, 1), round(th, 1))
            )
            idx += 1
    return tiles


def render_category(it, rx, ry, rw, rh):
    lines = ["    <g>"]
    tiles = tile_grid(it["done"], it["prog"], it["rem"], rx, ry, rw, rh, gap=2)
    for st, tx, ty, tw, th in tiles:
        cls = {"done": "tile-done", "prog": "tile-prog", "rem": "tile-rem"}[st]
        lines.append(f'      <rect x="{tx}" y="{ty}" width="{tw}" height="{th}" class="{cls}"/>')

    raw_name = it["name"]
    max_chars = max(int((rw - 10) / 6.2), 1)
    if len(raw_name) > max_chars and max_chars >= 4:
        display_name = raw_name[: max_chars - 1].rstrip() + "…"
    else:
        display_name = raw_name
    escaped_name = html.escape(display_name)

    tag_w = min(round(len(display_name) * 6.1 + 10, 1), round(max(rw - 8, 12), 1))
    tag_h = 16
    px = round(rx + 3, 1)
    py = round(ry + 3, 1)
    tx_label = round(px + tag_w / 2, 1)
    ty_label = round(py + 11.5, 1)
    lines.append(f'      <rect x="{px}" y="{py}" width="{tag_w}" height="{tag_h}" class="cat-tag"/>')
    lines.append(f'      <line x1="{px}" y1="{py + 0.5}" x2="{round(px + tag_w, 1)}" y2="{py + 0.5}" class="glass-rim-line"/>')
    lines.append(f'      <text x="{tx_label}" y="{ty_label}" text-anchor="middle" class="cat-label">{escaped_name}</text>')
    lines.append("    </g>")
    return "\n".join(lines)


def summarize(cats):
    done = sum(c["done"] for c in cats)
    prog = sum(c["prog"] for c in cats)
    total = sum(c["done"] + c["prog"] + c["rem"] for c in cats)
    pct = (done / total * 100.0) if total else 0.0
    return done, prog, total, pct


def build_svg(menu_coverage=None, roadmap_progress=None):
    menu_coverage = [dict(c) for c in (menu_coverage or DEFAULT_MENU_COVERAGE)]
    roadmap_progress = [dict(c) for c in (roadmap_progress or DEFAULT_ROADMAP_PROGRESS)]

    for c in menu_coverage + roadmap_progress:
        c["weight"] = max(c["done"] + c["prog"] + c["rem"], 1)

    l_done, l_prog, l_total, l_pct = summarize(menu_coverage)
    r_done, r_prog, r_total, r_pct = summarize(roadmap_progress)

    bar_w = 379.0
    l_done_w = round(bar_w * (l_done / max(l_total, 1)), 1)
    l_prog_w = round(bar_w * (l_prog / max(l_total, 1)), 1)
    l_prog_x = round(30.0 + l_done_w + 2.0, 1)

    r_done_w = round(bar_w * (r_done / max(r_total, 1)), 1)
    r_prog_w = round(bar_w * (r_prog / max(r_total, 1)), 1)
    r_prog_x = round(451.0 + r_done_w + 2.0, 1)

    left_layout = binary_treemap(menu_coverage, 30, 96, 379, 284, gap=4)
    right_layout = binary_treemap(roadmap_progress, 451, 96, 379, 284, gap=4)

    left_svg = "\n".join(render_category(it, rx, ry, rw, rh) for it, rx, ry, rw, rh in left_layout)
    right_svg = "\n".join(render_category(it, rx, ry, rw, rh) for it, rx, ry, rw, rh in right_layout)

    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 860 410" width="860" height="410" role="img" aria-label="Dōri Project Status: Menu Coverage {l_pct:.1f}% ({l_done}/{l_total}) and Roadmap Progress {r_pct:.1f}% ({r_done}/{r_total})">
  <defs>
    <!-- Subtle ambient monochrome light field behind the glass panels -->
    <radialGradient id="ambientGlowLeft" cx="0.24" cy="0.18" r="0.55">
      <stop offset="0%" stop-color="#FFFFFF" stop-opacity="0.06"/>
      <stop offset="100%" stop-color="#FFFFFF" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="ambientGlowRight" cx="0.76" cy="0.22" r="0.55">
      <stop offset="0%" stop-color="#FFFFFF" stop-opacity="0.045"/>
      <stop offset="100%" stop-color="#FFFFFF" stop-opacity="0"/>
    </radialGradient>

    <!-- Specular glass rim stroke (bright top-left, fading bottom-right) -->
    <linearGradient id="glassStroke" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#FFFFFF" stop-opacity="0.22"/>
      <stop offset="35%" stop-color="#FFFFFF" stop-opacity="0.08"/>
      <stop offset="100%" stop-color="#FFFFFF" stop-opacity="0.03"/>
    </linearGradient>

    <!-- Sharp diagonal glass reflection sheen -->
    <linearGradient id="glassSheen" x1="0" y1="0" x2="0.65" y2="0.85">
      <stop offset="0%" stop-color="#FFFFFF" stop-opacity="0.065"/>
      <stop offset="45%" stop-color="#FFFFFF" stop-opacity="0.015"/>
      <stop offset="100%" stop-color="#FFFFFF" stop-opacity="0"/>
    </linearGradient>

    <clipPath id="leftPanelClip">
      <rect x="16" y="44" width="407" height="350" rx="2"/>
    </clipPath>
    <clipPath id="rightPanelClip">
      <rect x="437" y="44" width="407" height="350" rx="2"/>
    </clipPath>
  </defs>

  <style>
    text {{
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
    }}
    .brand {{
      fill: #F2F3F5;
      font-size: 13px;
      font-weight: 700;
      letter-spacing: -0.01em;
    }}
    .brand-sub {{
      fill: #787D87;
      font-size: 11.5px;
      font-weight: 500;
    }}
    .legend-bar {{
      fill: #181A20;
      fill-opacity: 0.68;
      stroke: url(#glassStroke);
      stroke-width: 1;
    }}
    .legend-text {{
      fill: #9CA1AB;
      font-size: 10.5px;
      font-weight: 500;
    }}
    .glass-panel {{
      fill: #181A20;
      fill-opacity: 0.65;
      stroke: url(#glassStroke);
      stroke-width: 1;
    }}
    .glass-rim-line {{
      stroke: #FFFFFF;
      stroke-opacity: 0.24;
      stroke-width: 1;
    }}
    .panel-title {{
      fill: #F2F3F5;
      font-size: 13px;
      font-weight: 700;
    }}
    .stat-pct {{
      fill: #F7F8FA;
      font-size: 13px;
      font-weight: 700;
      font-variant-numeric: tabular-nums;
    }}
    .stat-frac {{
      fill: #8E949E;
      font-size: 12px;
      font-weight: 500;
      font-variant-numeric: tabular-nums;
    }}
    .tile-done {{
      fill: #DCE0E7;
      fill-opacity: 0.88;
      stroke: #FFFFFF;
      stroke-opacity: 0.35;
      stroke-width: 0.5;
    }}
    .tile-prog {{
      fill: #6C7483;
      fill-opacity: 0.42;
      stroke: #FFFFFF;
      stroke-opacity: 0.16;
      stroke-width: 0.75;
    }}
    .tile-rem {{
      fill: #13151A;
      fill-opacity: 0.72;
      stroke: #FFFFFF;
      stroke-opacity: 0.06;
      stroke-width: 0.75;
    }}
    .cat-tag {{
      fill: #0D0E12;
      fill-opacity: 0.82;
      stroke: #FFFFFF;
      stroke-opacity: 0.12;
      stroke-width: 0.75;
    }}
    .cat-label {{
      fill: #F2F3F5;
      font-size: 10px;
      font-weight: 600;
    }}
  </style>

  <!-- Outer Smoked Base -->
  <rect x="0.5" y="0.5" width="859" height="409" rx="2" fill="#0D0E12" stroke="url(#glassStroke)" stroke-width="1"/>
  <rect x="1" y="1" width="858" height="408" fill="url(#ambientGlowLeft)"/>
  <rect x="1" y="1" width="858" height="408" fill="url(#ambientGlowRight)"/>

  <!-- Top Header & Glass Legend Bar -->
  <text x="20" y="28" class="brand">D&#x14D;ri <tspan class="brand-sub">&#x00B7; Project Status</tspan></text>

  <g transform="translate(608, 14)">
    <rect x="0" y="0" width="236" height="22" rx="1" class="legend-bar"/>
    <line x1="1" y1="0.5" x2="235" y2="0.5" class="glass-rim-line"/>

    <rect x="10" y="7" width="8" height="8" class="tile-done"/>
    <text x="23" y="14.5" class="legend-text">Done</text>

    <rect x="66" y="7" width="8" height="8" class="tile-prog"/>
    <text x="79" y="14.5" class="legend-text">In Progress</text>

    <rect x="152" y="7" width="8" height="8" class="tile-rem"/>
    <text x="165" y="14.5" class="legend-text">Remaining</text>
  </g>

  <!-- Left Glass Panel: Menu Coverage -->
  <rect x="16" y="44" width="407" height="350" rx="2" class="glass-panel"/>
  <polygon points="16,44 260,44 120,240 16,240" fill="url(#glassSheen)" clip-path="url(#leftPanelClip)"/>
  <line x1="17" y1="44.5" x2="422" y2="44.5" class="glass-rim-line"/>

  <text x="30" y="68" class="panel-title">Menu Coverage</text>
  <text x="409" y="68" text-anchor="end">
    <tspan class="stat-pct">{l_pct:.1f}%</tspan>
    <tspan class="stat-frac"> &#x00B7; {l_done} / {l_total}</tspan>
  </text>

  <!-- Left Glass Progress Bar -->
  <rect x="30" y="79" width="379" height="5" fill="#13151A" fill-opacity="0.8" stroke="#FFFFFF" stroke-opacity="0.07" stroke-width="0.75"/>
  <rect x="30" y="79" width="{l_done_w}" height="5" fill="#DCE0E7" fill-opacity="0.9"/>
  <rect x="{l_prog_x}" y="79" width="{l_prog_w}" height="5" fill="#6C7483" fill-opacity="0.5"/>

  <!-- Left Treemap -->
  <g id="menu-treemap">
{left_svg}
  </g>

  <!-- Right Glass Panel: Roadmap Progress -->
  <rect x="437" y="44" width="407" height="350" rx="2" class="glass-panel"/>
  <polygon points="437,44 681,44 541,240 437,240" fill="url(#glassSheen)" clip-path="url(#rightPanelClip)"/>
  <line x1="438" y1="44.5" x2="843" y2="44.5" class="glass-rim-line"/>

  <text x="451" y="68" class="panel-title">Roadmap Progress</text>
  <text x="830" y="68" text-anchor="end">
    <tspan class="stat-pct">{r_pct:.1f}%</tspan>
    <tspan class="stat-frac"> &#x00B7; {r_done} / {r_total}</tspan>
  </text>

  <!-- Right Glass Progress Bar -->
  <rect x="451" y="79" width="379" height="5" fill="#13151A" fill-opacity="0.8" stroke="#FFFFFF" stroke-opacity="0.07" stroke-width="0.75"/>
  <rect x="451" y="79" width="{r_done_w}" height="5" fill="#DCE0E7" fill-opacity="0.9"/>
  <rect x="{r_prog_x}" y="79" width="{r_prog_w}" height="5" fill="#6C7483" fill-opacity="0.5"/>

  <!-- Right Treemap -->
  <g id="roadmap-treemap">
{right_svg}
  </g>
</svg>'''


def main():
    parser = argparse.ArgumentParser(description="Generate docs/status.svg for Dōri")
    parser.add_argument("--input", "-i", type=str, help="Path to JSON status input file or '-' for stdin")
    parser.add_argument("--output", "-o", type=str, default="docs/status.svg", help="Output SVG path")
    args = parser.parse_args()

    menu_coverage = None
    roadmap_progress = None

    input_text = None
    if args.input:
        if args.input == "-":
            input_text = sys.stdin.read()
        else:
            input_text = Path(args.input).read_text(encoding="utf-8")
    elif not sys.stdin.isatty():
        try:
            input_text = sys.stdin.read()
        except Exception:
            pass

    if input_text and input_text.strip():
        try:
            data = json.loads(input_text)
            menu_coverage = data.get("menu_coverage")
            roadmap_progress = data.get("roadmap_progress")
        except Exception as e:
            print(f"Warning: Failed to parse input JSON: {e}", file=sys.stderr)

    svg = build_svg(menu_coverage, roadmap_progress)
    ET.fromstring(svg)
    out = Path(args.output)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(svg, encoding="utf-8")
    print(f"Wrote {out} ({len(svg)} bytes)")


if __name__ == "__main__":
    main()
