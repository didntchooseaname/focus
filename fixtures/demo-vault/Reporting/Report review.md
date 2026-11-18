---
type: reporting
status: ready
tags: [reporting, writing]
last_verified: 2026-11-08
---

# Report review

A second person reads every report before it leaves the team. The author does not review their own text.

## Pass 1: facts

- [ ] Hosts, URLs and dates match the scope in [[Scope table]]
- [ ] Every finding has evidence that supports its claim
- [ ] Vector strings recompute to the stated score, see [[Severity and CVSS]]
- [ ] No real secrets in screenshots, see [[Evidence handling]]

## Pass 2: structure

- [ ] Findings follow [[Finding template]]
- [ ] Ids are unique and in order
- [ ] Terms are defined in [[Report glossary]] or in the text

## Pass 3: language

- [ ] Titles state the problem
- [ ] Remediation starts with a verb, see [[Remediation wording]]
- [ ] The [[Executive summary]] matches the final counts

## Comments

Reviewers leave comments as a list with the finding id first, for example `F-003: evidence does not show the host name`. The author answers each one and marks it done.

<details>
<summary>Common misses</summary>

Copied text from an earlier report with the old client name, inconsistent date formats, and severity changed without updating the summary table.

</details>
