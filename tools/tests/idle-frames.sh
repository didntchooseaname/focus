#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
bin=${FOCUS_BIN:-$root/target/debug/focus}
vault=${1:-$root/fixtures/vault}
first=${2:-}
work=$root/target/idle-frames
mkdir -p "$work"
{ [ -n "$first" ] && printf "%b\n" "$first"; echo "wait 16000"; echo "quit"; } >"$work/idle.focus"
WAYLAND_DEBUG=1 FOCUS_BIN=$bin "$root/tools/shot.sh" "$work/idle.focus" "$vault" >"$work/log.txt" 2>&1 &
shot=$!
pid=""
for _ in $(seq 100); do
    sleep 0.2
    pid=$(pgrep -n -f "^$bin( |$)" || true)
    [ -n "$pid" ] && break
done
[ -z "$pid" ] && { echo "FAIL the application did not start"; exit 1; }
sleep 4
pid=$(pgrep -n -f "^$bin( |$)" || true)
sample() { awk '/^voluntary_ctxt_switches/ {print $2}' "/proc/$pid/status"; }
cpu() { awk '{print $14 + $15}' "/proc/$pid/stat"; }
line0=$(wc -l <"$work/log.txt"); switches0=$(sample); cpu0=$(cpu)
sleep 10
line1=$(wc -l <"$work/log.txt"); switches1=$(sample); cpu1=$(cpu)
slice=$(sed -n "$((line0 + 1)),${line1}p" "$work/log.txt")
commits=$(grep -cE -- '-> wl_surface[@#][0-9]+\.commit\(' <<<"$slice")
frames=$(grep -cE -- '-> wl_surface[@#][0-9]+\.frame\(' <<<"$slice")
ticks=$(getconf CLK_TCK)
echo "10 s at rest: $commits wl_surface.commit, $frames wl_surface.frame, $((switches1 - switches0)) voluntary context switches, $(( (cpu1 - cpu0) * 1000 / ticks )) ms CPU"
wait "$shot"
