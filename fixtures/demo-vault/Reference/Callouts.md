---
type: reference
status: ready
tags: [reference, writing]
last_verified: 2026-11-12
---

# Callouts

Every common type, used sparingly in [[Executive summary]] and [[Report review]]. Syntax overview in [[Markdown tour]].

> [!note]
> A neutral remark.

> [!info]
> Context that helps the reader.

> [!tip]
> A shortcut that saves time.

> [!example]
> Title: `Weekly status, 2026-11-06`

> [!question]
> Who signs the authorization letter?

> [!warning]
> Production data is out of scope.

> [!danger]
> Stop the test and call the contact if a system stops responding.

> [!bug]
> The export loses the last row when the table is empty.

> [!success]
> All findings retested and closed.

> [!failure]
> The restore test of the backup failed.

> [!quote]
> Write for the person who fixes it.

> [!abstract]
> Three weeks, four findings, no critical issue.

> [!todo]
> Send the cleanup confirmation.

## Foldable

> [!note]- Collapsed by default
> This text is hidden until the callout is opened.

> [!tip]+ Open by default
> This text is visible and can be folded.

> [!warning] Custom title
> A callout with a title of its own and a nested one:
>
> > [!info]
> > Nested callout.
