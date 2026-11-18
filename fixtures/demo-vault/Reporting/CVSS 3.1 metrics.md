---
type: reporting
status: ready
tags: [reporting, cvss]
last_verified: 2026-10-30
---

# CVSS 3.1 metrics

CVSS 3.1 describes a weakness with eight base metrics. Each takes one of a few values, and together they give a score from 0.0 to 10.0. Rating bands are in [[Severity and CVSS]].

## Vector string

The vector lists the version, then each metric as `abbreviation:value`, separated by slashes.

```text
CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H
```

This one reads: reachable over the network, low complexity, no privileges, no user interaction, scope unchanged, high impact on all three properties of the CIA triad. It scores 9.8.

## Exploitability metrics

| Metric | Values | Question it answers |
| --- | --- | --- |
| AV, attack vector | N, A, L, P | From where can it be reached: network, adjacent, local, physical |
| AC, attack complexity | L, H | Does success depend on conditions outside the attacker's control |
| PR, privileges required | N, L, H | What access does the attacker need first |
| UI, user interaction | N, R | Must another person do something |

## Scope and impact metrics

| Metric | Values | Meaning |
| --- | --- | --- |
| S, scope | U, C | Does the impact cross a security boundary |
| C, confidentiality | N, L, H | How much data is disclosed |
| I, integrity | N, L, H | How much data can be altered |
| A, availability | N, L, H | How much service is lost |

## Practical notes

- Score the weakness as found, not the worst imaginable chain.
- Temporal and environmental metrics exist but most reports stop at base.
- The newer revision is covered in [[CVSS 4.0 overview]].
- Store the vector in the finding, see [[Finding template]].
