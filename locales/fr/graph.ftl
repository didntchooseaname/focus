graph-title = Graphe
graph-empty = Aucune note à dessiner pour l'instant.
graph-stats =
    { $notes ->
        [one] { $notes } note
       *[other] { $notes } notes
    }, { $links ->
        [one] { $links } lien
       *[other] { $links } liens
    }
graph-filter-query = Filtrer par requête
graph-filter-folders = Dossiers
graph-filter-orphans = Orphelins
graph-show-tags = Afficher les nœuds de tags
graph-show-unresolved = Afficher les liens non résolus
graph-show-attachments = Afficher les pièces jointes
graph-depth = Profondeur { $depth }
graph-find-node = Chercher un nœud
graph-forces = Forces
graph-force-repulsion = Répulsion
graph-force-distance = Distance des liens
graph-force-gravity = Gravité
graph-settling = Calcul de la disposition…
graph-node-degree =
    { $incoming ->
        [one] { $incoming } lien entrant
       *[other] { $incoming } liens entrants
    }, { $outgoing ->
        [one] { $outgoing } lien sortant
       *[other] { $outgoing } liens sortants
    }
graph-controls = Réglages du graphe
graph-zoom-in = Zoomer
graph-zoom-out = Dézoomer
graph-zoom-fit = Tout afficher
graph-zoom-level = { $percent } %
graph-find-none = Aucun nœud ne correspond à « { $query } »
graph-animate = Animer
graph-animate-preparing = Préparation de la relecture…
graph-animate-play = Lire
graph-animate-pause = Pause
graph-animate-end = Graphe complet
graph-animate-speed-half = 0,5x
graph-animate-speed-one = 1x
graph-animate-speed-two = 2x
graph-animate-speed-four = 4x
