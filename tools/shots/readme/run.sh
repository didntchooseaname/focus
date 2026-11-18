#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
work=$root/target/readme
export FOCUS_SHOT_DIR=${FOCUS_SHOT_DIR:-$root/media}
names=("$@")
[ ${#names[@]} -eq 0 ] && names=($(cd "$here" && ls *.focus | sed 's/\.focus$//'))
for name in "${names[@]}"; do
    rm -rf "$work"
    mkdir -p "$work"
    cp -r "$root/fixtures/demo-vault" "$work/vault"
    active=$(sed -n 's/^# active: //p' "$here/$name.focus" | head -1)
    touch "$work/vault/Engagements/${active:-Northwind internal}/README.md"
    "$root/tools/shot.sh" "$here/$name.focus" "$work/vault"
done
