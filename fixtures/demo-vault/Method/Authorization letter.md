---
type: method
status: ready
tags: [method, scoping, client]
last_verified: 2026-09-18
---

# Authorization letter

The letter is the legal basis for the test. It is signed by someone who has the authority to allow testing of the listed assets, which is not always the person we talk to every day.

## Content

- Names of the client entity and of our team
- Reference to the [[Scope table]] by version and date
- Test window, copied from the [[Rules of engagement]]
- Statement that third-party hosts need their own approval
- Signature, date and role of the signer

## Third parties

Cloud and hosting providers sometimes own the ground under a target. Ask the client who hosts what, and note the answer in the [[Asset inventory]].

> [!warning] Hosting you do not control
> A signature from Northwind Labs does not cover a mail provider, a CDN or a shared hosting platform.

## Filing

- [x] PDF stored in the engagement folder
- [x] Date and signer copied into the [[Engagement lifecycle]] gate list
- [ ] Reminder set for the end date

```yaml
authorization:
  client: Northwind Labs
  signed_by: IT director
  scope_version: 3
  valid_until: 2026-11-30
```
