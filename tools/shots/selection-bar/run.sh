#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
vault="$root/target/test-vault/selection-bar"
for lang in en ar; do
    rm -rf "${vault:?}" && mkdir -p "$vault/.obsidian"
    echo '{}' > "$vault/.obsidian/app.json"
    cat > "$vault/Notes.md" <<'NOTE'
# Field notes

The scan found three hosts with plain services and one web server.

Credentials were reused across the two domain controllers.

- Enumerate the shares
- Check the web server
  - Look for the admin panel
- Write the report

## Next steps

Rotate the keys before the second pass.
NOTE
    for n in $(seq 1 30); do printf '\nLine %s of the closing notes.\n' "$n" >> "$vault/Notes.md"; done
    sed "s/^lang .*/lang $lang/; s/@LANG@/$lang/g" "$root/tools/shots/selection-bar/selection-bar.focus" \
        > "$vault/script.focus"
    "$root/tools/shot.sh" "$vault/script.focus" "$vault"
done
