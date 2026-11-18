#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-template-insert"
rm -rf "${work:?}" && mkdir -p "$work/vault/Templates" "$work/config"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
echo '{"language": "en"}' > "$work/config/settings.json"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
printf -- '---\ntags:\n  - work\n---\n# Kickoff\n\nNotes.\n' > "$work/vault/Kickoff.md"
printf -- '---\ntags:\n  - meeting\nattendees:\n---\nMeeting {{title}} on {{date:YYYY}}\n' > "$work/vault/Templates/Meeting.md"
cat > "$work/insert.focus" <<'SCRIPT'
wait 300
open Kickoff.md
view source
focus editor
key ctrl-end
type /
type template
wait 200
shot template-command
key enter
wait 200
type Meet
wait 200
shot template-list
key enter
wait 200
key ctrl-s
wait 1000
quit
SCRIPT
FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/insert.focus" "$work/vault" >"$work/insert.log" 2>&1
note=$work/vault/Kickoff.md
year=$(date +%Y)
check "the body is inserted at the caret with its variables" 'grep -qx "Meeting Kickoff on $year" "$note" && [ "$(tail -n 1 "$note")" = "Meeting Kickoff on $year" ]'
check "the slash query is gone" '! grep -q "/template\|/Meet" "$note"'
check "the template tag joins the note's" 'grep -qx "  - work" "$note" && grep -qx "  - meeting" "$note"'
check "a property the note lacked is added" 'grep -q "^attendees:" "$note"'
check "the template's frontmatter is not inserted in the body" '[ "$(grep -c "^---$" "$note")" -eq 2 ]'
exit $status
