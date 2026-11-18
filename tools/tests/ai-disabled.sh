#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
work="$root/target/test-ai-disabled"
port=8766
rm -rf "$work" && mkdir -p "$work"
python3 -u - "$port" "$work/requests.log" >/dev/null 2>&1 <<'PY' &
import http.server, sys
port, log = int(sys.argv[1]), sys.argv[2]
class H(http.server.BaseHTTPRequestHandler):
    def record(self):
        auth = 'with Authorization' if self.headers.get('Authorization') else 'no Authorization'
        open(log, 'a').write(f'{self.command} {self.path} {auth}\n')
        self.send_response(500); self.end_headers()
    do_GET = do_POST = record
    def log_message(self, *a): pass
http.server.HTTPServer(('127.0.0.1', port), H).serve_forever()
PY
server=$!
trap 'kill $server' EXIT
sleep 0.5
status=0

run() {
    local enabled=$1 extra=${2:-}
    rm -rf "$work/vault" "$work/config" && mkdir -p "$work/config" && cp -r "$root/fixtures/vault" "$work/vault"
    printf '{"language": "en", "ai_base_url": "http://127.0.0.1:%s", "ui": {"settings": {"ai.enabled": %s%s}}}\n' "$port" "$enabled" "$extra" > "$work/config/settings.json"
    : > "$work/requests.log"
    cat > "$work/run.focus" <<'SCRIPT'
open Red Team/Kerberoasting.md
right ai
wait 300
shot ai-tab
key ctrl-shift-a
wait 300
type What is kerberoasting?
key enter
wait 2000
quit
SCRIPT
    DEEPSEEK_API_KEY=sk-test-not-a-key FOCUS_CONFIG_DIR="$work/config" FOCUS_SHOT_DIR="$work/$enabled" "$root/tools/shot.sh" "$work/run.focus" "$work/vault" >"$work/run-$enabled.log" 2>&1 || true
}

run false
if [ ! -s "$work/requests.log" ]; then echo "ok   ai.enabled off: no request reached the server"; else echo "FAIL ai.enabled off: $(head -1 "$work/requests.log")"; status=1; fi
run true
if [ -s "$work/requests.log" ]; then echo "ok   ai.enabled on: the question reached the server ($(head -1 "$work/requests.log"))"; else echo "FAIL ai.enabled on: no request, the check sees nothing"; status=1; fi
run true ", \"ai.provider\": 2, \"ai.ollama.base_url\": \"http://127.0.0.1:$port\", \"ai.ollama.model\": \"llama3\""
if grep -q "POST /chat/completions no Authorization" "$work/requests.log"; then echo "ok   provider ollama: the question reached the local server without Authorization"; else echo "FAIL provider ollama: $(head -1 "$work/requests.log")"; status=1; fi
exit $status
