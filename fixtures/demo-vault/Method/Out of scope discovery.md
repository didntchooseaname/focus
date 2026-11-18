---
type: method
status: draft
tags: [method, scoping, client]
last_verified: 2026-10-30
---

# Out of scope discovery

During testing we sometimes meet hosts or data that the [[Scope table]] does not list. We do not use them.

## Procedure

1. Stop at the boundary. Do not scan or log in.
2. Record what made us see it: a DNS name, a link, a certificate.
3. Add a row to the [[Asset inventory]] with status "to confirm".
4. Ask the client ([[Client communication]]).
5. Wait for a written answer.

## Example

> [!example] Unlisted host
> While reviewing `portal.northwind.example.com` we saw a redirect to `staging.northwind.example.com` (203.0.113.40). The host is not in S1 to S4. We noted the redirect, did not connect, and asked the client.

## Possible answers

- Added to scope: new version of the table and a new line in the [[Rules of engagement]] if limits differ.
- Refused: it stays in the inventory as out of scope, and the report can mention it as an observation ([[Writing a finding]]).

> [!question] Is a DNS lookup a test?
> Passive lookups of public names are fine. Anything that sends traffic to the host is not.

Related: [[Authorization letter]] for third-party hosts.
