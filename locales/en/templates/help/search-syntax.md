# Search syntax

The search panel finds text in every note as you type. Back to [[index|Help]].

## Terms

Words separated by a space must all be in the note. Case is ignored unless the term has an upper case letter, and accents are ignored unless the term has one (a setting can change this). A term of fewer than three characters only matches whole words.

| You write | Focus finds |
|---|---|
| `golden ticket` | notes that contain both words |
| `"golden ticket"` | notes that contain the exact phrase |
| `kerberoast OR asreproast` | notes that contain either word |
| `golden -silver` | notes with `golden` and without `silver` |
| `(kerberoast OR asreproast) -lab` | parentheses group terms |
| `/CVE-\d{4}-\d+/` | a regular expression between slashes |

`OR` binds less tightly than a space: `a b OR c` means `(a and b) or c`.

## Operators

| Operator | Restricts to | Example |
|---|---|---|
| `tag:` | notes with this tag, nested tags included | `tag:#red-team` |
| `path:` | notes whose path contains the text | `path:Certifications` |
| `file:` | notes whose file name contains the text | `file:index` |
| `title:` | notes whose title contains the text | `title:Kerberos` |
| `heading:` | notes with a heading that contains the text | `heading:Enumeration` |
| `task:` | notes with a task that contains the text | `task:review` |
| `task-todo:`, `task-done:` | the same, open tasks or done tasks only | `task-todo:report` |
| `prop:` | notes whose frontmatter property matches | `prop:status=ready` |

An operator applies to the term that follows it. An unknown operator, such as `foo:bar`, is searched as text and the panel says so under the field. An unclosed quotation mark or parenthesis is also reported there, and the query still runs.

## Options and results

- Three toggles under the field: match case, whole word, and regular expression. With the regular expression toggle on, the whole text of the field is one expression.
- The sort order is relevance, path, newest first or oldest first.
- Results are grouped by note. Each note shows its title, its folder and its number of matches, and can be folded. Each line shows its line number and the match.
- The selected line shows its matches in inverse video, the other lines show them in bold with an underline. Arrows move, Enter opens the note at that line.
- The query is run again after every keystroke and the previous run is cancelled, so typing never waits for a search.

## Assisted search

With an API key and the AI switch on, pressing Enter on a query asks the model to widen it into several queries, runs them, merges and ranks the results, and writes a short answer with numbered citations above the list. The generated queries are shown as chips that you can edit, remove or add to. See [[ai-and-privacy|AI and privacy]] for what is sent. Without a key, or when the request fails, the local results stay and a single line says what went wrong.

## Replace across the vault

Ctrl Shift H adds a replace field under the query. Focus lists, note by note, each line before and after the change, with a checkbox on every note and every line. Nothing is written until you press Apply.

- The text replaced comes from the query: its plain terms, a phrase when you write words one after the other, or the whole expression in regular expression mode. Operators such as `tag:` only choose which notes are looked at.
- In regular expression mode, `$1` in the replacement stands for the first group.
- A note that changed on disk after the preview is left as it is and listed in the report.
- Files that are not valid UTF-8 are left out and counted.
