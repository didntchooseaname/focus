#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
copy=$root/target/ops-vault
rm -rf "$copy"
cp -r "$root/fixtures/vault" "$copy"
export FOCUS_BIN=$root/target/debug/examples/demo-ops
"$root/tools/shot.sh" "$here/ops.focus" "$copy"
"$root/tools/shot.sh" "$here/ops-i18n.focus" "$copy"
FOCUS_OPS_EMPTY=1 "$root/tools/shot.sh" "$here/ops-empty.focus" "$copy"
rm -rf "$copy"
