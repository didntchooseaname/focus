#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-ops-variables-live"
rm -rf "$work" && mkdir -p "$work/config" "$work/out" && cp -r "$root/fixtures/vault" "$work/vault"
echo '{"language": "en"}' > "$work/config/settings.json"
mkdir -p "$work/vault/Ops notes"
cat > "$work/vault/Ops notes/Vars.md" <<'NOTE'
# Vars

Tool notes.

```bash
nxc smb $TARGET -u $USER -p '$PASSWORD' --port $PROXY_PORT
```
NOTE
cat > "$work/vault/Ops notes/Other.md" <<'NOTE'
# Other

See [[Vars]] for the commands.
NOTE
cat > "$work/vault/Ops notes/Hosts list.md" <<'NOTE'
# Hosts list

![[Vars]]
NOTE
out=$work/out
python3 -I -c "import sys
with open(sys.argv[1], 'w') as f:
    f.write('# Journal\n\n')
    for i in range(150000):
        f.write(f'- 2026-11-17 {i // 3600 % 24:02}:{i // 60 % 60:02}:{i % 60:02} [10.0.0.{i % 250}] entry {i} of a long run\n')
" "$work/journal.md"
cat > "$work/run.focus" <<SCRIPT
ops on
wait 300
engagement Lab
wait 800
disk Engagements/Lab/Journal.md $work/journal.md
wait 500
open Ops notes/Vars.md
view reading
wait 600
opsvar RHOST 10.0.0.1
codes $out/bar-read.json
view live
wait 400
opsvar RHOST 10.0.0.2
codes $out/bar-live.json
caret 3:0
wait 300
opsvar RHOST 10.0.0.3
codes $out/bar-inplace.json
view reading
wait 400
opsvar PROXY_PORT 8080
codes $out/bar-added.json
opsvar RHOST
codes $out/bar-cleared.json
opsvar RHOST 10.0.0.4
credential alice password Winter2026! corp.local
wait 800
usecredential alice
codes $out/used.json
credential bob password Spring2026! corp.local
wait 800
opslink USER bob
codes $out/linked.json
credential alice password Autumn2026! corp.local
wait 800
codes $out/followed.json
variables 5
wait 300
type proxy
key tab
key ctrl-a
type 9090
key enter
codes $out/menu-set.json
key tab
key ctrl-a
type 1111
key escape
codes $out/menu-escaped.json
key ctrl-a
key backspace
type lport
key tab
type 4444
key enter
key ctrl-a
key backspace
type web
key down
key enter
key tab
type http://10.0.0.5/
key enter
wait 300
key escape
wait 300
variables 5
wait 300
type proxy
key tab
key ctrl-a
key backspace
key enter
codes $out/menu-cleared.json
key escape
wait 300
substitute off
codes $out/box-off.json
substitute on
codes $out/box-on.json
set ops.code_variables false
codes $out/setting-off.json
set ops.code_variables true
codes $out/setting-on.json
open Ops notes/Other.md
wait 600
opsvar RHOST 10.0.0.9
open Ops notes/Vars.md
wait 800
codes $out/behind.json
split
wait 800
group 1
open Ops notes/Vars.md
view reading
wait 800
opsvar RHOST 10.0.0.10
codes $out/groups.json
open Ops notes/Hosts list.md
view reading
wait 800
opsvar RHOST 10.0.0.11
codes $out/embed.json
open Ops notes/Other.md
view reading
wait 800
point Vars
wait 1000
opsvar RHOST 10.0.0.12
codes $out/preview.json
opsscreen commands
wait 1500
opsvar RHOST 10.0.0.13
codes $out/commands.json
engagement Second
wait 1500
codes $out/engagement.json
quit
SCRIPT
FOCUS_CONFIG_DIR="$work/config" FOCUS_SHOT_DIR="$work" "$root/tools/shot.sh" "$work/run.focus" "$work/vault" >"$work/run.log" 2>&1 || true
python3 -I "$root/tools/tests/ops-variables-live.py" "$out" "$work/vault/Engagements" "$work/run.log"
