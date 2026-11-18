#!/usr/bin/env python3
import argparse, json, os, re, statistics, subprocess, sys, time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import interaction as ia

ROOT = Path(__file__).resolve().parents[2]
BENCH = ROOT / "target" / "bench"
FOCUS_BIN = Path(os.environ.get("FOCUS_BIN", str(ROOT / "target/release/focus")))
MCP_BIN = Path(os.environ.get("FOCUS_MCP_BIN", str(FOCUS_BIN.with_name("focus-mcp"))))
MARK = re.compile(r"focus:\s+([\d.]+) ms\s+heap\s+([\d.]+) MiB\s+anon\s+([\d.]+) MiB\s+(.*)")
GPU_MAPPING = re.compile(r"(nvidia|/dri/|mesa|vulkan|libGL|libEGL|amdgpu|radeon|libdrm|intel|iris|radv|anv|llvmpipe)", re.I)


def sh(*cmd, default=""):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=20).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return default


def percentile(values, p):
    values = sorted(values)
    return values[min(len(values) - 1, round((len(values) - 1) * p))]


def sig(x, digits=4):
    return x if x == 0 or not isinstance(x, float) else float(f"{x:.{digits}g}")


def entry(id, area, notes, unit, values, command, **extra):
    return {"id": id, "area": area, "vault_notes": notes, "unit": unit, "runs": len(values), "min": sig(min(values)),
            "median": sig(statistics.median(values)), "p95": sig(percentile(values, 0.95)), "p99": sig(percentile(values, 0.99)),
            "max": sig(max(values)), "command": command, **extra}


def load_state():
    procs = sh("sh", "-c", "pgrep -c -x 'cargo|rustc' || true")
    return {"loadavg": open("/proc/loadavg").read().split()[:3], "cargo_rustc_processes": int(procs or 0)}


def machine():
    cpu = next((l.split(":", 1)[1].strip() for l in open("/proc/cpuinfo") if l.startswith("model name")), "")
    mem_kb = next(int(l.split()[1]) for l in open("/proc/meminfo") if l.startswith("MemTotal"))
    gpu = sh("sh", "-c", "lspci 2>/dev/null | grep -iE 'vga|3d|display' | head -1 | cut -d: -f3-").strip()
    status = sh("git", "-C", str(ROOT), "status", "--porcelain", "--untracked-files=no")
    return {
        "cpu": cpu, "logical_cores": os.cpu_count(), "memory_mib": mem_kb // 1024,
        "kernel": sh("uname", "-r"), "compositor": os.environ.get("XDG_CURRENT_DESKTOP", "unknown"),
        "gpu": gpu or "unknown", "rustc": sh("rustc", "--version"),
        "commit": sh("git", "-C", str(ROOT), "rev-parse", "--short=10", "HEAD") + (" (modified tree)" if status else ""),
        "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "app_binary": {"size_bytes": FOCUS_BIN.stat().st_size if FOCUS_BIN.exists() else None,
                       "modified_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(FOCUS_BIN.stat().st_mtime)) if FOCUS_BIN.exists() else None},
    }


def rust_suite(notes, mode, command_name):
    vault = BENCH / f"vault-{notes}"
    out = BENCH / f"suite-{notes}.json"
    exe = ROOT / "target/release/examples/bench_suite"
    command = f"cargo run --release -p focus-search --example bench_suite -- target/bench/vault-{notes} {notes} {mode} <out.json> <command>"
    subprocess.run([str(exe), str(vault), str(notes), mode, str(out), command], check=True)
    return json.load(open(out))


