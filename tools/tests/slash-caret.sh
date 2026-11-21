#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-slash-caret"
rm -rf "${work:?}" && mkdir -p "$work/vault/Templates" "$work/config"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
echo '{"language": "en"}' > "$work/config/settings.json"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
printf '# Target\n' > "$work/vault/Target.md"
printf 'Meeting notes\n' > "$work/vault/Templates/Meeting.md"
script=$work/caret.focus
echo "wait 300" > "$script"
note() {
    local name=$1 view=$2
    printf '# Case\n\nNotes here.\n' > "$work/vault/$name.md"
    printf 'open %s.md\nview %s\nwait 250\n' "$name" "$view" >> "$script"
    if [ "$view" = live ]; then
        printf 'click Notes here.|5\nkey end\nkey enter\n' >> "$script"
    else
        printf 'focus editor\nkey ctrl-end\n' >> "$script"
    fi
    cat >> "$script"
    printf 'wait 300\nkey escape\nkey ctrl-s\nwait 450\n' >> "$script"
}
tail_of() { sed -e '1,/^Notes here\.$/d' "$work/vault/$1.md"; }

note live-keys live <<'SCRIPT'
type /
type bold
key enter
type XYZ
SCRIPT
note live-mouse live <<'SCRIPT'
type /
pick bold
type XYZ
SCRIPT
note live-list live <<'SCRIPT'
type /
pick bullets
type XYZ
SCRIPT
note live-language live <<'SCRIPT'
type /
pick codeblock
wait 250
menu
type pyth
wait 200
key enter
type print(1)
SCRIPT
note live-fence live <<'SCRIPT'
type /
pick codeblock
wait 250
key escape
wait 300
type js
SCRIPT
note live-link live <<'SCRIPT'
type /
pick wikilink
type Targ
wait 300
menu
key enter
type after
SCRIPT
note live-rule live <<'SCRIPT'
type /
pick rule
type XYZ
SCRIPT
note live-template live <<'SCRIPT'
type /
pick template
wait 200
type Meet
wait 250
menu
key enter
wait 250
type XYZ
SCRIPT
note live-format live <<'SCRIPT'
key ctrl-b
type XYZ
SCRIPT
note live-overlay live <<'SCRIPT'
type /
pick bold
wait 300
type AB
key ctrl-k
wait 200
key escape
wait 200
type CD
key ctrl-f
wait 200
key escape
wait 200
type EF
SCRIPT
note live-title live <<'SCRIPT'
type /
wait 150
pick title
menu
type bold
key enter
type XYZ
SCRIPT
note source-mouse source <<'SCRIPT'
type /
pick bold
type XYZ
SCRIPT
note source-title source <<'SCRIPT'
type /
wait 150
pick title
menu
type bold
key enter
type XYZ
SCRIPT
note source-rule source <<'SCRIPT'
type /
pick rule
type XYZ
SCRIPT
note source-far source <<'SCRIPT'
type /
key down
key down
key down
key down
key down
key down
key down
key down
key down
key down
key down
key down
wait 150
menu
key escape
SCRIPT
echo quit >> "$script"
FOCUS_SHOT_TIMEOUT=300 FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$script" "$work/vault" >"$work/caret.log" 2>&1
menus() { grep '^menu: ' "$work/caret.log" | sed -n "$1p"; }

check "Live, Enter on a row: the text typed next goes between the marks" '[ "$(tail_of live-keys)" = "**XYZ**" ]'
check "Live, a press on a row does the same" '[ "$(tail_of live-mouse)" = "**XYZ**" ]'
check "Live, an empty list item under a line of text takes the text typed" '[ "$(tail_of live-list)" = "- XYZ" ]'
check "Live, the language menu shows after a code block is chosen" 'menus 1 | grep -q "^menu: languages | .* | shown$"'
check "Live, the language chosen is written and the caret goes into the block" '[ "$(tail_of live-language)" = "$(printf "\140\140\140python\nprint(1)\n\140\140\140")" ]'
check "Live, Escape on the language menu leaves the fence to type in" '[ "$(tail_of live-fence | head -n 1)" = "$(printf "\140\140\140js")" ]'
check "Live, the notes menu shows for a link" 'menus 2 | grep -q "^menu: notes | Targ | .* | shown$"'
check "Live, the note chosen is linked and typing goes on after it" '[ "$(tail_of live-link)" = "[Target](Target.md)after" ]'
check "Live, a rule under a line of text leaves that line a paragraph" '[ "$(tail_of live-rule)" = "$(printf "\n---\nXYZ")" ]'
check "Live, the templates menu shows" 'menus 3 | grep -q "^menu: templates | Meet | .* | shown$"'
check "Live, typing goes on after an inserted template" 'tail_of live-template | grep -q "^Meeting notes" && tail_of live-template | grep -q "XYZ"'
check "Live, Ctrl B on an empty line takes the text typed" '[ "$(tail_of live-format)" = "**XYZ**" ]'
check "Live, a source block keeps the text typed after the palette and the find bar close" '[ "$(tail_of live-overlay)" = "**ABCDEF**" ]'
check "Live, a press on a group title leaves the menu open" 'menus 4 | grep -q "^menu: commands | "'
check "Live, and leaves the caret where typing goes on" '[ "$(tail_of live-title)" = "**XYZ**" ]'
check "Edit, a press on a row puts the caret between the marks" '[ "$(tail_of source-mouse)" = "**XYZ**" ]'
check "Edit, a press on a group title leaves the menu open" 'menus 5 | grep -q "^menu: commands | "'
check "Edit, and leaves the caret where typing goes on" '[ "$(tail_of source-title)" = "**XYZ**" ]'
check "Edit, a rule under a line of text leaves that line a paragraph" '[ "$(tail_of source-rule)" = "$(printf "\n---\nXYZ")" ]'
check "the row reached with the arrows stays in view past the group titles" 'menus 6 | grep -q " | shown$"'
check "no step of the script failed" '! grep -q "^focus: script" "$work/caret.log"'
exit $status
