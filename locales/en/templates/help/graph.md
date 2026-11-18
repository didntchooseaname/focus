# The graph

The graph draws the vault: one node per note, one edge per link. Back to [[index|Help]].

## The whole graph

It opens in a central tab. A layout based on forces is computed in the background and stops by itself once the graph is stable, so an open graph uses no processor time at rest.

- Hover a node: the node, its neighbors and their edges stand out and the rest fades; the title is shown.
- Click a node to open its note. Drag a node to move it.
- Zoom with the wheel, centered on the pointer. Pan by dragging the background, or with the arrow keys; `+` and `-` zoom, `0` fits the whole graph in the view, Escape clears the selection.
- Labels appear according to the zoom and to how many links a node has, and nodes outside the view are not drawn.

The graph uses no color to say what a node is. The shape does:

| Shape | Node |
|---|---|
| round, bigger with more links | note |
| square | tag |
| empty ring | link to a note that does not exist |

## Options and filters

| Control | Effect |
|---|---|
| Filter by query | keeps the notes that match a query written like in [[search-syntax\|the search panel]] |
| Folders | keeps the notes of some folders |
| Orphans | shows or hides notes that have no links |
| Show tag nodes | adds a square node per tag, linked to its notes and to its parent tag |
| Show unresolved links | adds the ring nodes |
| Show attachments | adds the attached files |
| Find a node | centers the view on a note and highlights it |
| Forces | repulsion, link distance and gravity of the layout |

Positions are kept from one session to the next, and a new note appears near its neighbors.

## The local graph

The Links tab of the right bar draws the neighborhood of the active note, at a depth of 1 to 3 links, and follows you as you move between notes. Clicking a node opens its note.

> [!tip] Reading a vault by theme
> Turn on the tag nodes: notes that share a tag gather around it, and nested tags (`attack/T1558`) hang from their parent.
