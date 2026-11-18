#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
copy=$root/target/ai-vault
rm -rf "$copy" "$root/target/ai-demo-history"
mkdir -p "$root/target"
cp -r "$root/fixtures/vault" "$copy"
export FOCUS_BIN=$root/target/debug/examples/demo
"$root/tools/shot.sh" "$here/ai.focus" "$copy"
rm -rf "$copy" "$root/target/ai-demo-history"
cp -r "$root/fixtures/vault" "$copy"
"$root/tools/shot.sh" "$here/ai-i18n.focus" "$copy"
rm -rf "$copy" "$root/target/ai-demo-history"
cp -r "$root/fixtures/vault" "$copy"
"$root/tools/shot.sh" "$here/ai-more.focus" "$copy"
