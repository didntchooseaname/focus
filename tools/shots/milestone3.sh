#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
here="$root/tools/shots"
vault="$root/target/test-vault/fixtures"
rm -rf "$vault" && mkdir -p "$(dirname "$vault")" && cp -r "$root/fixtures/vault" "$vault"
first="$root/target/shot-config/first-launch"
rm -rf "$first" && mkdir -p "$first"
FOCUS_CONFIG_DIR=$first "$root/tools/shot.sh" "$here/welcome.focus" -
"$root/tools/shot.sh" "$here/shell-lang.focus"
"$root/tools/shot.sh" "$here/shell-ar.focus" "$vault"
"$root/tools/shot.sh" "$here/settings-lang.focus"
"$root/tools/shot.sh" "$here/palette-ar.focus"
"$root/tools/shot.sh" "$here/slash-anchored.focus" "$vault"
"$root/tools/shot.sh" "$here/ai-integrated.focus"
"$root/tools/shot.sh" "$here/export-dialog.focus"
for name in paste-image find-replace slash-rows slash-chain ops-integrated; do
    rm -rf "$vault" && cp -r "$root/fixtures/vault" "$vault"
    "$root/tools/shot.sh" "$here/$name.focus" "$vault"
done
"$root/tools/shot.sh" "$here/graph-preview.focus"
