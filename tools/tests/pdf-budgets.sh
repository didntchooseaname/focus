#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$root/target/pdf-budgets
bin=${FOCUS_BIN:-$root/target/debug/focus}
rm -rf "${work:?}" && mkdir -p "$work"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
cp -r "$root/fixtures/demo-vault" "$work/vault"
python3 -I "$root/tools/tests/file-viewers.py" "$work/vault/Files" 100
{
    echo "# Twenty"
    for page in $(seq 1 20); do printf '\n![[report.pdf#page=%s|200]]\n' "$page"; done
} >"$work/vault/Twenty.md"
cat >"$work/budgets.focus" <<'SCRIPT'
wait 800
expand Files
right hidden
wait 300
wait 1000
stamp click
press tree:Files/report.pdf
stamp drawn
wait 1000
stamp start
scroll 20000 2000
stamp end
wait 4000
quit
SCRIPT
WAYLAND_DEBUG=1 FOCUS_BIN=$bin FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/budgets.focus" "$work/vault" >"$work/log.txt" 2>&1 &
shot=$!
rss() { [ -n "$1" ] && awk '/^VmRSS/ {print int($2 / 1024) " MiB"}' "/proc/$1/status" || echo "not found"; }
for _ in $(seq 600); do
    grep -q "stamp click" "$work/log.txt" 2>/dev/null && break
    sleep 0.1
done
before=$(rss "$(pgrep -n -f "^$bin( |$).*--script" || true)")
for _ in $(seq 600); do
    grep -q "stamp end" "$work/log.txt" 2>/dev/null && break
    sleep 0.1
done
sleep 1.5
app=$(pgrep -n -f "^$bin( |$).*--script" || true)
worker=$(pgrep -n -f "^$bin --pdf-worker" || true)
echo "resident memory before the PDF opens: application $before"
echo "resident memory after the scroll: application $(rss "$app"), worker $(rss "$worker")"
wait "$shot"
python3 - "$work/log.txt" <<'PY'
import re, sys
def ms(t):
    _, m, s = t.split(":")
    return (int(m) * 60 + float(s)) * 1000
stamps, commits, current = {}, [], None
for line in open(sys.argv[1], errors="replace"):
    m = re.match(r"\[(\d\d:\d\d:\d\d\.\d+)\] (.*)", line)
    if not m:
        continue
    t, rest = ms(m.group(1)), m.group(2)
    s = re.match(r"stamp (\w+)$", rest)
    if s:
        stamps[s.group(1)] = t
    elif re.search(r"-> wl_surface[@#]\d+\.commit\(", rest):
        commits.append(t)
if not {"click", "drawn", "start", "end"} <= stamps.keys():
    sys.exit("FAIL stamps missing from the log")
print(f"press to the pages drawn (upper bound of the first page): {stamps['drawn'] - stamps['click']:.0f} ms")
during = [t for t in commits if stamps["start"] <= t <= stamps["end"]]
span = stamps["end"] - stamps["start"]
gaps = [b - a for a, b in zip(during, during[1:])]
gaps.sort()
p = lambda q: gaps[min(len(gaps) - 1, int(q * len(gaps)))] if gaps else 0
print(f"scroll 20000 px in 2000 ms: {span:.0f} ms between the stamps, {len(during)} commits ({len(during) / span * 1000:.0f} per second), interval median {p(0.5):.1f} ms, 95th percentile {p(0.95):.1f} ms, longest {gaps[-1] if gaps else 0:.1f} ms")
PY

cat >"$work/embeds.focus" <<'SCRIPT'
wait 800
right hidden
wait 1000
stamp note
open Twenty.md
view reading
stamp embeds
wait 3000
quit
SCRIPT
FOCUS_BIN=$bin FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/embeds.focus" "$work/vault" >"$work/embeds.txt" 2>&1 &
shot=$!
for _ in $(seq 600); do
    grep -q "stamp note" "$work/embeds.txt" 2>/dev/null && break
    sleep 0.1
done
before=$(rss "$(pgrep -n -f "^$bin( |$).*--script" || true)")
for _ in $(seq 600); do
    grep -q "stamp embeds" "$work/embeds.txt" 2>/dev/null && break
    sleep 0.1
done
sleep 1
app=$(pgrep -n -f "^$bin( |$).*--script" || true)
workers=$(pgrep -fc -- "^$bin --pdf-worker" || true)
worker=$(pgrep -n -f "^$bin --pdf-worker" || true)
echo "twenty embeds: application $before before the note, $(rss "$app") with it; $workers worker of $(rss "$worker")"
wait "$shot"
python3 - "$work/embeds.txt" <<'PY'
import re, sys
stamps = {}
for line in open(sys.argv[1], errors="replace"):
    m = re.match(r"\[\d\d:(\d\d):(\d\d\.\d+)\] stamp (\w+)$", line.strip())
    if m:
        stamps[m.group(3)] = (int(m.group(1)) * 60 + float(m.group(2))) * 1000
if not {"note", "embeds"} <= stamps.keys():
    sys.exit("FAIL stamps missing from the log")
print(f"note opened to the embeds in view drawn (upper bound): {stamps['embeds'] - stamps['note']:.0f} ms")
PY
