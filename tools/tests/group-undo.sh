#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-group-undo"
rm -rf "$work" && mkdir -p "$work"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }
run() {
    local name=$1
    rm -rf "$work/vault" "$work/config" && mkdir -p "$work/config"
    cp -r "$root/fixtures/demo-vault" "$work/vault"
    echo '{"language": "en"}' > "$work/config/settings.json"
    { printf 'wait 300\nopen Home.md\nview source\nsplit\ngroup 1\nfocus editor\nkey ctrl-end\ngroup 2\nfocus editor\nkey ctrl-end\n'
      cat; printf 'wait 1500\nquit\n'; } > "$work/$name.focus"
    FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data FOCUS_CONFIG_DIR=$work/config \
        "$root/tools/shot.sh" "$work/$name.focus" "$work/vault" >"$work/$name.log" 2>&1
    grep "focus:" "$work/$name.log" | grep -v listening && status=1
    cp "$work/vault/Home.md" "$work/$name.md"
}
fixture=$root/fixtures/demo-vault/Home.md
is() { [ "$(cat "$work/$1.md")" = "$(printf %s "$2"; cat "$fixture"; printf %s "$3")" ]; }

steps='group 1
focus editor
key ctrl-end
type AAA
wait 1500
group 2
focus editor
key ctrl-end
type BBB
wait 1500
group 1
focus editor
key ctrl-end
type CCC
wait 1500
group 2
focus editor'
run undo-one <<<"$steps
key ctrl-z"
check "one Ctrl Z in the follower undoes the last change, typed in the other group" 'is undo-one "" AAABBB'
run undo-three <<<"$steps
key ctrl-z
key ctrl-z
key ctrl-z"
check "three Ctrl Z undo the three steps, whichever group typed them" 'is undo-three "" ""'
run undo-lead <<<"$steps
group 1
focus editor
key ctrl-z
key ctrl-z"
check "the first group undoes the same way" 'is undo-lead "" AAA'
run caret <<'SCRIPT'
group 1
focus editor
key ctrl-home
type Z
wait 1500
group 2
focus editor
type Y
SCRIPT
check "the follower keeps its caret at the end while the other group types at the start" 'is caret Z Y'
run redo <<'SCRIPT'
group 2
focus editor
type AAA
wait 1500
key ctrl-z
wait 1500
group 1
focus editor
key ctrl-home
type Q
wait 1500
group 2
focus editor
key ctrl-y
SCRIPT
check "a redo after a change from the other group does nothing" 'is redo Q ""'
exit $status
