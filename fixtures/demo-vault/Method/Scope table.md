---
type: method
status: ready
tags: [method, scoping]
last_verified: 2026-10-14
---

# Scope table

One table per engagement, versioned. It lists every target we may touch and every target we must avoid.

## Format

| Id | Target | Type | Status | Note |
|----|--------|------|--------|------|
| S1 | 192.0.2.0/25 | Network range | In scope | Office LAN |
| S2 | `portal.northwind.example.com` | Web application | In scope | Test accounts provided |
| S3 | `mail.northwind.example.com` | Hosted mail | Out of scope | Run by a provider |
| S4 | 198.51.100.0/28 | Network range | In scope | Business hours only |

![[scope-map.png]]

## Rules

- A target is in scope only if a row says so.
- A change is a new version of the table, signed off by the client contact ([[Client communication]]).
- Hosts found later go through [[Out of scope discovery]].
- The table feeds the [[Asset inventory]] and the [[Service inventory]].

> [!tip]
> Keep the status column to three values: in scope, out of scope, to confirm. It makes filtering easy.

Version history lives in the note properties, see [[Properties]].
