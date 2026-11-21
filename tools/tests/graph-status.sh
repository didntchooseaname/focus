#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-graph-status"
rm -rf "${work:?}" && mkdir -p "$work"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }

cp -r "$root/fixtures/demo-vault" "$work/vault"
mkdir -p "$work/config" && echo '{"language": "en"}' > "$work/config/settings.json"
cat > "$work/graph.focus" <<'SCRIPT'
wait 800
open Home.md
wait 300
status
graph
wait 4000
status
shot graph
open Home.md
wait 300
status
graph
wait 500
lang fr
wait 300
status
quit
SCRIPT
FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work/shots "$root/tools/shot.sh" "$work/graph.focus" "$work/vault" >"$work/graph.log" 2>&1

statuses() { grep '^status: ' "$work/graph.log"; }
check "a note in front shows no graph counter" '! statuses | sed -n 1p | grep -Eq "links?$"'
check "the graph in front puts its counts in the status bar" \
    'statuses | sed -n 2p | grep -Eq "^status: [0-9,]+ notes?, [0-9,]+ links?$"'
check "a note back in front takes the counter away" '! statuses | sed -n 3p | grep -Eq "links?$"'
check "the counter follows the language" \
    'statuses | sed -n 4p | grep -Eq "^status: [^,]+ notes?, [^,]+ liens?$"'
check "the capture was taken" '[ -s "$work/shots/graph.png" ]'
check "the script ran without an error" '! grep -q "script error\|panicked\|focus: script" "$work/graph.log"'
exit $status
