use std::ops::Range;

use focus_kit::Tokens;
use focus_ops::vars::{self, Context, Substituted};
use gpui::{HighlightStyle, UnderlineStyle, px};

pub fn code_block_variables(code: &str, context: &Context) -> Substituted {
    vars::substitute(code, context)
}

pub fn variable_highlights(
    t: &Tokens,
    substituted: &Substituted,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut out: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    for (range, _) in &substituted.resolved {
        out.push((
            range.clone(),
            HighlightStyle {
                color: Some(t.bg),
                background_color: Some(t.ink_3),
                ..HighlightStyle::default()
            },
        ));
    }
    for (range, _) in &substituted.missing {
        out.push((
            range.clone(),
            HighlightStyle {
                underline: Some(UnderlineStyle {
                    thickness: px(1.),
                    color: Some(t.warn),
                    wavy: true,
                }),
                ..HighlightStyle::default()
            },
        ));
    }
    out.sort_by_key(|(range, _)| range.start);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(pairs: &[(&str, &str)]) -> Context {
        Context {
            values: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn resolved_and_missing_ranges_point_into_the_resolved_text() {
        let result = code_block_variables(
            "ping $RHOST -c 1 $LHOST",
            &context(&[("RHOST", "10.0.0.5")]),
        );
        assert_eq!(result.text, "ping 10.0.0.5 -c 1 $LHOST");
        let (range, name) = &result.resolved[0];
        assert_eq!(
            (&result.text[range.clone()], name.as_str()),
            ("10.0.0.5", "RHOST")
        );
        let (range, name) = &result.missing[0];
        assert_eq!(
            (&result.text[range.clone()], name.as_str()),
            ("$LHOST", "LHOST")
        );
    }

    #[test]
    fn highlights_are_sorted_and_cover_both_kinds() {
        let result = code_block_variables("a $LHOST b $RHOST", &context(&[("RHOST", "x")]));
        let styles = variable_highlights(&Tokens::of(focus_kit::Mode::Dark), &result);
        assert_eq!(styles.len(), 2);
        assert!(styles[0].0.start < styles[1].0.start);
        assert!(styles[0].1.underline.is_some() && styles[1].1.background_color.is_some());
    }
}
