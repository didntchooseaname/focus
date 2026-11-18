---
id: F-001
title: Print server accepts the vendor default password
severity: critical
cvss: CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H
cvss_score: 9.8
status: confirmed
assets: [198.51.100.40]
cwe: [CWE-798]
attack: [T1078]
---
# Print server accepts the vendor default password

## Description

The administration page of the print server accepts the password the vendor ships with the device. Method notes: [[Writing a finding]], [[Severity and CVSS]].

## Evidence

Screenshot of the login result at 2026-11-10 10:47, referenced in the [[Journal]].

## Impact

Anyone on the office network can change the device settings and read stored job metadata.

## Remediation

Set a unique password, disable the administration page from user networks, and add the device to the password inventory.

## References

- [[CWE shortlist]]
