#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-note-versions"
rm -rf "$work" && mkdir -p "$work/config"
cp -r "$root/fixtures/demo-vault" "$work/vault"
readme="Engagements/Practice exam/README.md"
printf '{"language": "en", "ops_enabled": true, "ui": {"ops_engagement": "%s"}}\n' "$work/vault/Engagements/Practice exam" > "$work/config/settings.json"
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
}
stored() { find "$work"/data/history/*/blobs -type f -exec cmp -s "$1" {} \; -print | grep -q .; }

cp "$work/vault/Home.md" "$work/home-original.md"
cp "$work/vault/$readme" "$work/readme-original.md"

run first <<'SCRIPT'
wait 800
open Home.md
view source
focus editor
key ctrl-end
type First edit.
wait 2500
quit
SCRIPT
cp "$work/vault/Home.md" "$work/home-first.md"
check "the first write keeps the original text" 'stored "$work/home-original.md"'
check "it is one version of Home.md" '[ "$(versions_of Home.md | wc -l)" = 1 ]'

sleep 310

run second <<'SCRIPT'
set editor.autosave 5000
wait 800
open Home.md
view source
split
group 1
focus editor
key ctrl-end
type Second edit.
palette >Show the versions of the note
wait 300
key enter
wait 1500
shot versions-list
key down
wait 1000
shot versions-oldest
restore
wait 1500
shot versions-restored
key escape
wait 800
shot versions-tabs
tabs
quit
SCRIPT
check "a write more than 5 minutes later keeps a second version" 'stored "$work/home-first.md"'
check "the restore puts the oldest version on disk" 'cmp -s "$work/vault/Home.md" "$work/home-original.md"'
check "the unsaved text, written before the dialog opened, is the version the restore kept" 'find "$work"/data/history/*/blobs -type f -exec grep -l "First edit.Second edit." {} \; | grep -q .'
check "Home.md has three versions, the last one a restore" '[ "$(versions_of Home.md | wc -l)" = 3 ] && versions_of Home.md | sort | tail -1 | grep -q -- "-r-"'
check "no conflict copy: both tabs took the restored text" '! find "$work/vault" -name "*copy*" | grep -q .'
check "both groups still show Home" 'grep -qx "tabs: Home || Home" "$work/second.log"'

run ops <<'SCRIPT'
wait 1000
target standalone-2
wait 1500
quit
SCRIPT
check "focus-ops wrote the README" 'grep -q "^focus: standalone-2" "$work/vault/$readme"'
check "the README it replaced is kept as a version" 'stored "$work/readme-original.md" && [ "$(versions_of "$readme" | wc -l)" = 1 ]'
check "store folders are 0700 and files 0600" '[ -z "$(find "$work/data/history" -type d ! -perm 700)" ] && [ -z "$(find "$work/data/history" -type f ! -perm 600)" ]'
grep "focus:" "$work"/*.log | grep -v "listening" && status=1
exit $status
