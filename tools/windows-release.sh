#!/usr/bin/env bash
set -euo pipefail
arch=${1:-aarch64}
case "$arch" in
aarch64 | x86_64) ;;
*) echo "usage: $0 [aarch64|x86_64]" >&2; exit 2 ;;
esac
cd "$(dirname "$0")/.."
export XWIN_CROSS_COMPILER=clang
exec ${CARGO:-cargo} xwin build --release --locked -j "${JOBS:-3}" --target "$arch-pc-windows-msvc" -p focus -p focus-mcp
