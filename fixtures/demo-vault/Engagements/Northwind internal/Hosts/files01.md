---
ip: 198.51.100.20
hostname: files01
os: Windows Server
status: user
tags: [internal]
---
# files01 (198.51.100.20)

## Notes

File server. Related finding: F-002.

## Services

| Port | Proto | State | Service | Product | Version | Notes |
|---|---|---|---|---|---|---|
| 445 | tcp | open | microsoft-ds |  |  | three shares |
| 3389 | tcp | open | ms-wbt-server |  |  | network level authentication on |
