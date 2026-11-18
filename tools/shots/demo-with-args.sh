#!/usr/bin/env bash
exec "$(dirname "$0")/../../target/debug/examples/search-demo" "$@" ${FOCUS_DEMO_ARGS:-}
