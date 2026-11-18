# Focus changes to mermaid-rs-renderer 0.3.1

- `src/text_metrics.rs`: font database provided by the caller (`init_fonts`, `font_database`), measurement and rasterisation on the same database; removed the font cache written to `~/.cache/mmdr`.
- `src/lib.rs`: re-exports `init_fonts` and `font_database`.
- `src/theme.rs`: adds `mix_colors` (mix of two `#rrggbb` colors) and `readable_on` (most contrasting text on a background).
- `src/render.rs`: single text size (state diagrams no longer use 0.85 times `font_size`); gantt and timeline titles at 1.15 times `font_size`.
- `src/render.rs` (gantt): grid in `primary_border_color` instead of `#E2E8F0`, section label centered on the full section height, rules between tasks stopped before the section column, fonts aligned on `font_size`, bar text color chosen by contrast.
- `src/layout/gantt.rs`: grayscale palette derived from the theme instead of shifted hues; section column as wide as its text, task column limited to labels that do not fit in their bar.
- `src/render.rs` (pie): slice text chosen by contrast with the slice; text outside slices in `text_color`.
- `src/config.rs`: mindmap fields `node_stroke`, `root_stroke`, `stroke_width`.
- `src/layout/mindmap.rs`: configurable outline for nodes and root; edges run from the border of one box to the border of the other.
- `src/render.rs` (mindmap): no divider line under nodes when `divider_line_width` is 0.
- `src/layout/label_placement.rs`: class cardinalities placed beside the edge, clear of the arrow tip and of the box.
- `src/layout/er.rs` (new), `src/layout/mod.rs`: row based layout of entity relationship diagrams, with room for labels and cardinality symbols.
- `src/render.rs` (ER): cardinality symbols drawn outward from the box and oriented at both ends, circle filled with the background color; opaque background behind relation labels.
- `src/layout/gantt.rs`: axis ticks on whole days with a step of at least one day, so a short chart never repeats a date.
- `src/parser.rs`: a chain of open links (`A["x"] --- B["y"] --- C`) was read as the edge `A -- B["y"] -- C` with a text label, which gave the middle node its whole source as label. The text-label form now needs an opening link of exactly two dashes or equal signs (`labelled_arrow`), as in Mermaid's grammar.
