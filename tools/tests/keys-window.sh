#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-keys-window"
rm -rf "${work:?}" && mkdir -p "$work/vault/K" "$work/config"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
echo '{"language": "en"}' > "$work/config/settings.json"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
for n in a b c; do printf '# %s\n\nalpha beta gamma\n' "$n" > "$work/vault/K/$n.md"; done
printf '# Recipes\n\nBread\n' > "$work/vault/Recipes.md"
cp "$root/fixtures/demo-vault/assets/scope-map.png" "$work/vault/scope-map.png"
script=$work/window.focus
cat > "$script" <<SCRIPT
wait 800
open K/a.md
open K/b.md
open K/c.md
openpath $work/vault/scope-map.png
wait 500
status
key ctrl-tab
front
key ctrl-shift-tab
status
key ctrl-w
tabs
status
key ctrl-shift-t
wait 400
status
key ctrl-1
front
key ctrl-9
status
key ctrl-pageup
front
key ctrl-pagedown
status
key ctrl-2
key ctrl-w
tabs
key ctrl-shift-t
wait 300
tabs
front
key ctrl-e
front
key ctrl-e
front
key ctrl-shift-f
type Recipes
wait 600
key down
key enter
wait 400
front
open K/a.md
view source
focus editor
key ctrl-end
set shortcuts.overrides {"ui-cmd-toggle-left": ["ctrl-alt-l"]}
key ctrl-b
type B
set shortcuts.overrides {}
wait 300
key ctrl-=
key ctrl-=
key ctrl--
wait 200
key ctrl-n
wait 400
type Fresh name
key enter
wait 400
type Body text
wait 1000
front
quit
SCRIPT
FOCUS_SHOT_TIMEOUT=240 FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$script" "$work/vault" >"$work/window.log" 2>&1
line() { grep -E '^(status|front|tabs): ' "$work/window.log" | sed -n "$1p"; }
image="status: scope-map.png | 720 × 300 px | 7.5 KB"
setting() { python3 -c "import json,sys; print(json.load(open('$work/config/settings.json'))['ui']['settings'].get('$1'))"; }

check "the image tab is in front" '[ "$(line 1)" = "$image" ]'
check "Ctrl Tab goes from the image to the first note" '[ "$(line 2)" = "front: K/a.md Live line 1" ]'
check "Ctrl Shift Tab comes back to the image tab" '[ "$(line 3)" = "$image" ]'
check "Ctrl W closes the image in front, not a note" '[ "$(line 4)" = "tabs: a | b | c" ]'
check "Ctrl Shift T reopens the image" '[ "$(line 6)" = "$image" ]'
check "Ctrl 1 goes to the first tab" '[ "$(line 7)" = "front: K/a.md Live line 1" ]'
check "Ctrl 9 goes to the last tab" '[ "$(line 8)" = "$image" ]'
check "Ctrl PageUp goes to the previous tab" '[ "$(line 9)" = "front: K/c.md Live line 1" ]'
check "Ctrl PageDown goes to the next tab" '[ "$(line 10)" = "$image" ]'
check "Ctrl 2 then Ctrl W closes the second note" '[ "$(line 11)" = "tabs: a | c" ]'
check "Ctrl Shift T reopens that note" '[ "$(line 12)" = "tabs: a | c | b" ] && [ "$(line 13)" = "front: K/b.md Live line 1" ]'
check "Ctrl E goes from Live to Read" '[ "$(line 14)" = "front: K/b.md Reading line 3" ]'
check "Ctrl E goes back to Live" '[ "$(line 15)" = "front: K/b.md Live line 1" ]'
check "Ctrl Shift F with a note open searches the vault" '[ "$(line 16)" = "front: Recipes.md Live line 1" ]'
check "a changed window shortcut leaves Ctrl B to bold" 'tail -n 1 "$work/vault/K/a.md" | grep -qx "\*\*B\*\*"'
check "Ctrl = twice and Ctrl - once leave the interface one step larger" '[ "$(setting appearance.ui_scale)" = "110.0" ]'
check "Ctrl N names a new note and the caret goes to its body" '[ "$(cat "$work/vault/Fresh name.md")" = "Body text" ] && [ "$(line 17)" = "front: Fresh name.md Live line 1" ]'
check "no step of the script failed" '! grep -q "^focus: script" "$work/window.log"'
exit $status
