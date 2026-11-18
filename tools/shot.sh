#!/usr/bin/env bash
set -euo pipefail
need=${FOCUS_SHOT_MIN_GIB:-3}
while [ "$(awk '/MemAvailable/ {print int($2/1048576)}' /proc/meminfo)" -lt "$need" ]; do sleep 15; done
echo 1000 > /proc/self/oom_score_adj 2>/dev/null || true
root=$(cd "$(dirname "$0")/.." && pwd)
script=${1:?usage: tools/shot.sh <script> [vault]}
if [ -n "${2:-}" ]; then
    vault=$2
else
    vault=$root/target/shot-vault
    rm -rf "$vault"
    cp -r "$root/fixtures/demo-vault" "$vault"
fi
out=${FOCUS_SHOT_DIR:-$root/shots}
bin=${FOCUS_BIN:-$root/target/debug/focus}
mode=${FOCUS_CAPTURE:-1}
class=focus-capture
[ "$mode" = sm ] && class=focus-capture-sm
mkdir -p "$out"
if [ -z "${FOCUS_CONFIG_DIR:-}" ]; then
    FOCUS_CONFIG_DIR=$root/target/shot-config/$(basename "$script" .focus)
    rm -rf "$FOCUS_CONFIG_DIR"
    mkdir -p "$FOCUS_CONFIG_DIR"
    echo '{"language": "en"}' > "$FOCUS_CONFIG_DIR/settings.json"
fi
export FOCUS_CONFIG_DIR
export FOCUS_CACHE_DIR=${FOCUS_CACHE_DIR:-$root/target/shot-cache}
export FOCUS_DATA_DIR=${FOCUS_DATA_DIR:-$root/target/shot-data}
vault_args=(--vault "$vault")
[ "$vault" = - ] && vault_args=()

exec 9>"${XDG_RUNTIME_DIR:-/tmp}/focus-capture.lock"
flock -w 600 9

"$root/tools/capture-env.sh" ensure

FOCUS_CAPTURE=$mode FOCUS_CAPTURE_CLASS=$class FOCUS_SHOT_DIR=$out FOCUS_SHOT_CMD="$root/tools/grab.sh" \
    timeout "${FOCUS_SHOT_TIMEOUT:-120}" "$bin" "${vault_args[@]}" --script "$script" &
pid=$!

placement=none
for _ in $(seq 1 60); do
    placement=$(hyprctl clients -j | python3 -c '
import json, sys
wins = [c for c in json.load(sys.stdin) if c["class"] == sys.argv[1]]
if not wins:
    print("none")
elif all(c["workspace"]["name"] == "focuscap" and c["floating"] for c in wins):
    print("ok")
else:
    print("bad")' "$class")
    [ "$placement" != none ] && break
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.1
done
if [ "$placement" = bad ]; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    echo "capture window opened outside the capture output: stopped." >&2
    exit 4
fi
wait "$pid"
