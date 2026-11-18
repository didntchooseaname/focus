#!/usr/bin/env python3
import json, sys
from collections import OrderedDict

AREAS = OrderedDict([
    ("startup", "Startup"),
    ("index", "Index, update and rename"),
    ("search", "Search"),
    ("memory", "Memory of the application"),
    ("graph", "Graph"),
    ("interaction", "Interaction and idle"),
    ("binaries", "Binaries"),
])
TITLES = {
    "startup.window_created": "Process start to window created",
    "startup.first_frame": "Process start to first frame",
    "startup.vault_ready": "Process start to vault ready (tree shown, search usable)",
    "startup.vault_and_index_open": "Vault and index opening, inside the line above",
    "vault.open.no_cache": "Vault open, no cache",
    "vault.open.with_cache": "Vault open, cache present",
    "index.build": "Search index build, after the vault is open",
    "index.build.notes_per_s": "Vault open and index build, notes per second",
    "index.build.mib_per_s": "Vault open and index build, corpus MiB per second",
    "index.build.peak_rss": "Peak resident memory of the first build (process total)",
    "index.cache_size": "Index and vault cache on disk",
    "vault.corpus_size": "Size of the notes",
    "index.load": "Index load from cache",
    "index.update_after_write": "File written to term findable",
    "note.rename_with_links": "Note renamed, links of other notes rewritten",
    "note.rename_with_links.links_rewritten": "Links rewritten by that rename",
    "index.steady_rss": "Resident memory of the vault and index, steady state",
    "search.one_term": "One term",
    "search.two_terms": "Two terms",
    "search.exact_phrase": "Exact phrase",
    "search.tag_filter": "Tag filter",
    "search.path_filter": "Path filter and a term",
    "search.no_result": "Query with no result",
    "search.typing.palette_per_keystroke": "Typing in the palette, per keystroke",
    "search.typing.fulltext_per_keystroke": "Typing in full text search, per keystroke (3 characters and more)",
    "search.typo.palette_two_letters_swapped": "Palette, word typed with two letters swapped",
    "memory.one_note.private_anonymous": "One note open, 5 s idle: private (anonymous) memory",
    "memory.one_note.pss": "One note open, 5 s idle: PSS",
    "memory.one_note.rss": "One note open, 5 s idle: RSS",
    "memory.one_note.gpu_driver_pss": "One note open: PSS of graphics driver mappings (part of the PSS)",
    "memory.five_tabs.private_anonymous": "Five tabs, 5 s idle: private (anonymous) memory",
    "memory.five_tabs.pss": "Five tabs, 5 s idle: PSS",
    "memory.five_tabs.rss": "Five tabs, 5 s idle: RSS",
    "memory.five_tabs.gpu_driver_pss": "Five tabs: PSS of graphics driver mappings (part of the PSS)",
    "graph.build.notes": "Graph build, notes and links",
    "graph.settle.notes": "Layout to a settled state from scratch (build included), notes and links",
    "graph.build.tags": "Graph build, with tags",
    "graph.settle.tags": "Layout to a settled state from scratch (build included), with tags",
    "graph.nodes.notes": "Nodes, notes and links", "graph.edges.notes": "Edges, notes and links",
    "graph.nodes.tags": "Nodes, with tags", "graph.edges.tags": "Edges, with tags",
    "graph.update.added": "Update after a note is added, linking to the largest hub",
    "graph.update.hub_edited": "Update after the largest hub is edited",
    "graph.update.removed": "Update after a note is removed",
    "graph.update.added.full_rebuild": "For comparison: full rebuild after a note is added",
    "graph.update.hub_edited.full_rebuild": "For comparison: full rebuild after the hub is edited",
    "graph.update.removed.full_rebuild": "For comparison: full rebuild after a note is removed",
    "mcp.first_answer": "focus-mcp: start to first answer",
    "idle.commits_30s": "Idle, note shown and editor focused: commits (one per drawn frame)",
    "idle.frame_requests_30s": "Idle: frame requests to the compositor",
    "idle.wakeups_30s": "Idle: voluntary context switches (process wakeups)",
    "idle.cpu_ms_30s": "Idle: processor time",
    "latency.key_to_frame.edit_200_lines": "Key to presented frame, Edit, note of 200 lines",
    "latency.key_to_frame.edit_10000_lines": "Key to presented frame, Edit, note of 10 000 lines",
    "latency.key_to_frame.live_200_lines": "Key to presented frame, Live, note of 200 lines",
    "latency.key_to_frame.live_10000_lines": "Key to presented frame, Live, note of 10 000 lines",
    "latency.scroll_frame_interval": "Interval between presented frames, continuous scroll of the 10 000 line note",
    "latency.scroll_frames_over_two_periods": "Scroll: intervals longer than two periods of the output",
    "latency.open_10000_lines": "Opening the note of 10 000 lines to its first presented frame",
    "binary.focus.size": "focus binary",
    "binary.focus-mcp.size": "focus-mcp binary",
}
LATENCY = {"search.", "index.update", "note.rename"}


