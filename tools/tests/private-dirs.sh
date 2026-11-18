#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
base="$root/target/test-private-dirs"
rm -rf "$base" && mkdir -p "$base/config" "$base/cache" "$base/data"
echo '{"language": "en"}' > "$base/config/settings.json"
chmod 755 "$base/config" "$base/cache" "$base/data"
chmod 644 "$base/config/settings.json"
vault="$base/vault"
cp -r "$root/fixtures/vault" "$vault"
printf 'open 00 Accueil.md\nquit\n' > "$base/open.focus"
FOCUS_CONFIG_DIR="$base/config" FOCUS_CACHE_DIR="$base/cache" FOCUS_DATA_DIR="$base/data" "$root/tools/shot.sh" "$base/open.focus" "$vault" >/dev/null
fail=0
for path in "$base/config" "$base/cache" "$base/cache"/*/ "$base/data"; do
    [ "$(stat -c %a "$path")" = 700 ] || { echo "not 700: $path $(stat -c %a "$path")"; fail=1; }
done
[ "$(stat -c %a "$base/config/settings.json")" = 600 ] || { echo "not 600: settings.json"; fail=1; }
[ $fail = 0 ] && echo "ok private-dirs"
exit $fail
