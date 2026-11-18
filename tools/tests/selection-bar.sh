#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
bin=${FOCUS_BIN:-$root/target/debug/focus}
[ -x "$bin" ] || { echo "FAIL selection-bar: $bin is not built"; exit 1; }
exec python3 -I "$root/tools/tests/selection-bar.py" "$root" "$bin" "$@"
