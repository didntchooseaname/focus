#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-flush"
vault="$work/vault"
note="Red Team/Kerberoasting.md"
status=0

run() {
    local name=$1 close=$2 mark=$3
    rm -rf "$vault" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$vault"
    cat > "$work/$name.focus" <<SCRIPT
open $note
view source
focus editor
type $mark
$close
SCRIPT
    FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/$name.focus" "$vault" >/dev/null 2>&1 || true
    if grep -q "$mark" "$vault/$note"; then
        echo "ok   $name"
    else
        echo "FAIL $name: the typed text is not in the file"
        status=1
    fi
}

run close-tab $'key ctrl-w\nwait 300\nquit' FLUSH-TAB-MARK
run quit quit FLUSH-QUIT-MARK
exit $status
