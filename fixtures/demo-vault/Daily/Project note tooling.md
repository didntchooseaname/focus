---
type: daily
status: draft
tags: [daily, journal]
last_verified: 2026-11-11
---

# Project note tooling

Side project: small scripts around the notes. Naming rules come from [[Note naming]] and [[Tagging scheme]].

## Ideas

- [ ] Check that every note has a `last_verified` date
- [ ] List notes older than 60 days, see [[Weekly review]]
- [x] Rename screenshots from a folder

## Sketch

```python
from pathlib import Path

for note in Path("vault").rglob("*.md"):
    if "last_verified" not in note.read_text():
        print(note)
```

> [!info]
> The script only reads files. It never edits notes.
