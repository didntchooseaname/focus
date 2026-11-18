#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
export FOCUS_BIN=$root/tools/shots/demo-with-args.sh
shot() { tools/shot.sh "tools/shots/$1.shot" "${VAULT:-fixtures/demo-vault}"; }
for name in search-main search-narrow search-ai search-light search-fr search-ar search-keys search-ai-edit search-diagnostic; do
    shot "$name"
done
FOCUS_DEMO_ARGS="--chunk-ms 400" shot search-ai-running
FOCUS_DEMO_ARGS="--fail" shot search-ai-error
shot search-measure

shot search-replace
rm -rf target/replace-vault && cp -r fixtures/vault target/replace-vault
VAULT=$root/target/replace-vault shot search-replace-apply
