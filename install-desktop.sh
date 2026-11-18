#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
entry="$root/focus.desktop"
binary="$root/target/release/focus"
applications="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
installed="$applications/focus.desktop"

if [[ ! -x $binary ]]; then
    echo "binary missing: run cargo build --release -p focus first" >&2
    exit 1
fi

desktop-file-validate "$entry"
install -Dm0644 "$entry" "$installed"
icon="$root/assets/icons/focus.png"
sed -i "s|^Exec=.*|Exec=$binary %F|; s|^TryExec=.*|TryExec=$binary|; s|^Icon=.*|Icon=$icon|" "$installed"

if [[ $# -ge 1 ]]; then
    vault=$(realpath "$1")
    if [[ ! -d $vault ]]; then
        echo "not a directory: $vault" >&2
        exit 1
    fi
    sed -i "s|^Exec=.*|Exec=$binary --vault \"$vault\" %F|" "$installed"
    desktop-file-validate "$installed"
fi

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$applications"
fi

echo "Focus registered: $installed"
