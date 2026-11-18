---
type: method
status: ready
tags: [method, evidence]
last_verified: 2026-10-16
---

# Evidence handling

Evidence is what lets the client reproduce a finding. It is stored once, named well and never edited.

## Rules

- Keep raw output, not only the cleaned version.
- Do not collect more personal data than the finding needs. Blur or truncate, and say so.
- Hash each file when it is stored.
- Evidence stays in the engagement folder until the [[Cleanup and handover]].

## Layout

```text
evidence/
  2026-10-07-nmap-s1.xml
  2026-10-07-portal-login.png
  sha256sums.txt
```

```bash
sha256sum evidence/* > evidence/sha256sums.txt
```

## Checklist for one finding

- [x] Raw capture stored
- [x] Screenshot taken ([[Screenshot conventions]])
- [x] Journal entry links the file ([[Journal keeping]])
- [ ] Sensitive values masked in the copy used for the report

> [!warning] Real data in a test
> If we see live customer data, we stop, note it, and follow [[Incident during a test]]. The [[Finding template]] has a field for what was exposed, not for the data itself.
