#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-files-outside"
bin=${FOCUS_BIN:-$root/target/debug/focus}
rm -rf "$work" && mkdir -p "$work/outside/img" "$work/config"
cp -r "$root/fixtures/demo-vault" "$work/vault"
echo '{"language": "en"}' > "$work/config/settings.json"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
script() { cat > "$work/$1.focus"; }
shoot() {
    FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/$1.focus" "$work/vault" >"$work/$1.log" 2>&1
}

cp "$root/fixtures/demo-vault/assets/scope-map.png" "$work/outside/img/scope-map.png"
cat > "$work/outside/Loose.md" <<'NOTE'
# Loose

![Map](img/scope-map.png)

See [the other note](Sibling.md) and [[Home]] of the vault.

![[Sibling]]
NOTE
printf '# Sibling\n\nText of the sibling note.\n' > "$work/outside/Sibling.md"
script loose <<SCRIPT
wait 300
openpath $work/outside/Loose.md
view reading
wait 500
shot loose-links
wait 3000
view source
focus editor
key ctrl-end
type Typed after.
wait 1500
view reading
click the other note
wait 500
tabs
shot loose-sibling
quit
SCRIPT
shoot loose &
pid=$!
for _ in $(seq 300); do [ -f "$work/loose-links.png" ] && break; sleep 0.1; done
sleep 0.5
printf 'Changed outside.\n' >> "$work/outside/Loose.md"
wait "$pid"
check "a loose note follows a change made outside, then saves on it" 'grep -q "Changed outside." "$work/outside/Loose.md" && grep -q "Typed after." "$work/outside/Loose.md"'
check "a relative link to a note beside it opens that note loose" 'grep -qx "tabs: Loose | Sibling" "$work/loose.log"'
check "nothing of the outside folder enters the vault" '! find "$work/vault" -name "Sibling.md" | grep -q .'

mkdir -p "$work/vault2/.obsidian"
printf '# Other\n\nA note of the second vault.\n' > "$work/vault2/Other.md"
script switch <<'SCRIPT'
wait 300
listen
set editor.autosave 5000
open Home.md
view source
focus editor
key ctrl-end
type  Pending line.
wait 4000
shot switch-ask
key escape
tabs
focus editor
type  More.
wait 2500
shot switch-again
key enter
wait 2000
tabs
shot switch-done
quit
SCRIPT
shoot switch &
first=$!
for _ in $(seq 6500); do grep -q "^listening " "$work/switch.log" 2>/dev/null && break; sleep 0.1; done
if ! grep -q "^listening " "$work/switch.log" 2>/dev/null; then
    echo "FAIL the first window listens"
    kill "$first" 2>/dev/null
    wait "$first" 2>/dev/null
    exit 1
fi
second() {
    FOCUS_CONFIG_DIR=$work/config FOCUS_CAPTURE_CLASS=focus-capture timeout 10 "$bin" "$work/vault2/Other.md" >>"$work/switch-launch.log" 2>&1
}
sleep 2; second
for _ in $(seq 100); do grep -q "^tabs: Home" "$work/switch.log" 2>/dev/null && break; sleep 0.1; done
sleep 1; second
wait "$first"
check "the refused switch keeps the open vault" 'grep -qx "tabs: Home" "$work/switch.log"'
check "the accepted switch opens the other vault and its note" 'grep -qx "tabs: Other" "$work/switch.log"'
check "unsaved text is written before the switch" 'grep -q "Pending line.More." "$work/vault/Home.md"'

python3 - "$work/many" <<'PY'
import os, sys
base = sys.argv[1]
for d in range(40):
    folder = os.path.join(base, f"d{d:02}")
    os.makedirs(folder)
    for f in range(500):
        with open(os.path.join(folder, f"f{f:03}.txt"), "wb") as out:
            out.write(b"x" * 4096)
PY
script copy-progress <<SCRIPT
wait 300
import $work/many
shot copy-running
quit
SCRIPT
shoot copy-progress
rm -rf "$work/vault/many"
script copy <<SCRIPT
wait 300
import $work/many
wait 150
import cancel
wait 1500
shot copy-cancelled
quit
SCRIPT
shoot copy
copied=$(find "$work/vault/many" -type f 2>/dev/null | wc -l)
check "a cancelled copy stops before the end ($copied of 20000 files)" '[ "$copied" -lt 20000 ]'
grep -h "focus:" "$work"/*.log | grep -v "listening" && status=1
exit $status
