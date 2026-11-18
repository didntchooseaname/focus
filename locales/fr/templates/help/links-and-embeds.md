# Liens et inclusions

Un lien compte partout où il est écrit, et Focus garde les liens valides quand vous renommez ou déplacez des notes. Retour à [[index|l'aide]].

## Sortes de liens

| Vous écrivez | Signification |
|---|---|
| `[[Note]]` | lien vers une note, par son nom |
| `[[Note\|texte]]` | la même chose, affichée `texte` |
| `[[Note#Titre]]` | lien vers un titre de la note |
| `[[Note#^bloc]]` | lien vers un bloc qui se termine par `^bloc` |
| `[texte](dossier/note.md)` | lien Markdown avec un chemin relatif à la note courante |
| `![[Note]]` | inclusion : le contenu de la note est montré sur place |
| `![[Note#Titre]]` | inclure une seule section |
| `![[image.png\|300]]` | image, 300 pixels de large |
| `#tag/sous-tag` | tag ; un clic lance la recherche `tag:` |

Cliquez sur un lien pour l'ouvrir, Ctrl clic pour l'ouvrir dans un nouvel onglet. Un lien qui ne mène nulle part est dessiné avec un soulignement ondulé et compte comme lien non résolu, listé dans le panneau du bas sous Liens cassés.

## Comment un nom est résolu

Focus suit les règles d'Obsidian, dans cet ordre :

1. un chemin relatif à la note qui contient le lien ;
2. un chemin depuis la racine du coffre ;
3. le seul fichier de ce nom, où qu'il soit ;
4. un alias déclaré dans le frontmatter d'une note.

Si rien ne correspond, le lien est non résolu.

## Liens hors du texte

Un lien est reconnu à tous les endroits où il peut être écrit :

- texte courant, titres, listes, tableaux, callouts, notes de bas de page et légendes d'images ;
- propriétés du frontmatter, en valeur (`related: "[[Note]]"`) ou dans une liste. En vue de lecture la propriété est montrée en pastille dont le lien se clique ;
- diagrammes Mermaid, dans le libellé d'un nœud (`A["Voir [[Note]]"]`, affiché sans les crochets) ou avec une directive click (`click A "[[Note]]"`).

Dans l'export HTML un nœud de diagramme lié est un lien. La vue de lecture ne dessine pas encore de nœuds cliquables.

Tous ces liens comptent pour les rétroliens, les liens sortants, le graphe et les outils MCP. Quand vous renommez ou déplacez une note, Focus réécrit les liens qui la visent dans les autres notes (les réglages, section Fichiers et liens, peuvent le désactiver), et le sous-dossier de ses pièces jointes la suit.

## Notes de bas de page

`[^1]` dans le texte et `[^1]: la note` seule sur sa ligne. Les références sont numérotées dans l'ordre d'apparition, quelle que soit l'étiquette choisie. Survoler une référence montre la note, cliquer y mène, et une flèche dans la note ramène au texte.

## Propriétés, tags et commentaires

- Le frontmatter entre deux lignes `---` est montré en pastilles en haut d'une note en vue de lecture.
- `%% texte %%` est un commentaire : masqué en lecture, conservé dans le fichier.
- `==texte==` surligne.

> [!note] Callouts
> Une citation qui commence par `[!note]`, `[!warning]` et les autres types d'Obsidian est dessinée comme un callout. `[!note]+` le rend repliable et ouvert, `[!note]-` repliable et fermé. Ce cadre en est un.
