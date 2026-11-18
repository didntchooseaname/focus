---
type: reference
status: draft
domain: rendu
tags: [focus/demo, markdown]
---

# Tout le Markdown

Paragraphe avec **gras**, *italique*, ~~barré~~, ==surligné==, `code en ligne`, un tag
#markdown/inline, une note de bas de page[^1] et un lien vers [[Accueil|l'accueil]].
Deuxième ligne du même paragraphe, assez longue pour vérifier la mesure de la colonne
de lecture, l'interligne et l'absence de coupure au milieu d'un mot composé.

%% Ce commentaire ne doit pas apparaître dans le rendu. %%

## Titres

### Titre de niveau 3

#### Titre de niveau 4

##### Titre de niveau 5

###### Titre de niveau 6

## Listes

- Élément simple
- Élément avec sous-liste
  - Niveau 2
    - Niveau 3
- Dernier élément

1. Premier
2. Deuxième
   1. Imbriqué
3. Troisième

- [ ] Tâche ouverte
- [x] Tâche terminée
- [/] Tâche en cours
- [-] Tâche annulée

## Citations et callouts

> Une citation simple, sur deux lignes,
> avec un **mot en gras**.

> [!note]
> Callout sans titre.

> [!tip] Astuce avec titre
> Corps de l'astuce, avec `du code` et un [[Kerberoasting|lien]].

> [!warning]+ Avertissement repliable, ouvert
> Contenu visible au chargement.

> [!danger]- Danger repliable, fermé
> Contenu masqué au chargement.

> [!example] Exemple avec liste
> - premier point
> - second point
>
> ```bash
> echo "bloc de code dans un callout"
> ```

> [!question] Type moins courant
> Les types `success`, `failure`, `bug`, `quote`, `abstract` existent aussi.

## Code

```bash
# Énumération SMB
nxc smb $RHOST -u '$USER' -p '$PASS' --shares
smbclient //$RHOST/share -U "$DOMAIN/$USER%$PASS"
```

```powershell
Get-ADUser -Filter {ServicePrincipalName -ne "$null"} -Properties ServicePrincipalName
```

```python
def crack(hashes: list[str], wordlist: str) -> dict[str, str]:
    """Renvoie les mots de passe retrouvés."""
    return {h: p for h in hashes if (p := lookup(h, wordlist))}
```

```
Bloc sans langage, avec une ligne très longue qui doit défiler horizontalement plutôt que de revenir à la ligne dans le bloc de code, pour garder l'alignement des sorties de terminal.
```

## Tableaux

| Port | Protocole | Service | Remarque |
|-----:|:---------:|---------|----------|
| 88 | tcp | kerberos | KDC du domaine |
| 389 | tcp | ldap | signature non exigée |
| 445 | tcp | microsoft-ds | SMBv1 désactivé |
| 5985 | tcp | winrm | accès avec `svc_backup` |

## Images et inclusions

![[capture-exemple.png|420]]

![Image par lien Markdown](../Red%20Team/assets/capture-exemple.png)

![[Kerberoasting#Détection]]

## Mathématiques

En ligne : $H = -\sum_i p_i \log_2 p_i$. En bloc :

$$
P(\text{collision}) \approx 1 - e^{-n^2 / 2N}
$$

---

Texte après un filet horizontal. Identifiant de bloc en fin de ligne. ^bloc-final

[^1]: Texte de la note de bas de page, avec un [lien](https://example.org).
