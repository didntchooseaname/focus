#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
here="$root/tools/shots/notices"
vault="$root/target/test-vault/notices"
for name in notices notices-ar; do
    rm -rf "$vault" && mkdir -p "$(dirname "$vault")" && cp -r "$root/fixtures/vault" "$vault"
    "$root/tools/shot.sh" "$here/$name.focus" "$vault"
done
