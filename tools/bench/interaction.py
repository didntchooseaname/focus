import re, statistics, sys, time
from pathlib import Path

LINE = re.compile(r"\[(\d\d):(\d\d):(\d\d\.\d+)\]\s+(.*)")
ATTACH = re.compile(r"-> wl_surface[@#]\d+\.attach\(")
COMMIT = re.compile(r"-> wl_surface[@#]\d+\.commit\(")
FRAME = re.compile(r"-> wl_surface[@#]\d+\.frame\(")
MODE = re.compile(r"wl_output[@#]\d+\.mode\((\d+), (\d+), (\d+), (\d+)\)")
HOUR = 3_600_000.0


def events(text):
    out = []
    for line in text.splitlines():
        m = LINE.match(line)
        if m:
            out.append(((int(m.group(1)) * 3600 + int(m.group(2)) * 60 + float(m.group(3))) * 1000, m.group(4)))
    offsets = []
    for i, (t, rest) in enumerate(out):
        if rest.startswith("stamp") and i > 0:
            previous = next((o for o in reversed(out[:i]) if not o[1].startswith("stamp")), None)
            if previous:
                offsets.append(round((t - previous[0]) / HOUR))
    if offsets:
        shift = statistics.mode(offsets) * HOUR
        out = [(t - shift if rest.startswith("stamp") else t, rest) for t, rest in out]
    return out


def stamps(evs, label=None):
    return [(t, r[5:].strip()) for t, r in evs if r.startswith("stamp") and (label is None or r[5:].strip() == label)]


def presented(evs):
    out, attached = [], False
    for t, rest in evs:
        if ATTACH.search(rest):
            attached = True
        elif COMMIT.search(rest):
            if attached:
                out.append(t)
            attached = False
    return out


def key_latencies(evs, label="key"):
    frames = presented(evs)
    out = []
    marks = [t for t, _ in stamps(evs, label)]
    for i, t in enumerate(marks):
        end = marks[i + 1] if i + 1 < len(marks) else float("inf")
        nxt = [f for f in frames if f >= t - 0.001 and f < end]
        if nxt:
            out.append((nxt[0] - t) % HOUR if nxt[0] < t else nxt[0] - t)
    return out


def frame_intervals(evs, start="scroll-start", end="scroll-end"):
    s, e = stamps(evs, start), stamps(evs, end)
    if not s or not e:
        return []
    frames = [f for f in presented(evs) if s[0][0] <= f <= e[0][0]]
    return [b - a for a, b in zip(frames, frames[1:])]


ENTER = re.compile(r"wl_surface[@#]\d+\.enter\(wl_output[@#](\d+)\)")


def output_refresh_hz(text):
    entered = ENTER.findall(text)
    if not entered:
        return None
    wanted = entered[-1]
    rate = None
    for m in re.finditer(r"wl_output[@#](\d+)\.mode\((\d+), (\d+), (\d+), (\d+)\)", text):
        if m.group(1) == wanted and int(m.group(2)) & 1:
            rate = int(m.group(5)) / 1000
    return rate


def count_between(text, t0_line, t1_line):
    lines = text.splitlines()[t0_line:t1_line]
    return {"commits": sum(1 for l in lines if COMMIT.search(l)), "frame_requests": sum(1 for l in lines if FRAME.search(l))}


def note_text(lines):
    out = ["---", "title: Latency note", "tags:", "  - bench", "---", ""]
    i = 0
    while len(out) < lines:
        i += 1
        kind = i % 6
        if kind == 1:
            out += [f"## Section {i}", ""]
        elif kind == 2:
            out += [f"Paragraph {i}: the quick brown fox jumps over the lazy dog while [[Link {i}]] points elsewhere and `code {i}` stays inline.", ""]
        elif kind == 3:
            out += [f"- item {i} **bold** and *italic*", f"- item {i + 1} with a [link](https://example.test/{i})", ""]
        elif kind == 4:
            out += ["| Name | Value |", "|---|---|", f"| row {i} | {i * 7} |", ""]
        elif kind == 5:
            out += ["```bash", f"echo line {i}", "```", ""]
        else:
            out += [f"- [ ] task {i}", f"- [x] task {i + 1}", ""]
    return "\n".join(out[:lines]) + "\n"


