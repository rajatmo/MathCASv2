//! Running one cell (a source text) in an engine and reporting what happened as [`CellEvent`]s and a [`Status`]:
//! shared by the protocol server ([`crate::server`]) and the notebook runner ([`crate::notebook::run`]).
use mc_bench::{EngineError, StopFlag};
use mc_engine::{CoreEngine, Event, LoadError, Origin, RunError, Script};
use serde::{Deserialize, Serialize};

/// How an evaluation ended (the `status` of the protocol's `done`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// every statement ran and none had a failure as its value
    Ok,
    /// every statement ran, and some top-level statement's value was a `failure(...)`
    Failure,
    /// a read, resolve or load error (nothing of the cell ran), or a malformed request
    Error,
    /// stopped by an interrupt
    Interrupted,
    /// stopped at an engine limit (evaluation depth, fuel)
    Limit,
}

/// An error with its position: the protocol's `error` response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorReport {
    /// the main message, without position
    pub message: String,
    /// the 1-based line of the error in `origin`, if it has a position
    pub line: Option<u32>,
    /// the 1-based column (in characters) of the error in `origin`, if it has a position
    pub column: Option<u32>,
    /// the source the error is in: the cell's origin, or a module file the cell imported
    pub origin: Option<String>,
    /// every message as `origin:line:column: message`, with the source line and a caret
    pub rendered: String,
}

impl ErrorReport {
    /// An error without a position or origin.
    pub fn plain(message: impl Into<String>) -> ErrorReport {
        let message = message.into();
        ErrorReport { rendered: message.clone(), message, line: None, column: None, origin: None }
    }

    fn from_load(e: &LoadError) -> ErrorReport {
        let pos = e.diagnostics.first().map(|d| line_col(&e.text, d.span.start as usize));
        ErrorReport {
            message: e.first_message().to_string(),
            line: pos.map(|p| p.0),
            column: pos.map(|p| p.1),
            origin: Some(e.origin.clone()),
            rendered: e.render(),
        }
    }
}

/// The 1-based line and column (in characters) of byte `offset` of `text`, as `LoadError::render` counts them.
fn line_col(text: &str, offset: usize) -> (u32, u32) {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = before.matches('\n').count() + 1;
    let column = before[line_start..].chars().count() + 1;
    (line as u32, column as u32)
}

/// One thing a cell produced, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellEvent {
    /// a line of `print` output
    Output(String),
    /// the value of the last statement (not sent when it is `null` or a failure already sent)
    Value(String),
    /// a top-level statement whose value is a `failure(...)`, printed
    Failure(String),
    /// the cell did not run (read error) or stopped at a limit
    Error(ErrorReport),
}

/// Evaluate `src` from `origin` in `engine`, handing events to `sink` as they happen (output after each top-level
/// statement), and return how the evaluation ended.
///
/// `line` is the 1-based line of `src`'s first line in its file (1 for a whole file): positions of errors are
/// counted from there, so that a notebook cell's errors point into the notebook. An interrupt sends no event; the
/// status says it.
pub fn run_cell(
    engine: &mut CoreEngine,
    src: &str,
    origin: &Origin,
    line: u32,
    stop: &StopFlag,
    sink: &mut dyn FnMut(CellEvent),
) -> Status {
    // blank lines in front put the text's lines at their place in the file, so every position (also in rendered
    // messages) is counted as in the file, without translating the loader's errors
    let padded;
    let text = if line > 1 {
        padded = format!("{}{src}", "\n".repeat(line as usize - 1));
        padded.as_str()
    } else {
        src
    };
    let mut on_event = |e: &Event| {
        sink(match e {
            Event::Output(s) => CellEvent::Output(s.clone()),
            Event::Failure(s) => CellEvent::Failure(s.clone()),
        })
    };
    match engine.eval_streaming(text, origin, stop, &mut on_event) {
        Ok(script) => {
            if let Some(v) = shown_value(&script) {
                sink(CellEvent::Value(v.to_string()));
            }
            if script.failed() {
                Status::Failure
            } else {
                Status::Ok
            }
        }
        Err(RunError::Read(e)) => {
            sink(CellEvent::Error(ErrorReport::from_load(&e)));
            Status::Error
        }
        Err(RunError::Engine(EngineError::Stopped)) => Status::Interrupted,
        Err(RunError::Engine(EngineError::Limit(what))) => {
            sink(CellEvent::Error(ErrorReport::plain(format!("limit: {what}"))));
            Status::Limit
        }
        Err(RunError::Engine(e)) => {
            sink(CellEvent::Error(ErrorReport::plain(e.to_string())));
            Status::Error
        }
    }
}

/// The value of the last statement as a cell shows it: not `null`, and not a failure (those are shown as they
/// happen). The same rule as scripts.
fn shown_value(s: &Script) -> Option<&str> {
    let failed_last = matches!(s.events.last(), Some(Event::Failure(f)) if *f == s.last.printed);
    (!s.last_is_null && !failed_last).then_some(s.last.printed.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_characters_from_one() {
        assert_eq!(line_col("ab\ncd", 0), (1, 1));
        assert_eq!(line_col("ab\ncd", 4), (2, 2));
        assert_eq!(line_col("\u{3b1}\u{3b2} x", 5), (1, 4));
        assert_eq!(line_col("ab", 99), (1, 3));
    }
}
