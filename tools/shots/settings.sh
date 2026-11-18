#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
vault=$root/target/settings-vault
rm -rf "$vault"
cp -r "$root/fixtures/vault" "$vault"

seed() {
    local dir=$root/target/shot-config/$1
    rm -rf "$dir"
    mkdir -p "$dir"
    cat > "$dir/settings.json" <<'JSON'
{"language": "en", "ui": {"settings": {
  "ai.actions_json": "[{\"id\":\"explain-code\",\"label\":\"Explain this code\",\"description\":\"Reads the selection as code.\",\"prompt\":\"Explain what this code does, step by step:\\n\\n{{selection}}\",\"scope\":\"selection\",\"output\":\"chat\",\"model\":null,\"ops_only\":false,\"enabled\":true,\"builtin\":false},{\"id\":\"rephrase\",\"label\":\"ai-action-rephrase-label\",\"description\":\"ai-action-rephrase-desc\",\"prompt\":\"Rephrase: {{selection}}\",\"scope\":\"selection\",\"output\":\"replace\",\"model\":null,\"ops_only\":false,\"enabled\":false,\"builtin\":true}]",
  "ai.mcp_json": "[{\"name\":\"docs\",\"command\":\"docs-server\",\"args\":[\"--root\",\"/srv/docs\"],\"env\":{},\"allowed_tools\":[\"search\",\"read\"],\"enabled\":true},{\"name\":\"tickets\",\"command\":\"/opt/tickets/bin/server\",\"args\":[],\"env\":{},\"allowed_tools\":[],\"enabled\":false}]"
}}}
JSON
    export FOCUS_CONFIG_DIR=$dir
}

run() {
    local name=$1 open=${2:-}
    seed "$name"
    FOCUS_SETTINGS_OPEN=$open "$root/tools/shot.sh" "$here/$name.focus" "$vault"
}

run settings-ai
run settings-ai-actions
run settings-ai-actions-edit action
run settings-ai-mcp server
run settings-shortcuts
run settings-shortcut-conflict record:ui-cmd-toggle-right
run settings-transfer
run settings-search
run settings-invalid focus:editor.autosave
run settings-fr
run settings-ar
run settings-about
run settings-zh
run settings-ops-zh
