#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
vault="$root/target/test-vault/bookmarks"
rm -rf "$vault" && mkdir -p "$(dirname "$vault")" && cp -r "$root/fixtures/vault" "$vault"
"$root/tools/shot.sh" "$root/tools/shots/bookmarks/bookmarks.focus" "$vault"
