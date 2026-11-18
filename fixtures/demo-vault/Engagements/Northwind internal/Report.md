# Northwind internal: report

## Summary

Northwind Labs asked for an internal assessment of the office network between 2026-11-09 and 2026-11-20. Five findings are recorded: one critical, one high, two medium and one low. The critical finding concerns a network printer that still accepts the vendor password, and the client has already changed it. Fix the permission of the service folder next, then the intranet search and error page.

## Scope

Included: 198.51.100.0/24 and intranet.northwind.test. Excluded: 198.51.100.250 and payroll.northwind.test.

## Methodology

The work followed the [[Engagement lifecycle]] note: inventory, analysis, confirmation, retest. Every observation has a line in the [[Journal]].

## Findings

| Id | Title | Severity | Score | Status |
|---|---|---|---|---|
| F-001 | Print server accepts the vendor default password | critical | 9.8 | confirmed |
| F-002 | Ordinary users can modify a service folder on the file server | high | 7.8 | confirmed |
| F-003 | Intranet search shows documents outside the reader's group | medium | 6.5 | reported |
| F-004 | Intranet error page repeats a request parameter | medium | 6.1 | draft |
| F-005 | Old TLS protocol version still enabled on the intranet | low | 3.7 | confirmed |

## Hosts

| Host | Address | Status |
|---|---|---|
| dc01 | 198.51.100.10 | enum |
| files01 | 198.51.100.20 | user |
| intranet | 198.51.100.30 | enum |
| print01 | 198.51.100.40 | user |

## Appendices

Checklist progress and the credential table stay in the engagement folder and are not copied here.
