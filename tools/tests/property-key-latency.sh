#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
bin=${FOCUS_BIN:-$root/target/debug/focus}
[ -x "$bin" ] || { echo "FAIL property-key-latency: $bin is not built"; exit 1; }
keys=${1:-60}
work=$root/target/test-property-latency
rm -rf "${work:?}" && mkdir -p "$work"
python3 -I - "$root" "$work" "$keys" <<'PY'
import sys
from pathlib import Path
root, work, keys = Path(sys.argv[1]), Path(sys.argv[2]), int(sys.argv[3])
sys.path.insert(0, str(root / "tools/bench"))
import interaction as ia
vault = work / "vault"
(vault / ".obsidian").mkdir(parents=True)
text = ia.note_text(10000).replace("title: Latency note\n", "title: Latency note\ncount: 1\ndue: 2020-08-21\n", 1)
(vault / "Latency.md").write_text(text)
lines = text.split("\n")
count = lines.index("count: 1") + 1
para = next(i for i, l in enumerate(lines) if l.startswith("Paragraph")) + 1
body = "open Latency.md\nwait 800\nview live\nwait 1500\n"
body += f"caret {count}:8\nwait 1500\n" + "".join("wait 1000\nstamp field\ntype 1\n" for _ in range(keys))
body += f"caret {para}:20\nwait 1500\n" + "".join("wait 1000\nstamp para\ntype a\n" for _ in range(keys))
(work / "latency.focus").write_text(body + "wait 500\nquit\n")
PY
FOCUS_SHOT_TIMEOUT=$((keys * 3 + 120)) WAYLAND_DEBUG=1 FOCUS_BIN=$bin FOCUS_SHOT_DIR=$work/shots \
    "$root/tools/shot.sh" "$work/latency.focus" "$work/vault" >"$work/latency.log" 2>&1
python3 -I - "$root" "$work/latency.log" "$work/vault/Latency.md" "$keys" <<'PY'
import statistics, sys
from pathlib import Path
sys.path.insert(0, str(Path(sys.argv[1]) / "tools/bench"))
import interaction as ia
evs = ia.events(open(sys.argv[2], errors="replace").read())
ok = True
for label, what in (("field", "number property"), ("para", "paragraph in place")):
    v = sorted(ia.key_latencies(evs, label))
    if not v:
        print(f"FAIL {what}: no key timed")
        ok = False
        continue
    p95 = v[min(len(v) - 1, int(len(v) * 0.95))]
    print(f"{what}: {len(v)} keys, median {statistics.median(v):.2f} ms, 95th {p95:.2f} ms, max {v[-1]:.2f} ms")
note = open(sys.argv[3]).read()
written = "count: 1" + "1" * int(sys.argv[4])
print("value written as typed:", "yes" if f"\n{written}\n" in note else "NO")
sys.exit(0 if ok else 1)
PY
