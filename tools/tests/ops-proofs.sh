#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-ops-proofs"
rm -rf "$work" && mkdir -p "$work/config" && cp -r "$root/fixtures/demo-vault" "$work/vault"
printf '{"language": "en", "ops_enabled": true, "ui": {"ops_engagement": "%s"}}\n' "$work/vault/Engagements/Practice exam" > "$work/config/settings.json"
printf 'wait 500\nhost standalone-1\nwait 600\nshot ops-proofs\nquit\n' > "$work/run.focus"
FOCUS_CONFIG_DIR="$work/config" FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/run.focus" "$work/vault" >"$work/run.log" 2>&1 || true
status=0
if grep -q "focus: script" "$work/run.log"; then echo "FAIL $(grep -m1 'focus: script' "$work/run.log")"; status=1; else echo "ok   host standalone-1 selected, its sheet open (target/test-ops-proofs/ops-proofs.png)"; fi
[ -f "$work/ops-proofs.png" ] || { echo "FAIL no capture"; status=1; }
exit $status
