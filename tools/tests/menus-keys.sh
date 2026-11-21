#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-menus-keys"
rm -rf "${work:?}" && mkdir -p "$work/vault/Method" "$work/vault/Archive" "$work/config"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
echo '{"language": "en"}' > "$work/config/settings.json"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
printf '# Alpha\n\nFirst note.\n' > "$work/vault/Method/Alpha.md"
printf '# Beta\n\nSecond note.\n' > "$work/vault/Method/Beta.md"
printf '# Old\n\nArchived.\n' > "$work/vault/Archive/Old.md"
printf '# Tagged\n\nA #demo note.\n' > "$work/vault/Tagged.md"
cat > "$work/menus.focus" <<'SCRIPT'
wait 500
left files
expand Method
wait 300
focus tree
hover tree:Method/Alpha.md
key ctrl-m
wait 200
key down
key down
key escape
wait 200
key escape
wait 200
key enter
wait 500
focus tree
hover tree:Method/Alpha.md
key ctrl-m
wait 200
key down
key enter
wait 800
focus tree
hover tree:Method/Beta.md
key shift-f10
wait 200
keymenu
key down
keymenu
key end
keymenu
key home
keymenu
key escape
wait 200
keymenu
hover tree:Method/Beta.md
key shift-f10
wait 200
key down
key down
key down
key down
key down
keymenu
key enter
wait 300
type Gamma
key enter
wait 800
open Tagged.md
wait 300
crumb 0
wait 200
keymenu
key end
keymenu
key escape
wait 200
keymenu
rmouse 330 57
wait 300
keymenu
key down
keymenu
key escape
wait 200
keymenu
rmouse 135 97
wait 300
keymenu
key end
keymenu
key enter
wait 200
keymenu
left tags
wait 400
rpress tag:demo
wait 200
keymenu
key down
keymenu
key escape
wait 200
keymenu
open Tagged.md
view source
focus editor
key ctrl-end
key enter
type #de
wait 300
menu
key enter
wait 300
menu
key ctrl-s
wait 500
quit
SCRIPT
FOCUS_SHOT_TIMEOUT=200 FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/menus.focus" "$work/vault" >"$work/menus.log" 2>&1
menu() { grep '^keymenu: ' "$work/menus.log" | sed -n "$1p"; }
completion() { grep '^menu: ' "$work/menus.log" | sed -n "$1p"; }

check "Escape closes the folder picker and a later Enter moves nothing" '[ ! -f "$work/vault/Alpha.md" ]'
check "Down in the folder picker moves its highlight, Enter moves into that folder" '[ -f "$work/vault/Archive/Alpha.md" ] && [ ! -f "$work/vault/Alpha.md" ]'
check "Shift F10 opens the tree menu with the focus and no row highlighted" '[ "$(menu 1)" = "keymenu: tree-menu | none" ]'
check "Down highlights the first row" '[ "$(menu 2)" = "keymenu: tree-menu | menu-open-tab" ]'
check "End highlights the last row" '[ "$(menu 3)" = "keymenu: tree-menu | menu-trash" ]'
check "Home highlights the first row" '[ "$(menu 4)" = "keymenu: tree-menu | menu-open-tab" ]'
check "Escape closes the tree menu" '[ "$(menu 5)" = "keymenu: none" ]'
check "Down skips the separators" '[ "$(menu 6)" = "keymenu: tree-menu | menu-rename" ]'
check "Enter runs the highlighted row (rename)" '[ -f "$work/vault/Method/Gamma.md" ] && [ ! -f "$work/vault/Method/Beta.md" ]'
check "the breadcrumb menu takes the keyboard" '[ "$(menu 7)" = "keymenu: crumb-menu | none" ] && [ "$(menu 8)" = "keymenu: crumb-menu | crumb-reveal" ] && [ "$(menu 9)" = "keymenu: none" ]'
check "a right press on a tab opens its menu with the focus" '[ "$(menu 10)" = "keymenu: tab-menu | none" ] && [ "$(menu 11)" = "keymenu: tab-menu | tab-menu-split" ] && [ "$(menu 12)" = "keymenu: none" ]'
check "a right press in a text field opens the Focus field menu" '[ "$(menu 13)" = "keymenu: field-menu | none" ]'
check "End reaches the last row, Enter runs it" '[ "$(menu 14)" = "keymenu: field-menu | field-menu-select-all" ] && [ "$(menu 15)" = "keymenu: none" ]'
check "the tag menu takes the keyboard" '[ "$(menu 16)" = "keymenu: tag-menu | none" ] && [ "$(menu 17)" = "keymenu: tag-menu | tag-menu-rename" ] && [ "$(menu 18)" = "keymenu: none" ]'
check "typing a tag opens its completion" 'completion 1 | grep -q "^menu: tags | de | tag-demo | shown$"'
check "Enter writes the tag and the completion does not come back" '[ "$(completion 2)" = "menu: none" ] && tail -n 1 "$work/vault/Tagged.md" | grep -qx "#demo"'
check "no step of the script failed" '! grep -q "^focus: script\|^Error" "$work/menus.log"'
exit $status
