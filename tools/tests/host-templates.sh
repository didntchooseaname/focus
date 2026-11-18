#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-host-templates"
rm -rf "$work" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$work/vault"
cat > "$work/run.focus" <<'SCRIPT'
lang fr
palette >Nouvelle note depuis un gabarit
wait 300
key enter
wait 300
type oscp
wait 300
shot host-templates-fr
key enter
wait 1500
quit
SCRIPT
FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/run.focus" "$work/vault" >"$work/run.log" 2>&1 || true
status=0
note=$(grep -rl "exam: oscp" "$work/vault" --include=*.md | head -1 || true)
if [ -n "$note" ]; then echo "ok   a note was made from exam-journal-oscp: ${note#$work/vault/}"; else echo "FAIL no note made from the template"; status=1; fi
if [ -n "$note" ] && ! grep -q "{{date}}\|{{title}}" "$note" && grep -q "date: $(date +%Y-%m-%d)" "$note"; then echo "ok   its variables are filled"; else echo "FAIL its variables are not filled"; status=1; fi
exit $status
