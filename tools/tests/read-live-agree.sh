#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-read-live-agree"
rm -rf "${work:?}" && mkdir -p "$work/vault/Folder" "$work/shots"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data FOCUS_SHOT_DIR=$work/shots
cat > "$work/vault/Folder/Agree.md" <<'NOTE'
---
status: draft
tags: [audit]
---
# Agree

The first paragraph of the note, long enough to show where the text column starts.

## Section

The second paragraph sits under a rule drawn twelve pixels above the heading.
NOTE
status=0
for scale in 100 120; do
    script=$work/agree-$scale.focus
    {
        echo "wait 600"
        echo "set appearance.ui_scale $scale"
        echo "open Folder/Agree.md"
        layout() {
            local name=$1
            for view in reading live source; do
                printf 'view %s\nwait 400\nshot %s-%s\n' "$view" "$name" "$view"
            done
        }
        layout both
        echo "left hidden"
        layout noleft
        echo "right hidden"
        layout none
        echo "left files"
        layout noright
        echo "right outline"
        echo "split"
        layout split
        echo "quit"
    } > "$script"
    for size in 1 sm; do
        config=$work/config-$scale-$size
        mkdir -p "$config" && echo '{"language": "en"}' > "$config/settings.json"
        FOCUS_CAPTURE=$size FOCUS_CONFIG_DIR=$config FOCUS_SHOT_DIR=$work/shots/$scale/$size \
            "$root/tools/shot.sh" "$script" "$work/vault" > "$work/agree-$scale-$size.log" 2>&1 || status=1
    done
done
python3 -I "$root/tools/tests/read-live-agree.py" "$work/shots" || status=1
exit $status
