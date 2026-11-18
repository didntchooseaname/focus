#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-inplace-setting"
rm -rf "${work:?}" && mkdir -p "$work"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }

base=$'# Réglage\n\n- premier élément\n- deuxième élément\n\nUn paragraphe simple.\n'
OPEN=$'open Inplace.md\nview live\nwait 500'
PALETTE=$'key ctrl-shift-p\nwait 200\ntype Open block source\nwait 300\nkey enter\nwait 300'
END=$'key escape\nwait 1500\nquit'

run() {
    local name=$1
    mkdir -p "$work/$name"
    cp -r "$root/fixtures/demo-vault" "$work/$name/vault"
    printf '%s' "$base" > "$work/$name/vault/Inplace.md"
    cat > "$work/$name/script.focus"
    FOCUS_SHOT_DIR=$work/$name "$root/tools/shot.sh" "$work/$name/script.focus" "$work/$name/vault" \
        > "$work/$name/log" 2>&1
}
is() {
    local name=$1 text=$base
    shift
    while [ $# -gt 0 ]; do text=${text/"$1"/"$2"}; shift 2; done
    printf '%s' "$text" > "$work/$name/expected"
    cmp -s "$work/$name/expected" "$work/$name/vault/Inplace.md"
}

run off <<SCRIPT
set editor.edit_in_place off
$OPEN
click premier
wait 300
key home
type @
$END
SCRIPT
check "off: a click opens the source of the list, Home goes before the marker" \
    'is off "- premier" "@- premier"'

run on-home <<SCRIPT
$OPEN
click premier
wait 300
key home
type @
$END
SCRIPT
check "on (the default): the caret stays in the item, Home goes before its text" \
    'is on-home "- premier" "- @premier"'

run on-type <<SCRIPT
$OPEN
click premier
wait 300
type @
$END
SCRIPT
run on-key <<SCRIPT
$OPEN
click premier
wait 300
key ctrl-shift-m
wait 300
type @
$END
SCRIPT
check "Ctrl Shift M keeps the caret at the same byte" \
    '! is on-type && cmp -s "$work/on-type/vault/Inplace.md" "$work/on-key/vault/Inplace.md"'

run on-key-home <<SCRIPT
$OPEN
click premier
wait 300
key ctrl-shift-m
wait 300
key home
type @
$END
SCRIPT
check "Ctrl Shift M opens the source of the block: Home goes before the marker" \
    'is on-key-home "- premier" "@- premier"'

run on-command <<SCRIPT
$OPEN
click premier
wait 300
$PALETTE
type @
$END
SCRIPT
check "the palette command keeps the caret at the same byte" \
    '! is on-type && cmp -s "$work/on-type/vault/Inplace.md" "$work/on-command/vault/Inplace.md"'

run on-command-home <<SCRIPT
$OPEN
click premier
wait 300
$PALETTE
key home
type @
$END
SCRIPT
check "the palette command opens the source of the block" \
    'is on-command-home "- premier" "@- premier"'

run off-then-on <<SCRIPT
set editor.edit_in_place off
$OPEN
click premier
wait 300
key home
type @
key escape
wait 300
set editor.edit_in_place on
wait 300
click deuxième
wait 300
key home
type =
$END
SCRIPT
check "turned on, the open tab edits in place at once" \
    'is off-then-on "- premier" "@- premier" "- deuxième" "- =deuxième"'

run on-then-off <<SCRIPT
$OPEN
click premier
wait 300
key home
type @
key escape
wait 300
set editor.edit_in_place off
wait 300
click deuxième
wait 300
key home
type =
$END
SCRIPT
check "turned off, the open tab opens sources again at once" \
    'is on-then-off "- premier" "- @premier" "- deuxième" "=- deuxième"'

run second-group <<SCRIPT
set editor.edit_in_place off
$OPEN
split
wait 300
group 2
tabs
set editor.edit_in_place on
wait 300
click premier
wait 300
key home
type @
$END
SCRIPT
check "turned on, a tab of the second group edits in place at once" \
    'grep -qx "tabs: Réglage || Réglage" "$work/second-group/log" && is second-group "- premier" "- @premier"'

for log in "$work"/*/log; do
    grep -h -E "^(focus:|Error:)" "$log" && status=1
done
exit $status