def fmt(x):
    if x >= 1000:
        return f"{x:,.0f}".replace(",", " ")
    if x >= 100:
        return f"{x:.0f}"
    if x >= 10:
        return f"{x:.1f}"
    if x >= 1:
        return f"{x:.2f}"
    return f"{x:.3f}"


def table(rows, with_notes):
    head = "| Measure | " + ("Notes | " if with_notes else "") + "Unit | Runs | Min | Median | 95th | 99th | Max |"
    sep = "|---|" + ("---:|" if with_notes else "") + "---|---:|---:|---:|---:|---:|---:|"
    lines = [head, sep]
    for r in rows:
        base, _, cache = r["id"].rpartition(".")
        if r["area"] == "startup" and cache in ("no_cache", "with_cache"):
            title = TITLES.get(base, base) + (", no cache" if cache == "no_cache" else ", cache present")
        else:
            title = TITLES.get(r["id"], r["id"])
        if r.get("private"):
            title += " (private vault)"
        cells = [title] + ([fmt(r["vault_notes"])] if with_notes else []) + [r["unit"], str(r["runs"])] + [fmt(r[k]) for k in ("min", "median", "p95", "p99", "max")]
        lines.append("| " + " | ".join(cells) + " |")
    return lines


def main():
    data = json.load(open(sys.argv[1]))
    machine, results = data["machine"], data["measurements"]
    done = [r for r in results if "status" not in r]
    pending = [r for r in results if "status" in r]
    out = ["# Benchmarks", "", "Generated by `tools/bench/report.py` from `docs/benchmarks/results.json`. Do not edit by hand; rerun `tools/bench/run.sh`.", ""]
    cores, threshold = machine["logical_cores"], machine.get("load_threshold", 0.25 * machine["logical_cores"])
    start = float(data["load_start"]["loadavg"][0])
    if data["loaded"]:
        out += [f"> Taken on a loaded machine (`\"loaded\": true`): the one minute load average was {start:g} at the start, {start / cores:.2f} per logical core ({cores} cores), above the threshold of 0.25 per core ({threshold:g}). About {min(100, round(100 * start / cores))}% of the logical cores were busy with other work, which adds to every time below.", ""]
    out += ["## Machine and run", "",
            f"- Processor: {machine['cpu']}, {machine['logical_cores']} logical cores",
            f"- Memory: {machine['memory_mib']} MiB; kernel {machine['kernel']}; session {machine['compositor']}; graphics {machine['gpu']}",
            f"- Compiler: {machine['rustc']}; commit {machine['commit']}; application binary built {machine['app_binary']['modified_utc']}",
            f"- Output refresh rate seen by the application: {', '.join(f'{h:g} Hz' for h in machine.get('output_refresh_hz', [])) or 'not recorded'}",
            f"- Date: {machine['date_utc']}; mode: {data['mode']}; duration: {data['duration_s']} s",
            f"- Load average (1, 5, 15 min) at the start: {' '.join(data['load_start']['loadavg'])} on {cores} logical cores ({start / cores:.2f} per core; a run is loaded above {threshold:g}); at the end: {' '.join(data['load_end']['loadavg'])}, which includes the work of the suite itself; compiler processes running: {data['load_start']['cargo_rustc_processes']} then {data['load_end']['cargo_rustc_processes']}",
            "", "## Method", "",
            "- Vaults are generated by `crates/focus-search/examples/support/vaultgen.rs` with a fixed seed: notes of 0.5 to 4 KB with frontmatter, tags, links, headings, code blocks and tasks, in a folder tree, 1 000, 10 000 and 100 000 notes. They live under `target/bench/`. The same command always produces the same vault.",
            "- Times are wall clock. Each line shows the number of runs and the minimum, median, 95th and 99th percentile and maximum of those runs. Latencies of a query class use at least 1 000 samples in a full run (200 in a quick run).",
            "- Queries are drawn with a fixed seed from the vocabulary of the vault titles. A search time is the call of `SearchIndex::search` with its default options (at most 500 notes and 20 matching lines per note, 40 ms verification budget), including the matching lines a result panel would draw. Drawing is not included.",
            "- The system page cache is warm: dropping it needs root. \"No cache\" means without Focus's own cache.",
            "- Startup is measured through `tools/shot.sh` on the capture workspace with `FOCUS_PROFILE=1`, from entering `main`. Memory is read from `/proc/<pid>/smaps_rollup` 5 s after the vault is open and 5 s after five more notes are opened in tabs; graphics driver mappings are summed from `/proc/<pid>/smaps`.",
            "- Interaction: a script drives the application through `tools/shot.sh` with `WAYLAND_DEBUG=1`; a `stamp` line marks each key, scroll start and end, and the time to the next `wl_surface.commit` that carries a buffer is the latency. Reading the debug stream slows the client a little. Idle counts commits, frame requests, voluntary context switches and processor time over 30 s after 4 s of rest, a note shown and the editor focused. A result marked \"frame loop not fixed in this binary\" was taken with an application that still redraws a still window.",
            "- Key latency: 200 keys per case in a full run, 1.5 s of rest before each key (30 keys in a quick run). Scroll: 30 000 px over 6 s of the generated 10 000 line note, three passes in a full run.",
            "- No other application was measured: there is no comparison here.", ""]
    for area, title in AREAS.items():
        rows = [r for r in done if r["area"] == area]
        if not rows:
            continue
        out += [f"## {title}", ""]
        if area == "binaries":
            out += table(rows, False)
        elif area == "interaction":
            rates = sorted({r.get("output_hz") for r in rows if r.get("output_hz")}) or [None]
            for rate in rates:
                label = f"Output at {rate} Hz" if rate else "Output rate not recorded"
                out += [f"### {label}", ""] + table([r for r in rows if r.get("output_hz") == rate and "status" not in r], False) + [""]
        else:
            sizes = sorted({r["vault_notes"] for r in rows})
            for size in sizes:
                out += [f"### {fmt(size)} notes", ""] + table([r for r in rows if r["vault_notes"] == size], False) + [""]
        out.append("")
    flagged = sorted({r["frame_loop"] for r in done if "frame_loop" in r})
    if flagged:
        out += ["> Interaction results marked: " + "; ".join(flagged) + ".", ""]
    if pending:
        out += ["## Not measured", "", "Not invented here:", ""]
        out += [f"- {r['title']}{' at ' + str(r['output_hz']) + ' Hz' if r.get('output_hz') else ''}: {r['status']}" for r in pending] + [""]
    out += ["## Not measured, and why", "",
            "- Cold page cache: needs root to drop the cache.",
            "- Graph times below 10 ms: `graph_bench` prints them rounded to the millisecond, so a build of 1 ms means \"under 1.5 ms\".",
            "- Search with typo tolerance in full text: the query language has none; only the palette matches approximately (measured above).",
            "- Other platforms and other machines: one machine per file.", "",
            "## Commands", "", "Everything: `tools/bench/run.sh --full` (or `--quick` for tuning, `--allow-load` on a busy machine). Per measure:", ""]
    seen = set()
    for r in done:
        key = (r["command"].split("<")[0] if r["area"] != "startup" else r["command"])
        if (r["area"], r["command"]) in seen:
            continue
        seen.add((r["area"], r["command"]))
        out.append(f"- `{r['id'].split('.')[0]}`, {fmt(r['vault_notes'])} notes: `{r['command']}`")
    open(sys.argv[2], "w").write("\n".join(out) + "\n")


if __name__ == "__main__":
    main()
