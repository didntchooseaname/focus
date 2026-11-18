#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
bin=${FOCUS_BIN:-$root/target/debug/focus}
[ -x "$bin" ] || { echo "FAIL inplace-tables: $bin is not built"; exit 1; }
exec python3 -I "$root/tools/tests/inplace-tables.py" "$root" "$bin" "$@"