def graph_suite(notes, runs):
    exe = ROOT / "target/release/examples/graph_bench"
    vault = BENCH / f"vault-{notes}"
    full = f"cargo run --release -p focus-graph --example graph_bench -- target/bench/vault-{notes}"
    series = {}
    for _ in range(runs):
        text = subprocess.run([str(exe), str(vault)], capture_output=True, text=True, check=True).stdout
        for m in re.finditer(r"(\d+) nodes, (\d+) edges(, tags)?: settled without position cache in ([\d.]+) ms \(build ([\d.]+) ms, (\d+) steps\)", text):
            tag = "tags" if m.group(3) else "notes"
            series.setdefault(f"graph.build.{tag}", []).append(float(m.group(5)))
            series.setdefault(f"graph.settle.{tag}", []).append(float(m.group(4)))
            series.setdefault(f"graph.nodes.{tag}", []).append(float(m.group(1)))
            series.setdefault(f"graph.edges.{tag}", []).append(float(m.group(2)))
    for _ in range(runs):
        text = subprocess.run([str(exe), str(vault), "--update"], capture_output=True, text=True, check=True).stdout
        for label, key in (("note added", "added"), ("largest hub edited", "hub_edited"), ("note removed", "removed")):
            m = re.search(label + r"[^:]*: update ([\d.]+) ms, full build ([\d.]+) ms", text)
            if m:
                series.setdefault(f"graph.update.{key}", []).append(float(m.group(1)))
                series.setdefault(f"graph.update.{key}.full_rebuild", []).append(float(m.group(2)))
    out = []
    for id, values in series.items():
        unit = "nodes" if ".nodes." in id else "edges" if ".edges." in id else "ms"
        out.append(entry(id, "graph", notes, unit, values, full + (" --update" if ".update." in id else "")))
    return out


def run_shot(script, vault, cache, log, extra_env=None):
    env = {**os.environ, "FOCUS_BIN": str(FOCUS_BIN), "FOCUS_PROFILE": "1", "FOCUS_CACHE_DIR": str(cache),
           "FOCUS_SHOT_DIR": str(BENCH / "shots"), "FOCUS_SHOT_TIMEOUT": "180", **(extra_env or {})}
    return subprocess.Popen([str(ROOT / "tools/shot.sh"), str(script), str(vault)], env=env, stdout=subprocess.DEVNULL,
                            stderr=open(log, "w"), cwd=ROOT)


def marks(log):
    found = {}
    for line in open(log, errors="replace"):
        m = MARK.search(line)
        if m:
            found.setdefault(m.group(4).strip(), (float(m.group(1)), float(m.group(2)), float(m.group(3))))
    return found


def startup(vault, notes, runs, private=False):
    script = BENCH / "startup.focus"
    script.write_text(f"wait {1500 if notes <= 10000 else 10000}\nquit\n")
    cache = BENCH / "startup-cache" / (vault.name if not private else "private")
    series = {}
    command = f"FOCUS_BIN=<focus> FOCUS_PROFILE=1 FOCUS_CACHE_DIR=<empty or filled folder> tools/shot.sh <script: wait 1500, quit> target/bench/vault-{notes}"
    for with_cache in (False, True):
        for i in range(runs):
            if not with_cache:
                subprocess.run(["rm", "-rf", str(cache)])
            log = BENCH / "startup.log"
            code = run_shot(script, vault, cache, log).wait()
            if code != 0:
                raise SystemExit(f"tools/shot.sh exited with {code} (see {log})")
            for label, value in marks(log).items():
                key = "vault_ready" if label.startswith("vault open") else label.replace(" ", "_")
                if key in ("window_created", "first_frame", "vault_ready"):
                    series.setdefault((key, with_cache), []).append(value[0])
                    if key == "vault_ready":
                        m = re.search(r"vault and index ([\d.]+) ms", label)
                        if m:
                            series.setdefault(("vault_and_index_open", with_cache), []).append(float(m.group(1)))
    out = []
    for (key, with_cache), values in series.items():
        id = f"startup.{key}.{'with_cache' if with_cache else 'no_cache'}"
        out.append(entry(id, "startup", notes, "ms", values, command, **({"private": True} if private else {})))
    return out


