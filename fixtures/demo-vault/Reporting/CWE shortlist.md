---
type: reporting
status: ready
tags: [reporting, cwe]
last_verified: 2026-11-05
---

# CWE shortlist

Ids that come up often in our findings. Check the exact wording on the official catalogue before copying a name into a report, see [[Common weakness enumeration]].

| Id | Short name | In one sentence |
| --- | --- | --- |
| CWE-79 | Cross-site scripting | The application puts untrusted input into a page without encoding it for that context. |
| CWE-89 | SQL injection | Input becomes part of a database query instead of staying a value. |
| CWE-287 | Improper authentication | The application accepts a claim of identity without proving it. |
| CWE-306 | Missing authentication for critical function | A sensitive action can be reached with no login at all. |
| CWE-798 | Hard-coded credentials | A password or key is fixed in code or configuration shipped to others. |
| CWE-200 | Exposure of sensitive information | Data reaches someone who should not see it. |
| CWE-319 | Cleartext transmission | Sensitive data travels over a channel without encryption. |
| CWE-521 | Weak password requirements | The password policy lets users choose guessable secrets. |

## Using the list

- Pick the id that matches the root cause, not the symptom.
- Put it in the finding header from [[Finding template]].
- Pair it with a rating from [[Severity and CVSS]].
- Fix advice that fits each class is drafted in [[Remediation wording]].

> [!note] Not exhaustive
> Anything missing here is looked up and then added to this table.
