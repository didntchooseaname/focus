#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-compose"
rm -rf "${work:?}" && mkdir -p "$work"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data XDG_DATA_HOME=$work/xdg
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }

vault() {
    local v=$work/$1
    rm -rf "${v:?}" && mkdir -p "$v/Notes"
    printf -- '---\ntags:\n  - red/ad\n  - blue\n---\n# List\nBody #red and #red/web, not #redder.\n' > "$v/List.md"
    printf -- '---\ntags: red, blue\n---\nInline `#red` and https://example.org/#red stay.\n' > "$v/Line.md"
    printf -- '---\ntags: [red, crimson]\n---\n#red here\n' > "$v/Twice.md"
    printf -- '---\ntags: [a]\n---\n# Dest\nSee [[Src]].\n' > "$v/Dest.md"
    printf -- '---\ntags: [b]\n---\n## Part\nSee [up](../Other.md) and [[Src#Part]].\n' > "$v/Notes/Src.md"
    printf -- '[[Src]] and [s](Notes/Src.md)\n' > "$v/Other.md"
    printf -- '# Long\n\nFirst paragraph.\n\nMove this line away.\n\nLast paragraph.\n' > "$v/Notes/Long.md"
    printf -- '# In place\n\nKeep this.\n\nTake this away.\n' > "$v/Inplace.md"
}

run() {
    local name=$1 lang=$2
    mkdir -p "$work/config-$name"
    printf '{"language": "%s"}\n' "$lang" > "$work/config-$name/settings.json"
    cat > "$work/$name.focus"
    FOCUS_CONFIG_DIR=$work/config-$name FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/$name.focus" "$work/$3" >"$work/$name.log" 2>&1
    grep -E '^focus: script|error' "$work/$name.log" | head -5
}

for lang in en ar; do
    vault "vault-$lang"
    run "compose-$lang" "$lang" "vault-$lang" <<SCRIPT
wait 800
left tags
wait 300
rpress tag:red
wait 300
shot compose-tag-menu-$lang
press menu:rename-tag
wait 500
key ctrl-a
type crimson
wait 800
shot compose-tag-dialog-$lang
key enter
wait 1500
shot compose-tag-done-$lang
left files
expand Notes
wait 300
rpress tree:Notes/Src.md
wait 300
shot compose-merge-menu-$lang
press menu:merge
wait 500
type Dest
wait 500
shot compose-merge-picker-$lang
key enter
wait 800
shot compose-merge-dialog-$lang
key enter
wait 1500
shot compose-merge-done-$lang
open Notes/Long.md
view source
focus editor
key ctrl-home
key down
key down
key down
key down
key shift-end
wait 300
rpress editor
wait 300
shot compose-extract-menu-edit-$lang
key escape
wait 300
lang en
palette >Extract current selection
wait 300
key enter
wait 500
lang $lang
type Moved line
wait 500
shot compose-extract-picker-$lang
key ctrl-enter
wait 1500
shot compose-extract-done-$lang
open Inplace.md
view live
wait 500
caret 5:0
key shift-end
wait 300
rpress editor
wait 300
shot compose-extract-menu-live-$lang
press menu:extract
wait 500
type Dest
wait 300
shot compose-extract-live-picker-$lang
key enter
wait 1500
shot compose-extract-live-done-$lang
quit
SCRIPT
    v=$work/vault-$lang
    check "$lang: tag renamed in a block list and the body" \
        '[ "$(cat "$v/List.md")" = "$(printf -- "---\ntags:\n  - crimson/ad\n  - blue\n---\n# List\nBody #crimson and #crimson/web, not #redder.")" ]'
    check "$lang: tag renamed on one line, code and URL left" \
        '[ "$(cat "$v/Line.md")" = "$(printf -- "---\ntags: crimson, blue\n---\nInline \`#red\` and https://example.org/#red stay.")" ]'
    check "$lang: a property list keeps the merged tag once" \
        '[ "$(cat "$v/Twice.md")" = "$(printf -- "---\ntags: [crimson]\n---\n#crimson here")" ]'
    check "$lang: merge joins the notes and rewrites the links" \
        '[ "$(cat "$v/Dest.md")" = "$(printf -- "---\ntags:\n  - a\n  - b\n---\n# Dest\nSee [[Dest]].\n\n## Part\nSee [up](Other.md) and [[Dest#Part]].\n\nTake this away.")" ]'
    check "$lang: other notes link to the destination" \
        '[ "$(cat "$v/Other.md")" = "[[Dest]] and [s](Dest.md)" ]'
    check "$lang: the source is in the vault trash" '[ ! -e "$v/Notes/Src.md" ] && [ -f "$v/.trash/Src.md" ]'
    check "$lang: the extracted line is a new note beside its source" \
        '[ "$(cat "$v/Notes/Moved line.md")" = "Move this line away." ]'
    check "$lang: a link replaces the selection" \
        '[ "$(cat "$v/Notes/Long.md")" = "$(printf -- "# Long\n\nFirst paragraph.\n\n[[Moved line]]\n\nLast paragraph.")" ]'
    check "$lang: Live in place, the selection goes to the end of an existing note" \
        '[ "$(tail -n 1 "$v/Dest.md")" = "Take this away." ]'
    check "$lang: Live in place, a link replaces the selection" \
        '[ "$(cat "$v/Inplace.md")" = "$(printf -- "# In place\n\nKeep this.\n\n[[Dest]]")" ]'
done

vault vault-system
run compose-system en vault-system <<'SCRIPT'
set files.trash 1
wait 800
expand Notes
wait 300
rpress tree:Notes/Src.md
wait 300
press menu:merge
wait 500
type Dest
wait 500
key enter
wait 800
shot compose-merge-dialog-system-en
key enter
wait 1500
quit
SCRIPT
v=$work/vault-system
check "system trash: the source is in the FreeDesktop trash" \
    '[ -f "$work/xdg/Trash/files/Src.md" ] && grep -q "^Path=.*/vault-system/Notes/Src.md$" "$work/xdg/Trash/info/Src.md.trashinfo"'
check "system trash: not in the vault trash, gone from its folder" '[ ! -e "$v/.trash" ] && [ ! -e "$v/Notes/Src.md" ]'
check "system trash: the destination holds the merged text" '[ "$(tail -n 1 "$v/Dest.md")" = "See [up](Other.md) and [[Dest#Part]]." ]'
exit $status
