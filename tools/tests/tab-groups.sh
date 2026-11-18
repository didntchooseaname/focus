#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-tab-groups"
rm -rf "$work" && mkdir -p "$work/config"
cp -r "$root/fixtures/demo-vault" "$work/vault"
echo '{"language": "en"}' > "$work/config/settings.json"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
run() {
    local name=$1 vault=${2:-$work/vault}
    cat > "$work/$name.focus"
    FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/$name.focus" "$vault" >"$work/$name.log" 2>&1
}

run split <<'SCRIPT'
wait 300
open Home.md
view source
split
tabs
focus editor
key ctrl-end
type Typed in the second group.
wait 300
shot groups-typed
group 1
focus editor
key ctrl-end
type  Then in the first.
wait 300
shot groups-mirrored
open Method/Evidence handling.md
tabs
movetab 1 1
tabs
divider 0.4
wait 2000
shot groups-moved
quit
SCRIPT
check "split opens the only tab in the second group too" 'grep -qx "tabs: Home || Home" "$work/split.log"'
check "the text typed in either group reaches the file" 'grep -q "Typed in the second group.Then in the first." "$work/vault/Home.md"'
check "it is written once, without a conflict or a copy" '! find "$work/vault" -name "*copy*" | grep -q .'
check "a note opens in the focused group" 'grep -qx "tabs: Home | Evidence handling || Home" "$work/split.log"'
check "a moved tab leaves its group for the other one" 'grep -qx "tabs: Home || Home | Evidence handling" "$work/split.log"'
check "the session keeps the second group and the divider" 'grep -q "\"second\"" "$work/config/settings.json" && grep -q "\"share\": *0.4" "$work/config/settings.json"'

run restored - <<'SCRIPT'
wait 800
tabs
shot groups-restored
group 2
key ctrl-w
key ctrl-w
tabs
quit
SCRIPT
check "a relaunch finds both groups" 'grep -qx "tabs: Home || Home | Evidence handling" "$work/restored.log"'
check "closing the last tab of a group removes the group" 'grep -qx "tabs: Home" "$work/restored.log"'
grep "focus:" "$work"/*.log | grep -v "listening" && status=1
exit $status
