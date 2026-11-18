#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-ops-code-variables"
rm -rf "$work" && mkdir -p "$work/config" && cp -r "$root/fixtures/vault" "$work/vault"
echo '{"language": "en"}' > "$work/config/settings.json"
cat > "$work/run.focus" <<SCRIPT
ops on
wait 300
engagement Lab
set ops.variables DC=10.10.10.5
open Red Team/Kerberoasting.md
view reading
wait 600
dump $work/on.json
shot code-variables-on
set ops.code_variables false
wait 600
dump $work/off.json
shot code-variables-off
settings ops
wait 400
shot settings-ops
quit
SCRIPT
FOCUS_CONFIG_DIR="$work/config" FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/run.focus" "$work/vault" >"$work/run.log" 2>&1 || true
status=0
code_line() {
    python3 -I -c "import json,sys; texts=[t['text'] for t in json.load(open(sys.argv[1]))['texts']]; print(next((t for t in texts if 'nxc ldap' in t), ''))" "$1" 2>/dev/null || true
}
on=$(code_line "$work/on.json")
off=$(code_line "$work/off.json")
case "$on" in
    *"nxc ldap 10.10.10.5"*) echo "ok   on: the code block shows \$DC resolved" ;;
    *) echo "FAIL on: the code block shows: ${on:0:80}"; status=1 ;;
esac
case "$off" in
    *'nxc ldap $DC'*) echo "ok   off: the code block shows its code as written" ;;
    *) echo "FAIL off: the code block shows: ${off:0:80}"; status=1 ;;
esac
if python3 -I -c "import json,sys; s=json.load(open(sys.argv[1]))['ui']['settings']; sys.exit(0 if s.get('ops.substitute', True) is True else 1)" "$work/config/settings.json"; then
    echo "ok   ops.substitute stays on"
else
    echo "FAIL ops.substitute changed"; status=1
fi
exit $status
