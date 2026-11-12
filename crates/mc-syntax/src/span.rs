//! Source positions and diagnostics.
use std::fmt;

/// A byte range `start..end` in the source text.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        Span { start: start as u32, end: end as u32 }
    }
    /// The smallest span covering both.
    pub fn to(self, other: Span) -> Span {
        Span { start: self.start.min(other.start), end: self.end.max(other.end) }
    }
}

/// A line and column, both 1-based; the column counts characters, not bytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for LineCol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// Converts byte offsets of one source text to lines and columns.
pub struct LineIndex<'s> {
    src: &'s str,
    /// Byte offset of the start of every line.
    starts: Vec<usize>,
}

impl<'s> LineIndex<'s> {
    pub fn new(src: &'s str) -> LineIndex<'s> {
        let mut starts = vec![0];
        starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { src, starts }
    }

    pub fn line_col(&self, offset: u32) -> LineCol {
        let offset = (offset as usize).min(self.src.len());
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let start = self.starts[line];
        // Offsets produced by the lexer are always on character boundaries; clamp defensively anyway.
        let mut end = offset;
        while !self.src.is_char_boundary(end) {
            end -= 1;
        }
        let col = self.src[start..end].chars().count();
        LineCol { line: line as u32 + 1, col: col as u32 + 1 }
    }
}

/// An error found while reading source text (lexing, parsing or lowering).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    pub fn new(span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic { span, message: message.into() }
    }

    /// The position of the start of the span in `src`.
    pub fn position(&self, src: &str) -> LineCol {
        LineIndex::new(src).line_col(self.span.start)
    }

    /// `line:col: message`.
    pub fn render(&self, src: &str) -> String {
        format!("{}: {}", self.position(src), self.message)
    }
}

/// Renders a list of diagnostics, one per line.
pub fn render_all(diags: &[Diagnostic], src: &str) -> String {
    let index = LineIndex::new(src);
    let mut out = String::new();
    for d in diags {
        out.push_str(&format!("{}: {}\n", index.line_col(d.span.start), d.message));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_columns() {
        let src = "ab\nθx = 1\n";
        let ix = LineIndex::new(src);
        assert_eq!(ix.line_col(0), LineCol { line: 1, col: 1 });
        assert_eq!(ix.line_col(3), LineCol { line: 2, col: 1 });
        // 'θ' is two bytes; 'x' is the second character of line 2.
        assert_eq!(ix.line_col(5), LineCol { line: 2, col: 2 });
        assert_eq!(ix.line_col(src.len() as u32), LineCol { line: 3, col: 1 });
    }
}
