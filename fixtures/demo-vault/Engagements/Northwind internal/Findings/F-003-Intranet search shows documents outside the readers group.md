---
id: F-003
title: Intranet search shows documents outside the reader's group
severity: medium
cvss: CVSS:3.1/AV:N/AC:L/PR:L/UI:N/S:U/C:H/I:N/A:N
cvss_score: 6.5
status: reported
assets: [198.51.100.30]
cwe: [CWE-863]
attack: [T1083]
---
# Intranet search shows documents outside the reader's group

## Description

Search results include titles and excerpts of documents restricted to the finance group. Method notes: [[Writing a finding]], [[Severity and CVSS]].

## Evidence

Two search result pages saved with the account name and time.

## Impact

An employee can read excerpts of restricted documents without opening them.

## Remediation

Apply the document permissions to the search index and rebuild it.

## References

- [[CWE shortlist]]
