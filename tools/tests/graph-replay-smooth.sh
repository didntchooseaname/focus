#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
bin=${FOCUS_BIN:-$root/target/debug/examples/demo}
work=$root/target/test-graph-replay
vault=$work/replay-vault
mkdir -p "$work"
python3 -I "$root/tools/tests/graph-vault.py" "$vault" || { echo "FAIL vault"; exit 1; }
cache=$root/target/demo-cache/replay-vault
rm -rf "${cache:?}"
status=0
for speed in 1 4; do
    printf 'measure rest\nmeasure-spread rest\nspeed %s\nanimate\nmeasure-motion replay 60\nmeasure-spread end\nmeasure after\nmeasure-replay rest 3\nquit\n' "$speed" >"$work/replay-$speed.focus"
    FOCUS_BIN=$bin FOCUS_SHOT_TIMEOUT=200 "$root/tools/shot.sh" "$work/replay-$speed.focus" "$vault" >"$work/replay-$speed.log" 2>&1
    python3 -I - "$work/replay-$speed.log" "$speed" <<'PY' || status=1
import re, sys
text = open(sys.argv[1], errors="replace").read()
speed = sys.argv[2]
line = re.search(r"measure-motion replay: (.*)", text)
rest = re.search(r"measure-replay rest: ([\d.]+) frames/s", text)
links = re.search(r"measure-links replay: (.*)", text)
layout = re.search(r"measure-layout replay: (.*)", text)
rhythm = re.search(r"measure-rhythm replay: (.*)", text)
spread = {k: re.search(rf"measure-spread {k}: (.*)", text) for k in ("rest", "end")}
if not all([line, rest, links, layout, rhythm, *spread.values()]):
    sys.exit(f"FAIL speed {speed}: no measure in {sys.argv[1]}")
def numbers(m):
    out = {}
    for part in m.group(1).split("; "):
        words = part.split()
        out[words[0]] = tuple(map(float, words[1:]))
    return out
links, layout, rhythm = numbers(links), numbers(layout), numbers(rhythm)
at_rest, at_end = numbers(spread["rest"]), numbers(spread["end"])
nodes = at_rest["nodes"][0]
births_per_second = nodes * float(speed) / max(30.0, nodes / 100.0)
fields = {}
for part in line.group(1).split("; "):
    words = part.split()
    if len(words) == 4 and words[0].endswith(("_px", "_pct", "_ms", "_scale", "_link")):
        fields[words[0]] = tuple(map(float, words[1:]))
frames = int(re.search(r"frames (\d+) over ([\d.]+) s", line.group(1)).group(1))
seconds = float(re.search(r"frames \d+ over ([\d.]+) s", line.group(1)).group(1))
med, p95, mx = 0, 1, 2
checks = [
    ("placed nodes, 95th percentile, px/frame", fields["placed_px"][p95], 1.5),
    ("placed nodes, largest, px/frame", fields["placed_px"][mx], 12.0),
    ("any node, largest, px/frame", fields["node_px"][mx], 16.0),
    ("change of a node's move, 95th percentile, px", fields["jerk_px"][p95], 1.0),
    ("change of a placed node's move, largest, px", fields["placed_jerk_px"][mx], 10.0),
    ("camera pan, largest, px/frame", fields["pan_px"][mx], 4.0),
    ("camera zoom, largest, %/frame", fields["zoom_pct"][mx], 5.0),
    ("newborn size on its first frame, largest, share of full", fields["birth_scale"][mx], 0.1),
    ("newborn to nearest linked note over the median link drawn, median", fields["birth_gap_link"][med], 1.5),
    ("newborn links not drawn growing from the older note, share", 1 - links["links_grown"][0], 0.0),
    ("time a newborn link stays bright, median, off 1.6 s by", abs(links["bright_s"][0] - 1.6), 0.15),
    ("notes overlapping during the replay, median, points above rest", layout["overlap_pct"][med] - at_rest["overlap_pct"][0], 1.0 if speed == "1" else 4.0),
    ("notes overlapping during the replay, 95th percentile, points above rest", layout["overlap_pct"][p95] - at_rest["overlap_pct"][0], 5.0 if speed == "1" else 9.0),
    ("end of the replay against rest: overlap, points", abs(at_end["overlap_pct"][0] - at_rest["overlap_pct"][0]), 0.01),
    ("end of the replay against rest: link ratio", abs(at_end["link_ratio"][0] - at_rest["link_ratio"][0]), 0.01),
    ("end of the replay against rest: view filled, points", abs(at_end["fill_pct"][0] - at_rest["fill_pct"][0]), 0.1),
    ("births in one frame, largest", rhythm["births_frame_max"][0], -(-births_per_second * fields["interval_ms"][mx] // 1000) + 1),
    ("births in one second, largest", rhythm["births_second_max"][0], births_per_second * 61 / 60 + 2),
    ("times the zoom turned back", rhythm["zoom_turns"][0], 2),
    ("frames missed, share", 1 - frames / seconds * fields["interval_ms"][med] / 1000, 0.05),
    ("frames per second once the replay ended", float(rest.group(1)), 0.5),
]
failed = False
for label, value, limit in checks:
    ok = value <= limit
    failed |= not ok
    print(f"{'ok  ' if ok else 'FAIL'} speed {speed}: {label}: {value:.2f} (at most {limit})")
print(line.group(0))
for m in re.finditer(r"measure-(links|layout|rhythm|spread) .*", text):
    print(m.group(0))
sys.exit(1 if failed else 0)
PY
done
printf 'measure rest\nanimate\nwait 3000\nanimate pause\nwait 6000\nmeasure-replay paused 3\nanimate end\nquit\n' >"$work/replay-pause.focus"
FOCUS_BIN=$bin FOCUS_SHOT_TIMEOUT=200 "$root/tools/shot.sh" "$work/replay-pause.focus" "$vault" >"$work/replay-pause.log" 2>&1
paused=$(grep -oE 'measure-replay paused: [0-9.]+' "$work/replay-pause.log" | grep -oE '[0-9.]+$')
if [ -z "$paused" ]; then
    echo "FAIL pause: no measure in $work/replay-pause.log"
    status=1
elif awk -v f="$paused" 'BEGIN { exit !(f <= 0.5) }'; then
    echo "ok   pause: frames per second while paused, the layout at rest: $paused (at most 0.5)"
else
    echo "FAIL pause: frames per second while paused, the layout at rest: $paused (at most 0.5)"
    status=1
fi
exit $status
