#!/usr/bin/env python3
"""Writes the "Performance" section of the README from docs/benchmarks/results.json.

    python3 tools/bench/readme_section.py [results.json] [--readme README.md]

Without --readme the section goes to standard output. With it, the section replaces the existing
"## Performance" section of the file, or is inserted before "## Screenshots". Every number comes
from the JSON. Standard library only.
"""
import argparse
import json
import re


class Results:
    def __init__(self, data):
        self.data = data
        self.items = data["measurements"]
        self.machine = data["machine"]

    def row(self, ident, notes=None, hz=None):
        found = [m for m in self.items if m["id"] == ident and (notes is None or m["vault_notes"] == notes) and (hz is None or m.get("output_hz") == hz)]
        if not found:
            raise KeyError(f"{ident} notes={notes} hz={hz}")
        return found[0]

    def value(self, ident, field="median", notes=None, hz=None):
        return self.row(ident, notes, hz)[field]

    def triple(self, ident, notes=None, hz=None):
        r = self.row(ident, notes, hz)
        return f"{fmt(r['median'])} / {fmt(r['p95'])} / {fmt(r['p99'])}"

    def pair(self, ident, notes=None, hz=None):
        r = self.row(ident, notes, hz)
        return f"{fmt(r['median'])} / {fmt(r['p95'])}"


def fmt(value):
    if value >= 100:
        return f"{value:,.0f}"
    if value >= 10:
        return f"{value:.1f}"
    if value >= 1:
        return f"{value:.1f}"
    return f"{value:.2f}"


def gpu_name(text):
    inside = re.search(r"\[(.*?)\]", text)
    vendor = text.split(" Corporation")[0].split()[0]
    return f"{vendor} {inside.group(1)}" if inside else text


def table(header, rows):
    out = ["| " + " | ".join(header) + " |", "|" + "|".join("---" if i == 0 else "---:" for i in range(len(header))) + "|"]
    out += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
    return out


def picture(name, alt):
    return f'<picture><source media="(prefers-color-scheme: light)" srcset="media/bench-{name}-light.svg"><img src="media/bench-{name}-dark.svg" alt="{alt}"></picture>'


