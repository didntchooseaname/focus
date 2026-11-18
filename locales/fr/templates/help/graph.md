# Le graphe

Le graphe dessine le coffre : un nœud par note, une arête par lien. Retour à [[index|l'aide]].

## Le graphe entier

Il s'ouvre dans un onglet central. Une disposition par forces est calculée en arrière-plan et s'arrête d'elle-même quand le graphe est stable : un graphe ouvert ne consomme pas de processeur au repos.

- Survolez un nœud : le nœud, ses voisins et leurs arêtes ressortent, le reste s'estompe ; le titre s'affiche.
- Cliquez sur un nœud pour ouvrir sa note. Glissez un nœud pour le déplacer.
- Zoomez avec la molette, centré sur le pointeur. Déplacez la vue en glissant le fond, ou avec les flèches ; `+` et `-` zooment, `0` ajuste tout le graphe à la vue, Échap efface la sélection.
- Les libellés apparaissent selon le zoom et le nombre de liens d'un nœud, et les nœuds hors de la vue ne sont pas dessinés.

Le graphe n'emploie aucune couleur pour dire ce qu'est un nœud. C'est la forme :

| Forme | Nœud |
|---|---|
| rond, plus gros avec plus de liens | note |
| carré | tag |
| anneau vide | lien vers une note qui n'existe pas |

## Options et filtres

| Contrôle | Effet |
|---|---|
| Filtrer par requête | garde les notes qui correspondent à une requête écrite comme dans [[search-syntax\|le panneau de recherche]] |
| Dossiers | garde les notes de certains dossiers |
| Orphelines | montre ou masque les notes sans liens |
| Afficher les nœuds de tags | ajoute un nœud carré par tag, lié à ses notes et à son tag parent |
| Afficher les liens non résolus | ajoute les nœuds en anneau |
| Afficher les pièces jointes | ajoute les fichiers joints |
| Trouver un nœud | centre la vue sur une note et la met en évidence |
| Forces | répulsion, distance des liens et gravité de la disposition |

Les positions sont conservées d'une session à l'autre, et une nouvelle note apparaît près de ses voisines.

## Le graphe local

L'onglet Liens de la barre droite dessine le voisinage de la note active, sur une profondeur de 1 à 3 liens, et vous suit quand vous passez d'une note à l'autre. Cliquer sur un nœud ouvre sa note.

> [!tip] Lire un coffre par thème
> Activez les nœuds de tags : les notes qui partagent un tag se rassemblent autour de lui, et les tags imbriqués (`attack/T1558`) pendent à leur parent.
