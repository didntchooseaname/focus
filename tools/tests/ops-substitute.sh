#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-ops-substitute"
rm -rf "$work" && mkdir -p "$work/config" && cp -r "$root/fixtures/vault" "$work/vault"
echo '{"language": "en"}' > "$work/config/settings.json"
cat > "$work/run.focus" <<'SCRIPT'
ops on
wait 300
engagement Lab
set ops.variables DC=10.10.10.5
open Red Team/Kerberoasting.md
view reading
wait 600
shot substitute-on
substitute off
wait 600
shot substitute-off
quit
SCRIPT
FOCUS_CONFIG_DIR="$work/config" FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/run.focus" "$work/vault" >"$work/run.log" 2>&1 || true
status=0
if python3 -c "import json,sys; s=json.load(open('$work/config/settings.json')); sys.exit(0 if s['ui']['settings']['ops.substitute'] is False else 1)"; then
    echo "ok   unchecking the box writes ops.substitute"
else
    echo "FAIL ops.substitute was not written"; status=1
fi
if [ -f "$work/substitute-on.png" ] && [ -f "$work/substitute-off.png" ] && ! cmp -s "$work/substitute-on.png" "$work/substitute-off.png"; then
    echo "ok   the code blocks change with the box (substitute-on.png, substitute-off.png)"
else
    echo "FAIL the note did not change with the box"; status=1
fi
exit $status
