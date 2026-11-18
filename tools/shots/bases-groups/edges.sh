#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../../.." && pwd)
vault="$root/target/test-vault/table-edges"
rm -rf "${vault:?}" && mkdir -p "$vault/Tasks"
task() {
    printf -- '---\nhours: %s\nnote: %s\n---\n' "$2" "$3" > "$vault/Tasks/$1.md"
}
task Recon 3 'Passive and active reconnaissance of the perimeter, then a list of the services exposed on each host'
task Report 8 'Draft, review, delivery'
cat > "$vault/Edges.base" <<'BASE'
filters: 'file.inFolder("Tasks")'
views:
  - type: table
    name: Wide
    order: [file.name, note.note, note.hours]
  - type: table
    name: Narrow
    order: [file.name, note.hours]
BASE
cat > "$vault/Tables.md" <<'NOTE'
# Tables

The [[Edges.base|page of the base]].

## Wide

| Name | Note | Hours |
|---|---|---|
| Recon | Passive and active reconnaissance of the perimeter, then a list of the services exposed on each host | 3 |
| Report | Draft | 8 |

## Narrow

| Name | Hours |
|---|---|
| Recon | 3 |

## Base wide

![[Edges.base#Wide]]

## Base narrow

![[Edges.base#Narrow]]
NOTE
cat > "$vault/Arabic.md" <<'NOTE'
# جداول

## عريض

| الاسم | الملاحظة | الساعات |
|---|---|---|
| الاستطلاع | استطلاع سلبي ونشط للمحيط، ثم قائمة بالخدمات المكشوفة على كل مضيف في الشبكة الداخلية والخارجية | 3 |

## ضيق

| الاسم | الساعات |
|---|---|
| الاستطلاع | 3 |
NOTE
for name in edges edges-ar; do
    "$root/tools/shot.sh" "$root/tools/shots/bases-groups/$name.focus" "$vault"
done
for shot in edges-notes edges-arabic edges-page edges-ar-notes edges-ar-arabic edges-ar-page; do
    echo "$shot"
    python3 "$root/tools/shots/bases-groups/edges.py" "$root/shots/$shot.png"
done
