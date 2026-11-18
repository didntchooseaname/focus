#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-quick-capture"
bin=${FOCUS_BIN:-$root/target/debug/focus}
rm -rf "${work:?}" && mkdir -p "$work/run" "$work/vault/Templates" "$work/vault/Daily"
chmod 700 "$work/run"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
vault=$work/vault
printf '# Inbox\n\n- earlier\n' > "$vault/Inbox.md"
printf '# {{title}}\n\n## Log\n' > "$vault/Templates/Day.md"
printf '# Home\n' > "$vault/Home.md"
settings() {
    mkdir -p "$1"
    cat > "$1/settings.json" <<JSON
{"language": "en", "last_vault": "$vault", "recent_vaults": ["$vault"],
 "ui": {"settings": {"general.capture_note": "$2", "general.daily_folder": "Daily",
  "general.daily_template": "Templates/Day", "general.capture_format": "- {{time}} {{text}}"}}}
JSON
}
capture() {
    local config=$1
    shift
    FOCUS_CONFIG_DIR=$config XDG_RUNTIME_DIR=${RUNTIME:-$work/run} FOCUS_CAPTURE_CLASS=focus-capture \
        timeout 20 "$bin" --capture "$@"
}

settings "$work/config-note" Inbox
capture "$work/config-note" "First line" 2>"$work/first.err"
code=$?
check "without a window the line is written and the code is 0 ($code)" '[ "$code" -eq 0 ]'
check "the line ends the note, in the form of the setting" 'tail -n 1 "$vault/Inbox.md" | grep -qx -- "- [0-9][0-9]:[0-9][0-9] First line"'
check "the earlier text is kept" 'grep -qx -- "- earlier" "$vault/Inbox.md"'
check "one line on stderr says where" '[ "$(wc -l < "$work/first.err")" -eq 1 ] && grep -q "line added to $vault/Inbox.md" "$work/first.err"'
check "a version of the note is kept" '[ -n "$(find "$work/data/history" -type f 2>/dev/null | head -n 1)" ]'

printf 'From the standard input\n' | capture "$work/config-note" - 2>"$work/stdin.err"
code=$?
check "the text is read from the standard input with - ($code)" '[ "$code" -eq 0 ] && tail -n 1 "$vault/Inbox.md" | grep -q " From the standard input$"'

printf '\n' | capture "$work/config-note" - 2>"$work/empty.err"
code=$?
check "an empty text writes nothing and exits with 2 ($code)" '[ "$code" -eq 2 ] && grep -q "empty" "$work/empty.err"'

mkdir -p "$work/config-none" && echo '{"language": "en"}' > "$work/config-none/settings.json"
capture "$work/config-none" "Nowhere" 2>"$work/none.err"
code=$?
check "without a vault remembered it exits with 2 ($code)" '[ "$code" -eq 2 ]'

settings "$work/config-daily" ""
today=$(date +%Y-%m-%d)
capture "$work/config-daily" "Standup" 2>"$work/daily.err"
code=$?
check "an empty capture note setting writes today's note ($code)" '[ "$code" -eq 0 ] && [ -f "$vault/Daily/$today.md" ]'
check "the daily note is created from its template" '[ "$(head -n 1 "$vault/Daily/$today.md")" = "# $today" ] && grep -qx "## Log" "$vault/Daily/$today.md"'
check "and the line ends it" 'tail -n 1 "$vault/Daily/$today.md" | grep -q " Standup$"'

cat > "$work/window.focus" <<'SCRIPT'
wait 300
open Inbox.md
listen
wait 4000
tabs
shot capture-window
quit
SCRIPT
FOCUS_CONFIG_DIR=$work/config-note FOCUS_SHOT_DIR=$work \
    "$root/tools/shot.sh" "$work/window.focus" "$vault" >"$work/window.log" 2>&1 &
first=$!
for _ in $(seq 300); do grep -q "^listening " "$work/window.log" 2>/dev/null && break; sleep 0.1; done
started=$(date +%s%N)
RUNTIME=$XDG_RUNTIME_DIR capture "$work/config-note" "Through the window" 2>"$work/window.err"
code=$?
elapsed=$(( ($(date +%s%N) - started) / 1000000 ))
wait "$first"
check "the window writes the line and the code is 0 ($code, $elapsed ms)" '[ "$code" -eq 0 ] && [ "$elapsed" -lt 5000 ]'
check "the window answered where" 'grep -q "line added to $vault/Inbox.md" "$work/window.err"'
check "the line is on disk" 'tail -n 1 "$vault/Inbox.md" | grep -q " Through the window$"'
check "the window shows the note it had open" 'grep -qx "tabs: Inbox" "$work/window.log"'
exit $status
