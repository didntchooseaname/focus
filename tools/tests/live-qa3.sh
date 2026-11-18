#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-live-qa3"
vault="$work/vault"
note="Rendu/Tout le Markdown.md"
status=0

fresh() {
    rm -rf "$vault" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$vault"
    printf '# CRLF\r\n\r\nUn paragraphe.\r\n' > "$vault/CRLF.md"
}

run() {
    local name=$1
    { echo "set editor.edit_in_place off"; cat; } > "$work/$name.focus"
    FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/$name.focus" "$vault" >"$work/$name.log" 2>&1 || true
}

verdict() {
    if "$@"; then echo "ok   $label"; else echo "FAIL $label"; status=1; fi
}

fresh
run up <<SCRIPT
open $note
view live
wait 300
goto Images et inclusions
wait 300
block 106
wait 300
key up
key up
wait 300
type UP1
wait 1500
quit
SCRIPT
label="M1 Up into the table above the window takes the keystrokes"
verdict grep -q "UP1" "$vault/$note"

fresh
run header <<SCRIPT
open $note
view live
wait 300
block 1
wait 300
key down
key end
type  zz
key escape
block 8
wait 300
key end
type  T
key escape
wait 1500
quit
SCRIPT
label="M3 the properties open and take the keystrokes"
verdict grep -q "^type: referencezz" "$vault/$note"
label="M3 the title opens and takes the keystrokes"
verdict grep -q "^# Tout le MarkdownT$" "$vault/$note"

fresh
run crlf <<'SCRIPT'
open CRLF.md
view live
wait 300
block 3
wait 300
key end
key enter
type Nouvelle
key escape
block end
wait 300
type Fin.
key escape
wait 1500
quit
SCRIPT
label="M5 and m1 a CRLF note stays in CRLF, the end paragraph ends the note"
verdict test "$(od -An -c "$vault/CRLF.md" | tr -s ' \n' ' ')" = "$(printf '# CRLF\r\n\r\nUn paragraphe.\r\nNouvelle\r\n\r\nFin.\r\n' | od -An -c | tr -s ' \n' ' ')"

fresh
run crlf-edit <<'SCRIPT'
open CRLF.md
view source
focus editor
key ctrl-end
type Edit
key enter
type Deux
wait 1500
quit
SCRIPT
label="M5 Enter in the note editor keeps a CRLF note in CRLF"
verdict test "$(od -An -c "$vault/CRLF.md" | tr -s ' \n' ' ')" = "$(printf '# CRLF\r\n\r\nUn paragraphe.\r\nEdit\r\nDeux' | od -An -c | tr -s ' \n' ' ')"

fresh
run column <<SCRIPT
open $note
view live
wait 300
block 17
wait 300
key home
key right
key right
key right
key right
key right
key down
type X
key escape
wait 1500
quit
SCRIPT
label="m2 Down keeps the column"
verdict grep -q "^### TXitre de niveau 3" "$vault/$note"

fresh
cat > "$work/external.focus" <<SCRIPT
set editor.edit_in_place off
open $note
view live
wait 300
block 10
key end
type AA
wait 4000
type BB
wait 500
shot external-banner
quit
SCRIPT
FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/external.focus" "$vault" >"$work/external.log" 2>&1 &
app=$!
for _ in $(seq 1 600); do grep -q "tagAA" "$vault/$note" && break; sleep 0.1; done
grep -q "tagAA" "$vault/$note" || { echo "FAIL M2 AA never reached the file"; exit 1; }
sleep 0.5
printf '\nLigne ajoutée ailleurs.\n' >> "$vault/$note"
wait $app || true
copy=$(find "$vault/Rendu" -name "*unsaved copy*" -print -quit)
label="M2 the change on disk is kept"
verdict grep -q "Ligne ajoutée ailleurs." "$vault/$note"
label="M2 the keystrokes typed after it are kept in a conflict copy"
verdict test -n "$copy" -a -f "${copy:-/nonexistent}"
[ -n "$copy" ] && grep -q "AABB" "$copy" || { echo "FAIL M2 the conflict copy holds AABB"; status=1; }
exit $status
