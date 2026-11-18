#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-live-clicks"
vault="$work/vault"
note="Rendu/Tout le Markdown.md"
status=0

run() {
    local name=$1
    rm -rf "$vault" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$vault"
    { echo "set editor.edit_in_place off"; cat; } > "$work/$name.focus"
    FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/$name.focus" "$vault" >"$work/$name.log" 2>&1 || true
}
check() { if grep -q -- "$2" "$vault/$note"; then echo "ok   $1"; else echo "FAIL $1"; grep -n "$3" "$vault/$note" | head -2; status=1; fi; }

run word <<SCRIPT
open $note
view live
wait 400
click barré
wait 300
type X
wait 300
click #block 4
type W
key escape
wait 1500
quit
SCRIPT
check "a click on a word puts the caret before it" '~~XWbarré~~' 'barré'
check "a click inside the open block keeps it open and its caret" '~~XWbarré~~' 'barré'

run image <<SCRIPT
open $note
view live
wait 400
goto Images et inclusions
wait 400
click #image 1
wait 300
key end
type IMG
key escape
wait 1500
quit
SCRIPT
check "a click on an image opens its source" 'capture-exemple.png\(|420\]\]\|)\)IMG' 'capture-exemple'

run alt <<SCRIPT
open $note
view live
wait 400
altclick l'accueil
wait 300
type ALT
key escape
wait 1500
quit
SCRIPT
check "Alt click on a link opens the source of its block" 'ALT' 'accueil'

run tag <<SCRIPT
open $note
view live
wait 400
click #markdown/inline
wait 600
shot live-click-tag
quit
SCRIPT
if cmp -s "$root/fixtures/vault/$note" "$vault/$note"; then echo "ok   a click on a tag leaves the note as it is (search: live-click-tag.png)"; else echo "FAIL a click on a tag changed the note"; status=1; fi
exit $status
