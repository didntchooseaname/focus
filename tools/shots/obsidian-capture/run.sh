#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
work=$root/target/shots-obsidian-capture
out=${FOCUS_SHOT_DIR:-$root/shots}
status=0
for lang in en ar; do
    base=$work/$lang
    rm -rf "${base:?}" && mkdir -p "$base/Work/Projects" "$base/Personal" "$base/config"
    cat > "$base/Work/Home.md" <<'NOTE'
# Home

[Open the plan](obsidian://open?vault=Work&file=Projects%2FPlan%23Tasks) of this week.
NOTE
    printf '# Plan\n\nGoals.\n\n## Tasks\n\n- write the report\n' > "$base/Work/Projects/Plan.md"
    printf '# Inbox\n\n- earlier\n' > "$base/Work/Inbox.md"
    printf '# Journal\n' > "$base/Personal/Journal.md"
    cat > "$base/config/settings.json" <<JSON
{"language": "en", "last_vault": "$base/Work",
 "recent_vaults": ["$base/Work", "$base/Personal", "$base/Archive"]}
JSON
    cat > "$base/run.focus" <<SCRIPT
wait 300
lang $lang
open Home.md
view reading
wait 300
click Open the plan|2
wait 500
tabs
shot obsidian-open-$lang
address obsidian://new?vault=Work&file=Inbox&append&content=Call%20Sam%20back
wait 300
shot obsidian-confirm-$lang
key enter
wait 500
address obsidian://new?vault=Work&file=Inbox&overwrite&content=Replaced
wait 300
shot obsidian-overwrite-$lang
key escape
address obsidian://open?vault=Personal&file=Journal
wait 300
shot obsidian-other-vault-$lang
action capture
type Buy milk
wait 200
shot capture-dialog-$lang
key enter
wait 500
settings general
wait 300
shot settings-recent-$lang
press recent:2
wait 1500
tabs
shot vault-switched-$lang
quit
SCRIPT
    FOCUS_CONFIG_DIR=$base/config FOCUS_CACHE_DIR=$base/cache FOCUS_DATA_DIR=$base/data \
        FOCUS_SHOT_DIR=$out "$root/tools/shot.sh" "$base/run.focus" "$base/Work" >"$base/run.log" 2>&1 || status=1
    grep -E "^tabs:|error|failed" "$base/run.log" || true
    tail -n 1 "$base/Work/Inbox.md" | grep -q "^Call Sam back$" || { echo "FAIL $lang: the confirmed write is not in Inbox.md"; status=1; }
    grep -q "^Replaced" "$base/Work/Inbox.md" && { echo "FAIL $lang: the cancelled overwrite was written"; status=1; }
    tail -n 1 "$base/Work/$(date +%Y-%m-%d).md" 2>/dev/null | grep -q " Buy milk$" || { echo "FAIL $lang: the capture is not in today's note"; status=1; }
done
exit $status