def build(data):
    r = Results(data)
    m = r.machine
    sizes = (1000, 10000, 100000)
    hzs = m["output_refresh_hz"]
    start = data["load_start"]["loadavg"]
    day = m["date_utc"][:10]
    corpus = [r.value("vault.corpus_size", notes=n) for n in sizes]
    out = ["## Performance", ""]
    app = m["app_binary"].get("commit", m["commit"])
    built = f"the application and the measuring programs are the release build of commit {app}" if m["commit"].startswith(app) else f"the application is the release build of commit {app}, the measuring programs come from commit {m['commit']}"
    out.append(
        f"Measured on {day}: {built}; on an {m['cpu']} ({m['logical_cores']} logical cores) with {m['memory_mib']:,} MiB of memory, "
        f"Linux {m['kernel']}, {m['compositor']} and an {gpu_name(m['gpu'])}. "
        f"The run took {data['duration_s']:,} s. The load average at the start was {', '.join(start)} over 1, 5 and 15 minutes; a run counts as loaded above {m['load_threshold']:g} on the one minute figure (`loaded`: {str(data['loaded']).lower()})."
    )
    out += ["", f"The vaults are generated with a fixed seed: notes of 0.5 to 4 KB with frontmatter, tags, links, headings, code blocks and tasks, {corpus[0]:.1f}, {corpus[1]:.1f} and {corpus[2]:.1f} MiB of Markdown for 1,000, 10,000 and 100,000 notes. "
            "The page cache of the system was warm. To measure again: `tools/bench/run.sh --full`.", ""]
    out += [picture("search", "Bar chart of full-text search latency for one term: median, 95th and 99th percentile, for vaults of 1,000, 10,000 and 100,000 notes"), "",
            picture("startup", "Bar chart of median startup time by vault size, window shown, first frame and vault ready, without and with the index cache"), "",
            picture("latency", "Bar chart of the time from a key press to the presented frame, by case and output rate, median and 95th percentile"), ""]

    out += ["### Search", ""]
    classes = [("One term", "search.one_term"), ("Two terms", "search.two_terms"), ("Exact phrase", "search.exact_phrase"), ("Tag filter", "search.tag_filter"), ("Path filter and a term", "search.path_filter"), ("No result", "search.no_result"),
              ("Full-text search, each keystroke of a word", "search.typing.fulltext_per_keystroke"), ("Palette, each keystroke of a name", "search.typing.palette_per_keystroke")]
    out += table(["Query", "10,000 notes", "100,000 notes"], [[label, r.triple(i, 10000), r.triple(i, 100000)] for label, i in classes])
    low = min(r.value(i, "runs", n) for _, i in classes for n in (10000, 100000))
    out += ["", f"Milliseconds, median / 95th percentile / 99th percentile, from at least {low:,} samples per cell. The measured call is `SearchIndex::search` with its default options (at most 500 notes and 20 matching lines per note, a 40 ms verification budget), including the matching lines a result panel draws; drawing is not measured. "
            f"At 1,000 notes the one term search takes {r.triple('search.one_term', 1000)} ms.", ""]

    out += ["### Startup", ""]
    stages = [("Window shown, no index cache", "startup.window_created.no_cache"), ("Vault ready, no index cache", "startup.vault_ready.no_cache"), ("Window shown, index cache present", "startup.window_created.with_cache"), ("Vault ready, index cache present", "startup.vault_ready.with_cache")]
    out += table(["From the start of `main` to", "1,000 notes", "10,000 notes", "100,000 notes"], [[label] + [r.pair(i, n) for n in sizes] for label, i in stages])
    out += ["", f"Milliseconds, median / 95th percentile, 10 launches per cell. Vault ready means the tree is shown and search is usable. Opening the vault and the index alone takes {fmt(r.value('startup.vault_and_index_open.with_cache', notes=100000))} ms with the cache and {fmt(r.value('startup.vault_and_index_open.no_cache', notes=100000))} ms without at 100,000 notes. "
            f"Building the index from nothing takes {fmt(r.value('index.build', notes=100000))} ms ({r.value('index.build.notes_per_s', notes=100000):,.0f} notes per second, {fmt(r.value('index.build.mib_per_s', notes=100000))} MiB per second); the cache takes {fmt(r.value('index.cache_size', notes=100000))} MiB on disk.", ""]

    out += ["### Memory", ""]
    mem = [("Private (anonymous) memory, one note open", "memory.one_note.private_anonymous"), ("Private (anonymous) memory, five notes in tabs", "memory.five_tabs.private_anonymous"),
           ("Proportional set size (PSS), one note open", "memory.one_note.pss"), ("Part of the PSS that belongs to the graphics driver mappings", "memory.one_note.gpu_driver_pss")]
    out += table(["MiB, median", "1,000 notes", "10,000 notes", "100,000 notes"], [[label] + [fmt(r.value(i, notes=n)) for n in sizes] for label, i in mem])
    gpu = [r.value("memory.one_note.gpu_driver_pss", notes=n) for n in sizes]
    out += ["", f"Read from `/proc/<pid>/smaps_rollup` 5 s after the vault is open, five runs per cell; the graphics driver mappings are summed from `/proc/<pid>/smaps` and are kept apart: " + (f"they are {fmt(gpu[0])} MiB at every vault size." if fmt(min(gpu)) == fmt(max(gpu)) else f"they stay between {fmt(min(gpu))} and {fmt(max(gpu))} MiB whatever the vault size."), ""]

    out += ["### At rest", ""]
    rest = []
    for label, key in (("Note shown, nothing focused", "note_shown"), ("Editor focused, caret visible", "editor_focused")):
        for what, suffix in (("frames drawn", "frame_requests_30s"), ("voluntary context switches", "wakeups_30s"), ("processor time (ms)", "cpu_ms_30s")):
            rest.append([f"{label}: {what}, 30 s"] + [f"{r.value(f'idle.{key}.{suffix}', hz=hz):,.0f}" for hz in hzs])
    out += table(["Over 30 s, medians of 3 runs"] + [f"{hz} Hz output" for hz in hzs], rest)
    shown = [r.value("idle.note_shown.frame_requests_30s", hz=hz) for hz in hzs]
    caret = [r.value("idle.editor_focused.frame_requests_30s", hz=hz) for hz in hzs]
    out += ["", f"A frame drawn is a frame callback asked of the compositor (`wl_surface.frame`). With a note shown the window draws {shown[0]:,.0f} frames per second at the three output rates; with the editor focused it draws {caret[0] / 30:.0f} per second ({caret[0]:,.0f} in 30 s), the blinking caret.", ""]

    out += ["### Typing and scrolling", ""]
    cases = [("Key press to presented frame, Edit, note of 200 lines", "latency.key_to_frame.edit_200_lines"), ("Key press to presented frame, Live, note of 200 lines", "latency.key_to_frame.live_200_lines"),
             ("Key press to presented frame, Edit, note of 10,000 lines", "latency.key_to_frame.edit_10000_lines"), ("Key press to presented frame, Live, note of 10,000 lines", "latency.key_to_frame.live_10000_lines"),
             ("Interval between presented frames, continuous scroll of the 10,000 line note", "latency.scroll_frame_interval"), ("Open the 10,000 line note", "latency.open_10000_lines")]
    out += table(["Median / 95th percentile, ms"] + [f"{hz} Hz output" for hz in hzs], [[label] + [r.pair(i, hz=hz) for hz in hzs] for label, i in cases])
    out += ["", f"{r.value('latency.key_to_frame.edit_200_lines', 'runs', hz=hzs[0])} key presses per case and rate, 1.5 s of rest before each. The time is taken from the Wayland debug stream, from the key to the next commit that carries a buffer; reading that stream slows the client a little."]
    kept = [h for h in hzs if r.row("latency.scroll_frame_interval", hz=h)["median"] <= 1000 / h * 1.05 and r.value("latency.scroll_frames_over_two_periods", hz=h) == 0]
    if kept:
        out[-1] += f" The scroll presents one frame per period of the output at {' and '.join(f'{h} Hz' for h in kept)}, with no interval longer than two periods."
    out += [""]

    out += ["### Graph, binaries and agent server", ""]
    graph = [["Graph of the notes: build", *[fmt(r.value("graph.build.notes", notes=n)) + " ms" for n in sizes]],
             ["Graph of the notes: layout until stable", *[fmt(r.value("graph.settle.notes", notes=n)) + " ms" for n in sizes]],
             ["Graph: update after editing a hub note", *[fmt(r.value("graph.update.hub_edited", notes=n)) + " ms" for n in sizes]],
             ["Index: update until a new term is found", *[fmt(r.value("index.update_after_write", notes=n)) + " ms" for n in sizes]],
             ["Rename a note and rewrite its links", *[fmt(r.value("note.rename_with_links", notes=n)) + " ms" for n in sizes]],
             ["`focus-mcp`: start to first answer", *[fmt(r.value("mcp.first_answer", notes=n)) + " ms" for n in sizes]]]
    out += table(["Median", "1,000 notes", "10,000 notes", "100,000 notes"], graph)
    out += ["", f"The `focus` binary is {fmt(r.value('binary.focus.size'))} MiB and `focus-mcp` {fmt(r.value('binary.focus-mcp.size'))} MiB (`stat`, stripped release build). The graph layout figures come from `graph_bench`, which rounds to the millisecond.", ""]

    e200 = [r.value("latency.key_to_frame.edit_200_lines", hz=h) for h in hzs] + [r.value("latency.key_to_frame.live_200_lines", hz=h) for h in hzs]
    big = [r.value(f"latency.key_to_frame.{c}_10000_lines", hz=h) for c in ("edit", "live") for h in hzs]
    worst = max(((r.value(f"latency.key_to_frame.{c}_10000_lines", "p95", hz=h), c.capitalize(), h) for c in ("edit", "live") for h in hzs))
    slower = ["### What is slower", "",
              f"- A key press in a note of 10,000 lines takes {fmt(min(big))} to {fmt(max(big))} ms at the median, against {fmt(min(e200))} to {fmt(max(e200))} ms for a note of 200 lines; the 95th percentile reaches {fmt(worst[0])} ms ({worst[1]} at {worst[2]} Hz)."]
    for h in hzs:
        s, over = r.row("latency.scroll_frame_interval", hz=h), r.value("latency.scroll_frames_over_two_periods", hz=h)
        if s["median"] > 1000 / h * 1.05:
            slower.append(f"- At {h} Hz, continuous scrolling of that note presents a frame every {fmt(s['median'])} ms at the median, about {1000 / s['median']:.0f} frames per second for a period of {fmt(1000 / h)} ms; {over:,.0f} of {s['runs']:,} intervals last more than two periods, the longest {fmt(s['max'])} ms.")
    out += slower + [
            f"- At 100,000 notes the application holds {fmt(r.value('memory.one_note.private_anonymous', notes=100000))} MiB of private memory with one note open and {fmt(r.value('memory.five_tabs.private_anonymous', notes=100000))} MiB with five tabs, against {fmt(r.value('memory.one_note.private_anonymous', notes=1000))} MiB at 1,000 notes.",
            f"- The tag filter takes {r.triple('search.tag_filter', 100000)} ms (median / 95th / 99th percentile) at 100,000 notes; one term takes {r.triple('search.one_term', 100000)} ms.",
            f"- Typing in the full-text search costs {r.pair('search.typing.fulltext_per_keystroke', 100000)} ms per keystroke (median / 95th percentile) at 100,000 notes; the palette costs {r.pair('search.typing.palette_per_keystroke', 100000)} ms.",
            f"- Without the index cache, a vault of 100,000 notes is ready after {fmt(r.value('startup.vault_ready.no_cache', notes=100000))} ms at the median and {fmt(r.value('startup.vault_ready.no_cache', 'p95', notes=100000))} ms at the 95th percentile, against {fmt(r.value('startup.vault_ready.with_cache', notes=100000))} ms with it. "
            f"At 1,000 notes without the cache the 95th percentile is {fmt(r.value('startup.vault_ready.no_cache', 'p95', notes=1000))} ms for a median of {fmt(r.value('startup.vault_ready.no_cache', notes=1000))} ms.",
            f"- The layout of the graph takes {fmt(r.value('graph.settle.notes', notes=100000))} ms to settle at 100,000 notes.", ""]

    out += ["### Not measured", "",
            "- Cold page cache: dropping it needs root.",
            "- Other machines, other output rates and other platforms: this is one machine and one run.",
            "- Search with typo tolerance in full text: the query language has none; only the palette matches approximately.",
            "- Graph steps under a millisecond.",
            "- Any other application: nothing here is compared with one.", ""]
    return "\n".join(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("results", nargs="?", default="docs/benchmarks/results.json")
    parser.add_argument("--readme")
    args = parser.parse_args()
    with open(args.results) as handle:
        section = build(json.load(handle))
    if not args.readme:
        print(section)
        return
    with open(args.readme) as handle:
        text = handle.read()
    match = re.search(r"^## Performance\n.*?(?=^## )", text, flags=re.S | re.M)
    if match:
        text = text[: match.start()] + section + "\n" + text[match.end():]
    else:
        at = text.index("## Screenshots")
        text = text[:at] + section + "\n" + text[at:]
    with open(args.readme, "w") as handle:
        handle.write(text)


if __name__ == "__main__":
    main()
