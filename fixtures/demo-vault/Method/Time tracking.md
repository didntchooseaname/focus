---
type: method
status: ready
tags: [method, time]
last_verified: 2026-10-24
---

# Time tracking

Time is tracked per activity, not per minute. The numbers feed the invoice and the next quote.

## Categories

| Code | Activity |
|------|----------|
| SCO | Scoping and paperwork |
| REC | Discovery and inventory |
| TST | Testing |
| DOC | Notes and reporting |
| COM | Calls and mail |

## Rules

1. Round to 15 minutes.
2. Log at the end of the day in the [[Daily wrap-up]].
3. Keep the totals in the engagement note.
4. Compare estimate and actual at [[Cleanup and handover]].

## Example

```json
{ "date": "2026-10-07", "entries": [
  { "code": "REC", "hours": 2.5 },
  { "code": "TST", "hours": 4.0 } ] }
```

> [!tip] Estimates
> Quote reporting at one third of the testing time. In our last three engagements it was between 0.28 and 0.4.

See also [[Time management]] for exam practice and [[Journal keeping]] for the time stamps.
