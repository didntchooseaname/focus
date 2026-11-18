#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
vault=$root/target/test-inplace-idle
rm -rf "${vault:?}" && mkdir -p "$vault/.obsidian"
echo '{}' >"$vault/.obsidian/app.json"
cat >"$vault/Idle.md" <<'NOTE'
---
status: draft
---
# Idle

| Name | Value |
|---|---|
| alpha | 1 |

- first item
- [ ] a task

> [!note] Title
> Body of the callout.
NOTE
out=$("$root/tools/tests/idle-frames.sh" "$vault" 'set editor.edit_in_place on\nopen Idle.md\nview live' | tail -1)
echo "$out"
case "$out" in
"10 s at rest: 0 wl_surface.commit,"*) echo "ok   no frame at rest in Live without a caret" ;;
*) echo "FAIL frames at rest in Live without a caret"; exit 1 ;;
esac
