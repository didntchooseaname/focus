#!/usr/bin/env python3
"""Draws the benchmark charts of the README from docs/benchmarks/results.json.

    python3 tools/bench/chart.py [results.json] [--out media] [--search-class one_term]

Writes bench-search-{dark,light}.svg and bench-startup-{dark,light}.svg. Standard library only.
Reads `measurements` ({id, area, vault_notes, unit, runs, min, median, p95, p99, max, output_hz}) and
`machine` of docs/benchmarks/results.json. Ids used: search.<class>, startup.<stage>.<no_cache|with_cache>,
latency.key_to_frame.<edit|live>_<200|10000>_lines (one value per output_hz).
"""
import argparse
import json
import math
import os
import sys

FONT = "Inter, 'Helvetica Neue', Arial, sans-serif"
PALETTES = {
    "dark": dict(bg="#070707", hatch="#1c1c1e", line="#303033", ink="#f5f5f6", ink2="#a1a1aa", grid="#232325"),
    "light": dict(bg="#ffffff", hatch="#e4e4e7", line="#d6d6da", ink="#0b0b0c", ink2="#4b4b52", grid="#e6e6e8"),
}
W, H = 960, 440
SHEET_X0, SHEET_X1 = 40, 920


def load(path):
    with open(path) as handle:
        data = json.load(handle)
    items = data if isinstance(data, list) else data.get("measurements") or data.get("results") or []
    return (data if isinstance(data, dict) else {}), items


def nice_max(value):
    if value <= 0:
        return 1.0
    exp = 10 ** math.floor(math.log10(value))
    for step in (1, 2, 2.5, 5, 10):
        if value <= step * exp:
            return step * exp
    return 10 * exp


def number(value):
    if value >= 100:
        return f"{value:,.0f}"
    if value >= 10:
        return f"{value:.0f}"
    if value >= 1:
        return f"{value:.1f}"
    return f"{value:.2f}"


def esc(text):
    return str(text).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


