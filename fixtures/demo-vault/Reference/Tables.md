---
type: reference
status: ready
tags: [reference, writing]
last_verified: 2026-11-12
---

# Tables

Alignment and a long table. Applied in [[Scope table]], [[Risk rating overview]] and [[CWE shortlist]].

## Alignment

| Left | Center | Right |
| :--- | :----: | ----: |
| a | b | 1 |
| longer text | mid | 22 |
| x | y | 333 |

## Long table

| Id | Asset | Address | Service | Port | Status |
| -- | ----- | ------- | ------- | ---: | ------ |
| 1 | web01 | 192.0.2.10 | HTTP | 80 | open |
| 2 | web01 | 192.0.2.10 | HTTPS | 443 | open |
| 3 | db01 | 192.0.2.11 | PostgreSQL | 5432 | filtered |
| 4 | mail01 | 192.0.2.12 | SMTP | 25 | open |
| 5 | mail01 | 192.0.2.12 | IMAPS | 993 | open |
| 6 | dns01 | 192.0.2.13 | DNS | 53 | open |
| 7 | nas01 | 192.0.2.20 | SMB | 445 | filtered |
| 8 | nas01 | 192.0.2.20 | SSH | 22 | open |
| 9 | print01 | 192.0.2.30 | IPP | 631 | open |
| 10 | jump01 | 198.51.100.5 | SSH | 22 | open |
| 11 | vpn01 | 198.51.100.6 | OpenVPN | 1194 | open |
| 12 | app01 | 203.0.113.10 | HTTPS | 8443 | open |
| 13 | app01 | 203.0.113.10 | SSH | 22 | filtered |
| 14 | ci01 | 203.0.113.11 | HTTPS | 443 | open |
| 15 | wiki01 | 203.0.113.12 | HTTP | 80 | closed |

Table with formatting: **bold**, `code`, [[Weekly review]] link.
