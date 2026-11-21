#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-search-arrival"
rm -rf "${work:?}" && mkdir -p "$work"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
note() {
    printf '# Where\n\n'
    for n in $(seq 3 90); do
        case $n in
            5) echo "The first needle is near the top." ;;
            45) echo "A second needle sits in the middle." ;;
            85) echo "The last needle waits near the end." ;;
            *) echo "Line $n of plain filler text." ;;
        esac
        echo
    done
}
boxed() {
    python3 -I - "$1" <<'PY'
import sys
from PIL import Image

image = Image.open(sys.argv[1]).convert("RGB")
width, height = image.size
pixels = image.load()
want = (245, 245, 246)
rows = 0
best = 0
for y in range(80, height - 30):
    run = 0
    longest = 0
    for x in range(300, width - 330):
        if max(abs(a - b) for a, b in zip(pixels[x, y], want)) <= 2:
            run += 1
            longest = max(longest, run)
        else:
            run = 0
    if longest >= 30:
        rows += 1
        best = max(best, rows)
    else:
        rows = 0
sys.exit(0 if best >= 8 else 1)
PY
}
run() {
    local name=$1 view=$2
    rm -rf "$work/$name" && mkdir -p "$work/$name/vault" "$work/$name/config"
    echo '{"language": "en"}' > "$work/$name/config/settings.json"
    note > "$work/$name/vault/Where.md"
    cat > "$work/$name/script.focus" <<SCRIPT
wait 800
open Where.md
view $view
wait 500
left search
focus search
type needle
wait 900
key down
key down
key enter
wait 700
front
shot $name-1
focus search
key down
key enter
wait 700
front
shot $name-2
focus search
key down
key enter
wait 700
front
shot $name-3
focus search
key up
key up
key enter
wait 700
front
shot $name-4
key escape
wait 400
shot $name-clear
quit
SCRIPT
    FOCUS_CONFIG_DIR=$work/$name/config FOCUS_SHOT_DIR=$work/$name "$root/tools/shot.sh" \
        "$work/$name/script.focus" "$work/$name/vault" >"$work/$name/log" 2>&1
}
line() { grep '^front: ' "$work/$1/log" | sed -n "$2p" | sed 's/.* line //'; }

for view in live source reading; do
    run "$view" "$view"
    for n in 1 2 3 4; do
        check "$view: the match of result $n is drawn in inverse video" "boxed '$work/$view/$view-$n.png'"
    done
    check "$view: Escape drops the highlight" "! boxed '$work/$view/$view-clear.png'"
done
for view in live source; do
    check "$view: the first result puts the caret on its line" '[ "$(line '"$view"' 1)" = 7 ]'
    check "$view: the second result of the same note moves there" '[ "$(line '"$view"' 2)" = 87 ]'
    check "$view: the third result of the same note moves there" '[ "$(line '"$view"' 3)" = 167 ]'
    check "$view: going back to the first result moves back" '[ "$(line '"$view"' 4)" = 7 ]'
done
rm -rf "$work/palette" && mkdir -p "$work/palette/vault" "$work/palette/config"
echo '{"language": "en"}' > "$work/palette/config/settings.json"
note > "$work/palette/vault/Where.md"
cat > "$work/palette/script.focus" <<'SCRIPT'
wait 800
open Where.md
view live
wait 500
scroll 1
wait 300
palette /needle
wait 900
key enter
wait 700
front
shot palette-1
quit
SCRIPT
FOCUS_CONFIG_DIR=$work/palette/config FOCUS_SHOT_DIR=$work/palette "$root/tools/shot.sh" \
    "$work/palette/script.focus" "$work/palette/vault" >"$work/palette/log" 2>&1
check "palette: a full text result of the open note moves to its line" '[ "$(line palette 1)" = 7 ]'
check "palette: its match is drawn in inverse video" "boxed '$work/palette/palette-1.png'"
check "reading: each result of the same note scrolls the note" \
    '[ "$(line reading 1)" != "$(line reading 2)" ] && [ "$(line reading 2)" != "$(line reading 3)" ] && [ "$(line reading 4)" = "$(line reading 1)" ]'
exit $status
