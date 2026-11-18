---
type: reporting
status: ready
tags: [reporting, cvss]
aliases: [Scoring]
last_verified: 2026-10-27
---

# Severity and CVSS

Severity is the label the client acts on. CVSS is one input to it. The score comes from a formula, the label from the score plus context.

## Qualitative bands

The CVSS specification maps scores to names.

| Score | Rating |
| --- | --- |
| 0.0 | None |
| 0.1 to 3.9 | Low |
| 4.0 to 6.9 | Medium |
| 7.0 to 8.9 | High |
| 9.0 to 10.0 | Critical |

## Using the score

- Compute the base score with [[CVSS 3.1 metrics]] or [[CVSS 4.0 overview]].
- Record the vector string next to the number, so anyone can recompute it.
- Adjust the final rating only with a written reason, for example a compensating control seen during the test.

> [!note] One version per report
> Mixing 3.1 and 4.0 scores in the same report confuses comparisons. Choose one in the kickoff, see [[Kickoff meeting]].

## Beyond the score

A score of 5.3 on the public login page can matter more than a 7.5 on a lab server nobody uses. The team rating used during the test is explained in [[Risk rating overview]]. Write the reasoning in the finding, following [[Writing a finding]].

## Example

```text
CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:N/A:N : 7.5 High
```
