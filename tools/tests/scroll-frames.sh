#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$root/target/scroll-frames
rm -rf "$work" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$work/vault"
printf 'open Rendu/Tout le Markdown.md\nview live\nwait 800\nstamp start\nscroll 6000 2000\nstamp end\nquit\n' >"$work/scroll.focus"
WAYLAND_DEBUG=1 "$root/tools/shot.sh" "$work/scroll.focus" "$work/vault" >"$work/scroll.log" 2>&1
python3 - "$work/scroll.log" <<'PY'
import re, sys
def ms(t):
    h, m, s = t.split(":")
    return (int(h) * 3600 + int(m) * 60 + float(s)) * 1000
start = end = None
commits = 0
for line in open(sys.argv[1], errors="replace"):
    m = re.match(r"\[(\d\d:\d\d:\d\d\.\d+)\] (.*)", line)
    if not m:
        continue
    t, rest = ms(m.group(1)), m.group(2)
    if rest == "stamp start":
        start = t
    elif rest == "stamp end":
        end = t
    elif start is not None and end is None and re.search(r"-> wl_surface[@#]\d+\.commit\(", rest):
        commits += 1
if start is None or end is None:
    sys.exit("FAIL no stamps in the log")
span = (end - start) % 86_400_000
print(f"scroll 6000 2000: {span:.0f} ms between the stamps, {commits} commits ({commits / span * 1000:.0f} per second)")
sys.exit(0 if abs(span - 2000) <= 20 else 1)
PY
