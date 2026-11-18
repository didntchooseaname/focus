---
type: reference
status: ready
tags: [reference, writing]
last_verified: 2026-11-12
---

# Tabs

Tabs group alternatives. Used in [[Finding template]], [[Remediation wording]] and [[Code samples]].

## Inventory commands by platform

````tabs
tab: Linux

List listening services on a host you own:

```bash
ss -tlnp
```

> [!tip]
> Save the output as evidence, see [[Evidence handling]].

tab: Windows

```powershell
Get-NetTCPConnection -State Listen
```

```tabs
tab: Short
One line per listener.
tab: Detailed
Add the owning process with `Get-Process`.
```

tab: Notes

## Hidden heading

- [ ] Compare with the [[Service inventory]]
- Link to [[Home]]
````

## Two tabs, plain text

````tabs
tab: English
Remediation: restrict access to the management interface.

tab: French
Remédiation : restreindre l'accès à l'interface d'administration.
````
