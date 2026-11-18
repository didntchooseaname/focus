#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-code-variables"
rm -rf "$work" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$work/vault"
note="Red Team/Kerberoasting.md"
cat > "$work/run.focus" <<SCRIPT
ops on
wait 300
open $note
view reading
wait 500
variables 17
wait 300
type DOMAIN
wait 300
shot code-variables
key enter
wait 1500
quit
SCRIPT
FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/run.focus" "$work/vault" >"$work/run.log" 2>&1 || true
if sed -n 18p "$work/vault/$note" | grep -q 'kerberoast.txt \$DOMAIN$'; then echo "ok   \$DOMAIN went to the end of the first line of code"; else echo "FAIL line 18: $(sed -n 18p "$work/vault/$note")"; exit 1; fi
