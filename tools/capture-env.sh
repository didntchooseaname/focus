#!/usr/bin/env bash
set -euo pipefail
focused() { hyprctl monitors -j | python3 -c 'import json,sys; print([m["name"] for m in json.load(sys.stdin) if m["focused"]][0])'; }
case ${1:-status} in
up)
    hyprctl eval 'hl.monitor({ output = "FOCUS-B", mode = "1800x1100@60", position = "12000x12000", scale = 1 })' >/dev/null
    hyprctl eval 'hl.workspace_rule({ workspace = "name:focuscap", monitor = "FOCUS-B", default = true, persistent = true })' >/dev/null
    for spec in "focus-capture:1600 1000" "focus-capture-sm:1180 760"; do
        hyprctl eval "hl.window_rule({ match = { class = \"^(${spec%%:*})\$\" }, workspace = \"name:focuscap silent\", float = true, size = \"${spec##*:}\", no_initial_focus = true, opacity = \"1.0 override\", rounding = 0, border_size = 0, no_shadow = true, no_blur = true, no_anim = true })" >/dev/null
    done
    before=$(focused)
    hyprctl monitors -j | grep -q '"name": "FOCUS-B"' || hyprctl output create headless FOCUS-B >/dev/null
    sleep 0.3
    [ "$(focused)" = "$before" ] || hyprctl eval "hl.dispatch(hl.dsp.focus({ monitor = \"$before\" }))" >/dev/null
    ;;
ensure)
    state=$(hyprctl monitors -j | python3 -c '
import json, sys
m = [m for m in json.load(sys.stdin) if m["name"] == "FOCUS-B"]
ok = m and m[0]["width"] == 1800 and m[0]["height"] == 1100 and m[0]["x"] >= 10000 and m[0]["activeWorkspace"]["name"] == "focuscap"
print("ok" if ok else "lost")')
    if [ "$state" != ok ] || ! hyprctl workspacerules | grep -q 'name:focuscap'; then
        "$0" up
        sleep 0.3
        hyprctl workspacerules | grep -q 'name:focuscap' || { echo "capture environment could not be restored" >&2; exit 3; }
    fi
    ;;
down)
    hyprctl output remove FOCUS-B
    ;;
status)
    hyprctl monitors -j | python3 -c 'import json,sys; [print(m["name"], m["width"], m["height"], "focused" if m["focused"] else "") for m in json.load(sys.stdin)]'
    ;;
esac
