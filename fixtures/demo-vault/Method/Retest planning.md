---
type: method
status: draft
tags: [method, client, cvss]
last_verified: 2026-11-06
---

# Retest planning

A retest checks that findings were fixed. It is smaller than the first test and its scope is the list of findings, not the network.

## Inputs

- The report and its findings ([[Finding template]])
- The client's list of fixes, with dates
- The original [[Scope table]]

## Plan

1. Group findings by system.
2. Start with the highest [[Severity and CVSS]] items.
3. Reuse the original evidence names with a suffix, for example `-retest`.
4. Agree the window in a new [[Rules of engagement]] document.

## Result values

| Value | Meaning |
|-------|---------|
| Fixed | Not reproducible |
| Partly fixed | Reproducible with changes |
| Not fixed | Same as before |
| Not testable | System unavailable |

> [!tip]
> Offer the retest in the [[Cleanup and handover]] message, with a time limit such as 90 days.

Risk labels stay as in the [[Risk rating overview]], even if the client changed the context.
