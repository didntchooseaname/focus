#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-ops-dialog"
rm -rf "$work" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$work/vault"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }

cat > "$work/plain.focus" <<'SCRIPT'
ops on
wait 300
action ops.new-engagement
wait 300
type Lab saisie
wait 300
shot ops-dialog-typed
key enter
wait 800
quit
SCRIPT
FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/plain.focus" "$work/vault" >"$work/plain.log" 2>&1 || true
check "typed into the name field, Enter creates the engagement" '[ -f "$work/vault/Engagements/Lab saisie/README.md" ]'

rm -rf "$work/vault" && cp -r "$root/fixtures/vault" "$work/vault"
cat > "$work/live.focus" <<'SCRIPT'
ops on
wait 300
open Rendu/Tout le Markdown.md
view live
wait 300
block 10
wait 300
action ops.new-engagement
wait 300
type Lab live
key enter
wait 800
quit
SCRIPT
FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/live.focus" "$work/vault" >"$work/live.log" 2>&1 || true
check "with a Live block open, the keys go to the dialog" '[ -f "$work/vault/Engagements/Lab live/README.md" ]'
check "and the note does not get them" 'cmp -s "$root/fixtures/vault/Rendu/Tout le Markdown.md" "$work/vault/Rendu/Tout le Markdown.md"'
exit $status
