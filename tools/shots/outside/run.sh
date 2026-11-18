#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
base="$root/target/test-vault/outside"
rm -rf "$base" && mkdir -p "$base/elsewhere" && cp -r "$root/fixtures/vault" "$base/vault"
printf '# Secret\n\nA note out of the vault.\n' > "$base/elsewhere/Secret.md"
ln -s ../../elsewhere/Secret.md "$base/vault/Red Team/Secret.md"
"$root/tools/shot.sh" "$root/tools/shots/outside/outside.focus" "$base/vault"