class Svg:
    def __init__(self, theme, title, footer):
        self.p = PALETTES[theme]
        p = self.p
        self.parts = [
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" role="img" aria-label="{esc(title)}">',
            "<defs>",
            f'<pattern id="hatch" width="12" height="12" patternUnits="userSpaceOnUse"><path d="M0 12L12 0M-3 3L3 -3M9 15L15 9" stroke="{p["hatch"]}" stroke-width="1" fill="none"/></pattern>',
            f'<pattern id="bar-hatch" width="5" height="5" patternUnits="userSpaceOnUse"><path d="M0 5L5 0M-1 1L1 -1M4 6L6 4" stroke="{p["ink"]}" stroke-width="1" fill="none"/></pattern>',
            "</defs>",
            f'<rect width="{W}" height="{H}" fill="{p["bg"]}"/>',
            f'<rect width="{W}" height="{H}" fill="url(#hatch)"/>',
            f'<rect x="{SHEET_X0}" width="{SHEET_X1 - SHEET_X0}" height="{H}" fill="{p["bg"]}"/>',
            f'<path d="M{SHEET_X0 + 0.5} 0V{H}M{SHEET_X1 + 0.5} 0V{H}" stroke="{p["line"]}" stroke-width="1"/>',
        ]
        self.text(SHEET_X0 + 24, 40, title, 17, p["ink"], weight=600)
        for index, line in enumerate(footer if isinstance(footer, list) else [footer]):
            self.text(SHEET_X0 + 24, H - 30 + 15 * index, line, 11, p["ink2"])

    def text(self, x, y, content, size, fill, anchor="start", weight=400):
        self.parts.append(
            f'<text x="{x}" y="{y}" font-family="{FONT}" font-size="{size}" font-weight="{weight}" fill="{fill}" text-anchor="{anchor}">{esc(content)}</text>'
        )

    def line(self, x0, y0, x1, y1, color, width=1):
        self.parts.append(f'<path d="M{x0} {y0}L{x1} {y1}" stroke="{color}" stroke-width="{width}" fill="none"/>')

    def bar(self, x, y, w, h, style):
        p = self.p
        if style == 0:
            self.parts.append(f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{max(h, 0.5):.1f}" fill="{p["ink"]}"/>')
        elif style == 1:
            self.parts.append(
                f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{max(h, 0.5):.1f}" fill="url(#bar-hatch)" stroke="{p["ink"]}" stroke-width="1"/>'
            )
        else:
            self.parts.append(
                f'<rect x="{x + 0.75:.1f}" y="{y:.1f}" width="{w - 1.5:.1f}" height="{max(h, 0.5):.1f}" fill="{p["bg"]}" stroke="{p["ink"]}" stroke-width="1.5"/>'
            )

    def legend(self, x, y, labels):
        for index, label in enumerate(labels):
            self.bar(x, y - 10, 14, 10, index % 3)
            self.text(x + 20, y, label, 12, self.p["ink2"])
            x += 28 + 7.2 * len(label)

    def finish(self):
        return "\n".join(self.parts + ["</svg>", ""])


def frame_footer(meta, unit_note):
    machine = meta.get("machine", {})
    memory = f"{machine['memory_mib'] / 1024:.0f} GiB" if machine.get("memory_mib") else None
    bits = [machine.get("cpu"), memory, machine.get("kernel"), f"commit {machine['commit']}" if machine.get("commit") else None, (machine.get("date_utc") or "")[:10]]
    text = ", ".join(str(b) for b in bits if b)
    if meta.get("loaded"):
        text += ", machine loaded"
    return [unit_note, text] if text else [unit_note]


def plot_axes(svg, x0, y0, x1, y1, top, unit):
    p = svg.p
    top = nice_max(top)
    decimals = 0 if top / 5 >= 1 else (1 if top / 5 >= 0.1 else 2)
    for tick in range(0, 6):
        value = top * tick / 5
        y = y1 - (y1 - y0) * tick / 5
        svg.line(x0, y, x1, y, p["grid"] if tick else p["ink2"], 1)
        svg.text(x0 - 8, y + 4, f"{value:,.{decimals}f}", 11, p["ink2"], anchor="end")
    svg.text(x0 - 8, y0 - 14, unit, 11, p["ink2"], anchor="end")
    return top


def grouped_bars(svg, box, groups, series, top, unit):
    x0, y0, x1, y1 = box
    top = plot_axes(svg, x0, y0, x1, y1, top, unit)
    width = (x1 - x0) / max(len(groups), 1)
    bar_w = min(34, (width - 28) / max(len(series), 1))
    for gi, (label, values) in enumerate(groups):
        gx = x0 + gi * width + (width - bar_w * len(series)) / 2
        for si, value in enumerate(values):
            if value is None:
                continue
            h = (y1 - y0) * value / top
            svg.bar(gx + si * bar_w, y1 - h, bar_w - 2, h, si % 3)
            svg.text(gx + si * bar_w + (bar_w - 2) / 2, y1 - h - 5, number(value), 11, svg.p["ink"], anchor="middle")
        for line, part in enumerate(label.split("|")):
            svg.text(x0 + gi * width + width / 2, y1 + 18 + 14 * line, part, 12, svg.p["ink"], anchor="middle")


def panels(svg, names, builder, top, unit):
    panel_w = (SHEET_X1 - SHEET_X0 - 48 - 84) / len(names)
    for index, name in enumerate(names):
        x0 = SHEET_X0 + 84 + index * (panel_w + 24)
        groups, series = builder(index)
        grouped_bars(svg, (x0, 110, x0 + panel_w - (24 if len(names) > 1 else 0), 350), groups, series, top, unit if index == 0 else "")
        svg.text(x0 + 4, 98, name, 12, svg.p["ink2"])


def size_label(notes):
    return f"{notes:,} notes"


def by_id(items, ident):
    return sorted((m for m in items if m.get("id") == ident), key=lambda m: m.get("vault_notes", 0))


def search_chart(theme, meta, items, search_class):
    rows = by_id(items, f"search.{search_class}")
    if not rows:
        return None
    runs = min(m.get("runs", 0) for m in rows)
    series = ["median", "95th percentile", "99th percentile"]
    groups = [(size_label(m["vault_notes"]), [m["median"], m["p95"], m["p99"]]) for m in rows]
    top = max(v for _, vals in groups for v in vals)
    title = f"Search latency by vault size: {search_class.replace('_', ' ')}"
    svg = Svg(theme, title, frame_footer(meta, f"At least {runs} queries per size, generated vaults, default options, matching lines included, drawing excluded"))
    svg.legend(SHEET_X0 + 24, 66, series)
    grouped_bars(svg, (SHEET_X0 + 84, 96, SHEET_X1 - 24, 350), groups, series, top, rows[0]["unit"])
    return svg.finish()


STAGES = [("window_created", "window shown"), ("first_frame", "first frame"), ("vault_ready", "vault ready")]


def startup_chart(theme, meta, items):
    caches = [("no_cache", "Without index cache"), ("with_cache", "With index cache")]
    data = {(c, s): by_id(items, f"startup.{s}.{c}") for c, _ in caches for s, _ in STAGES}
    if not any(data.values()):
        return None
    sizes = sorted({m["vault_notes"] for rows in data.values() for m in rows})
    runs = min(m["runs"] for rows in data.values() for m in rows)
    top = max(m["median"] for rows in data.values() for m in rows)

    def builder(index):
        cache = caches[index][0]
        groups = []
        for n in sizes:
            values = []
            for stage, _ in STAGES:
                match = [m for m in data[(cache, stage)] if m["vault_notes"] == n]
                values.append(match[0]["median"] if match else None)
            groups.append((size_label(n), values))
        return groups, [label for _, label in STAGES]

    svg = Svg(theme, "Startup by vault size, median", frame_footer(meta, f"Median of {runs} launches per size, generated vaults, measured from the start of main, index cache on disk or absent"))
    svg.legend(SHEET_X0 + 24, 66, [label for _, label in STAGES])
    panels(svg, [name for _, name in caches], builder, top, "ms")
    return svg.finish()


CASES = [("edit_200_lines", "Edit|200 lines"), ("live_200_lines", "Live|200 lines"), ("edit_10000_lines", "Edit|10,000 lines"), ("live_10000_lines", "Live|10,000 lines")]


def latency_chart(theme, meta, items):
    rows = [m for m in items if m.get("id", "").startswith("latency.key_to_frame.")]
    if not rows:
        return None
    rates = sorted({m["output_hz"] for m in rows})
    runs = min(m["runs"] for m in rows)
    top = max(m["p95"] for m in rows)

    def builder(index):
        field = ("median", "p95")[index]
        groups = []
        for case, label in CASES:
            values = []
            for hz in rates:
                match = [m for m in rows if m["id"] == f"latency.key_to_frame.{case}" and m["output_hz"] == hz]
                values.append(match[0][field] if match else None)
            groups.append((label, values))
        return groups, [f"{hz} Hz" for hz in rates]

    svg = Svg(theme, "Key press to presented frame, by case and output rate", frame_footer(meta, f"{runs} key presses per case and rate, Wayland commit that carries a buffer, Hyprland"))
    svg.legend(SHEET_X0 + 24, 66, [f"output {hz} Hz" for hz in rates])
    panels(svg, ["Median", "95th percentile"], builder, top, "ms")
    return svg.finish()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("results", nargs="?", default="docs/benchmarks/results.json")
    parser.add_argument("--out", default="media")
    parser.add_argument("--search-class", default="one_term")
    args = parser.parse_args()
    meta, items = load(args.results)
    os.makedirs(args.out, exist_ok=True)
    written = 0
    for theme in PALETTES:
        for name, svg in (("search", search_chart(theme, meta, items, args.search_class)), ("startup", startup_chart(theme, meta, items)), ("latency", latency_chart(theme, meta, items))):
            if svg is None:
                print(f"not drawn: no data for {name}", file=sys.stderr)
                continue
            path = os.path.join(args.out, f"bench-{name}-{theme}.svg")
            with open(path, "w") as handle:
                handle.write(svg)
            written += 1
            print(path)
    if not written:
        sys.exit("nothing drawn")


if __name__ == "__main__":
    main()
