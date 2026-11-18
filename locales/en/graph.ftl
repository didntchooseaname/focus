graph-title = Graph
graph-empty = No notes to draw yet.
graph-stats =
    { $notes ->
        [one] { $notes } note
       *[other] { $notes } notes
    }, { $links ->
        [one] { $links } link
       *[other] { $links } links
    }
graph-filter-query = Filter by query
graph-filter-folders = Folders
graph-filter-orphans = Orphans
graph-show-tags = Show tag nodes
graph-show-unresolved = Show unresolved links
graph-show-attachments = Show attachments
graph-depth = Depth { $depth }
graph-find-node = Find a node
graph-forces = Forces
graph-force-repulsion = Repulsion
graph-force-distance = Link distance
graph-force-gravity = Gravity
graph-settling = Laying out the graph…
graph-node-degree =
    { $incoming ->
        [one] { $incoming } incoming link
       *[other] { $incoming } incoming links
    }, { $outgoing ->
        [one] { $outgoing } outgoing link
       *[other] { $outgoing } outgoing links
    }
graph-controls = Graph settings
graph-zoom-in = Zoom in
graph-zoom-out = Zoom out
graph-zoom-fit = Fit to view
graph-zoom-level = { $percent }%
graph-find-none = No node matches “{ $query }”
graph-animate = Animate
graph-animate-preparing = Preparing the replay…
graph-animate-play = Play
graph-animate-pause = Pause
graph-animate-end = Full graph
graph-animate-speed-half = 0.5x
graph-animate-speed-one = 1x
graph-animate-speed-two = 2x
graph-animate-speed-four = 4x
