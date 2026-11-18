---
type: method
status: ready
tags: [method, journal]
last_verified: 2026-10-19
---

# Tagging scheme

Tags group notes across folders. We keep a short list and avoid inventing new ones in the moment.

## Tag list

| Tag | Use |
|-----|-----|
| #method | Method notes |
| #scoping | Scope, authorization, kickoff |
| #evidence | Capture and storage |
| #journal | Journal and naming |
| #time | Hours and planning |
| #client | Anything the client sees or says |
| #cvss | Scoring notes |

## Rules

- Tags go in the frontmatter list, and inline only when a sentence needs one.
- Lower case, no spaces, no accents.
- A new tag needs a row in the table above.
- Do not tag by status. The `status` property exists for that, see [[Properties]].

## Example

```yaml
tags: [method, evidence]
status: draft
```

> [!info]
> The same list applies to [[Journal keeping]] and [[Note naming]]. Reporting notes add `reporting` and `writing`, see [[Finding template]].
