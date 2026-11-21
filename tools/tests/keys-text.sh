#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-keys-text"
rm -rf "${work:?}" && mkdir -p "$work/vault/K" "$work/config"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
echo '{"language": "en"}' > "$work/config/settings.json"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
script=$work/text.focus
echo "wait 800" > "$script"
note() {
    local name=$1 view=$2 line=$3 col=$4
    printf '# Case\n\nalpha beta gamma beta\n\n- item one\n' > "$work/vault/K/$name.md"
    printf 'open K/%s.md\nview %s\nwait 300\n' "$name" "$view" >> "$script"
    case $view in
        live) printf 'caret %s:%s\n' "$line" "$col" >> "$script" ;;
        source)
            printf 'focus editor\nkey ctrl-home\n' >> "$script"
            for _ in $(seq 2 "$line"); do echo "key down" >> "$script"; done
            echo "key end" >> "$script"
            ;;
    esac
    cat >> "$script"
    printf 'wait 1000\n' >> "$script"
}
body() { sed -n '3,$p' "$work/vault/K/$1.md"; }
item() { sed -n '5,$p' "$work/vault/K/$1.md"; }

note edit-tab source 5 0 <<'S'
key enter
type two
key tab
type X
S
note edit-shift-tab source 5 0 <<'S'
key enter
type two
key tab
key shift-tab
type Y
S
note edit-redo source 3 0 <<'S'
type ZZ
wait 1200
key ctrl-z
wait 200
key ctrl-shift-z
S
note edit-page source 1 0 <<'S'
key ctrl-home
key shift-pagedown
wait 200
type P
S
for v in edit live; do
    view=$v
    [ "$v" = edit ] && view=source
    note "$v-heading" "$view" 3 21 <<'S'
key shift-home
key ctrl-shift-h
S
    note "$v-bullets" "$view" 3 21 <<'S'
key shift-home
key ctrl-shift-8
S
    note "$v-task" "$view" 3 21 <<'S'
key shift-home
key ctrl-l
S
    note "$v-link" "$view" 3 21 <<'S'
key shift-home
key ctrl-shift-k
wait 200
key ctrl-shift-k
S
    note "$v-find" "$view" 3 21 <<'S'
key ctrl-f
type beta
key enter
wait 200
key escape
wait 300
type F
S
    note "$v-menu" "$view" 3 21 <<'S'
key shift-home
key shift-f10
wait 300
press menu:extract
wait 300
key escape
S
done
note edit-f3 source 1 0 <<'S'
key ctrl-home
key ctrl-f
type beta
wait 200
focus editor
key f3
key f3
wait 200
type G
key escape
S
note read-find reading 1 0 <<'S'
key ctrl-f
type gamma
key enter
wait 200
key escape
wait 300
front
S
echo quit >> "$script"
FOCUS_SHOT_TIMEOUT=300 FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$script" "$work/vault" >"$work/text.log" 2>&1

check "Edit, Tab indents a list item under its sibling" '[ "$(item edit-tab)" = "$(printf -- "- item one\n  - twoX")" ]'
check "Edit, Shift Tab outdents it back" '[ "$(item edit-shift-tab)" = "$(printf -- "- item one\n- twoY")" ]'
check "Edit, Ctrl Shift Z redoes" '[ "$(sed -n 3p "$work/vault/K/edit-redo.md")" = "alpha beta gamma betaZZ" ]'
check "Edit, Shift PageDown selects a page" '[ "$(cat "$work/vault/K/edit-page.md")" = "P" ]'
for v in edit live; do
    check "$v, Ctrl Shift H makes a heading" '[ "$(sed -n 3p "$work/vault/K/$v-heading.md")" = "# alpha beta gamma beta" ]'
    check "$v, Ctrl Shift 8 makes a bulleted item" '[ "$(sed -n 3p "$work/vault/K/$v-bullets.md")" = "- alpha beta gamma beta" ]'
    check "$v, Ctrl L makes a task" '[ "$(sed -n 3p "$work/vault/K/$v-task.md")" = "- [ ] alpha beta gamma beta" ]'
    check "$v, Ctrl Shift K twice gives the text back" '[ "$(sed -n 3p "$work/vault/K/$v-link.md")" = "alpha beta gamma beta" ]'
    check "$v, Escape on the find bar leaves the match selected" '[ "$(sed -n 3p "$work/vault/K/$v-find.md")" = "alpha beta gamma F" ]'
done
check "Shift F10 opens the editor menu in Edit and in Live" '! grep -q "no editor menu open" "$work/text.log"'
check "F3 with the caret in the text goes to the next match" '[ "$(sed -n 3p "$work/vault/K/edit-f3.md")" = "alpha G gamma beta" ]'
check "Read, the find bar gives the note back in Read" 'grep -q "^front: K/read-find.md Reading" "$work/text.log"'
check "no step of the script failed" '! grep -q "^focus: script" "$work/text.log"'
exit $status
