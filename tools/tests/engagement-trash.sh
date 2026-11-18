#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-engagement-trash"
rm -rf "$work" && mkdir -p "$work/config" && cp -r "$root/fixtures/vault" "$work/vault"
echo '{"language": "en"}' > "$work/config/settings.json"
printf 'ops on\nwait 300\nengagement Demo\nwait 500\ntrash-engagement\nwait 1500\nquit\n' > "$work/run.focus"
FOCUS_CONFIG_DIR="$work/config" FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/run.focus" "$work/vault" >"$work/run.log" 2>&1 || true
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
check "the engagement left Engagements/" '[ ! -e "$work/vault/Engagements/Demo" ]'
check "it is in .trash/ with its README" '[ -n "$(find "$work/vault/.trash" -path "*Demo*" -name README.md 2>/dev/null)" ]'
check "the remembered engagement is forgotten" '! grep -q "ops_engagement" "$work/config/settings.json"'
exit $status
