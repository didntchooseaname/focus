#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-tabs-pinned"
rm -rf "${work:?}" && mkdir -p "$work/vault" "$work/config"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
echo '{"language": "en"}' > "$work/config/settings.json"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
for n in a b c d e f; do printf '# %s\n\nSee [[f]].\n' "$n" > "$work/vault/$n.md"; done
cp "$root/fixtures/demo-vault/assets/scope-map.png" "$work/vault/scope-map.png"
run() {
    FOCUS_SHOT_TIMEOUT=240 FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work \
        "$root/tools/shot.sh" "$work/$1.focus" "$2" >"$work/$1.log" 2>&1
}
cat > "$work/first.focus" <<SCRIPT
wait 800
open a.md
open b.md
open c.md
open d.md
openpath $work/vault/scope-map.png
wait 400
tabbar
pintab 2
tabbar
pintab 4
tabbar
pintab 0
tabbar
droptab 2 4
tabbar
droptab 2 0
tabbar
key ctrl-1
status
key ctrl-w
tabbar
key ctrl-tab
front
key ctrl-pagedown
front
key ctrl-9
front
tabclose 0
tabbar
pintab 0
key ctrl-1
view reading
wait 300
click f|0
wait 400
front
tabs
key ctrl-1
key ctrl-p
type e
wait 300
key enter
wait 400
front
tabs
key ctrl-1
key ctrl-w
key ctrl-shift-t
wait 300
tabbar
split
wait 300
tabbar
group 2
pintab 0
group 1
movetab 1 0
wait 300
group 1
pintab 0
tabbar
group 2
tabmenu 0
wait 200
key down
keymenu
key enter
wait 300
tabbar
group 1
tabmenu 1
wait 200
key down
key down
keymenu
key enter
wait 300
tabbar
wait 600
quit
SCRIPT
run first "$work/vault"
cat > "$work/second.focus" <<'SCRIPT'
wait 1200
tabs
group 1
key ctrl-1
key ctrl-shift-e
wait 200
key f2
type renamed
key enter
wait 600
tabs
quit
SCRIPT
run second -
cat > "$work/third.focus" <<'SCRIPT'
wait 1200
tabs
quit
SCRIPT
run third -
line() { grep -E '^(tabs|tabbar|status|front): ' "$work/first.log" | sed -n "$1p"; }
image="scope-map.png"

check "five tabs in the order they were opened" '[ "$(line 1)" = "tabbar: a | b | c | d | $image" ]'
check "pinning moves the tab to the start" '[ "$(line 2)" = "tabbar: [p] c | a | b | d | $image" ]'
check "a second pin lands after the first, a file tab too" '[ "$(line 3)" = "tabbar: [p] c | [p] $image | a | b | d" ]'
check "unpinning moves the tab to the first place after the pinned ones" '[ "$(line 4)" = "tabbar: [p] $image | c | a | b | d" ]'
check "a drag reorders the tabs that are not pinned" '[ "$(line 5)" = "tabbar: [p] $image | c | b | d | a" ]'
check "a drag never crosses the pinned ones" '[ "$(line 6)" = "tabbar: [p] $image | c | b | d | a" ]'
check "Ctrl 1 goes to the first tab drawn, the pinned image" 'line 7 | grep -q "^status: $image"'
check "Ctrl W leaves a pinned tab open" '[ "$(line 8)" = "tabbar: [p] $image | c | b | d | a" ]'
check "Ctrl Tab follows the order drawn" '[ "$(line 9)" = "front: c.md Live line 1" ]'
check "Ctrl PageDown too" '[ "$(line 10)" = "front: b.md Live line 1" ]'
check "Ctrl 9 goes to the last tab drawn" '[ "$(line 11)" = "front: a.md Live line 1" ]'
check "Close tab of its menu closes a pinned tab" '[ "$(line 12)" = "tabbar: c | b | d | a" ]'
check "a link followed from a pinned tab opens in another tab" '[ "$(line 13)" = "front: f.md Live line 1" ] && [ "$(line 14)" = "tabs: [p] c | f | b | d | a" ]'
check "a note opened from the palette with a pinned tab in front opens in another tab" '[ "$(line 15)" = "front: e.md Live line 1" ] && [ "$(line 16)" = "tabs: [p] c | e | f | b | d | a" ]'
check "a tab reopened with Ctrl Shift T comes back unpinned" '[ "$(line 17)" = "tabbar: [p] c | e | f | b | d | a | $image" ]'
check "Split right moves the front tab to the other group" '[ "$(line 18)" = "tabbar: [p] c | e | f | b | d | a || $image" ]'
check "a pinned tab moved to the other group stays pinned, after its pinned tabs" '[ "$(line 19)" = "tabbar: [p] e | f | b | d | a || [p] $image | [p] c" ]'
keymenu() { grep '^keymenu: ' "$work/first.log" | sed -n "$1p"; }
check "Down reaches Unpin in the menu of a file tab, Enter unpins it" '[ "$(keymenu 1)" = "keymenu: tab-menu | tab-menu-pin" ] && [ "$(line 20)" = "tabbar: [p] e | f | b | d | a || [p] c | $image" ]'
check "Down twice reaches Pin in the menu of a note tab, Enter pins it" '[ "$(keymenu 2)" = "keymenu: tab-menu | tab-menu-pin" ] && [ "$(line 21)" = "tabbar: [p] e | [p] f | b | d | a || [p] c | $image" ]'
check "a restart restores the pins of each group" '[ "$(grep "^tabs: " "$work/second.log" | head -n 1)" = "tabs: [p] e | [p] f | b | d | a || [p] c" ]'
check "a pinned note renamed in the tree stays pinned, after a restart too" '[ -f "$work/vault/renamed.md" ] && grep "^tabs: " "$work/second.log" | tail -n 1 | grep -q "^tabs: \[p\] " && [ "$(grep "^tabs: " "$work/third.log")" = "$(grep "^tabs: " "$work/second.log" | tail -n 1)" ]'
check "no step of the script failed" '! grep -q "^focus: script" "$work/first.log" "$work/second.log" "$work/third.log"'
exit $status
