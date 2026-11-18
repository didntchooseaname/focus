#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-read-properties"
rm -rf "${work:?}" && mkdir -p "$work"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }

clipboard=0
if command -v wl-copy >/dev/null && command -v wl-paste >/dev/null && [ -n "${WAYLAND_DISPLAY:-}" ]; then
    clipboard=1
    saved_type=$(wl-paste --list-types 2>/dev/null | head -n 1 || true)
    [ -n "$saved_type" ] && wl-paste --no-newline --type "$saved_type" > "$work/saved" 2>/dev/null || true
    restore() {
        if [ -n "$saved_type" ] && [ -s "$work/saved" ]; then wl-copy --type "$saved_type" < "$work/saved"; else wl-copy --clear; fi
    }
    trap restore EXIT
fi

run() {
    local name=$1
    mkdir -p "$work/$name/vault" "$work/$name/config"
    echo '{"language": "en"}' > "$work/$name/config/settings.json"
    printf -- '---\nsummary: One value\ncount: 42\n---\n# Props\n\nBody text.\n' > "$work/$name/vault/Props.md"
    printf '# Other\n\nText.\n' > "$work/$name/vault/Other.md"
    { cat; printf 'wait 1500\nquit\n'; } > "$work/$name/script.focus"
    FOCUS_CONFIG_DIR=$work/$name/config FOCUS_CACHE_DIR=$work/$name/cache FOCUS_DATA_DIR=$work/$name/data \
        FOCUS_SHOT_DIR=$work/$name "$root/tools/shot.sh" "$work/$name/script.focus" "$work/$name/vault" \
        > "$work/$name/log" 2>&1
}
line() { sed -n "${2}p" "$work/$1/vault/Props.md"; }
clean() { ! grep -q "focus: script, line" "$work/$1/log"; }

run undo <<'EOF'
open Props.md
view reading
wait 500
click One value|9
type , more
wait 1200
key ctrl-z
EOF
check "Read: Ctrl Z takes back a value typed" '[ "$(line undo 2)" = "summary: One value" ] && clean undo'

run redo <<'EOF'
open Props.md
view reading
wait 500
click One value|9
type , more
wait 1200
key ctrl-z
key ctrl-shift-z
EOF
check "Read: Ctrl Shift Z puts it back" '[ "$(line redo 2)" = "summary: One value, more" ] && clean redo'

if [ "$clipboard" = 1 ]; then
    printf ' two\nlines' | wl-copy
    run paste <<'EOF'
open Props.md
view reading
wait 500
click One value|9
key ctrl-v
wait 600
key enter
EOF
    check "Read: text pasted into a value, on one line" '[ "$(line paste 2)" = "summary: One value two lines" ] && clean paste'

    printf 'abc' | wl-copy
    run paste-refused <<'EOF'
open Props.md
view reading
wait 500
click #field 1
key ctrl-v
wait 600
key enter
EOF
    check "Read: letters pasted into a number are refused" '[ "$(line paste-refused 3)" = "count: 42" ] && clean paste-refused'
else
    echo "skip paste: needs wl-copy, wl-paste and a Wayland session"
fi

run slash-read <<'EOF'
open Props.md
view reading
wait 500
click One value|9
key space
type /
wait 300
key enter
EOF
check "Read: / in a value opens no slash menu" '[ "$(line slash-read 2)" = "summary: One value /" ] && clean slash-read'

run slash-live <<'EOF'
open Props.md
view live
wait 500
caret 2:18
key space
type /
wait 300
key enter
key escape
EOF
check "Live: / in a value opens no slash menu" '[ "$(line slash-live 2)" = "summary: One value /" ] && clean slash-live'

run link-read <<'EOF'
open Props.md
view reading
wait 500
click One value|9
key space
type [[Oth
wait 500
shot read-properties-link-read
key enter
wait 300
key enter
EOF
check "Read: [[ completes a link in a value" '[ "$(line link-read 2)" = "summary: One value [[Other]]" ] && clean link-read'

run link-live <<'EOF'
open Props.md
view live
wait 500
caret 2:18
key space
type [[Oth
wait 500
shot read-properties-link-live
key enter
wait 300
key escape
EOF
check "Live: [[ completes a link in a value" '[ "$(line link-live 2)" = "summary: One value [[Other]]" ] && clean link-live'

check "the body is untouched" '[ "$(tail -n 1 "$work/link-read/vault/Props.md")" = "Body text." ]'
exit $status
