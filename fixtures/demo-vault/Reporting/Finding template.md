---
type: reporting
status: ready
tags: [reporting, writing]
aliases: [Finding skeleton]
last_verified: 2026-10-20
---

# Finding template

Every finding in a report uses the same fields in the same order. The reader learns where to look, and the retest can compare like with like. See [[Writing a finding]] for the style of each field.

## Fields

| Field | Content |
| --- | --- |
| Title | What is wrong, in one line |
| Severity | Rating from [[Severity and CVSS]] |
| Weakness | A CWE id from [[CWE shortlist]] |
| Affected | Host, URL or component |
| Description | What the issue is and why it matters |
| Evidence | Screenshot or output, see [[Evidence handling]] |
| Remediation | Concrete fix, see [[Remediation wording]] |

## Skeleton

```markdown
## F-001 Title of the finding

- Severity: High
- Weakness: CWE-306
- Affected: https://portal.northwind.example.com/admin

### Description

### Evidence

### Remediation
```

> [!tip] Keep ids stable
> Number findings once and never reuse a number, even when a finding is dropped. The retest in [[Retest planning]] refers to these ids.

## Checklist before saving

- [x] Title names the weakness, not the tool
- [x] Affected line lists every instance
- [ ] Evidence is redacted where needed
