//! [`LoadError`]: an error found while reading, resolving or loading source, with its position.
use mc_syntax::{Diagnostic, LineIndex};
use std::fmt;

/// A read-time error (decision F.6): the source it is in, and one or more positioned messages (none for a file
/// that could not be read at all). Nothing of the input that caused it was evaluated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    /// the origin's name: a file path, `<repl>`, `<-e>`, ...
    pub origin: String,
    /// the text the diagnostics point into
    pub text: String,
    /// the errors, the first one being the main one
    pub diagnostics: Vec<Diagnostic>,
    /// a message without position (a file that could not be read)
    pub message: Option<String>,
    /// whether the error is in the text given to the loader, rather than in a module file it loaded
    pub in_input: bool,
}

impl LoadError {
    /// Errors at positions of `text`.
    pub(crate) fn at(origin: &str, text: &str, diagnostics: Vec<Diagnostic>, in_input: bool) -> LoadError {
        LoadError { origin: origin.into(), text: text.into(), diagnostics, message: None, in_input }
    }

    /// An error without a position.
    pub(crate) fn plain(origin: &str, message: String, in_input: bool) -> LoadError {
        LoadError { origin: origin.into(), text: String::new(), diagnostics: vec![], message: Some(message), in_input }
    }

    /// The main message, without position.
    pub fn first_message(&self) -> &str {
        match (&self.message, self.diagnostics.first()) {
            (Some(m), _) => m,
            (None, Some(d)) => &d.message,
            (None, None) => "error",
        }
    }

    /// Every message as `origin:line:column: message`, each followed by its source line and a caret under the
    /// column.
    pub fn render(&self) -> String {
        if let Some(m) = &self.message {
            return format!("{}: {m}", self.origin);
        }
        let index = LineIndex::new(&self.text);
        let mut out = String::new();
        for (i, d) in self.diagnostics.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let pos = index.line_col(d.span.start);
            let line = self.text.lines().nth(pos.line as usize - 1).unwrap_or("").trim_end_matches('\r');
            // tabs stay tabs, so the caret lines up however the terminal shows them
            let caret: String =
                line.chars().take(pos.col as usize - 1).map(|c| if c == '\t' { '\t' } else { ' ' }).collect();
            out.push_str(&format!("{}:{}: {}\n  {line}\n  {caret}^", self.origin, pos, d.message));
        }
        out
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

impl std::error::Error for LoadError {}
