#!/usr/bin/env bash
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-update-banner"
feed="$root/target/debug/examples/feed"
[ -x "$feed" ] || { echo "missing $feed: cargo build -p focus-update --example feed" >&2; exit 2; }
rm -rf "${work:?}" && mkdir -p "$work/config" "$work/first" "$work/declined" "$work/app"
echo '{"language": "en"}' > "$work/config/settings.json"
export FOCUS_CACHE_DIR=$work/cache FOCUS_DATA_DIR=$work/data
running=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$root/Cargo.toml")
status=0
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; status=1; fi; }

name=focus-9.0.0-linux-x86_64
pack=$work/pack/$name
mkdir -p "$pack"
printf '#!/bin/sh\necho "focus 9.0.0 started" > "%s"\n' "$work/restarted" > "$pack/focus"
printf '#!/bin/sh\necho "focus-mcp 9.0.0"\n' > "$pack/focus-mcp"
chmod 755 "$pack/focus" "$pack/focus-mcp"
head -c 24000000 /dev/urandom > "$pack/README.md"
tar --create --gzip --directory "$work/pack" --file "$work/$name.tar.gz" "$name"
"$feed" "$work/feed" 9.0.0 "$work/$name.tar.gz"
served=$work/feed/download/v9.0.0/$name.tar.gz

python3 -I - "$work/feed" > "$work/port" <<'PY' &
import http.server
import pathlib
import sys
import time

root = pathlib.Path(sys.argv[1])


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        file = root / self.path.lstrip("/")
        if not file.is_file():
            self.send_error(404)
            return
        data = file.read_bytes()
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        slow = self.path.startswith("/download/")
        step = max(1, len(data) // 10) if slow else max(1, len(data))
        for start in range(0, len(data), step):
            self.wfile.write(data[start:start + step])
            self.wfile.flush()
            if slow:
                time.sleep(0.25)

    def log_message(self, *arguments):
        pass


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_address[1], flush=True)
server.serve_forever()
PY
server=$!
trap 'kill "$server" 2>/dev/null' EXIT
for _ in $(seq 1 50); do [ -s "$work/port" ] && break; sleep 0.1; done
port=$(head -n 1 "$work/port")
releases=http://127.0.0.1:$port

run() {
    local name=$1 config=${2:-$work/config} vault=${3:-}
    cat > "$work/$name.focus"
    cp "$root/target/debug/focus" "$work/app/focus"
    rm -f "$work/app/focus-mcp"
    FOCUS_BIN=$work/app/focus FOCUS_UPDATE_RELEASES=$releases FOCUS_UPDATE_KEY=$work/feed/release.pub \
        FOCUS_CONFIG_DIR=$config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/$name.focus" $vault >"$work/$name.log" 2>&1
}
leftovers() { find "$work/app" -name '.focus-update-*' | grep -q .; }

run install <<SCRIPT
wait 500
update
update check
update
shot update-offered
lang fr
wait 300
shot update-offered-fr
lang ar
wait 300
shot update-offered-ar
lang zh-CN
wait 300
shot update-offered-zh
lang en
theme light
wait 300
shot update-offered-light
theme dark
update download
wait 900
snap update-downloading
shot update-installed
update
update restart
SCRIPT
check "nothing is offered before a check" 'grep -qx "update: none" "$work/install.log"'
check "the signed release is offered with both versions and its page" 'grep -qxF "update: offered | $running | 9.0.0 | in place | $releases/tag/v9.0.0" "$work/install.log"'
check "the banner is drawn in four languages and both themes" '[ -s "$work/update-offered.png" ] && [ -s "$work/update-offered-fr.png" ] && [ -s "$work/update-offered-ar.png" ] && [ -s "$work/update-offered-zh.png" ] && [ -s "$work/update-offered-light.png" ]'
check "the download shows while it runs" '[ -s "$work/update-downloading.png" ]'
check "the update is installed" 'grep -q "^update: installed | " "$work/install.log" && [ -s "$work/update-installed.png" ]'
check "the program is the one of the release" 'cmp -s "$pack/focus" "$work/app/focus" && [ -x "$work/app/focus" ]'
check "the second program came with it" 'cmp -s "$pack/focus-mcp" "$work/app/focus-mcp" && [ -x "$work/app/focus-mcp" ]'
check "nothing else was written beside them" '! leftovers && [ "$(ls -A "$work/app" | wc -l)" = 2 ]'
for _ in $(seq 1 30); do [ -s "$work/restarted" ] && break; sleep 0.1; done
check "Restart starts the new program" 'grep -qx "focus 9.0.0 started" "$work/restarted"'

