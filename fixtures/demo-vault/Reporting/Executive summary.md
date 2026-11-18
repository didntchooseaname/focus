---
type: reporting
status: draft
tags: [reporting, writing, client]
last_verified: 2026-11-06
---

# Executive summary

The executive summary is read by people who will not open the findings. It fits on one page and avoids technical terms the reader has to look up.

## Structure

1. Scope and dates: what was tested and when.
2. Overall result in two or three sentences.
3. The three findings that matter most, each with its business consequence.
4. Next steps and who owns them.

## Example paragraph

> [!quote] Northwind Labs, internal test
> Between 12 and 23 October 2026 we tested the internal network and the customer portal. We found that the administration console can be used without signing in, which exposes customer records. We recommend fixing this first.

## Tone

- Plain words, short sentences.
- Say what happened and what it means, not how it was done.
- Counts by severity can go in a small table, ratings come from [[Severity and CVSS]].

| Severity | Count |
| --- | --- |
| Critical | 1 |
| High | 3 |
| Medium | 5 |
| Low | 4 |

## Before sending

The summary is checked last, after the findings are final, as part of [[Report review]]. Contact with the client about it follows [[Client communication]]. Wording of fixes stays in [[Remediation wording]].
