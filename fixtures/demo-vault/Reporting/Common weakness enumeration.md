---
type: reporting
status: ready
tags: [reporting, cwe]
aliases: [CWE]
last_verified: 2026-10-24
---

# Common weakness enumeration

CWE is a community catalogue of software and hardware weakness types. Each entry has an id such as CWE-79 and a short name. It classifies the kind of mistake, not a specific product bug.

## CWE and CVE

A CVE identifies one flaw in one product version. A CWE names the class that flaw belongs to. A report finding usually cites a CWE, and a CVE only when a known product issue is involved.

## Why cite one

- The client's tracker and scanners often group by CWE.
- The remediation advice for a class is well documented.
- Retests can search for the same class elsewhere, see [[Retest planning]].

## Choosing the id

1. Describe the root cause in one sentence.
2. Pick the most specific entry that fits, not a broad parent.
3. If two fit, choose the one closest to the fix.

The ids used most often by this team are in [[CWE shortlist]]. Put the id in the finding header, see [[Finding template]].

> [!tip] Look it up
> Names and hierarchy change between catalogue releases. Check the current entry on the official CWE site before quoting a name in a deliverable.

The tag `#cwe` marks notes about this topic, see [[Tagging scheme]].
