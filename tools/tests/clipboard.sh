#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-clipboard"
command -v wl-copy >/dev/null && command -v wl-paste >/dev/null && [ -n "${WAYLAND_DISPLAY:-}" ] || { echo "skip: needs wl-copy, wl-paste and a Wayland session"; exit 0; }
rm -rf "$work" && mkdir -p "$work"

saved_type=$(wl-paste --list-types 2>/dev/null | head -n 1 || true)
[ -n "$saved_type" ] && wl-paste --no-newline --type "$saved_type" > "$work/saved" 2>/dev/null || true
restore() {
    if [ -n "$saved_type" ] && [ -s "$work/saved" ]; then wl-copy --type "$saved_type" < "$work/saved"; else wl-copy --clear; fi
}
trap restore EXIT

status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
image="$root/fixtures/vault/Red Team/assets/capture-exemple.png"

run() {
    local name=$1
    rm -rf "$work/vault" && cp -r "$root/fixtures/vault" "$work/vault"
    cat > "$work/$name.focus"
    FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/$name.focus" "$work/vault" >"$work/$name.log" 2>&1 || true
}

wl-copy --type image/png < "$image"
run image <<'SCRIPT'
open 00 Accueil.md
view source
focus editor
key ctrl-end
key enter
key ctrl-v
wait 2000
quit
SCRIPT
check "an image pasted into a note is attached and linked" 'grep -q "capture\|\.png" "$work/vault/00 Accueil.md" && [ "$(find "$work/vault" -name "*.png" | wc -l)" -gt 1 ]'

printf 'FOCUS-CLIPBOARD-TEXT é' | wl-copy --type 'text/plain;charset=utf-8'
run text <<'SCRIPT'
open 00 Accueil.md
view live
wait 300
block 12
key end
key ctrl-v
wait 2000
key escape
wait 1500
quit
SCRIPT
check "text pasted into a Live block reaches the note" 'grep -q "FOCUS-CLIPBOARD-TEXT é" "$work/vault/00 Accueil.md"'

wl-copy --clear
run copy <<'SCRIPT'
open 00 Accueil.md
palette >Copy the note as Markdown
wait 300
key enter
wait 1500
quit
SCRIPT
check "a note copied as Markdown is on the system clipboard" 'wl-paste --no-newline 2>/dev/null | grep -q "Coffre d.essai de Focus"'

wl-copy --type image/png < "$image"
run evidence <<'SCRIPT'
ops on
wait 300
engagement Demo
wait 300
open 00 Accueil.md
view source
evidence
wait 2000
quit
SCRIPT
check "a screenshot pasted as Ops evidence lands in Evidence/" '[ -n "$(find "$work/vault/Engagements/Demo/Evidence" -name "*.png" 2>/dev/null)" ]'
exit $status
