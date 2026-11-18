#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-palette-enter"
rm -rf "$work" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$work/vault"
if FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$root/tools/tests/palette-enter.focus" "$work/vault" 2>"$work/stderr.txt"; then
    echo "ok   palette-enter"
else
    echo "FAIL palette-enter: $(grep -m1 panicked "$work/stderr.txt" || echo "exit status $?")"
    exit 1
fi
