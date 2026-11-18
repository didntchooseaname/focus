#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
here="$root/tools/shots"
vault="$root/target/test-vault/fixtures"
rm -rf "$vault" && mkdir -p "$(dirname "$vault")" && cp -r "$root/fixtures/vault" "$vault"
for name in read-top read-callouts read-code read-table read-images read-mermaid read-mermaid-zoom read-blanc slash find-replace; do
    "$root/tools/shot.sh" "$here/$name.focus" "$vault"
done
FOCUS_CAPTURE=sm "$root/tools/shot.sh" "$here/read-sm.focus" "$vault"
"$root/tools/shot.sh" "$here/read-demo.focus"
