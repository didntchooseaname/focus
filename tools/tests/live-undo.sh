#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-live-undo"
vault="$work/vault"
note="Rendu/Tout le Markdown.md"
mark=LIVEMARK
rm -rf "$work" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$vault"
cat > "$work/live-undo.focus" <<SCRIPT
set editor.edit_in_place off
open $note
view live
wait 300
block 10
key end
type $mark
key escape
wait 4000
key ctrl-z
key ctrl-z
key ctrl-z
key ctrl-z
key ctrl-z
key ctrl-z
key ctrl-z
key ctrl-z
key escape
wait 2000
quit
SCRIPT
FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/live-undo.focus" "$vault" >"$work/run.log" 2>&1 &
app=$!
written=no
for _ in $(seq 1 600); do
    if grep -q "$mark" "$vault/$note"; then written=yes; break; fi
    sleep 0.1
done
wait $app || true
status=0
if [ $written = yes ]; then echo "ok   typed in a block, on disk after the autosave"; else echo "FAIL the text typed in the block never reached the file"; status=1; fi
if cmp -s "$root/fixtures/vault/$note" "$vault/$note"; then
    echo "ok   Ctrl Z after closing the block restored the file"
else
    echo "FAIL the file differs from the original after Ctrl Z:"
    diff "$root/fixtures/vault/$note" "$vault/$note" || true
    status=1
fi

end_mark=LIVEEND
rm -rf "$vault" && cp -r "$root/fixtures/vault" "$vault"
cat > "$work/live-end.focus" <<SCRIPT
set editor.edit_in_place off
open $note
view live
wait 300
block end
wait 300
type $end_mark
key escape
wait 1500
quit
SCRIPT
FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/live-end.focus" "$vault" >"$work/run-end.log" 2>&1 || true
if [ "$(tail -n 1 "$vault/$note")" = "$end_mark" ]; then
    echo "ok   typed in a paragraph opened under the last block"
else
    echo "FAIL the paragraph under the last block lost its text:"
    tail -n 3 "$vault/$note"
    status=1
fi
exit $status
