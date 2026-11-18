#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
export FOCUS_BIN=$root/target/debug/examples/transfer-demo
demo=${VAULT:-fixtures/demo-vault}
for name in transfer-export transfer-attachments transfer-light transfer-fr transfer-ar; do
    tools/shot.sh "tools/shots/$name.shot" "$demo"
done
rm -rf target/transfer-vault target/import-source target/transfer-out
cp -r fixtures/vault target/transfer-vault
mkdir -p target/import-source/notes/.git target/import-source/notes/img
printf '# First\n\nSee [[Second]].\n' > target/import-source/notes/First.md
printf '# Second\n\n![](img/pic.png)\n' > target/import-source/notes/Second.md
printf 'png' > target/import-source/notes/img/pic.png
printf 'x' > target/import-source/notes/.git/config
tools/shot.sh tools/shots/transfer-import.shot target/transfer-vault
