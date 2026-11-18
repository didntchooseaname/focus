#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
"$here/../shot.sh" "$here/milestone1.focus" "$@"
FOCUS_CAPTURE=sm "$here/../shot.sh" "$here/shell-sm.focus" "$@"
