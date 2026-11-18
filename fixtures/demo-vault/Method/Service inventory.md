---
type: method
status: ready
tags: [method, scoping, evidence]
last_verified: 2026-10-13
---

# Service inventory

One row per listening service. It extends the [[Asset inventory]] and is the base for deciding what to look at first.

## Example

| Host | Port | Service | Version banner | Checked |
|------|-----:|---------|----------------|---------|
| 192.0.2.14 | 445 | smb | not recorded | no |
| 192.0.2.14 | 3389 | rdp | enabled | yes |
| 192.0.2.22 | 443 | https | reverse proxy | yes |
| 192.0.2.60 | 9100 | print | raw printing | no |

## How to collect

```bash
nmap -sV "$TARGET" -oX "$OUT/services.xml"
curl -I "https://$DOMAIN"
```

Keep the raw output as evidence ([[Evidence handling]]) and copy only the useful columns here.

## Reading the table

- Checked means we looked at it and wrote the result in the [[Journal keeping]] note.
- A service with a banner that reveals a version is worth a line in a finding, see [[Writing a finding]].

> [!tip]
> Sort by Checked. The empty rows are tomorrow's work, see [[Daily wrap-up]].