def find_pid(script):
    for pid in filter(str.isdigit, os.listdir("/proc")):
        try:
            cmd = open(f"/proc/{pid}/cmdline", "rb").read().split(b"\0")
            if os.path.realpath(f"/proc/{pid}/exe") == os.path.realpath(FOCUS_BIN) and str(script).encode() in cmd:
                return int(pid)
        except OSError:
            continue
    return None


def smaps(pid):
    rollup = {}
    for line in open(f"/proc/{pid}/smaps_rollup"):
        parts = line.split()
        if len(parts) >= 2 and parts[0].endswith(":") and parts[1].isdigit():
            rollup[parts[0][:-1]] = int(parts[1]) / 1024
    gpu = 0.0
    current_gpu = False
    for line in open(f"/proc/{pid}/smaps", errors="replace"):
        if re.match(r"^[0-9a-f]+-[0-9a-f]+ ", line):
            current_gpu = bool(GPU_MAPPING.search(line))
        elif current_gpu and line.startswith("Pss:"):
            gpu += int(line.split()[1]) / 1024
    return rollup, gpu


def wait_mark(log, label, timeout=120):
    end = time.time() + timeout
    while time.time() < end:
        if label in marks(log):
            return True
        time.sleep(0.2)
    return False


def memory(vault, notes, runs, private=False):
    cache = BENCH / "startup-cache" / (vault.name if not private else "private")
    paths = sorted(str(p.relative_to(vault)) for p in vault.rglob("*.md") if ".obsidian" not in p.parts)
    step = max(1, len(paths) // 5)
    tabs = [paths[i * step] for i in range(5)]
    script = BENCH / "memory.focus"
    script.write_text("wait 5000\nmark idle-one-note\n" + "".join(f"open {p}\nwait 400\n" for p in tabs) + "wait 5000\nmark idle-five-tabs\nwait 500\nquit\n")
    series = {}
    command = f"FOCUS_BIN=<focus> FOCUS_PROFILE=1 tools/shot.sh <script: 5 s idle, five tabs, 5 s idle> target/bench/vault-{notes}; /proc/<pid>/smaps_rollup read after each idle"
    for _ in range(runs):
        log = BENCH / "memory.log"
        proc = run_shot(script, vault, cache, log)
        pid = None
        for _ in range(100):
            pid = find_pid(script)
            if pid:
                break
            time.sleep(0.2)
        for state in ("idle-one-note", "idle-five-tabs"):
            if pid and wait_mark(log, state):
                rollup, gpu = smaps(pid)
                key = "one_note" if state == "idle-one-note" else "five_tabs"
                for name, field in (("private_anonymous", "Anonymous"), ("pss", "Pss"), ("rss", "Rss")):
                    series.setdefault((key, name), []).append(rollup.get(field, 0.0))
                series.setdefault((key, "gpu_driver_pss"), []).append(gpu)
        proc.wait()
    return [entry(f"memory.{k}.{n}", "memory", notes, "MiB", v, command, **({"private": True} if private else {})) for (k, n), v in series.items()]


def mcp_first_answer(vault, notes, runs):
    if not MCP_BIN.exists():
        return []
    cache = BENCH / "startup-cache" / "mcp" / vault.name
    request = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "bench", "version": "0"}}}) + "\n"
    values = []
    for i in range(runs + 1):
        t = time.perf_counter()
        p = subprocess.Popen([str(MCP_BIN), "--vault", str(vault), "--cache-dir", str(cache)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        p.stdin.write(request.encode()); p.stdin.flush()
        line = p.stdout.readline()
        elapsed = (time.perf_counter() - t) * 1000
        p.stdin.close(); p.terminate(); p.wait()
        if i > 0 and b"result" in line:
            values.append(elapsed)
    if not values:
        return []
    return [entry("mcp.first_answer", "binaries", notes, "ms", values, f"focus-mcp --vault target/bench/vault-{notes} (initialize request on stdin, time to the first answer line)")]


PENDING = [
    ("latency.key_to_frame.edit_200_lines", "Key press to presented frame, Edit mode, note of 200 lines"),
    ("latency.key_to_frame.edit_10000_lines", "Key press to presented frame, Edit mode, note of 10 000 lines"),
    ("latency.key_to_frame.live_200_lines", "Key press to presented frame, Live mode, note of 200 lines"),
    ("latency.key_to_frame.live_10000_lines", "Key press to presented frame, Live mode, note of 10 000 lines"),
    ("latency.key_to_frame.inplace_200_lines", "Key press to presented frame, Live mode edited in place, note of 200 lines"),
    ("latency.key_to_frame.inplace_10000_lines", "Key press to presented frame, Live mode edited in place, note of 10 000 lines"),
    ("latency.scroll_frame_interval", "Interval between frames during continuous scrolling"),
    ("latency.open_10000_lines", "Opening a note of 10 000 lines"),
    ("idle.note_shown.commits_30s", "Idle, note shown: commits, frame requests, wakeups and processor time over 30 s"),
    ("idle.editor_focused.commits_30s", "Idle, editor focused: commits, frame requests, wakeups and processor time over 30 s"),
    ("latency.scroll_frames_over_two_periods", "Frames longer than two periods of the output during the scroll"),
]


UNSUPPORTED = "not measured: this application binary has no `stamp` and `scroll <pixels> <ms>` script instructions"
FRAME_LOOP = "frame loop not fixed in this binary"


def shot_log(script_text, name, vault, cache, timeout=180):
    script = BENCH / f"{name}.focus"
    script.write_text(script_text)
    log = BENCH / f"{name}.log"
    code = run_shot(script, vault, cache, log, {"WAYLAND_DEBUG": "1", "FOCUS_SHOT_TIMEOUT": str(timeout)}).wait()
    return code, log


def idle_once(vault, cache, seconds, focused):
    script = BENCH / "idle.focus"
    script.write_text(ia.idle_script(seconds, focused))
    log = BENCH / "idle.log"
    proc = run_shot(script, vault, cache, log, {"WAYLAND_DEBUG": "1", "FOCUS_SHOT_TIMEOUT": str(seconds + 60)})
    pid = None
    for _ in range(150):
        pid = find_pid(script)
        if pid:
            break
        time.sleep(0.2)
    if not pid:
        proc.wait()
        raise RuntimeError("the application did not start")
    time.sleep(4)

    def sample():
        fields = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
        switches = next(int(l.split()[1]) for l in open(f"/proc/{pid}/status") if l.startswith("voluntary_ctxt_switches"))
        return int(fields[11]) + int(fields[12]), switches

    line0 = sum(1 for _ in open(log, errors="replace"))
    cpu0, sw0 = sample()
    time.sleep(seconds)
    cpu1, sw1 = sample()
    line1 = sum(1 for _ in open(log, errors="replace"))
    proc.wait()
    text = open(log, errors="replace").read()
    counts = ia.count_between(text, line0, line1)
    ticks = os.sysconf("SC_CLK_TCK")
    return {"commits": counts["commits"], "frame_requests": counts["frame_requests"], "wakeups": sw1 - sw0,
            "cpu_ms": (cpu1 - cpu0) * 1000 / ticks, "hz": ia.output_refresh_hz(text)}


def interaction_suite(quick):
    vault = ia.make_vault(BENCH / "vault-latency")
    cache = BENCH / "startup-cache" / "latency"
    command = "tools/bench/collect.py (WAYLAND_DEBUG=1 FOCUS_BIN=<focus> tools/shot.sh <generated script> target/bench/vault-latency)"
    code, log = shot_log("stamp probe\nwait 200\nscroll 100 100\nwait 300\nquit\n", "probe", vault, cache)
    probe = open(log, errors="replace").read()
    if code != 0 or not ia.stamps(ia.events(probe), "probe"):
        return [{"id": i, "area": "interaction", "title": title, "status": UNSUPPORTED} for i, title in PENDING]
    hz = ia.output_refresh_hz(probe)
    hz_key = round(hz) if hz else None
    out = []
    runs = 1 if quick else 3
    shown = [idle_once(vault, cache, 30, False) for _ in range(runs)]
    focused = [idle_once(vault, cache, 30, True) for _ in range(runs)]
    flag = {} if max(r["commits"] for r in shown) / 30 <= 5 else {"frame_loop": FRAME_LOOP}
    for name, rows, what in (("note_shown", shown, "note shown, nothing focused"), ("editor_focused", focused, "note shown in Edit, editor focused")):
        for key, unit in (("commits", "commits in 30 s"), ("frame_requests", "frame requests in 30 s"), ("wakeups", "voluntary context switches in 30 s"), ("cpu_ms", "ms of processor time in 30 s")):
            out.append(entry(f"idle.{name}.{key}_30s", "interaction", 0, unit, [r[key] for r in rows], command + f" (idle: {what}, 4 s rest, then 30 s counted)", output_hz=hz_key, **flag))
    keys = 30 if quick else 200
    rest_ms = 1500
    timeout = int(keys * rest_ms / 1000 + 120)
    for mode in ("edit", "live", "inplace"):
        for n in (200, 10000):
            code, log = shot_log(ia.key_script(n, mode, keys, rest_ms), f"keys-{mode}-{n}", vault, cache, timeout)
            values = ia.key_latencies(ia.events(open(log, errors="replace").read()))
            id = f"latency.key_to_frame.{mode}_{n}_lines"
            if values:
                out.append(entry(id, "interaction", 0, "ms", values, command + f" (key script: {mode}, {n} lines, {keys} keys, 1.5 s rest before each)", output_hz=hz_key, **flag))
            else:
                out.append({"id": id, "area": "interaction", "title": dict(PENDING)[id], "status": f"failed: script exit {code}, no key timed", "output_hz": hz_key})
    intervals = []
    for _ in range(1 if quick else 3):
        code, log = shot_log(ia.scroll_script(30000, 6000), "scroll", vault, cache)
        intervals += ia.frame_intervals(ia.events(open(log, errors="replace").read()))
    if intervals:
        out.append(entry("latency.scroll_frame_interval", "interaction", 0, "ms", intervals, command + " (scroll 30000 px over 6000 ms of the 10 000 line note)", output_hz=hz_key, **flag))
        if hz:
            out.append(entry("latency.scroll_frames_over_two_periods", "interaction", 0, "frames", [float(sum(1 for i in intervals if i > 2000 / hz))], command + f" (intervals above {2000 / hz:.1f} ms, two periods of the output)", output_hz=hz_key, **flag))
    else:
        out.append({"id": "latency.scroll_frame_interval", "area": "interaction", "title": dict(PENDING)["latency.scroll_frame_interval"], "status": "failed: no frame between the stamps", "output_hz": hz_key})
    values = []
    for _ in range(3 if quick else 10):
        code, log = shot_log(ia.open_script(), "open", vault, cache)
        values += ia.key_latencies(ia.events(open(log, errors="replace").read()), "open")
    if values:
        out.append(entry("latency.open_10000_lines", "interaction", 0, "ms", values, command + " (stamp, open, first presented frame)", output_hz=hz_key, **flag))
    else:
        out.append({"id": "latency.open_10000_lines", "area": "interaction", "title": dict(PENDING)["latency.open_10000_lines"], "status": "failed: no frame after the stamp", "output_hz": hz_key})
    return out


DOMAINS = {"index": ("index", "search"), "graph": ("graph",), "startup": ("startup",), "memory": ("memory",), "binaries": ("binaries",), "interaction": ("interaction",)}


def round_measures(measurements):
    for r in measurements:
        for key in ("min", "median", "p95", "p99", "max"):
            if key in r:
                r[key] = sig(r[key])
    return measurements


def run_domain(domain, quick, runs, sizes):
    out = []
    for notes in sizes if domain != "interaction" else []:
        print(f"== {notes} notes: {domain}", flush=True)
        vault = BENCH / f"vault-{notes}"
        if domain == "index":
            out += rust_suite(notes, "quick" if quick else "full", "bench_suite")
        elif domain == "graph":
            out += graph_suite(notes, runs)
        elif domain == "startup":
            out += startup(vault, notes, runs)
        elif domain == "memory":
            out += memory(vault, notes, 1 if quick else 5)
        elif domain == "binaries":
            out += mcp_first_answer(vault, notes, runs)
    if domain == "binaries":
        out.append(entry("binary.focus.size", "binaries", 0, "MiB", [FOCUS_BIN.stat().st_size / 1048576], "stat -c %s <focus>"))
        if MCP_BIN.exists():
            out.append(entry("binary.focus-mcp.size", "binaries", 0, "MiB", [MCP_BIN.stat().st_size / 1048576], "stat -c %s <focus-mcp>"))
    if domain == "startup":
        private = os.environ.get("FOCUS_REAL_VAULT")
        if private and (Path(private) / ".obsidian").exists():
            vault = Path(private)
            count = sum(1 for p in vault.rglob("*.md") if ".obsidian" not in p.parts)
            out += startup(vault, count, runs, private=True)
            out += memory(vault, count, 1, private=True)
    if domain == "interaction":
        print("== interaction: idle, key latency, scroll, open", flush=True)
        out += interaction_suite(quick)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", choices=["quick", "full"], required=True)
    ap.add_argument("--allow-load", type=int, default=0)
    ap.add_argument("--out", required=True)
    ap.add_argument("--only", choices=sorted(DOMAINS), help="run one domain and merge it into the existing results")
    args = ap.parse_args()
    started = time.time()
    quick = args.mode == "quick"
    sizes = [1000, 10000] if quick else [1000, 10000, 100000]
    runs = 3 if quick else 10
    if not FOCUS_BIN.exists():
        raise SystemExit(f"FOCUS_BIN does not exist: {FOCUS_BIN}")
    BENCH.mkdir(parents=True, exist_ok=True)
    before = load_state()
    domains = [args.only] if args.only else list(DOMAINS)
    results = []
    for domain in domains:
        results += run_domain(domain, quick, runs, sizes)
    results = round_measures(results)
    after = load_state()
    cores = os.cpu_count() or 1
    threshold = 0.25 * cores
    loaded = float(before["loadavg"][0]) > threshold
    out = Path(args.out)
    if args.only and out.exists():
        doc = json.load(open(out))
        areas = {a for d in domains for a in DOMAINS[d]}
        new_hz = {r.get("output_hz") for r in results if r["area"] == "interaction"}
        keep = [r for r in doc["measurements"] if r["area"] not in areas or (r["area"] == "interaction" and r.get("output_hz") not in new_hz and "status" not in r)]
        doc["measurements"] = keep + results
        doc["loaded"] = float(doc["load_start"]["loadavg"][0]) > threshold or any(float(u["load_start"]["loadavg"][0]) > threshold for u in doc.get("updates", [])) or loaded
        doc.setdefault("updates", []).append({"domain": args.only, "mode": args.mode, "date_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                                              "load_start": before, "load_end": after, "duration_s": round(time.time() - started)})
    else:
        doc = {"mode": args.mode, "loaded": loaded, "machine": machine(), "load_start": before, "load_end": after,
               "duration_s": round(time.time() - started), "measurements": results}
    round_measures(doc["measurements"])
    doc["machine"]["load_threshold"] = threshold
    doc["machine"]["output_refresh_hz"] = sorted({r["output_hz"] for r in doc["measurements"] if r.get("output_hz")})
    out.write_text(json.dumps(doc, indent=1) + "\n")
    print(f"done in {round(time.time() - started)} s, loaded: {loaded} (load {before['loadavg'][0]} at the start, threshold {threshold:g} for {cores} logical cores)")


if __name__ == "__main__":
    main()
