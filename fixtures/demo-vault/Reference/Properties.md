---
type: reference
status: ready
tags: [reference, method]
aliases: [Frontmatter types]
last_verified: 2026-11-12
title_text: Properties demo
reviewers: [alice, bob, chen]
review_date: 2026-11-12
reviewed: true
score: 4
---

# Properties

The frontmatter above holds one property of each type: text (`title_text`), list (`reviewers`, `tags`), date (`review_date`), checkbox (`reviewed`) and number (`score`).

## Types

| Property | Type | Example |
| -------- | ---- | ------- |
| title_text | text | Properties demo |
| reviewers | list | alice, bob, chen |
| review_date | date | 2026-11-12 |
| reviewed | checkbox | true |
| score | number | 4 |

## Conventions

Keys are English and stable, see [[Note naming]] and [[Tagging scheme]]. Values drive the table of [[Findings overview.base|findings]] when notes carry findings.

> [!tip]
> Keep `last_verified` current, the weekly check lives in [[Weekly review]].
