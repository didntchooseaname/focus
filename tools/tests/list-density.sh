#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-list-density"
rm -rf "${work:?}" && mkdir -p "$work/config"
cp -r "$root/fixtures/demo-vault" "$work/vault"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
echo '{"language": "en"}' > "$work/config/settings.json"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
cat > "$work/density.focus" <<'SCRIPT'
wait 800
open Method/Evidence handling.md
view live
left files
expand Method
right outline
bottom tasks
wait 500
shot density-normal-tree
set appearance.density 1
wait 500
shot density-compact-tree
left tags
wait 300
shot density-compact-tags
left bookmarks
wait 300
shot density-compact-bookmarks
search evidence
wait 800
shot density-compact-search
lang fr
left files
wait 300
shot density-compact-tree-fr
set appearance.density 0
wait 500
shot density-normal-tree-fr
lang en
left tags
wait 300
shot density-normal-tags
left bookmarks
wait 300
shot density-normal-bookmarks
search evidence
wait 800
shot density-normal-search
quit
SCRIPT
FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/density.focus" "$work/vault" >"$work/density.log" 2>&1
active_row() {
    python3 - "$1" <<'PY'
import sys
from PIL import Image
image = Image.open(sys.argv[1]).convert("RGB")
selected = (0x20, 0x20, 0x22)
heights = []
run = 0
for y in range(80, image.height - 30):
    if image.getpixel((200, y)) == selected:
        run += 1
    elif run:
        heights.append(run)
        run = 0
print(max(heights) if heights else 0)
PY
}
check "a tree row is 26 px high in Normal" '[ "$(active_row "$work/density-normal-tree.png")" = 26 ]'
check "a tree row is 22 px high in Compact" '[ "$(active_row "$work/density-compact-tree.png")" = 22 ]'
check "the setting applies in French too" '[ "$(active_row "$work/density-compact-tree-fr.png")" = 22 ] && [ "$(active_row "$work/density-normal-tree-fr.png")" = 26 ]'
exit $status
