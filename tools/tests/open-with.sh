#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-open-with"
bin=${FOCUS_BIN:-$root/target/debug/focus}
rm -rf "$work" && mkdir -p "$work/outside" "$work/tools"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
fresh() {
    rm -rf "$work/vault" "$work/config" && cp -r "$root/fixtures/demo-vault" "$work/vault"
    mkdir -p "$work/config" && echo '{"language": "en"}' > "$work/config/settings.json"
}
run() {
    local name=$1
    cat > "$work/$name.focus"
    FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/$name.focus" "$work/vault" >"$work/$name.log" 2>&1
}

fresh
printf '# Loose\n\nFirst line.\n' > "$work/outside/Loose.md"
run loose <<SCRIPT
wait 300
openpath $work/outside/Loose.md
openpath $work/vault/Home.md
openpath $work/outside/Loose.md
view source
focus editor
key ctrl-end
type Edited outside.
key ctrl-s
wait 1500
tabs
shot loose-note
quit
SCRIPT
check "a loose note is saved in place" 'grep -q "Edited outside." "$work/outside/Loose.md"'
check "its folder is not copied into the vault" '! find "$work/vault" -name "Loose.md" | grep -q .'
check "a note of the vault opens in its own tab, the loose one once" 'grep -q "^tabs: .*Loose | Home$" "$work/loose.log"'
check "the session keeps no loose tab" '! grep -q "outside/Loose.md" "$work/config/settings.json"'

printf 'evidence one\n' > "$work/outside/evidence-1.txt"
printf 'evidence two\n' > "$work/outside/evidence-2.txt"
cat > "$work/tools/wl-copy" <<EOF
#!/bin/sh
cat > "$work/tools/copied"
EOF
cat > "$work/tools/wl-paste" <<EOF
#!/bin/sh
case "\$*" in
    *--list-types*) printf 'text/uri-list\nx-special/gnome-copied-files\ntext/plain;charset=utf-8\n' ;;
    *text/uri-list*) printf 'file://$work/outside/evidence-1.txt\r\nfile://$work/outside/evidence-2.txt\r\nsftp://host/x\r\n' ;;
    *gnome-copied-files*) printf 'copy\nfile://$work/outside/evidence-1.txt\nfile://$work/outside/evidence-2.txt\nsftp://host/x' ;;
    *) printf '$work/outside/evidence-1.txt\n$work/outside/evidence-2.txt\n' ;;
esac
EOF
chmod +x "$work/tools/wl-copy" "$work/tools/wl-paste"
fresh
PATH="$work/tools:$PATH" run paste <<'SCRIPT'
wait 300
open Home.md
view source
focus editor
key ctrl-end
key enter
key ctrl-v
wait 2000
shot paste-files
quit
SCRIPT
check "copied files are attached to the note" '[ "$(grep -c "^!\[\[.*\.txt|" "$work/vault/Home.md")" -eq 2 ]'
check "and saved in the vault" '[ "$(find "$work/vault" -name "*.txt" -newer "$work/paste.focus" | xargs -r cat | grep -c "^evidence ")" -eq 2 ]'
check "the paths are not pasted as text" '! grep -q "$work/outside" "$work/vault/Home.md"'

fresh
cat > "$work/second.focus" <<'SCRIPT'
wait 300
listen
wait 6000
tabs
shot handover
quit
SCRIPT
FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/second.focus" "$work/vault" >"$work/second.log" 2>&1 &
first=$!
for _ in $(seq 100); do grep -q "^listening " "$work/second.log" 2>/dev/null && break; sleep 0.1; done
socket=$(sed -n "s/^listening //p" "$work/second.log")
rights=$(stat -c %a "$socket" 2>/dev/null)
started=$(date +%s%N)
note=$(ls "$work/vault/Method" | grep '\.md$' | head -n 1)
FOCUS_CONFIG_DIR=$work/config FOCUS_CAPTURE_CLASS=focus-capture timeout 10 "$bin" "$work/vault/Method/$note" >"$work/second-launch.log" 2>&1
handed=$?
elapsed=$(( ($(date +%s%N) - started) / 1000000 ))
wait "$first"
check "the second launch hands over and exits ($elapsed ms)" '[ "$handed" -eq 0 ] && [ "$elapsed" -lt 2000 ]'
check "the first window opens the note" 'grep -qx "tabs: ${note%.md}" "$work/second.log"'
check "the socket is 0600" '[ "$rights" = 600 ]'
exit $status
