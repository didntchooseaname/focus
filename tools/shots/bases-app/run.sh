#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
vault="$root/target/test-vault/bases-app"
rm -rf "$vault" && mkdir -p "$(dirname "$vault")" && cp -r "$root/fixtures/vault" "$vault"
cat > "$vault/Suivi.base" <<'BASE'
filters:
  and:
    - 'file.ext == "md"'
views:
  - type: table
    name: "Notes"
    order:
      - file.name
      - note.type
      - note.status
BASE
printf '# Suivi des notes\n\nLa base [[Suivi.base|tableau des notes]] liste les notes du coffre.\n' > "$vault/Suivi des notes.md"
"$root/tools/shot.sh" "$root/tools/shots/bases-app/bases-app.focus" "$vault"
