#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-keys-focus"
rm -rf "${work:?}" && mkdir -p "$work/vault/Folder" "$work/out"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
for name in Alpha Maps Media Method Notes; do printf '# %s\n\nBody.\n' "$name" > "$work/vault/$name.md"; done
printf '# Inner\n' > "$work/vault/Folder/Inner.md"
run() {
    local name=$1
    rm -rf "${work:?}/config-$name" && mkdir -p "$work/config-$name"
    echo '{"language": "en"}' > "$work/config-$name/settings.json"
    cat > "$work/$name.focus"
    echo quit >> "$work/$name.focus"
    FOCUS_CONFIG_DIR=$work/config-$name FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/$name.focus" "$work/vault" >"$work/$name.log" 2>&1
}
walk() { grep '^focused: ' "$work/$1.log" | sed 's/^focused: //' | sed -n "$2" | paste -sd ' '; }
tabs() { for _ in $(seq "$1"); do printf 'key tab\nwait 80\nfocused\n'; done; }

{
    printf 'wait 800\nopen Alpha.md\nview source\nfocus editor\nkey ctrl-end\nkey up\nkey end\nwait 200\nkey ctrl-,\nwait 500\nfocused\n'
    tabs 10
    printf 'key shift-tab\nwait 80\nfocused\nkey escape\nwait 300\nfocused\ntype S\n'
    printf 'action capture\nwait 400\nfocused\ntype Captured\n'
    tabs 3
    printf 'key shift-tab\nwait 80\nfocused\nkey shift-tab\nwait 80\nkey enter\nwait 300\nfocused\ntype C\n'
    printf 'action capture\nwait 400\nfocus tree\nwait 100\nkey escape\nwait 300\nfocused\ntype E\n'
    printf 'palette >Export the note\nwait 300\nkey enter\nwait 600\nfocused\n'
    tabs 12
    printf 'key escape\nwait 300\nfocused\ntype X\n'
    printf 'palette >Export the note\nwait 300\nkey enter\nwait 600\nkey ctrl-a\ntype %s\nwait 200\nkey enter\nwait 2500\n' "$work/out"
} | run walks

settings="field settings-close section-list reset-section general.language reset-general.language general.open general.reopen_tabs general.default_view field field"
check "Settings opens on its search field and Tab walks the General section in reading order" '[ "$(walk walks 1,11p)" = "$settings" ]'
check "Shift Tab walks back" '[ "$(walk walks 12p)" = "field" ]'
check "Escape closes Settings and the focus goes back to the note" '[ "$(walk walks 13p)" = "editor" ]'
check "Quick capture opens on its field and Tab wraps inside the dialog" '[ "$(walk walks 14,17p)" = "field capture-cancel capture-add field" ]'
check "Shift Tab goes back from the field to the last button" '[ "$(walk walks 18p)" = "capture-add" ]'
check "Enter on a focused Cancel runs Cancel, not the primary button" '[ "$(walk walks 19p)" = "editor" ] && ! grep -rq "Captured" "$work/vault"'
check "Escape closes a dialog whose focus was moved behind the veil" '[ "$(walk walks 20p)" = "editor" ]'
export_walk="field export-choose export-cancel export-start dialog-close export-scope-choice export-format-choice export-theme export-code export-images export-fonts export-mermaid field"
check "Export opens on its destination field and Tab walks it once around" '[ "$(walk walks 21,33p)" = "$export_walk" ]'
check "Escape closes Export and the focus goes back to the note" '[ "$(walk walks 34p)" = "editor" ]'
check "Enter in the destination field runs Export" '[ -n "$(find "$work/out" -name "*.html" | head -n 1)" ]'
check "the letters typed after each dialog closed reached the note" 'grep -q "Body\.SCEX" "$work/vault/Alpha.md"'

{
    printf 'wait 800\nopen Alpha.md\nview source\nkey ctrl-,\nwait 500\n'
    printf 'key tab\nkey tab\nkey tab\nkey tab\nwait 80\nfocused\nkey down\nwait 150\nkey down\nwait 150\nkey escape\nwait 200\nfocused\nkey enter\nwait 150\nkey escape\nwait 200\n'
    printf 'key tab\nkey tab\nkey tab\nkey tab\nwait 80\nfocused\nkey right\nwait 200\n'
    printf 'key shift-tab\nwait 80\nfocused\nkey space\nwait 200\n'
    printf 'key escape\nwait 300\nfocused\n'
} | run controls
check "Down opens the language select and Escape closes it, not Settings" '[ "$(walk controls 1,2p)" = "general.language general.language" ]'
check "Right moves the segmented control toward the end" 'grep -q "\"general.default_view\": 2" "$work/config-controls/settings.json"'
check "Space toggles a checkbox" 'grep -q "\"general.reopen_tabs\": false" "$work/config-controls/settings.json"'
check "Enter and Escape on the open select left the language unchanged" 'grep -q "\"language\": \"en\"" "$work/config-controls/settings.json"'
check "the walk reached the segmented control and the checkbox" '[ "$(walk controls 3,4p)" = "general.default_view general.reopen_tabs" ]'
check "Escape after the controls closes Settings" '[ "$(walk controls 5p)" = "editor" ] || [ "$(walk controls 5p)" = "window" ]'

{
    printf 'wait 800\nlang ar\nopen Alpha.md\nkey ctrl-,\nwait 500\n'
    tabs 8
    printf 'key right\nwait 200\n'
} | run arabic
check "in Arabic the walk reaches the default view control" '[ "$(walk arabic 8p)" = "general.default_view" ]'
check "in Arabic Right moves the segmented control toward the start" 'grep -q "\"general.default_view\": 0" "$work/config-arabic/settings.json"'

{
    printf 'wait 800\nopen Alpha.md\nwait 300\npress tree:Alpha.md\nfocus tree\nwait 100\n'
    printf 'type m\nwait 100\nkey enter\nwait 400\nfront\n'
    printf 'wait 1100\nfocus tree\ntype me\nwait 100\nkey enter\nwait 400\nfront\n'
    printf 'wait 1100\nfocus tree\ntype m\ntype m\nwait 100\nkey enter\nwait 400\nfront\n'
    printf 'focus tree\nwait 1200\ntype n\nwait 100\nkey enter\nwait 400\nfront\n'
} | run typeahead
fronts() { grep '^front: ' "$work/typeahead.log" | sed -n "$1p" | awk '{print $2}'; }
check "a letter selects the next row of the tree that starts with it" '[ "$(fronts 1)" = "Maps.md" ]'
check "letters typed together select by prefix" '[ "$(fronts 2)" = "Media.md" ]'
check "the same letter twice moves on to the following row with that letter" '[ "$(fronts 3)" = "Maps.md" ]'
check "the prefix is forgotten after a pause" '[ "$(fronts 4)" = "Notes.md" ]'

check "no step of any script failed" '! grep -q "^focus: script" "$work"/*.log'
exit $status
