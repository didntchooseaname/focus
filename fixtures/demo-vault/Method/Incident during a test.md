---
type: method
status: ready
tags: [method, client, evidence]
last_verified: 2026-10-26
---

# Incident during a test

An incident is something unplanned that matters more than the test: an outage we may have caused, signs of a real attacker, or exposure of live data.

## First five minutes

1. Stop the activity that might be involved.
2. Write the time and the last action in the [[Journal keeping]] note.
3. Call the emergency contact from the [[Rules of engagement]].
4. Confirm by mail ([[Client communication]]).

## Cases

| Case | What we do |
|------|------------|
| Service degraded after our action | Stop, call, wait for the client decision |
| Traces of an earlier intrusion | Stop, preserve notes, call |
| Exposed personal data | Stop, do not copy more, call |
| Doubt about authorization | Stop, check the [[Authorization letter]] |

> [!warning]
> Do not clean up on your own. Deleting our own files can destroy what the client's responders need.

## Afterwards

- [ ] Write a short incident note with the timeline
- [ ] Decide with the client whether testing resumes
- [ ] Add the event to the [[Report review]] checklist
