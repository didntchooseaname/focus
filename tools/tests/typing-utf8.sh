#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
vault=$root/target/test-typing-utf8
rm -rf "$vault" && cp -r "$root/fixtures/vault" "$vault"
FOCUS_SHOT_DIR=$root/target/test-typing-utf8-shots "$root/tools/shot.sh" "$root/tools/tests/typing-utf8.focus" "$vault" >"$root/target/test-typing-utf8.log" 2>&1
code=$?
note="$vault/Rendu/Tout le Markdown.md"
fail=0
[ "$code" -eq 0 ] && echo "ok   the application ended normally" || { echo "FAIL the application ended with code $code"; fail=1; }
for text in "éa àù été" "你好" "مرحبا YY" "fin-edit" "éé àù 你好 مرحبا"; do
    grep -qF -- "$text" "$note" && echo "ok   on disk: $text" || { echo "FAIL not on disk: $text"; fail=1; }
done
exit $fail
