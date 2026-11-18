# Syntaxe de recherche

Le panneau de recherche trouve du texte dans toutes les notes pendant la frappe. Retour à [[index|l'aide]].

## Termes

Les mots séparés par une espace doivent tous figurer dans la note. La casse est ignorée sauf si le terme contient une majuscule, et les accents sont ignorés sauf si le terme en contient un (un réglage peut changer cela). Un terme de moins de trois caractères ne correspond qu'à des mots entiers.

| Vous écrivez | Focus trouve |
|---|---|
| `golden ticket` | les notes qui contiennent les deux mots |
| `"golden ticket"` | les notes qui contiennent la phrase exacte |
| `kerberoast OR asreproast` | les notes qui contiennent l'un ou l'autre |
| `golden -silver` | les notes avec `golden` et sans `silver` |
| `(kerberoast OR asreproast) -lab` | les parenthèses groupent les termes |
| `/CVE-\d{4}-\d+/` | une expression régulière entre barres obliques |

`OR` lie moins fort que l'espace : `a b OR c` signifie `(a et b) ou c`.

## Opérateurs

| Opérateur | Restreint à | Exemple |
|---|---|---|
| `tag:` | les notes avec ce tag, sous-tags compris | `tag:#red-team` |
| `path:` | les notes dont le chemin contient le texte | `path:Certifications` |
| `file:` | les notes dont le nom de fichier contient le texte | `file:index` |
| `title:` | les notes dont le titre contient le texte | `title:Kerberos` |
| `heading:` | les notes avec un titre de section qui contient le texte | `heading:Enumeration` |
| `task:` | les notes avec une tâche qui contient le texte | `task:review` |
| `task-todo:`, `task-done:` | la même chose, tâches ouvertes ou terminées seulement | `task-todo:report` |
| `prop:` | les notes dont une propriété du frontmatter correspond | `prop:status=ready` |

Un opérateur s'applique au terme qui le suit. Un opérateur inconnu, comme `foo:bar`, est cherché comme du texte et le panneau le signale sous le champ. Un guillemet ou une parenthèse non fermés sont aussi signalés ici, et la requête s'exécute quand même.

## Options et résultats

- Trois bascules sous le champ : respecter la casse, mot entier, et expression régulière. Avec l'expression régulière activée, tout le texte du champ est une seule expression.
- L'ordre de tri est la pertinence, le chemin, le plus récent d'abord ou le plus ancien d'abord.
- Les résultats sont groupés par note. Chaque note montre son titre, son dossier et son nombre d'occurrences, et peut être repliée. Chaque ligne montre son numéro de ligne et l'occurrence.
- La ligne sélectionnée montre ses occurrences en vidéo inverse, les autres lignes les montrent en gras soulignées. Les flèches déplacent, Entrée ouvre la note à cette ligne.
- La requête est relancée à chaque frappe et la précédente est annulée : la saisie n'attend jamais une recherche.

## Recherche assistée

Avec une clé d'API et l'interrupteur IA actif, Entrée sur une requête demande au modèle de l'élargir en plusieurs requêtes, les exécute, fusionne et classe les résultats, et écrit une courte réponse avec des citations numérotées au-dessus de la liste. Les requêtes générées sont affichées sous forme de pastilles que vous pouvez modifier, retirer ou compléter. Voir [[ai-and-privacy|IA et confidentialité]] pour ce qui est envoyé. Sans clé, ou quand la requête échoue, les résultats locaux restent et une seule ligne dit ce qui s'est passé.

## Remplacer dans tout le coffre

Ctrl Maj H ajoute un champ de remplacement sous la requête. Focus liste, note par note, chaque ligne avant et après le changement, avec une case à cocher sur chaque note et chaque ligne. Rien n'est écrit tant que vous n'appuyez pas sur Appliquer.

- Le texte remplacé vient de la requête : ses termes simples, une phrase quand vous écrivez des mots à la suite, ou toute l'expression en mode expression régulière. Les opérateurs comme `tag:` ne font que choisir les notes examinées.
- En mode expression régulière, `$1` dans le remplacement désigne le premier groupe.
- Une note modifiée sur le disque après l'aperçu est laissée telle quelle et listée dans le compte rendu.
- Les fichiers qui ne sont pas de l'UTF-8 valide sont écartés et comptés.
