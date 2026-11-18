use focus_md::{Block, BlockKind, ParseOptions, parse};

const NOTE: &str = "---
title: T
tags: [a]
---

# Heading one

Paragraph line one
line two #tag

- item one
- [ ] task two
  continued
  - nested item

1. first
2. second

> quote line
> second quote line

> [!warning]- Folded
> body of the callout
>
> - list in callout

| a | b |
|---|---|
| 1 | 2 |

```bash
echo hi
```

~~~
tilde fence
~~~

    indented code

$$
x^2
$$

```mermaid
flowchart LR
  A --> B
```

![[Other note]]

![pic](a.png)

---

Setext title
============

<div>raw html</div>

<details>
<summary>Sum</summary>

Inside paragraph

```
inside code
```

- inside list
</details>

<details><summary>One block</summary>Whole thing</details>

Last paragraph ^id
";

fn lines_of(source: &str) -> Vec<&str> {
    source.lines().collect()
}

fn walk<'a>(
    blocks: &'a [Block],
    parent: Option<&std::ops::Range<u32>>,
    depth: usize,
    lines: &[&'a str],
    out: &mut Vec<(usize, String, std::ops::Range<u32>)>,
) {
    let mut previous_end = parent.map_or(0, |p| p.start);
    for block in blocks {
        let r = &block.lines;
        assert!(
            r.start < r.end && (r.end as usize) <= lines.len(),
            "{:?} out of bounds {r:?}",
            block.kind
        );
        assert!(
            r.start >= previous_end,
            "{:?} overlaps the previous block ({r:?} after {previous_end})",
            block.kind
        );
        if let Some(p) = parent {
            assert!(
                r.start >= p.start && r.end <= p.end,
                "{:?} {r:?} leaves its container {p:?}",
                block.kind
            );
        }
        assert!(
            !lines[r.start as usize].trim().is_empty(),
            "{:?} starts on a blank line {r:?}",
            block.kind
        );
        assert!(
            !lines[r.end as usize - 1].trim().is_empty(),
            "{:?} ends on a blank line {r:?}",
            block.kind
        );
        previous_end = r.end;
        out.push((depth, lines[r.start as usize].to_string(), r.clone()));
        match &block.kind {
            BlockKind::Quote(inner)
            | BlockKind::Callout { blocks: inner, .. }
            | BlockKind::Details { blocks: inner, .. } => {
                walk(inner, Some(r), depth + 1, lines, out)
            }
            BlockKind::List { items, .. } => {
                let mut item_end = r.start;
                for item in items {
                    assert!(
                        item.lines.start >= item_end && item.lines.end <= r.end,
                        "item {:?} outside its list {r:?}",
                        item.lines
                    );
                    item_end = item.lines.end;
                    walk(&item.blocks, Some(&item.lines), depth + 1, lines, out);
                }
            }
            _ => {}
        }
    }
}

#[test]
fn every_block_covers_its_own_source_lines() {
    let doc = parse(NOTE, &ParseOptions::default());
    let lines = lines_of(NOTE);
    let mut found = Vec::new();
    walk(&doc.blocks, None, 0, &lines, &mut found);
    let ranges: Vec<(usize, u32, u32)> =
        found.iter().map(|(d, _, r)| (*d, r.start, r.end)).collect();
    assert_eq!(
        ranges,
        [
            (0, 5, 6),
            (0, 7, 9),
            (0, 10, 14),
            (1, 10, 11),
            (1, 11, 13),
            (1, 13, 14),
            (2, 13, 14),
            (0, 15, 17),
            (1, 15, 16),
            (1, 16, 17),
            (0, 18, 20),
            (1, 18, 20),
            (0, 21, 25),
            (1, 22, 23),
            (1, 24, 25),
            (2, 24, 25),
            (0, 26, 29),
            (0, 30, 33),
            (0, 34, 37),
            (0, 38, 39),
            (0, 40, 43),
            (0, 44, 48),
            (0, 49, 50),
            (0, 51, 52),
            (0, 53, 54),
            (0, 55, 57),
            (0, 58, 59),
            (0, 60, 71),
            (1, 63, 64),
            (1, 65, 68),
            (1, 69, 70),
            (2, 69, 70),
            (0, 72, 73),
            (1, 72, 73),
            (0, 74, 75),
        ]
    );
    assert_eq!(
        doc.frontmatter.as_ref().map(|f| f.lines.clone()),
        Some(0..4),
        "the frontmatter is a block of its own: the markers are part of its lines"
    );
    assert_eq!(doc.line_count as usize, lines.len());
}

#[test]
fn blank_lines_and_comments_never_move_a_range() {
    let doc = parse(
        "a\n\n\n\n%% comment\nspanning lines %%\n\nb\n\n\n- x\n\n\n- y\n\n\n",
        &ParseOptions::default(),
    );
    let ranges: Vec<_> = doc
        .blocks
        .iter()
        .map(|b| (b.lines.start, b.lines.end))
        .collect();
    assert_eq!(ranges, [(0, 1), (7, 8), (10, 14)]);
}
