#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-agent-socket"
dir="${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR is needed for the socket}/focus"
status=0

run() {
    local setting=$1 expected=$2
    rm -rf "$work" && mkdir -p "$work/config" && cp -r "$root/fixtures/vault" "$work/vault"
    printf '{"language": "en", "ui": {"settings": {"ai.agent_socket": %s}}}\n' "$setting" > "$work/config/settings.json"
    printf 'wait 3000\nquit\n' > "$work/run.focus"
    touch "$work/marker"
    FOCUS_CONFIG_DIR="$work/config" "$root/tools/shot.sh" "$work/run.focus" "$work/vault" >"$work/run.log" 2>&1 &
    local app=$! found=no
    for _ in $(seq 1 100); do
        if [ -d "$dir" ] && [ -n "$(find "$dir" -name '*.sock' -newer "$work/marker" -print -quit)" ]; then found=yes; break; fi
        sleep 0.1
    done
    wait $app || true
    if [ $found = "$expected" ]; then echo "ok   ai.agent_socket $setting: socket made $found"; else echo "FAIL ai.agent_socket $setting: socket made $found"; status=1; fi
}

run false no
run true yes
exit $status