def make_vault(root):
    root = Path(root)
    (root / ".obsidian").mkdir(parents=True, exist_ok=True)
    (root / ".obsidian/app.json").write_text("{}")
    (root / "Latency").mkdir(exist_ok=True)
    for n in (200, 10000):
        (root / "Latency" / f"{n} lines.md").write_text(note_text(n))
    return root


def key_script(note, mode, keys, rest_ms):
    open_block = {
        "edit": "view source\nfocus editor\n",
        "live": "view live\nblock 9\n",
        "inplace": "view live\nwait 500\ncaret 9:20\n",
    }[mode]
    body = "".join(f"wait {rest_ms}\nstamp key\ntype a\n" for _ in range(keys))
    return f"open Latency/{note} lines.md\nwait 800\n{open_block}wait 1500\n{body}wait 500\nquit\n"


def scroll_script(pixels, duration_ms):
    return f"open Latency/10000 lines.md\nview source\nwait 1500\nstamp scroll-start\nscroll {pixels} {duration_ms}\nstamp scroll-end\nwait 800\nquit\n"


def open_script():
    return "wait 1500\nstamp open\nopen Latency/10000 lines.md\nwait 1500\nquit\n"


def idle_script(seconds, focused):
    focus = "view source\nfocus editor\n" if focused else ""
    return f"open Latency/200 lines.md\n{focus}wait {int((seconds + 8) * 1000)}\nquit\n"


def summarize(values):
    values = sorted(values)
    pick = lambda p: values[min(len(values) - 1, round((len(values) - 1) * p))]
    return {"min": values[0], "median": statistics.median(values), "p95": pick(0.95), "p99": pick(0.99), "max": values[-1]}


def selftest():
    def stream(rows):
        return "\n".join(f"[00:00:{t:06.3f}] {r}" for t, r in rows)
    rows = [(1.000, "stamp key"), (1.003, "-> wl_surface#22.attach(wl_buffer#9, 0, 0)"), (1.0032, "-> wl_surface#22.commit()"),
            (3.000, "stamp key"), (3.002, "-> wl_surface#22.commit()"), (3.012, "-> wl_surface#22.attach(wl_buffer#9, 0, 0)"), (3.0121, "-> wl_surface#22.commit()")]
    evs = events(stream(rows))
    got = key_latencies(evs)
    assert len(got) == 2 and abs(got[0] - 3.0) < 0.01 and abs(got[1] - 12.0) < 0.01, got
    rows = [(1.0, "stamp scroll-start")] + [(1.0 + k * 0.0167, x) for k in range(1, 30) for x in ("-> wl_surface#22.attach(wl_buffer#9, 0, 0)", "-> wl_surface#22.commit()")] + [(2.0, "stamp scroll-end")]
    iv = frame_intervals(events(stream(rows)))
    assert len(iv) == 28 and all(abs(i - 16.7) < 1.1 for i in iv), iv[:3]
    stream_out = "wl_output#5.mode(3, 1920, 1080, 143981)\nwl_output#6.mode(3, 1920, 1080, 60000)\n-> wl_surface#22.enter(wl_output#5)"
    assert output_refresh_hz(stream_out) == 143.981 and output_refresh_hz("wl_output#5.mode(3, 1, 1, 60000)") is None
    tz = stream([(10.0, "-> wl_surface#22.attach(wl_buffer#9, 0, 0)")]) + "\n[01:00:20.000] stamp key\n[00:00:20.003] -> wl_surface#22.attach(wl_buffer#9, 0, 0)\n[00:00:20.0035] -> wl_surface#22.commit()"
    assert abs(key_latencies(events(tz))[0] - 3.5) < 0.01, key_latencies(events(tz))
    text = "\n".join(["-> wl_surface#22.frame(new id wl_callback#68)", "-> wl_surface#22.commit()", "x", "-> wl_surface#22.commit()"])
    assert count_between(text, 0, 4) == {"commits": 2, "frame_requests": 1}
    note = note_text(10000)
    assert note.count("\n") == 10000 and note_text(200).count("\n") == 200 and note == note_text(10000)
    print("selftest ok")


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest()
