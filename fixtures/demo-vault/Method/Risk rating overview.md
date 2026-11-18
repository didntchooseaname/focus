---
type: method
status: ready
tags: [method, cvss, client]
aliases: [Risk levels]
last_verified: 2026-11-10
---

# Risk rating overview

We rate each finding on five levels. The score from [[Severity and CVSS]] is an input, and context can move the final level by one step, never more.

## Levels

| Level | Meaning | Typical client reaction |
|-------|---------|-------------------------|
| Critical | Direct, large impact, easy to reach | Fix now |
| High | Large impact or easy to reach | Fix in the week |
| Medium | Needs conditions | Plan in the quarter |
| Low | Minor impact | Fix when convenient |
| Informational | No direct risk | Read and decide |

## Context that moves a rating

- Exposure: internet facing or internal only
- Compensating controls that we observed
- Value of the data on the system

> [!note] Write the reason
> When we move a level, the finding says why in one sentence ([[Writing a finding]]).

## Math

The scoring formula is shown in [[CVSS 3.1 metrics]]. For a reader who wants a rough idea: $risk \approx likelihood \times impact$.

## Where it is used

- [[Status updates]] mention levels only as counts.
- [[Retest planning]] sorts by level.
- [[Remediation wording]] depends on it.
