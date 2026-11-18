---
type: reporting
status: draft
tags: [reporting, cvss]
last_verified: 2026-11-03
---

# CVSS 4.0 overview

CVSS 4.0 keeps the idea of a base score from a vector string but reshapes the metrics. This note records the differences that matter when writing a report. For the previous version see [[CVSS 3.1 metrics]].

## What changed

- Scope is gone. Impact is split between the vulnerable system and any subsequent systems.
- A new base metric, attack requirements (AT), separates deployment conditions from attack complexity.
- User interaction has finer values: none, passive, active.
- Supplemental metrics such as safety and automatable carry context but do not change the score.
- Nomenclature names which parts were used: CVSS-B for base only, CVSS-BT when threat is added.

## Vector shape

```text
CVSS:4.0/AV:N/AC:L/AT:N/PR:N/UI:N/VC:H/VI:H/VA:H/SC:N/SI:N/SA:N
```

The impact letters carry a prefix: V for the vulnerable system, S for subsequent systems.

> [!question] Which version to use
> Clients with older tooling may only accept 3.1. Ask during the kickoff and note the answer in [[Severity and CVSS]].

## Working rule

Pick one version per engagement and state it in the methodology section of the report. The checks of [[Report review]] include a pass over vector strings for consistency.
