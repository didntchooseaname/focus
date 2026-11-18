#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
vault="$root/target/test-vault/properties"
for name in properties properties-ar properties-off; do
    rm -rf "${vault:?}" && mkdir -p "$vault/.obsidian"
    cat > "$vault/.obsidian/types.json" <<'JSON'
{"types":{"aliases":"aliases","tags":"tags","deadline":"date","score":"number","reviewed":"checkbox","when":"date","count":"number","stamp":"datetime","people":"multitext","flag":"checkbox"}}
JSON
    cat > "$vault/Properties.md" <<'NOTE'
---
reviewed: true
draft: false
year: 1977
score: 3.14
due: 2020-08-21
updated: 2020-08-21T10:30:00
cast:
  - Mark Hamill
  - Harrison Ford
aliases:
  - Types
summary: One property of each type
deadline: 21/08/2020
when:
count:
stamp:
note:
people:
flag:
tags:
  - journal
---
# Property types

Each property above is drawn by its type.
NOTE
    cat > "$vault/Tags.md" <<'NOTE'
---
tags:
status: draft
---
# Empty tags

The note has a `tags` property and no tag yet.
NOTE
    "$root/tools/shot.sh" "$root/tools/shots/properties/$name.focus" "$vault"
    grep -q '^due: 2020-08-21$' "$vault/Properties.md"
    grep -q '^deadline: 21/08/2020$' "$vault/Properties.md"
    for empty in stamp note people flag; do
        grep -q "^$empty:\$" "$vault/Properties.md"
    done
    if [ "$name" = properties-off ]; then
        grep -q '^draft: true$' "$vault/Properties.md"
        grep -q '^when:$' "$vault/Properties.md"
        grep -q '^count: 7$' "$vault/Properties.md"
        grep -q '^tags:$' "$vault/Tags.md"
    else
        grep -q '^draft: false$' "$vault/Properties.md"
        grep -q '^when: 2021-03-04$' "$vault/Properties.md"
        grep -q '^summary: One property of each type, edited in Read$' "$vault/Properties.md"
        grep -q '^count: 42$' "$vault/Properties.md"
        printf -- '---\ntags:\n  - recipe\n  - cooking\nstatus: draft\n---\n' > "$vault/expected"
        head -n 6 "$vault/Tags.md" | cmp - "$vault/expected"
        tail -n +7 "$vault/Tags.md" | grep -q '^# Empty tags$'
    fi
    grep -q '^Each property above is drawn by its type.$' "$vault/Properties.md"
    grep -q '^The note has a `tags` property and no tag yet.$' "$vault/Tags.md"
done
