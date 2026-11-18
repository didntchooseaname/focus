#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-pdf-embeds"
rm -rf "${work:?}" && mkdir -p "$work/tools" "$work/shots"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }

cat >"$work/tools/xdg-open" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >> "$work/opened.txt"
EOF
chmod +x "$work/tools/xdg-open"
export PATH="$work/tools:$PATH"

fresh() {
    rm -rf "${work:?}/vault" && cp -r "$root/fixtures/demo-vault" "$work/vault"
    python3 -I "$root/tools/tests/file-viewers.py" "$work/vault/Files"
    printf '%%PDF-1.7\nnot a document\n' >"$work/vault/Files/broken.pdf"
    cat >"$work/vault/Embeds.md" <<'NOTE'
# Embeds

First page:

![[report.pdf]]

Page three:

![[report.pdf#page=3|300]]

![[report.pdf#page=99]]

![[missing.pdf]]

![[broken.pdf]]

Text after.
NOTE
    printf '# Click\n\n![[report.pdf#page=3|300]]\n' >"$work/vault/Click.md"
    printf '# Move\n\nFirst line.\n\n![[report.pdf|120]]\n\nText after.\n' >"$work/vault/Move.md"
}

run() {
    local name=$1 lang=${2:-en} theme=${3:-dark}
    mkdir -p "$work/config-$name" && echo "{\"language\": \"$lang\"}" >"$work/config-$name/settings.json"
    { echo "wait 400"; echo "theme $theme"; cat; } >"$work/$name.focus"
    FOCUS_CONFIG_DIR=$work/config-$name FOCUS_SHOT_DIR=$work/shots "$root/tools/shot.sh" "$work/$name.focus" "$work/vault" >"$work/$name.log" 2>&1
}

fresh
run read <<'SCRIPT'
open Embeds.md
view reading
wait 600
embeds
status
scroll 1
wait 600
embeds
shot embed-en-dark
view live
wait 600
scroll 0
wait 300
embeds
scroll 1
wait 600
embeds
shot embed-live
open Click.md
view reading
wait 600
click #image 0
wait 800
status
quit
SCRIPT
log=$work/read.log
check "the first embed draws page 1 of 12" 'grep -q "^embed: report.pdf | page 1 of 12 | drawn" "$log"'
check "a named page draws that page" 'grep -q "^embed: report.pdf | page 3 of 12 | drawn" "$log"'
check "a page beyond the file shows the line of a missing embed" 'grep -q "^embed: report.pdf | missing: This PDF has 12 pages: there is no page 99" "$log"'
check "a missing file shows the line of a missing embed" 'grep -q "^embed: missing.pdf | missing: File not found in the vault" "$log"'
check "a damaged file shows the line of a missing embed" 'grep -q "^embed: broken.pdf | missing: This PDF cannot be read" "$log"'
check "Live draws the same embeds" '[ "$(grep -c "^embed: report.pdf | page 3 of 12 | drawn" "$log")" -eq 2 ] && [ "$(grep -c "^embed: report.pdf | page 1 of 12 | drawn" "$log")" -ge 2 ]'
check "the missing file counts as a broken link of the note" 'grep -q "^status: Embeds.md | 1 broken" "$log"'
check "a click on the embed of page 3 opens the file at page 3" 'grep -q "^status: Files/report.pdf | Page 3 of 12" "$log"'
check "no script error" '! grep -q "script error\|panicked" "$log"'

for variant in "en light" "ar dark" "ar light"; do
    lang=${variant% *} theme=${variant#* }
    run "embed-$lang-$theme" "$lang" "$theme" <<SCRIPT
open Embeds.md
view reading
wait 600
scroll 1
wait 600
shot embed-$lang-$theme
quit
SCRIPT
    check "embed-$lang-$theme was taken" '[ -s "$work/shots/embed-$lang-$theme.png" ]'
done

fresh
{
    echo "# Twenty"
    for page in $(seq 1 12) $(seq 1 8); do printf '\n![[report.pdf#page=%s]]\n' "$page"; done
} >"$work/vault/Twenty.md"
run workers <<'SCRIPT' &
open Twenty.md
view reading
wait 1000
scroll 1
wait 1500
embeds
wait 1500
quit
SCRIPT
runner=$!
most=0
while kill -0 "$runner" 2>/dev/null; do
    count=$(pgrep -fc -- "--pdf-worker$" || true)
    [ "$count" -gt "$most" ] && most=$count
    sleep 0.1
done
check "twenty embeds of one file start one worker" '[ "$most" -eq 1 ]'
check "the last embeds of the note are drawn once scrolled to" 'grep -q "^embed: report.pdf | page 8 of 12 | drawn" "$work/workers.log"'

fresh
run move <<'SCRIPT'
open Move.md
view live
wait 500
caret 1:0
wait 300
handle #image 0
handle move below Text after.|0
handle drop
wait 300
key ctrl-s
wait 1200
quit
SCRIPT
check "the handle moves the embed below the last paragraph" \
    'python3 -c "import sys; t=open(sys.argv[1]).read(); sys.exit(0 if t.index(\"Text after.\") < t.index(\"![[report.pdf|120]]\") else 1)" "$work/vault/Move.md"'
fresh
cp "$work/vault/Move.md" "$work/Move.before.md"
run undo <<'SCRIPT'
open Move.md
view live
wait 500
caret 1:0
wait 300
handle #image 0
handle move below Text after.|0
handle drop
wait 300
key ctrl-z
key ctrl-s
wait 1200
quit
SCRIPT
check "undo puts the moved embed back" 'cmp -s "$work/vault/Move.md" "$work/Move.before.md"'

fresh
run rename <<'SCRIPT'
expand Files
focus tree
hover tree:Files/report.pdf
key f2
wait 200
type rapport.pdf
key enter
wait 1500
quit
SCRIPT
check "a rename of the PDF rewrites its embeds" 'grep -q "!\[\[rapport.pdf#page=3|300\]\]" "$work/vault/Embeds.md" && ! grep -q "report.pdf" "$work/vault/Embeds.md"'

fresh
rm -f "$work/opened.txt"
run bar <<'SCRIPT'
expand Files
press tree:Files/report.pdf
wait 600
viewer drag 0.5
wait 600
status
shot pdf-scrollbar
rpress tree:Files/a-photo.png
shot menu-image
press menu:open-system
rpress tree:Files/report.pdf
press menu:open-system
rpress tree:Home.md
press menu:open-system
key escape
rpress tree:Files
press menu:open-system
quit
SCRIPT
log=$work/bar.log
check "no page is asked while the thumb is held" 'grep -q "^drag: 0 pages asked while the thumb was held" "$log"'
check "the drag to the middle shows a middle page" 'grep -Eq "^status: Files/report.pdf \| Page (6|7) of 12" "$log"'
check "the menu of an image and of a PDF open them with the system" \
    'grep -q "Files/a-photo.png$" "$work/opened.txt" && grep -q "Files/report.pdf$" "$work/opened.txt"'
check "the menu of a note and of a folder has no such row" \
    '[ "$(grep -c "menu:open-system: not drawn, or disabled" "$log")" -eq 2 ]'
for shot in embed-en-dark embed-live pdf-scrollbar menu-image; do
    check "$shot was taken" '[ -s "$work/shots/$shot.png" ]'
done
exit $status
