#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$root/target/idle-frames
mkdir -p "$work"
{ echo "wait 300"; echo "open Rendu/Tout le Markdown.md"; echo "live on"; echo "liveopen 10"; echo "wait 1000"
  for _ in $(seq 50); do echo "wait 2000"; echo "stamp"; echo "type a"; done; echo "quit"; } >"$work/latency.focus"
WAYLAND_DEBUG=1 FOCUS_BIN=${FOCUS_BIN:-$root/target/debug/examples/demo} "$root/tools/shot.sh" "$work/latency.focus" "$root/fixtures/vault" >"$work/latency.log" 2>&1
python3 - "$work/latency.log" <<'PY'
import re, statistics, sys
def ms(t):
    h, m, s = t.split(":")
    return (int(h) * 3600 + int(m) * 60 + float(s)) * 1000
waits, stamp, attached = [], None, False
for line in open(sys.argv[1], errors="replace"):
    m = re.match(r"\[(\d\d:\d\d:\d\d\.\d+)\]\s+(.*)", line)
    if not m:
        continue
    t, rest = ms(m.group(1)), m.group(2)
    if rest == "stamp":
        stamp, attached = t, False
    elif stamp is not None and re.search(r"-> wl_surface[@#]\d+\.attach\(", rest):
        attached = True
    elif stamp is not None and attached and re.search(r"-> wl_surface[@#]\d+\.commit\(", rest):
        waits.append((t - stamp) % 3_600_000)
        stamp = None
q = statistics.quantiles(waits, n=4) if len(waits) > 3 else [0, 0, 0]
print(f"{len(waits)} keys: median {statistics.median(waits):.2f} ms, quartiles {q[0]:.2f} to {q[2]:.2f} ms, max {max(waits):.2f} ms")
PY
