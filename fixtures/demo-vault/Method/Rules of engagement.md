---
type: method
status: ready
tags: [method, scoping, client]
aliases: [RoE]
last_verified: 2026-10-08
---

# Rules of engagement

The rules of engagement (RoE) state how we test. The [[Authorization letter]] says that we may test, and the RoE says under which limits.

## Sections of the document

- Test window, with time zone
- Allowed techniques, and techniques that need a call first
- Rate limits and load ceilings
- Source addresses we test from
- Emergency contact on both sides
- What we do on finding live malicious activity, see [[Incident during a test]]

## Example clause

> [!example] Window and rate
> Testing runs Monday to Friday, 08:00 to 18:00 UTC, from 198.51.100.20 and 198.51.100.21. Automated scans stay under 50 requests per second against `portal.northwind.example.com`.

## Rules we apply to ourselves

1. Stay inside the [[Scope table]]. A doubt is a question to the client, not a guess.
2. Write the action in the journal before running anything heavy ([[Journal keeping]]).
3. Stop and call when a service degrades.

> [!warning]
> A verbal change to the RoE is not a change. Ask for it by mail and file the reply with the [[Client communication]] notes.
