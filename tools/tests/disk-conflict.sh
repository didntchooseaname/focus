#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-disk-conflict"
rm -rf "${work:?}" && mkdir -p "$work/config"
cp -r "$root/fixtures/demo-vault" "$work/vault"
echo '{"language": "en"}' > "$work/config/settings.json"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
run() {
    local name=$1
    cat > "$work/$name.focus"
    FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/$name.focus" "$work/vault" >"$work/$name.log" 2>&1
}
versions_of() {
    local file
    for file in "$work"/data/history/*/notes/*/path; do
        [ "$(cat "$file")" = "$1" ] && ls "$(dirname "$file")" | grep -v '^path$'
    done
    return 0
}
stored() { find "$work"/data/history/*/blobs -type f -exec cmp -s "$1" {} \; -print | grep -q .; }

mkdir -p "$work/vault/Sync"
lines() { for i in $(seq 1 12); do echo "Line $i of the plan."; done; }
{ echo "# Plan"; echo; lines; } > "$work/vault/Sync/Plan.md"
{ echo "# Plan"; echo; lines; echo "Added on the phone."; } > "$work/plan-phone.md"
{ echo "# Plan, renamed on the laptop"; echo; lines; } > "$work/plan-laptop.md"
{ echo "# Other"; echo; echo "Shown before the reload."; } > "$work/vault/Sync/Other.md"
{ echo "# Other"; echo; echo "Truncated by a sync."; } > "$work/other-sync.md"
printf '# Same\n\nTyped here.' > "$work/same-typed.md"
{ echo "# Same"; echo; } > "$work/vault/Sync/Same.md"
{ echo "# Undo"; echo; lines; } > "$work/vault/Sync/Undo.md"
{ echo "# Undo"; echo; lines; echo "From the other program."; } > "$work/undo-disk.md"
{ echo "# Plan"; echo; echo "From the Syncthing device."; } > "$work/vault/Sync/Plan.sync-conflict-20261116-101500-MFZWI3D.md"
{ echo "# Plan"; echo; echo "From the Obsidian Sync device."; } > "$work/vault/Sync/Plan (Conflicted copy MyMacBook2 202611161020).md"
touch -d "2026-11-16 10:15" "$work/vault/Sync/Plan.sync-conflict-20261116-101500-MFZWI3D.md"
touch -d "2026-11-16 10:20" "$work/vault/Sync/Plan (Conflicted copy MyMacBook2 202611161020).md"

run merge <<SCRIPT
set editor.autosave 5000
wait 800
open Sync/Plan.md
view source
focus editor
key ctrl-home
type Draft.
disk Sync/Plan.md $work/plan-phone.md
wait 1500
shot conflict-banner
lang ar
wait 500
shot conflict-banner-ar
lang en
wait 300
banner merge
wait 6500
quit
SCRIPT
check "the merge keeps both changes" 'grep -q "^Draft.# Plan" "$work/vault/Sync/Plan.md" && grep -q "^Added on the phone." "$work/vault/Sync/Plan.md"'
check "no conflict marker" '! grep -q "^<<<<<<<\|^>>>>>>>\|^=======" "$work/vault/Sync/Plan.md"'
check "the text on disk is kept as a version (x)" 'versions_of Sync/Plan.md | grep -q -- "-x-" && stored "$work/plan-phone.md"'
check "the unsaved text is kept as a version (u)" 'versions_of Sync/Plan.md | grep -q -- "-u-"'
check "the banner showed its four actions" '[ -s "$work/conflict-banner.png" ] && ! grep -q "offers no merge" "$work/merge.log"'

cp "$work/vault/Sync/Plan.md" "$work/plan-before-both.md"
run both <<SCRIPT
set editor.autosave 5000
wait 800
open Sync/Plan.md
view source
focus editor
key ctrl-home
type Mine.
disk Sync/Plan.md $work/plan-laptop.md
wait 1500
shot conflict-banner-overlap
banner merge
banner both
wait 1500
shot conflict-both
quit
SCRIPT
check "overlapping changes offer no merge" 'grep -q "offers no merge" "$work/both.log"'
check "Keep both leaves the file as the other program wrote it" 'cmp -s "$work/vault/Sync/Plan.md" "$work/plan-laptop.md"'
check "Keep both writes the unsaved text beside the note" 'grep -q "^Mine.Draft.# Plan" "$work/vault/Sync/Plan (unsaved copy).md"'

cp "$work/vault/Sync/Other.md" "$work/other-original.md"
cp "$work/same-typed.md" "$work/same-expected.md"
run reload <<SCRIPT
set editor.autosave 5000
wait 800
open Sync/Other.md
view source
disk Sync/Other.md $work/other-sync.md
wait 1500
shot conflict-reload
open Sync/Same.md
view source
focus editor
key ctrl-end
type Typed here.
disk Sync/Same.md $work/same-typed.md
wait 1500
shot conflict-same
quit
SCRIPT
run undo <<SCRIPT
set editor.autosave 5000
wait 800
open Sync/Undo.md
view source
focus editor
key ctrl-home
type Mine.
disk Sync/Undo.md $work/undo-disk.md
wait 1500
banner merge
wait 500
focus editor
key ctrl-z
wait 6500
quit
SCRIPT
check "undo after a merge gives back the unsaved text" 'grep -q "^Mine.# Undo" "$work/vault/Sync/Undo.md" && ! grep -q "From the other program." "$work/vault/Sync/Undo.md"'
check "a reload keeps the text shown (l)" 'versions_of Sync/Other.md | grep -q -- "-l-" && stored "$work/other-original.md"'
check "the same text on disk raises no banner and keeps nothing" '[ -z "$(versions_of Sync/Same.md)" ] && cmp -s "$work/vault/Sync/Same.md" "$work/same-expected.md"'

for name in Split Lead; do
    { echo "# $name"; echo; lines; } > "$work/vault/Sync/$name.md"
    { echo "# $name"; echo; lines; echo "Added on the phone."; } > "$work/$name-phone.md"
    { echo "Draft.# $name"; echo; lines; echo "Added on the phone."; } > "$work/$name-merged.md"
done
run split <<SCRIPT
set editor.autosave 5000
wait 800
open Sync/Split.md
view source
split
wait 800
view source
group 1
focus editor
key ctrl-home
type Draft.
disk Sync/Split.md $work/Split-phone.md
wait 1500
shot conflict-split
group 2
banner merge
wait 6500
shot conflict-split-merged
quit
SCRIPT
run lead <<SCRIPT
set editor.autosave 5000
wait 800
open Sync/Lead.md
view source
split
wait 800
view source
focus editor
key ctrl-home
type Draft.
disk Sync/Lead.md $work/Lead-phone.md
wait 1500
group 1
banner merge
wait 6500
quit
SCRIPT
{ echo "# Parked"; echo; lines; } > "$work/vault/Sync/Parked.md"
{ echo "# Parked"; echo; lines; echo "Added on the phone."; } > "$work/Parked-phone.md"
{ echo "Draft.# Parked"; echo; lines; echo "Added on the phone."; } > "$work/Parked-merged.md"
{ echo "# Aside"; echo; echo "Opened in front of the parked tab."; } > "$work/vault/Sync/Aside.md"
run parked <<SCRIPT
set editor.autosave 5000
wait 800
open Sync/Parked.md
view source
split
wait 800
view source
open Sync/Aside.md
wait 800
tabs
group 1
focus editor
key ctrl-home
type Draft.
disk Sync/Parked.md $work/Parked-phone.md
wait 1500
group 2
open Sync/Parked.md
wait 800
view source
shot conflict-parked
banner merge
wait 6500
quit
SCRIPT
check "parked: the tab was behind another when the conflict came" 'grep -qx "tabs: Parked || Parked | Aside" "$work/parked.log"'
for name in Split Lead Parked; do
    check "two groups ($name): the banner pressed offered Merge" '! grep -q "offers no merge\|shows no conflict banner" "$work/${name,,}.log"'
    check "two groups ($name): the file holds the merged text once" 'cmp -s "$work/vault/Sync/$name.md" "$work/$name-merged.md"'
    check "two groups ($name): no conflict marker" '! grep -q "^<<<<<<<\|^>>>>>>>\|^=======" "$work/vault/Sync/$name.md"'
    check "two groups ($name): each text kept once (x, u), nothing else" '[ "$(versions_of Sync/$name.md | grep -c -- "-x-")" = 1 ] && [ "$(versions_of Sync/$name.md | grep -c -- "-u-")" = 1 ] && [ "$(versions_of Sync/$name.md | wc -l)" = 2 ]'
done

cp "$work/vault/Sync/Plan.md" "$work/plan-before-copy.md"
run copies <<'SCRIPT'
wait 800
open Sync/Plan.md
view live
wait 800
shot copies-line
palette >Show the versions of the note
wait 300
key enter
wait 1500
shot versions-copies
lang ar
wait 800
shot versions-copies-ar
shot copies-line-ar
lang en
wait 300
restore
wait 1500
shot versions-copy-restored
key down
wait 800
delete-copy
wait 1500
shot versions-copy-deleted
quit
SCRIPT
check "Restore puts the newest copy over the note" 'grep -q "From the Obsidian Sync device." "$work/vault/Sync/Plan.md"'
check "the replaced text is kept first" 'stored "$work/plan-before-copy.md"'
check "Restore leaves the copy" '[ -f "$work/vault/Sync/Plan (Conflicted copy MyMacBook2 202611161020).md" ]'
check "Delete the copy sends the Syncthing copy to the vault trash" '[ ! -e "$work/vault/Sync/Plan.sync-conflict-20261116-101500-MFZWI3D.md" ] && find "$work/vault/.trash" -name "Plan.sync-conflict-*" | grep -q .'
grep "focus:" "$work"/*.log | grep -v "listening\|offers no merge" && status=1
exit $status
