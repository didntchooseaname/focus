---
ip: 198.51.100.30
hostname: intranet
os: Linux
status: enum
tags: [internal]
---
# intranet (198.51.100.30)

## Notes

Intranet web server. Related findings: F-003, F-004, F-005.

## Services

| Port | Proto | State | Service | Product | Version | Notes |
|---|---|---|---|---|---|---|
| 22 | tcp | open | ssh | OpenSSH | 9.2 | key only |
| 80 | tcp | open | http | nginx | 1.24 | redirects to 443 |
| 443 | tcp | open | https | nginx | 1.24 | old protocol still offered |
