---
type: reporting
status: ready
tags: [reporting, writing]
last_verified: 2026-10-29
---

# Remediation wording

The remediation section is the part the client engineer reads closely. It has to say what to change, where, and how to confirm the change worked.

## Rules

- Start with a verb: "Require", "Remove", "Rotate", "Enable".
- Name the component and the setting, not just the goal.
- Give one action per sentence.
- Say how to verify the fix, so the retest in [[Retest planning]] is quick.
- Avoid "should consider" and "it is recommended". State the action.

## Examples

> [!example] Weak and strong
> Weak: "Authentication should be improved on the admin page."
> Strong: "Require a valid session for every route under /admin. Return 401 for requests without one. Verify by requesting /admin with no cookie."

| Class | Wording pattern |
| --- | --- |
| Missing authentication | Require a session on the route and test the unauthenticated request |
| Hard-coded credentials | Rotate the secret, move it to a secret store, remove it from the repository |
| Cleartext transmission | Enable TLS on the listener and redirect plain connections |

## Short term and long term

Give both when they differ. A quick mitigation such as a firewall rule buys time while the code change is scheduled. See [[Writing a finding]] for where each goes, and [[CWE shortlist]] for the classes above.

Pitfalls to avoid in the executive version are in [[Executive summary]].
