#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-ops-variables-latency"
rm -rf "$work" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$work/vault"
{
    printf '# Commands\n\nTyping here.\n\n'
    for i in $(seq 40); do
        printf '## Step %s\n\n```bash\nnxc smb $TARGET -u $USER -p $PASSWORD --port %s\nimpacket-secretsdump $DOMAIN/$USER@$DC_IP -hashes :$NT_HASH\n```\n\n' "$i" "$i"
    done
} > "$work/vault/Commands.md"
{
    if [ "${FOCUS_OPS:-on}" = on ]; then
        printf 'ops on\nwait 300\nengagement Lab\nwait 800\n'
        printf 'opsvar RHOST 10.0.0.5\nopsvar USER alice\nopsvar DOMAIN corp.local\nopsvar DC 10.0.0.1\n'
    fi
    printf 'open Commands.md\nview live\nwait 800\ncaret 3:6\nwait 1000\n'
    for _ in $(seq 50); do printf 'wait 1000\nstamp\ntype a\n'; done
    printf 'quit\n'
} > "$work/run.focus"
WAYLAND_DEBUG=1 "$root/tools/shot.sh" "$work/run.focus" "$work/vault" >"$work/run.log" 2>&1
python3 -I - "$work/run.log" <<'PY'
import re, statistics, sys
def ms(t):
    h, m, s = t.split(":")
    return (int(h) * 3600 + int(m) * 60 + float(s)) * 1000
waits, stamp, attached = [], None, False
for line in open(sys.argv[1], errors="replace"):
    m = re.match(r"\[(\d\d:\d\d:\d\d\.\d+)\]\s+(.*)", line)
    if not m:
        continue
    t, rest = ms(m.group(1)), m.group(2)
    if rest == "stamp":
        stamp, attached = t, False
    elif stamp is not None and re.search(r"-> wl_surface[@#]\d+\.attach\(", rest):
        attached = True
    elif stamp is not None and attached and re.search(r"-> wl_surface[@#]\d+\.commit\(", rest):
        waits.append((t - stamp) % 3_600_000)
        stamp = None
if len(waits) < 10:
    print(f"FAIL only {len(waits)} keys measured")
    sys.exit(1)
q = statistics.quantiles(waits, n=4)
print(f"keys {len(waits)}  median {statistics.median(waits):.2f} ms  quartiles {q[0]:.2f} / {q[2]:.2f} ms  max {max(waits):.2f} ms")
PY
