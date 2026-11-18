#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
vault="$root/target/test-vault/canvas-links"
rm -rf "${vault:?}" && mkdir -p "$vault/Boards" "$vault/Notes"
cat > "$vault/Notes/Scope.md" <<'NOTE'
# Scope

The hosts in scope, shown on the board. See also [[Findings]].
NOTE
cat > "$vault/Notes/Index.md" <<'NOTE'
# Index

- [[Scope]]
NOTE
cat > "$vault/Boards/Plan.canvas" <<'JSON'
{
	"nodes":[
		{"id":"a1","type":"file","file":"Notes/Scope.md","x":0,"y":0,"width":400,"height":300},
		{"id":"a2","type":"file","file":"Notes/Timeline.md","x":480,"y":0,"width":400,"height":300},
		{"id":"a3","type":"text","text":"Kick-off on Monday","x":0,"y":360,"width":300,"height":80}
	],
	"edges":[
		{"id":"e1","fromNode":"a1","toNode":"a2"}
	]
}
JSON
before=$(sha256sum "$vault/Boards/Plan.canvas")
"$root/tools/shot.sh" "$root/tools/shots/canvas-links/canvas-links.focus" "$vault"
[ "$before" = "$(sha256sum "$vault/Boards/Plan.canvas")" ]