printf 'x' >> "$served"
run damaged <<SCRIPT
wait 500
update check
update download
shot update-failed
update
update dismiss
update
SCRIPT
check "a file changed after the release was signed is refused" 'grep -qxF "update: failed (The download does not match the signed release. Nothing was installed.) | $running | 9.0.0 | in place | $releases/tag/v9.0.0" "$work/damaged.log" && [ -s "$work/update-failed.png" ]'
check "the program is left as it was" 'cmp -s "$root/target/debug/focus" "$work/app/focus" && ! leftovers'
check "closing the banner removes it" 'tail -n 1 "$work/damaged.log" | grep -qx "update: none"'
"$feed" "$work/feed" 9.0.0 "$work/$name.tar.gz"

chmod 555 "$work/app"
cat > "$work/readonly.focus" <<SCRIPT
wait 500
update check
update download
shot update-readonly
update
SCRIPT
FOCUS_BIN=$work/app/focus FOCUS_UPDATE_RELEASES=$releases FOCUS_UPDATE_KEY=$work/feed/release.pub \
    FOCUS_CONFIG_DIR=$work/config FOCUS_SHOT_DIR=$work "$root/tools/shot.sh" "$work/readonly.focus" >"$work/readonly.log" 2>&1
chmod 755 "$work/app"
check "a folder that cannot be written is reported" 'grep -q "^update: failed (Focus could not write in $work/app (" "$work/readonly.log"'

run skip <<SCRIPT
set general.update_check true
wait 11500
update
update dismiss
update
quit
SCRIPT
check "with the setting on, the release is found without asking" 'grep -q "^update: offered | " "$work/skip.log"'
check "the setting and the version hidden are saved" 'grep -q "\"general.update_check\": *true" "$work/config/settings.json" && grep -q "\"general.update_skipped\": *\"9.0.0\"" "$work/config/settings.json"'
run skipped <<SCRIPT
wait 11500
update
update check
update
quit
SCRIPT
check "a hidden version is not offered again by itself" 'sed -n "1p" <(grep "^update: " "$work/skipped.log") | grep -qx "update: none"'
check "a check asked for offers it again" 'sed -n "2p" <(grep "^update: " "$work/skipped.log") | grep -q "^update: offered | "'

run first "$work/first" - <<SCRIPT
wait 300
key enter
wait 300
shot welcome-updates
lang fr
wait 300
shot welcome-updates-fr
lang en
key down
wait 200
shot welcome-updates-no
key up
key enter
wait 600
quit
SCRIPT
check "the first launch asks once, after the language" '[ -s "$work/welcome-updates.png" ] && [ -s "$work/welcome-updates-no.png" ]'
check "Yes turns the checks on" 'grep -q "\"general.update_check\": *true" "$work/first/settings.json"'
run declined "$work/declined" - <<SCRIPT
wait 300
key enter
wait 300
key down
key enter
wait 600
quit
SCRIPT
check "No leaves them off" '[ -s "$work/declined/settings.json" ] && ! grep -q "\"general.update_check\": *true" "$work/declined/settings.json"'
run again "$work/first" - <<SCRIPT
wait 300
shot welcome-not-asked-again
quit
SCRIPT
check "the question does not come back" '[ -s "$work/welcome-not-asked-again.png" ]'

exit $status
