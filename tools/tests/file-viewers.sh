#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-file-viewers"
rm -rf "${work:?}" && mkdir -p "$work"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }

cp -r "$root/fixtures/demo-vault" "$work/vault"
python3 -I "$root/tools/tests/file-viewers.py" "$work/vault/Files"

run() {
    local lang=$1 theme=$2 name=viewer-$1-$2
    mkdir -p "$work/config-$name" && echo '{"language": "en"}' > "$work/config-$name/settings.json"
    cat > "$work/$name.focus" <<SCRIPT
wait 300
lang $lang
theme $theme
expand Files
press tree:Files/a-photo.png
wait 300
status
shot $name-image
viewer zoom 200
status
viewer next
wait 300
status
press tree:Files/report.pdf
right outline
wait 500
status
viewer page 5
wait 300
status
shot $name-pdf
key ctrl-f
type kerberos
wait 600
key enter
wait 300
status
shot $name-pdf-find
quit
SCRIPT
    FOCUS_CONFIG_DIR=$work/config-$name FOCUS_SHOT_DIR=$work/shots "$root/tools/shot.sh" "$work/$name.focus" "$work/vault" >"$work/$name.log" 2>&1
}

for lang in en ar; do
    for theme in dark light; do
        run "$lang" "$theme"
    done
done

log=$work/viewer-en-dark.log
statuses() { grep '^status: ' "$1"; }
check "the image opened from the tree shows its size and weight" \
    'statuses "$log" | sed -n 1p | grep -Eq "Files/a-photo.png \| 1,?600 × 1,?000 px \| [0-9.,]+ (KB|kB|B)"'
check "the next image of the folder replaces it in the same tab" \
    'statuses "$log" | sed -n 3p | grep -Eq "Files/b-photo.png \| 600 × 900 px"'
check "the PDF opens at page 1 of 12" 'statuses "$log" | sed -n 4p | grep -q "Files/report.pdf | Page 1 of 12"'
check "viewer page 5 shows page 5" 'statuses "$log" | sed -n 5p | grep -q "Page 5 of 12"'
check "find goes to the first match after page 5, on page 7" 'statuses "$log" | sed -n 6p | grep -q "Page 7 of 12"'
check "the Arabic run reports the same pages" 'statuses "$work/viewer-ar-dark.log" | sed -n 6p | grep -q "7"'
for name in viewer-en-dark viewer-en-light viewer-ar-dark viewer-ar-light; do
    for shot in image pdf pdf-find; do
        check "$name-$shot was taken" '[ -s "$work/shots/$name-$shot.png" ]'
    done
    check "$name ran without a script error" '! grep -q "script error\|panicked" "$work/$name.log"'
done
exit $status
