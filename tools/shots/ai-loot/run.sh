#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
vault="$root/target/test-vault/ai-loot"
rm -rf "$vault" && mkdir -p "$(dirname "$vault")" && cp -r "$root/fixtures/vault" "$vault"
demo="$vault/Engagements/Demo"
mkdir -p "$demo/Loot"
printf -- '---\ntype: lab\nstatus: active\nscope: []\nout_of_scope: []\nvars: {}\n---\n# Demo\n' > "$demo/README.md"
printf '# Credentials\n\n| User | Type | Secret | Domain | Source | Valid on | Notes |\n|---|---|---|---|---|---|---|\n| svc_sql | password | Pl4inBareSecret | CORP |  |  |  |\n' > "$demo/Loot/Credentials.md"
printf '# SQL server\n\n| Host | User | Value |\n|---|---|---|\n| sql01 | svc_sql | Pl4inBareSecret |\n' > "$demo/SQL server.md"
"$root/tools/shot.sh" "$root/tools/shots/ai-loot/ai-loot.focus" "$vault"
