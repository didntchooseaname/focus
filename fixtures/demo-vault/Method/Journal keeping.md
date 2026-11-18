---
type: method
status: ready
tags: [method, journal]
aliases: [Journal]
last_verified: 2026-10-22
---

# Journal keeping

The journal is one note per engagement, written while working. It is the source of truth for the timeline in the report.

## Entry format

```markdown
## 2026-10-07 14:32

Action: ran a service scan on S1.
Why: build the [[Service inventory]].
Result: 14 hosts up, 3 with remote desktop.
Evidence: evidence/2026-10-07-nmap-s1.xml
```

## Rules

1. Write the entry before the action when it is heavy, after it when it is light.
2. Time stamps use the local time zone and 24 hour clock.
3. One action, one entry. Do not merge.
4. Mistakes stay. Add a correction below, do not rewrite.

> [!tip] Keep it boring
> A journal that reads like a log is a good journal. Interpretation goes in findings.

## Links

- Files named in an entry follow the [[Note naming]] rules.
- Tags used in the journal are listed in the [[Tagging scheme]].
- The [[Daily wrap-up]] condenses each day.
- Timing data goes to [[Time tracking]].
