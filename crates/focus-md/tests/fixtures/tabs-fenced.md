---
title: Lab service certificates
tags: [lab, example]
---

# Issuing certificates in the lab

Invented notes about a fictional certificate service called Orchard CA. Nothing here describes a real product.

## Overview

The service signs short-lived certificates for lab machines. Each request goes through the same three steps.

> [!NOTE]
> The lab hosts are named after fruit trees.

## Requesting a certificate

````tabs

tab: Linux

Install the client from the package archive:

```bash
sudo apt install orchard-client
```

> [!TIP]
> Use the `--dry-run` flag to see what would be requested.

Then request the certificate:

```bash
orchard request --host pear.lab --out pear.pem
```

The file is written next to the key.

tab: Windows

Open an elevated prompt and run:

```powershell
orchard.exe request --host pear.lab --out pear.pem
```

Check the result:

```powershell
Get-ChildItem pear.pem
```

The same options exist on both systems.
````

## Renewing

 ```tabs

== Linux

Renew with `orchard renew`.

== Windows

Renew with `orchard.exe renew`.

> [!WARNING]
> Renewal fails after the expiry date.
 ```

## Revoking

`````tabs

Text before the first tab line belongs to a tab without a title.

tab: Server

````tabs

tab: Inner A

Leaf A.

tab: Inner B

Leaf B.

````

tab: Client

```text
tab: this line is code, not a tab
```

Client text.

`````

## See also

[Orchard CA home](https://example.org/orchard)
