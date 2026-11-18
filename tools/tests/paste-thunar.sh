#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-paste-thunar"
rm -rf "${work:?}" && mkdir -p "$work/bin" "$work/clip" "$work/outside"
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }

cat > "$work/bin/wl-paste" <<'TOOL'
#!/bin/sh
d=${FOCUS_FAKE_CLIPBOARD:?}
echo "wl-paste $*" >> "$d/calls"
list=0; nonl=0; type=
while [ $# -gt 0 ]; do
  case "$1" in
    -l|--list-types) list=1 ;;
    -n|--no-newline) nonl=1 ;;
    -t|--type) type=$2; shift ;;
    --type=*) type=${1#--type=} ;;
  esac
  shift
done
[ -s "$d/types" ] || { echo "Nothing is copied" >&2; exit 1; }
if [ $list = 1 ]; then cat "$d/types"; exit 0; fi
if [ -z "$type" ]; then
  if grep -qx 'text/plain;charset=utf-8' "$d/types"; then type='text/plain;charset=utf-8'; else type=$(head -n 1 "$d/types"); fi
fi
grep -qxF "$type" "$d/types" || { echo "No suitable type of content copied" >&2; exit 1; }
cat "$d/body.$(printf %s "$type" | tr '/;=' '___')"
case "$type" in text/*|UTF8_STRING|STRING|TEXT) [ $nonl = 1 ] || printf '\n' ;; esac
exit 0
TOOL
cat > "$work/bin/wl-copy" <<'TOOL'
#!/bin/sh
d=${FOCUS_FAKE_CLIPBOARD:?}
echo "wl-copy $*" >> "$d/calls"
cat > "$d/copied"
TOOL
chmod +x "$work/bin/wl-paste" "$work/bin/wl-copy"
export FOCUS_FAKE_CLIPBOARD=$work/clip

offer() {
    python3 - "$work/clip" "$@" <<'PY'
import os, sys, urllib.parse
d, mode, paths = sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else "", sys.argv[3:]
for name in os.listdir(d):
    if name.startswith("body.") or name == "types":
        os.remove(os.path.join(d, name))
open(os.path.join(d, "types"), "w").close()
if not paths:
    sys.exit(0)
uris = ["file://" + urllib.parse.quote(p, safe="/!$&'()*+,;=:@") for p in paths]
bodies = {
    "text/uri-list": "".join(u + "\r\n" for u in uris),
    "x-special/gnome-copied-files": mode + "\n" + "\n".join(uris),
    "text/plain;charset=utf-8": "\n".join(paths),
    "UTF8_STRING": "\n".join(paths),
}
with open(os.path.join(d, "types"), "w") as out:
    out.write("".join(t + "\n" for t in bodies))
for t, body in bodies.items():
    with open(os.path.join(d, "body." + t.replace("/", "_").replace(";", "_").replace("=", "_")), "wb") as out:
        out.write(body.encode())
PY
}

offer copy /home/user/Downloads/note.md
fake() { PATH=$work/bin:$PATH wl-paste "$@" | od -An -c | tr -s ' \n' ' '; }
want() { printf "$1" | od -An -c | tr -s ' \n' ' '; }
check "the stand-in lists Thunar's types in its order" \
    '[ "$(PATH=$work/bin:$PATH wl-paste --list-types | tr "\n" ,)" = "text/uri-list,x-special/gnome-copied-files,text/plain;charset=utf-8,UTF8_STRING," ]'
check "the stand-in gives text/uri-list with its CRLF" \
    '[ "$(fake --no-newline --type text/uri-list)" = "$(want "file:///home/user/Downloads/note.md\r\n")" ]'
check "the stand-in gives x-special/gnome-copied-files without a final newline" \
    '[ "$(fake --no-newline --type x-special/gnome-copied-files)" = "$(want "copy\nfile:///home/user/Downloads/note.md")" ]'
check "the stand-in gives the path as text, and a newline without --no-newline" \
    '[ "$(fake --type "text/plain;charset=utf-8")" = "$(want "/home/user/Downloads/note.md\n")" ]'

printf '# Scope notes\n\nFrom outside.\n' > "$work/outside/Scope notes é.md"
printf '# Half\n\nHalf done.\n' > "$work/outside/50% done #1.md"
mkdir -p "$work/outside/Evidence dir/sub"
printf 'raw\n' > "$work/outside/Evidence dir/a.txt"
printf '# Deep\n' > "$work/outside/Evidence dir/sub/b.md"
printf '# Moved\n' > "$work/outside/Move me.md"
one="$work/outside/Scope notes é.md"

run() {
    local name=$1
    rm -rf "${work:?}/$name" && mkdir -p "$work/$name"
    cp -r "$root/fixtures/demo-vault" "$work/$name/vault"
    cat > "$work/$name/script.focus"
    : > "$work/clip/calls"
    PATH=$work/bin:$PATH FOCUS_SHOT_DIR=$work/$name "$root/tools/shot.sh" \
        "$work/$name/script.focus" "$work/$name/vault" > "$work/$name/log" 2>&1
    cp "$work/clip/calls" "$work/$name/calls"
}
attached() {
    local vault=$work/$1/vault
    tail -n 2 "$vault/Home.md" | grep -q '^!\[\[.*\.md' &&
        grep -rlq "^From outside\.$" "$vault/assets"
}

offer copy "$one"
run tree-click <<'SCRIPT'
wait 300
press tree:Method
wait 200
key ctrl-v
wait 1500
quit
SCRIPT
check "a folder clicked in the tree, then Ctrl V, copies the file into it" \
    'cmp -s "$one" "$work/tree-click/vault/Method/Scope notes é.md"'
check "the files are read through wl-paste, as gnome-copied-files" \
    'grep -qx "wl-paste --no-newline --type x-special/gnome-copied-files" "$work/tree-click/calls"'

offer copy "$one" "$work/outside/50% done #1.md" "$work/outside/Evidence dir"
run tree-many <<'SCRIPT'
wait 300
press tree:Reporting
key ctrl-v
wait 2000
quit
SCRIPT
v=$work/tree-many/vault/Reporting
check "several files with spaces, accents, % and # are copied" \
    '[ -f "$v/Scope notes é.md" ] && [ -f "$v/50% done #1.md" ]'
check "a folder is copied with its subfolders" \
    'cmp -s "$work/outside/Evidence dir/sub/b.md" "$v/Evidence dir/sub/b.md" && [ -f "$v/Evidence dir/a.txt" ]'

offer cut "$work/outside/Move me.md"
run tree-cut <<'SCRIPT'
wait 300
press tree:Daily
key ctrl-v
wait 1500
quit
SCRIPT
check "a cut moves the file into the folder" \
    '[ -f "$work/tree-cut/vault/Daily/Move me.md" ] && [ ! -e "$work/outside/Move me.md" ]'

offer copy "$one"
run menu-folder <<'SCRIPT'
wait 300
rpress tree:Certification
wait 300
shot menu-folder
press menu:paste
wait 1500
quit
SCRIPT
check "\"Paste\" of a folder's menu copies into that folder" \
    '[ -f "$work/menu-folder/vault/Certification/Scope notes é.md" ]'

run menu-empty <<'SCRIPT'
wait 300
rpress tree:
wait 300
shot menu-empty
press menu:paste
wait 1500
quit
SCRIPT
check "\"Paste\" of the empty area's menu copies into the vault root" \
    '[ -f "$work/menu-empty/vault/Scope notes é.md" ]'

offer
run menu-disabled <<'SCRIPT'
wait 300
rpress tree:Method
wait 300
shot menu-disabled
press menu:paste
wait 500
quit
SCRIPT
check "without files on the clipboard \"Paste\" is disabled" \
    'grep -q "menu:paste: not drawn, or disabled" "$work/menu-disabled/log"'

offer copy "$one"
run edit <<'SCRIPT'
wait 300
open Home.md
view source
focus editor
key ctrl-end
key ctrl-v
wait 2000
quit
SCRIPT
check "Ctrl V in Edit attaches the file" 'attached edit'

run live-block <<'SCRIPT'
wait 300
open Home.md
view live
wait 300
block end
key ctrl-v
wait 2000
key escape
wait 1500
quit
SCRIPT
check "Ctrl V in a Live block attaches the file" 'attached live-block'

run live-note <<'SCRIPT'
wait 300
open Home.md
view live
wait 500
key ctrl-v
wait 2000
shot live-note
quit
SCRIPT
check "Ctrl V on a note in Live without a block attaches the file at its end" 'attached live-note'

run read-note <<'SCRIPT'
wait 300
open Home.md
view reading
wait 500
key ctrl-v
wait 2000
quit
SCRIPT
check "Ctrl V on a note in Read attaches the file at its end" 'attached read-note'

for log in "$work"/*/log; do
    grep -h "focus:" "$log" | grep -v "menu:paste: not drawn, or disabled" && status=1
done
grep -qs "^wl-copy" "$work"/*/calls && { echo "FAIL something was copied"; status=1; }
exit $status
