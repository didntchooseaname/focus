#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$root/target/test-live-typing
bin=${FOCUS_BIN:-$root/target/debug/focus}
[ -x "$bin" ] || { echo "FAIL live-typing-visible: $bin is not built"; exit 1; }

rm -rf "${work:?}" && mkdir -p "$work"
python3 -I - "$root/tools/bench/interaction.py" "$work/base" <<'PY'
import importlib.util, sys
from pathlib import Path
spec = importlib.util.spec_from_file_location("interaction", sys.argv[1])
ia = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ia)
root = Path(sys.argv[2])
(root / ".obsidian").mkdir(parents=True)
(root / ".obsidian/app.json").write_text("{}")
(root / "Typing").mkdir()
blocks = """---
title: Typing test
tags:
  - bench
---

# Typing test

Paragraph one with some words, a [[Other]] link and **bold** text to read.

## Heading two

- item one with words
- item two with words

```bash
echo hello world
```

> [!note] Callout title
> Callout body text with words.

| Name | Value |
|---|---|
| row | 7 |

Last paragraph of the block list.
"""
(root / "Typing/Blocks.md").write_text(blocks)
(root / "Typing/Other.md").write_text("# Other\n\nText.\n")
body = ia.note_text(10000).split("\n", 6)[6]
(root / "Typing/Long.md").write_text(blocks + "\n" + body)
PY

cases=()
for note in Blocks Long; do
    for kind in paragraph:9 heading:11 list:13 code:17 callout:21 table:25; do
        cases+=("live-${note,,}-${kind%%:*}|live|$note|${kind##*:}")
    done
    cases+=("edit-${note,,}|edit|$note|")
done

want=("$@")
selected() { [ ${#want[@]} -eq 0 ] && return 0; local w; for w in "${want[@]}"; do [ "$w" = "$1" ] && return 0; done; return 1; }

failed=0
for entry in "${cases[@]}"; do
    IFS='|' read -r name view note line <<<"$entry"
    selected "$name" || continue
    dir=$work/$name
    mkdir -p "$dir"
    rm -rf "${work:?}/vault" && cp -r "$work/base" "$work/vault"
    {
        echo "set editor.edit_in_place off"
        echo "open Typing/$note.md"
        echo "wait 800"
        if [ "$view" = live ]; then
            echo "view live"; echo "wait 500"; echo "block $line"
        else
            echo "view source"; echo "wait 500"; echo "focus editor"
        fi
        echo "wait 600"; echo "key end"; echo "wait 300"; echo "shot ref"
        for i in $(seq 1 48); do
            echo "type x"; echo "wait 40"
            [ $((i % 4)) = 0 ] && echo "snap k$i"
        done
        echo "wait 2500"; echo "shot after"; echo "quit"
    } >"$dir/script.focus"
    FOCUS_BIN="$bin" FOCUS_SHOT_DIR="$dir" "$root/tools/shot.sh" "$dir/script.focus" "$work/vault" >"$dir/log.txt" 2>&1
    code=$?
    if [ $code -ne 0 ] || [ ! -f "$dir/after.png" ]; then
        echo "FAIL $name: the run ended with status $code, see $dir/log.txt"
        failed=1
        continue
    fi
    snaps=$(ls "$dir"/k*.png 2>/dev/null | sort -V)
    if [ -z "$snaps" ]; then
        echo "FAIL $name: no capture taken while typing"
        failed=1
        continue
    fi
    if out=$(python3 -I "$root/tools/tests/live-typing-visible.py" "$view" "$dir/ref.png" $snaps "$dir/after.png"); then
        echo "ok   $name: $(echo "$out" | wc -l) captures"
    else
        echo "FAIL $name: $(echo "$out" | grep -c '^FAIL') of $(echo "$out" | wc -l) captures"
        echo "$out" | grep '^FAIL' | sed 's/^/     /'
        failed=1
    fi
done
exit $failed
