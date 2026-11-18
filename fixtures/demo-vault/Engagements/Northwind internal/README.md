---
type: internal-pentest
status: in-progress
client: Northwind Labs
started: 2026-11-09T09:00:00
deadline: 2026-11-20
scope:
  - 198.51.100.0/24
  - intranet.northwind.test
out_of_scope:
  - 198.51.100.250
  - payroll.northwind.test
vars:
  DOMAIN: northwind.test
  USER: svc-audit
---
# Northwind internal

## Objectives

Assess how far an employee workstation connection can reach inside the Northwind Labs office network, and report what should be fixed first. See [[Engagement lifecycle]] and [[Scope table]].

## Scope

Included: the office range 198.51.100.0/24 and the intranet site. Excluded: the backup appliance at 198.51.100.250 and the payroll site. The signed [[Authorization letter]] is stored with the client file.

## Rules of engagement

- Working hours 08:00 to 18:00, no tests on Fridays after 16:00.
- No denial of service, no social engineering, no physical access.
- Stop and call the contact below if a production service degrades. See [[Rules of engagement]] and [[Incident during a test]].

## Contacts

| Role | Name | Channel |
|---|---|---|
| Client sponsor | Alex Doe | alex.doe@northwind.test |
| Technical contact | Sam Roe | sam.roe@northwind.test |
| Emergency line | Northwind operations | +00 0 000 000 |

## Schedule

| Date | Step |
|---|---|
| 2026-11-09 | Kickoff and scope check |
| 2026-11-10 to 2026-11-13 | Inventory and analysis |
| 2026-11-16 to 2026-11-18 | Confirmation of findings |
| 2026-11-20 | Report delivered |

## Notes

Journal in [[Journal]], findings in the Findings folder, report in [[Report]].
