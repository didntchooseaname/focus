---
type: method
status: ready
tags: [method, scoping, evidence]
last_verified: 2026-10-12
---

# Asset inventory

The inventory lists what exists in the scope. It starts from the [[Scope table]] and grows during testing. The [[Service inventory]] holds what runs on each asset.

## Columns

| Column | Meaning |
|--------|---------|
| Asset | Host name or address |
| Owner | Person or team on the client side |
| Role | Server, workstation, appliance |
| Source | How we learned about it |
| Scope id | Row of the scope table |

## Example rows

- `fileserver.northwind.example.com` (192.0.2.14), file server, source: client list, S1
- `printer-2f.northwind.example.com` (192.0.2.60), printer, source: ping sweep, S1
- 203.0.113.9, unknown, source: certificate transparency, scope id to confirm

## Rules

> [!warning]
> An asset with no scope id is not tested. It goes to [[Out of scope discovery]].

- Every asset has a source, so the report can say how it was found.
- Screenshots of the network drawing go in [[Screenshot conventions]] format.

```bash
nmap -sn 192.0.2.0/25 -oG "$OUT/hosts.gnmap"
```

![[network-diagram.png]]
