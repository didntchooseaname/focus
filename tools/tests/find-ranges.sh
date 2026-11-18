#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-find-ranges"
rm -rf "${work:?}" && mkdir -p "$work"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }

base=$'# Find\n\nalpha one, alpha two.\nalpha three.\n'
EDIT=$'open Find.md\nview source\nwait 300\nfocus editor\nkey ctrl-home'
END=$'key escape\nwait 1500\nquit'

run() {
    local name=$1
    mkdir -p "$work/$name"
    cp -r "$root/fixtures/demo-vault" "$work/$name/vault"
    printf '%s' "$base" > "$work/$name/vault/Find.md"
    cat > "$work/$name/script.focus"
    FOCUS_SHOT_DIR=$work/$name "$root/tools/shot.sh" "$work/$name/script.focus" "$work/$name/vault" \
        > "$work/$name/log" 2>&1
}
is() { printf '%s' "$2" > "$work/$1/expected"; cmp -s "$work/$1/expected" "$work/$1/vault/Find.md"; }

run first <<SCRIPT
$EDIT
key ctrl-h
type alpha
key tab
type OMEGA
key enter
wait 300
$END
SCRIPT
check "Replace hits the first occurrence" 'is first "${base/alpha one/OMEGA one}"'

run next <<SCRIPT
$EDIT
key ctrl-h
type alpha
key enter
key tab
type OMEGA
key enter
wait 300
$END
SCRIPT
check "Replace hits the occurrence Enter moved to" 'is next "${base/alpha two/OMEGA two}"'

run shifted <<SCRIPT
$EDIT
key ctrl-h
type alpha
focus editor
key down
key down
type zz
key space
key ctrl-h
type OMEGA
key enter
wait 300
$END
SCRIPT
check "after typing above the occurrences, Replace hits the shifted range" \
    'is shifted "${base/alpha one/zz OMEGA one}"'

run live <<SCRIPT
open Find.md
view live
wait 500
key ctrl-h
type alpha
key tab
type OMEGA
key enter
wait 300
$END
SCRIPT
check "in Live, Replace writes the current occurrence into the note" 'is live "${base/alpha one/OMEGA one}"'

for log in "$work"/*/log; do
    grep -h -E "^(focus:|Error:)" "$log" && status=1
done
exit $status
