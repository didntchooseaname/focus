#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
name=${1:?usage: qa.sh <name> <vault> [sm]}
vault=${2:?vault}
mode=${3:-1}
out=$root/shots/qa
mkdir -p "$out" "$root/target/qa/logs"
log=$root/target/qa/logs/$name${3:+-$3}.log
prefix=$name${3:+-$3}
script=$root/target/qa/$prefix.focus
sed -E "s/^shot (.+)$/shot $prefix-\1/" "$root/tools/shots/qa/$name.focus" >"$script"
FOCUS_CAPTURE=$mode FOCUS_SHOT_DIR=$out FOCUS_BIN=$root/target/debug/focus \
    "$root/tools/shot.sh" "$script" "$vault" >"$log" 2>&1
echo "$name${3:+ $3}: exit $?"
