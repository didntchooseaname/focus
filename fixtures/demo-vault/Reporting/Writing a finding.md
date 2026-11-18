---
type: reporting
status: ready
tags: [reporting, writing]
last_verified: 2026-10-22
---

# Writing a finding

A finding has to be understood by an engineer who was not in the room and by a manager who will read only the first paragraph. The layout comes from [[Finding template]].

## Title

State the problem and the place. "Administration console reachable without authentication" says more than "Access control issue".

## Description

Three short parts, in this order:

1. What the weakness is.
2. Where it was observed, with the host or URL.
3. What an attacker gains, in terms of the client's data or service.

## Evidence

Show the minimum that proves the point. Crop screenshots, mask secrets, and name files following [[Screenshot conventions]].

> [!example] Before and after
> Weak: "The server is vulnerable to bad authentication."
> Better: "The /admin page on portal.northwind.example.com returns the dashboard without a session cookie."

## Rating

Pick the rating with [[Severity and CVSS]], then check that the text supports it. A Critical rating with a mild description is the first thing a reviewer will question, see [[Report review]].

## Remediation

One fix the client can act on this week, and one longer term fix if it exists. Wording advice is in [[Remediation wording]].

> [!warning] No payloads
> The report describes impact and reproduction at a level a defender can verify. It does not ship working attack material.
