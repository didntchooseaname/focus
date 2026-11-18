#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-selection-bar-latency"
count=${1:-50}
off=${2:-}
rm -rf "${work:?}" && mkdir -p "$work/vault" "$work/config"
echo '{"language": "en"}' > "$work/config/settings.json"
python3 -I - "$work/vault/Long.md" <<'PY'
import sys
lines = ["# Long note", ""]
for n in range(1, 66):
    lines += [f"Paragraph number {n} with some words to point at.", ""]
open(sys.argv[1], "w").write("\n".join(lines) + "\n")
PY
{
    if [ "$off" = off ]; then echo "set editor.selection_bar off"; echo "set editor.block_handles off"; fi
    echo "open Long.md"; echo "view live"; echo "wait 800"
    for n in $(seq "$count"); do
        a=$(( (n % 2) + 2 ))
        echo "wait 400"; echo "stamp hover"; echo "point Paragraph number $a with|4"
    done
    for n in $(seq "$count"); do
        echo "wait 400"; echo "stamp still"; echo "point Paragraph number 2 with|$(( (n % 20) + 2 ))"
    done
    echo "caret 5:10"; echo "wait 400"
    for n in $(seq "$count"); do echo "wait 400"; echo "stamp key"; echo "type a"; done
    echo "caret 9:0"; echo "wait 400"
    for n in $(seq "$count"); do echo "wait 400"; echo "stamp select"; echo "key shift-right"; done
    echo "wait 1000"; echo "stamp rest"; echo "wait 3000"
    echo "key escape"; echo "key escape"; echo "point Paragraph number 3 with|4"
    echo "wait 1000"; echo "stamp rest"; echo "wait 3000"
    echo "stamp end"; echo "quit"
} > "$work/script.focus"
WAYLAND_DEBUG=1 FOCUS_CONFIG_DIR=$work/config FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data \
    FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/script.focus" "$work/vault" > "$work/log" 2>&1
python3 -I - "$work/log" <<'PY'
import re, statistics, sys
def ms(t):
    h, m, s = t.split(":")
    return (int(h) * 3600 + int(m) * 60 + float(s)) * 1000
stamps, commits = [], []
attached = False
for line in open(sys.argv[1], errors="replace"):
    m = re.match(r"\[(\d\d:\d\d:\d\d\.\d+)\]\s+(.*)", line)
    if not m:
        continue
    t, rest = ms(m.group(1)), m.group(2)
    s = re.match(r"stamp (\w+)", rest)
    if s:
        stamps.append((t, s.group(1)))
    elif re.search(r"-> wl_surface[@#]\d+\.attach\(", rest):
        attached = True
    elif attached and re.search(r"-> wl_surface[@#]\d+\.commit\(", rest):
        attached = False
        commits.append(t)
first, drawn = {}, {}
for i, (t, label) in enumerate(stamps):
    end = stamps[i + 1][0] if i + 1 < len(stamps) else t + 400
    after = [(c - t) % 3_600_000 for c in commits if 0 <= (c - t) % 3_600_000 < (end - t) % 3_600_000]
    drawn[label] = drawn.get(label, 0) + len(after)
    if after:
        first.setdefault(label, []).append(min(after))
for label in ["hover", "still", "key", "select", "rest"]:
    found = first.get(label, [])
    count = sum(1 for _, l in stamps if l == label)
    line = f"{label}: {count} moves or keys, {drawn.get(label, 0)} frames"
    if found:
        q = statistics.quantiles(found, n=4) if len(found) > 3 else [min(found), 0, max(found)]
        line += f"; to the first frame: median {statistics.median(found):.2f} ms, quartiles {q[0]:.2f} to {q[2]:.2f} ms, max {max(found):.2f} ms"
    print(line)
PY
