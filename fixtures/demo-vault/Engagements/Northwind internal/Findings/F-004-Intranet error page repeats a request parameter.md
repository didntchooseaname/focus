---
id: F-004
title: Intranet error page repeats a request parameter
severity: medium
cvss: CVSS:3.1/AV:N/AC:L/PR:N/UI:R/S:C/C:L/I:L/A:N
cvss_score: 6.1
status: draft
assets: [198.51.100.30]
cwe: [CWE-79]
attack: [T1190]
---
# Intranet error page repeats a request parameter

## Description

The not-found page prints the requested path back into the page without encoding it. Method notes: [[Writing a finding]], [[Severity and CVSS]].

## Evidence

Page source excerpt, to be added.

## Impact

A crafted link could show altered content to the employee who opens it.

## Remediation

Encode the path in the error template and add a content security policy.

## References

- [[CWE shortlist]]
