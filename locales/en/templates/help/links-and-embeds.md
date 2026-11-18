# Links and embeds

A link counts wherever it is written, and Focus keeps links valid when you rename or move notes. Back to [[index|Help]].

## Kinds of links

| You write | Meaning |
|---|---|
| `[[Note]]` | link to a note, by name |
| `[[Note\|text]]` | the same, shown as `text` |
| `[[Note#Heading]]` | link to a heading of the note |
| `[[Note#^block]]` | link to a block that ends with `^block` |
| `[text](folder/note.md)` | Markdown link with a path relative to the current note |
| `![[Note]]` | embed: the content of the note is shown in place |
| `![[Note#Heading]]` | embed one section |
| `![[image.png\|300]]` | image, 300 pixels wide |
| `#tag/sub-tag` | tag; a click runs the `tag:` search |

Click a link to open it, Ctrl click to open it in a new tab. A link that points to nothing is drawn with a wavy underline and counts as an unresolved link, listed in the bottom panel under Broken links.

## How a name is resolved

Focus follows Obsidian's rules, in this order:

1. a path relative to the note that holds the link;
2. a path from the root of the vault;
3. the only file with that name, wherever it is;
4. an alias declared in the frontmatter of a note.

If none matches, the link is unresolved.

## Links outside the text

A link is recognised in every place it can be written:

- body text, headings, lists, tables, callouts, footnotes and image captions;
- frontmatter properties, as a value (`related: "[[Note]]"`) or in a list. In reading view the property is shown as a chip whose link can be clicked;
- Mermaid diagrams, in a node label (`A["See [[Note]]"]`, shown without the brackets) or with a click directive (`click A "[[Note]]"`).

In the HTML export a linked diagram node is a link. The reading view does not draw clickable diagram nodes yet.

All of these count for backlinks, outgoing links, the graph and the MCP tools. When you rename or move a note, Focus rewrites the links to it in other notes (Settings, section Files and links, can turn this off), and the subfolder of its attachments follows it.

## Footnotes

`[^1]` in the text and `[^1]: the note` on its own line. References are numbered in order of appearance, whatever label you chose. Hovering a reference shows the footnote, clicking goes to it, and an arrow in the footnote goes back.

## Properties, tags and comments

- The frontmatter between two `---` lines is shown as chips at the top of a note in reading view.
- `%% text %%` is a comment: hidden when reading, kept in the file.
- `==text==` highlights.

> [!note] Callouts
> A quote that starts with `[!note]`, `[!warning]` and the other Obsidian types is drawn as a callout. `[!note]+` makes it foldable and open, `[!note]-` foldable and closed. This box is one.
