#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
vault="$root/target/test-vault/remote-images"
served="$root/target/test-vault/remote-images-served"
port=8765
rm -rf "$vault" "$served" && mkdir -p "$served" && cp -r "$root/fixtures/vault" "$vault"
cp "$root/fixtures/vault/Red Team/assets/capture-exemple.png" "$served/capture.png"
printf '# Image distante\n\nAn image served over http:\n\n![capture](http://127.0.0.1:%s/capture.png)\n' "$port" > "$vault/Rendu/Image distante.md"
python3 -m http.server "$port" --bind 127.0.0.1 --directory "$served" >/dev/null 2>&1 &
server=$!
trap 'kill $server' EXIT
sleep 0.5
"$root/tools/shot.sh" "$root/tools/shots/remote-images/remote-images.focus" "$vault"
