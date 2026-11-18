#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
work="$root/target/shots-ops-variables-menu"
rm -rf "$work" && mkdir -p "$work"
vault() {
    rm -rf "$work/vault" && cp -r "$root/fixtures/vault" "$work/vault"
    mkdir -p "$work/vault/Ops notes"
    printf '# Vars\n\nTool notes.\n\n```bash\nnxc smb $TARGET -u $USER -p '"'"'$PASSWORD'"'"' --port $PROXY_PORT\n```\n' \
        > "$work/vault/Ops notes/Vars.md"
}
combos=("en dark" "ar dark" "en light" "ar light")
{
    printf 'ops on\nwait 300\nengagement Lab\nwait 800\n'
    printf 'credential alice password Winter2026! corp.local\nwait 800\nusecredential alice\n'
    printf 'opsvar RHOST 10.10.10.5\nopsvar PROXY_PORT 8080\n'
    printf 'open Ops notes/Vars.md\nview reading\nwait 600\n'
    for combo in "${combos[@]}"; do
        set -- $combo
        name="$1-$2"
        printf 'lang %s\ntheme %s\nwait 500\nvariables 5\nwait 300\n' "$1" "$2"
        printf 'shot menu-values-%s\n' "$name"
        printf 'key tab\nkey ctrl-a\ntype 10.10.10.7\nshot menu-editing-%s\nkey escape\n' "$name"
        printf 'type web\nkey down\nkey enter\nkey tab\ntype http://10.10.10.5/\nshot menu-add-%s\n' "$name"
        printf 'key escape\nkey escape\nwait 300\n'
    done
    printf 'quit\n'
} > "$work/engaged.focus"
{
    printf 'ops on\nwait 600\nopen Ops notes/Vars.md\nview reading\nwait 600\n'
    for combo in "${combos[@]}"; do
        set -- $combo
        printf 'lang %s\ntheme %s\nwait 500\nvariables 5\nwait 300\nkey up\nshot menu-none-%s-%s\nkey escape\nwait 300\n' \
            "$1" "$2" "$1" "$2"
    done
    printf 'quit\n'
} > "$work/none.focus"
vault
FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/engaged.focus" "$work/vault" > "$work/engaged.log" 2>&1
vault
FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/none.focus" "$work/vault" > "$work/none.log" 2>&1
ls "$work"/*.png
