#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
vault="$root/target/test-vault/bases-groups"
rm -rf "${vault:?}" && mkdir -p "$vault/Tasks"
task() {
    printf -- '---\n%s\n---\n%s\n' "$2" "$3" > "$vault/Tasks/$1.md"
}
task Scoping $'status: done\nhours: 2\ndue: 2026-10-28' 'Scope agreed with the client.'
task Recon $'status: done\nhours: 3\ndue: 2026-11-02' 'Passive and active reconnaissance.'
task Phishing $'status: doing\nhours: 5.5\ndue: 2026-11-20' 'Campaign in progress.'
task Report $'status: todo\nhours: 8\ndue: 2026-12-01' 'Draft, review, delivery.'
task Retest $'status: todo\nhours: two' 'Hours written as a word.'
task Kickoff 'hours: 1' 'No status yet.'
cat > "$vault/Work.base" <<'BASE'
filters: 'file.inFolder("Tasks")'
properties:
  note.hours:
    displayName: Hours
  formula.cost:
    displayName: Cost
  note.due:
    displayName: Due
formulas:
  cost: hours * 120
summaries:
  Largest: values.sort().reverse()[0]
views:
  - type: table
    name: By status
    order:
      - file.name
      - note.hours
      - formula.cost
      - note.due
    groupBy:
      property: note.status
      direction: ASC
    summaries:
      file.name: Filled
      note.hours: Sum
      formula.cost: Largest
      note.due: Earliest
  - type: table
    name: All tasks
    order:
      - file.name
      - note.status
      - note.hours
    summaries:
      note.status: Unique
      note.hours: Median
BASE
cat > "$vault/Views.base" <<'BASE'
filters: 'file.inFolder("Tasks")'
properties:
  note.status:
    displayName: Status
  note.hours:
    displayName: Hours
  formula.host:
    displayName: Shown in
formulas:
  host: this.file.name
views:
  - type: table
    name: Open work
    order:
      - file.name
      - note.hours
    groupBy:
      property: note.status
      direction: ASC
    groupOrder:
      - todo
      - doing
      - blocked
    summaries:
      note.hours: Average
  - type: cards
    name: Cards
  - type: table
    name: First tasks
    order:
      - file.name
      - note.status
      - note.hours
    limit: 3
    summaries:
      note.status: Unique
      note.hours: Sum
  - type: table
    name: Same status
    filters: 'note.status == this.status'
    order:
      - file.name
      - note.status
      - formula.host
BASE
cat > "$vault/Board.md" <<'NOTE'
# Board

The base [[Work.base|work table]] lists the tasks of the engagement, and the base
[[Views.base|other views]] shows them in other ways.

![[Work.base#By status]]

The same tasks without groups:

![[Work.base#All tasks]]
NOTE
cat > "$vault/Today.md" <<'NOTE'
---
status: todo
---
# Today

The tasks that share the status of this note:

![[Views.base#Same status]]
NOTE
for name in bases-groups bases-groups-ar; do
    "$root/tools/shot.sh" "$root/tools/shots/bases-groups/$name.focus" "$vault"
done
