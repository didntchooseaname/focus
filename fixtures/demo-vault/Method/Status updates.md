---
type: method
status: ready
tags: [method, client]
aliases: [Progress mail]
last_verified: 2026-10-15
---

# Status updates

A status update tells the client where we are, without surprises. It is short and regular.

## Rhythm

- Engagements under one week: one mail at the end.
- Longer engagements: one mail every Friday.
- Anything urgent: not a status update, see [[Incident during a test]].

## Format

1. Period covered
2. What we did, one line per area
3. What we plan next
4. Questions for the client
5. Findings that need early attention, without details

## Example

> [!example] Update of 2026-10-09
> Covered 2026-10-05 to 2026-10-09. Network discovery complete for S1 and S4. Web application review started. Next week: authenticated testing. One question: please confirm the test account does not expire on 2026-10-12.

## Notes

- Never send exploit details in a mail. Details belong in the report ([[Finding template]]).
- The source is the [[Daily wrap-up]].
- Contacts and channels are in [[Client communication]].
