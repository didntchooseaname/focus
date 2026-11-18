#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
class=${FOCUS_CAPTURE_CLASS:-focus-capture}
grim -g "$("$here/geom.py" "$class")" "${FOCUS_SHOT_DIR:?}/$1.png"
