#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
name=${1:?usage: qa2.sh <name> [sm]}
mode=${2:-1}
work=$root/target/qa2/$name${2:+-$2}
prefix=$name${2:+-$2}
rm -rf "$work"; mkdir -p "$work" "$root/shots/qa2"
base=${QA2_BASE:-base}
cp -r "$root/target/qa2/$base" "$work/vault"
sed -E "s/^shot (.+)$/shot $prefix-\\1/" "$root/tools/shots/qa2/$name.focus" >"$work/script.focus"
FOCUS_CAPTURE=$mode FOCUS_SHOT_DIR=$root/shots/qa2 FOCUS_BIN=$root/target/debug/focus \
    "$root/tools/shot.sh" "$work/script.focus" "$work/vault" >"$work/log.txt" 2>&1
code=$?
{
    echo "exit $code"
    [ -s "$work/log.txt" ] && { echo "--- log"; head -20 "$work/log.txt"; }
    echo "--- files changed"
    diff -rq "$root/target/qa2/$base" "$work/vault" | sed "s#$root/target/qa2/##"
    echo "--- content diff"
    diff -ru "$root/target/qa2/$base" "$work/vault" -x '.trash' | sed "s#$root/target/qa2/##" | grep -vE '^(\+\+\+|---|@@|[ ])' | grep -E '^(diff|[+-])' | head -80
    [ -d "$work/vault/.trash" ] && { echo "--- .trash"; (cd "$work/vault/.trash" && find . -type f | sort); }
} >"$work/report.txt"
cat "$work/report.txt"
