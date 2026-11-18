#!/usr/bin/env bash
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
vault=$root/target/mentions-query-vault
rm -rf "${vault:?}"
mkdir -p "$vault/Attacks" "$vault/Notes"
cat > "$vault/Kerberos.md" <<'NOTE'
---
aliases: [krb5]
---
# Kerberos

Tickets found in the vault:

```query
ticket
```

A query that matches nothing:

```query
zzzunknownword
```

A query that does not parse:

```query
"unclosed phrase
```
NOTE
cat > "$vault/Attacks/Golden ticket.md" <<'NOTE'
# Golden ticket

A forged Kerberos ticket signed with the krbtgt hash.
Request the ticket, then inject it into the session.
NOTE
cat > "$vault/Attacks/AS-REP roasting.md" <<'NOTE'
# AS-REP roasting

Accounts without krb5 pre-authentication give a crackable ticket.
`Kerberos` in code and [[Kerberos]] as a link do not count.
NOTE
cat > "$vault/Notes/Lab journal.md" <<'NOTE'
# Lab journal

Ran kerberos enumeration against the lab domain controller.
#kerberos is a tag, https://example.com/Kerberos an address.
NOTE
"$root/tools/shot.sh" "$here/mentions-query.focus" "$vault"
