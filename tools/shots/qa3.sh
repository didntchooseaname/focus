#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
name=${1:?usage: qa3.sh <name> [sm]}
mode=${2:-1}
prefix=$name${2:+-$2}
work=$root/target/qa3/$prefix
rm -rf "$work"; mkdir -p "$work" "$root/shots/qa3"
cp -r "$root/target/qa3/base" "$work/vault"
sed -E "s/^shot (.+)$/shot $prefix-\\1/" "$root/tools/shots/qa3/$name.focus" >"$work/script.focus"
hook=$root/tools/shots/qa3/$name.hook
[ -f "$hook" ] && (bash "$hook" "$work/vault" >/dev/null 2>&1 &)
FOCUS_CAPTURE=$mode FOCUS_SHOT_DIR=$root/shots/qa3 FOCUS_BIN=$root/target/debug/focus \
    "$root/tools/shot.sh" "$work/script.focus" "$work/vault" >"$work/log.txt" 2>&1
code=$?
{
    echo "exit $code"
    [ -s "$work/log.txt" ] && { echo "--- log"; head -20 "$work/log.txt"; }
    echo "--- bytes changed"
    python3 "$root/tools/shots/qa3-bytediff.py" "$root/target/qa3/base" "$work/vault"
} >"$work/report.txt"
cat "$work/report.txt"
