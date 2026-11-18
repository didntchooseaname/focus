#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
vault=$root/target/ops-app-vault
rm -rf "$vault"
cp -r "$root/fixtures/vault" "$vault"
mkdir -p "$vault/Engagements"
cp -r "$root/target/ops-demo/ACME internal 2026" "$vault/Engagements/"
for script in "$@"; do
    "$root/tools/shot.sh" "$here/$script.focus" "$vault"
    FOCUS_CAPTURE=sm "$root/tools/shot.sh" "$here/$script-sm.focus" "$vault"
done
