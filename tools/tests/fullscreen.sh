#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$root/target/fullscreen
rm -rf "$work" && mkdir -p "$work" && cp -r "$root/fixtures/vault" "$work/vault"
cat >"$work/fullscreen.focus" <<'EOF'
open Rendu/Tout le Markdown.md
view source
focus editor
wait 1500
stamp before
fullscreen
wait 2000
stamp fullscreen
type x
wait 1000
stamp typed
shot fullscreen-on
fullscreen
wait 2000
stamp restored
type y
wait 1000
stamp end
shot fullscreen-off
quit
EOF
WAYLAND_DEBUG=1 FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/fullscreen.focus" "$work/vault" >"$work/fullscreen.log" 2>&1
python3 - "$work/fullscreen.log" <<'PY'
import re, sys
spans = {}
current = None
for line in open(sys.argv[1], errors="replace"):
    m = re.match(r"\[(\d\d:\d\d:\d\d\.\d+)\] (.*)", line)
    if not m:
        continue
    rest = m.group(2)
    if rest.startswith("stamp "):
        current = rest[6:]
        spans[current] = {"configure": [], "attach": 0, "commit": 0, "frame": 0}
        continue
    if current is None:
        continue
    span = spans[current]
    c = re.search(r"xdg_toplevel[@#]\d+\.configure\((-?\d+), (-?\d+)", rest)
    if c:
        span["configure"].append(f"{c.group(1)}x{c.group(2)}")
    if re.search(r"-> wl_surface[@#]\d+\.attach\(", rest):
        span["attach"] += 1
    if re.search(r"-> wl_surface[@#]\d+\.commit\(", rest):
        span["commit"] += 1
    if re.search(r"-> wl_surface[@#]\d+\.frame\(", rest):
        span["frame"] += 1
failed = False
for name in ["before", "fullscreen", "typed", "restored", "end"]:
    if name not in spans:
        print(f"FAIL no stamp {name}: the script did not get that far")
        sys.exit(1)
def show(name, what):
    s = spans[name]
    return f"{what}: configures {s['configure'] or '-'}, {s['attach']} buffers attached, {s['commit']} commits, {s['frame']} frame requests"
print(show("before", "after the fullscreen request"))
print(show("fullscreen", "after the key in fullscreen"))
print(show("typed", "after leaving fullscreen"))
print(show("restored", "after the key once restored"))
for name, what in [("before", "entering fullscreen drew no new buffer"), ("fullscreen", "the key in fullscreen drew no new buffer"), ("typed", "leaving fullscreen drew no new buffer"), ("restored", "the key after leaving fullscreen drew no new buffer")]:
    if spans[name]["attach"] == 0:
        print(f"FAIL {what}")
        failed = True
if not failed:
    print("ok the window draws after entering and after leaving fullscreen")
sys.exit(1 if failed else 0)
PY
