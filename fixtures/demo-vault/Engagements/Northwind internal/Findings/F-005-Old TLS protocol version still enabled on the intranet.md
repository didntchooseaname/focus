---
id: F-005
title: Old TLS protocol version still enabled on the intranet
severity: low
cvss: CVSS:3.1/AV:N/AC:H/PR:N/UI:N/S:U/C:L/I:N/A:N
cvss_score: 3.7
status: confirmed
assets: [198.51.100.30]
cwe: [CWE-327]
attack: [T1040]
---
# Old TLS protocol version still enabled on the intranet

## Description

The web server still negotiates TLS 1.0 and 1.1 with clients that offer them. Method notes: [[Writing a finding]], [[Severity and CVSS]].

## Evidence

Protocol scan summary saved on 2026-11-10 16:41.

## Impact

Traffic from very old clients can be weakened more easily than with current versions.

## Remediation

Disable TLS 1.0 and 1.1 and keep 1.2 and 1.3.

## References

- [[CWE shortlist]]
