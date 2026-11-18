#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-default-view"
rm -rf "$work" && mkdir -p "$work/config" && cp -r "$root/fixtures/vault" "$work/vault"
cat > "$work/config/settings.json" <<JSON
{"language": "en", "default_view": "source", "last_vault": "$work/vault",
 "ui": {"settings": {"general.default_view": 0},
        "session": {"tabs": ["Rendu/Langues.md"], "views": ["reading"], "active": 0}}}
JSON
cat > "$work/default-view.focus" <<'SCRIPT'
wait 500
shot default-view-restored
open 00 Accueil.md
wait 400
shot default-view-new
quit
SCRIPT
FOCUS_CONFIG_DIR="$work/config" "$root/tools/shot.sh" "$work/default-view.focus" - >"$work/run.log" 2>&1 || true
status=0
check() {
    if python3 -c "import json,sys; s=json.load(open('$work/config/settings.json')); sys.exit(0 if $1 else 1)"; then echo "ok   $2"; else echo "FAIL $2"; status=1; fi
}
check 's["default_view"] == "live" and s["ui"]["settings_version"] == 1' "the default view moved to Live, version written"
check 's["ui"]["session"]["views"][0] == "reading"' "the restored tab kept its Read view"
check 's["ui"]["session"]["views"][1] == "live"' "a note opened afterwards is in Live"
exit $status
