#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-inplace-park"
rm -rf "${work:?}" && mkdir -p "$work"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }

run() {
    local name=$1
    mkdir -p "$work/$name/vault" "$work/$name/config"
    echo '{"language": "en"}' > "$work/$name/config/settings.json"
    printf '# First\n\n- alpha item\n- beta item\n' > "$work/$name/vault/First.md"
    printf '# Second\n\nText of the second note.\n' > "$work/$name/vault/Second.md"
    cat > "$work/$name/script.focus"
    FOCUS_CONFIG_DIR=$work/$name/config FOCUS_CACHE_DIR=$work/$name/cache FOCUS_DATA_DIR=$work/$name/data \
        FOCUS_SHOT_DIR=$work/$name "$root/tools/shot.sh" "$work/$name/script.focus" "$work/$name/vault" \
        > "$work/$name/log" 2>&1
}
first() { cat "$work/$1/vault/First.md"; }
expected=$'# First\n\n- @alpha item\n- beta item'

run control <<'EOF'
open First.md
view live
wait 500
click alpha item|3
key home
type @
key escape
wait 1500
quit
EOF
check "a tab that never left the front edits in place" '[ "$(first control)" = "$expected" ]'

run parked <<EOF
open First.md
view live
wait 500
openpath $work/parked/vault/Second.md
wait 500
tabs
open First.md
wait 700
tabs
click alpha item|3
key home
type @
key escape
wait 1500
quit
EOF
check "the second note opened in its own tab" 'grep -q "^tabs: First | Second" "$work/parked/log"'
check "a tab back from behind another edits in place" '[ "$(first parked)" = "$expected" ]'
check "the other note is untouched" '[ "$(cat "$work/parked/vault/Second.md")" = "$(printf "# Second\n\nText of the second note.")" ]'
exit $status
