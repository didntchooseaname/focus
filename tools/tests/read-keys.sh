#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-read-keys"
rm -rf "${work:?}" && mkdir -p "$work/vault" "$work/config"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data FOCUS_CONFIG_DIR=$work/config
echo '{"language": "en"}' > "$work/config/settings.json"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
cat > "$work/vault/Keys.md" <<'NOTE'
# Keys

See [[First]] and [[Second]].

## Tasks

- [ ] Ship the report

> [!note]- Folded
> Hidden text.
NOTE
printf '# Live\n\nSee [[First]] here.\n\n- [ ] Ship the report\n' > "$work/vault/Live.md"
printf '# First\n' > "$work/vault/First.md"
printf '# Second\n' > "$work/vault/Second.md"
cat > "$work/keys.focus" <<'SCRIPT'
wait 500
open Keys.md
view reading
wait 300
key tab
key enter
wait 400
front
open Keys.md
view reading
wait 300
key tab
key escape
key tab
key enter
wait 400
front
open Keys.md
view reading
wait 300
key tab
key tab
key tab
key tab
key space
wait 400
open Live.md
view live
wait 300
caret 3:7
key alt-enter
wait 400
front
open Live.md
view live
wait 300
caret 5:8
key ctrl-enter
wait 600
quit
SCRIPT
"$root/tools/shot.sh" "$work/keys.focus" "$work/vault" > "$work/keys.log" 2>&1 || status=1
fronts=$(grep '^front: ' "$work/keys.log")
check "Tab then Enter in Read follows the first link" '[ "$(echo "$fronts" | sed -n 1p | cut -d" " -f2)" = First.md ]'
check "Escape leaves the element, the next Tab starts at the first link again" '[ "$(echo "$fronts" | sed -n 2p | cut -d" " -f2)" = First.md ]'
check "Space on the focused task box in Read checks it" 'grep -q "^- \[x\] Ship the report" "$work/vault/Keys.md"'
check "Ctrl Enter at the caret in Live checks the task box of its line" 'grep -q "^- \[x\] Ship the report" "$work/vault/Live.md"'
check "Alt Enter in Live follows the link at the caret" '[ "$(echo "$fronts" | sed -n 3p | cut -d" " -f2)" = First.md ]'
exit $status
