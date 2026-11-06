//! The inline markup of prose cells: plain text and code spans between backticks.

/// A run of prose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Span<'a> {
    /// plain text
    Text(&'a str),
    /// the inside of a code span
    Code(&'a str),
}

/// The runs of `text`, or the byte offset of a backtick that opens a span nobody closes.
pub fn spans(text: &str) -> Result<Vec<Span<'_>>, usize> {
    let mut out = vec![];
    let mut rest = text;
    let mut at = 0;
    while let Some(open) = rest.find('`') {
        if open > 0 {
            out.push(Span::Text(&rest[..open]));
        }
        let after = &rest[open + 1..];
        let close = after.find('`').ok_or(at + open)?;
        out.push(Span::Code(&after[..close]));
        let used = open + 1 + close + 1;
        at += used;
        rest = &rest[used..];
    }
    if !rest.is_empty() {
        out.push(Span::Text(rest));
    }
    Ok(out)
}

/// The text without markup (an unclosed backtick is kept as it is).
pub fn plain(text: &str) -> String {
    match spans(text) {
        Ok(s) => s
            .iter()
            .map(|s| match s {
                Span::Text(t) | Span::Code(t) => *t,
            })
            .collect(),
        Err(_) => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_spans() {
        assert_eq!(spans("`d(f, x)` gives f'"), Ok(vec![Span::Code("d(f, x)"), Span::Text(" gives f'")]));
        assert_eq!(spans("a `b` c"), Ok(vec![Span::Text("a "), Span::Code("b"), Span::Text(" c")]));
        assert_eq!(spans(""), Ok(vec![]));
        assert_eq!(spans("x `y"), Err(2));
        assert_eq!(spans("`a` and `b"), Err(8));
        assert_eq!(plain("`a` b"), "a b");
    }
}
