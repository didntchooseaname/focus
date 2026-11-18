---
type: daily
status: stale
tags: [daily, lab]
last_verified: 2026-09-14
---

# Home network

Small network at home, also used as a target in [[Project training lab]].

## Layout

| Device | Address | Role |
| ------ | ------- | ---- |
| Router | 192.0.2.1 | gateway and DHCP |
| NAS | 192.0.2.20 | backups |
| Printer | 192.0.2.30 | printing |

Inventory approach follows [[Asset inventory]] and [[Service inventory]].

## Tasks

- [x] Change the default admin password of the router
- [ ] Move the printer to a guest network
- [ ] Test the NAS restore monthly

> [!note]
> The wifi password is not stored in this vault.
