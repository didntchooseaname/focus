---
id: F-002
title: Ordinary users can modify a service folder on the file server
severity: high
cvss: CVSS:3.1/AV:L/AC:L/PR:L/UI:N/S:U/C:H/I:H/A:H
cvss_score: 7.8
status: confirmed
assets: [198.51.100.20]
cwe: [CWE-732]
attack: [T1574]
---
# Ordinary users can modify a service folder on the file server

## Description

The permissions of a folder used by a Windows service allow write access to the Users group. Method notes: [[Writing a finding]], [[Severity and CVSS]].

## Evidence

Permission listing saved as Evidence/f-002-permissions.txt on 2026-11-11.

## Impact

A local user could change what the service runs and gain its privileges.

## Remediation

Restrict write access to administrators and the service account, then review other service folders.

## References

- [[CWE shortlist]]
