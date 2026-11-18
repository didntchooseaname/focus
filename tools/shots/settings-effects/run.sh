#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
here="$root/tools/shots/settings-effects"
vault="$root/target/test-vault/settings-effects"
for name in editor notes shortcuts find-info rebuild ops; do
    rm -rf "$vault" && mkdir -p "$(dirname "$vault")" && cp -r "$root/fixtures/vault" "$vault"
    "$root/tools/shot.sh" "$here/$name.focus" "$vault"
done
"$root/tools/shot.sh" "$here/search.focus"
