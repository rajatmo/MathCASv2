//! The engine interface: what the harness and the REPL need from an evaluator.
//!
//! The harness never parses or evaluates the concrete syntax itself. The caller supplies an [`Engine`] that takes
//! source text, evaluates it in its session and returns the printed result; the harness only times, guards and
//! checks. This keeps `mc-bench` free of dependencies on `mc-syntax` and `mc-eval` (decision D.1).
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A shared stop flag (semantics 10). The harness raises it when a case reaches its time limit, the REPL when the
/// user presses Ctrl+C. Engines poll it in every input-dependent loop and return [`EngineError::Stopped`].
#[derive(Clone, Debug, Default)]
pub struct StopFlag(Arc<AtomicBool>);

impl StopFlag {
    /// A new, lowered flag.
    pub fn new() -> StopFlag {
        StopFlag::default()
    }
    /// Raise the flag: the engine should stop at its next poll.
    pub fn raise(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    /// Lower the flag before a new evaluation.
    pub fn clear(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
    /// Whether the flag is raised. Cheap enough to call in inner loops.
    pub fn is_raised(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// The result of one successful evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evaluated {
    /// The result printed in the concrete syntax.
    pub printed: String,
    /// A structural fingerprint of the result, for `exact hash` checks of results too large to store. Engines that
    /// cannot compute one return `None`, and hash checks then report `unchecked`.
    pub fingerprint: Option<u64>,
}

/// Why an evaluation produced no result. Failures of the language (`failure(...)` terms, semantics 10) are ordinary
/// results, not errors: they are printed and checked like any value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    /// The source text did not parse; `offset` is a byte offset into the text given to [`Engine::eval`].
    Syntax { message: String, offset: usize },
    /// The stop flag was raised and the evaluation stopped cleanly.
    Stopped,
    /// The engine stopped at one of its own limits (for example a recursion or memory budget it enforces itself).
    Limit(String),
    /// Any other error the engine reports.
    Other(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Syntax { message, offset } => write!(f, "syntax error at byte {offset}: {message}"),
            EngineError::Stopped => write!(f, "stopped"),
            EngineError::Limit(what) => write!(f, "limit: {what}"),
            EngineError::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// An evaluator with a session: definitions made by one call to [`Engine::eval`] are visible to the next.
pub trait Engine {
    /// The name of the system and its version, recorded in every result row (for example `mathcas-v2 0.1`).
    fn name(&self) -> String;

    /// Parse and evaluate `src` (a sequence of statements in the concrete syntax) and return the value of the last
    /// statement. Must poll `stop` in every input-dependent loop.
    fn eval(&mut self, src: &str, stop: &StopFlag) -> Result<Evaluated, EngineError>;

    /// Define the symbol `name` with the value of the most recent successful evaluation, so that code evaluated
    /// later can refer to it (property checks use `result`, the REPL may use it for the last output).
    fn bind_last(&mut self, name: &str) -> Result<(), EngineError>;
}

/// Makes fresh engines: each case runs in a new session (02-benchmarks section 1), and cases that ask for it get a
/// new session for every repetition.
pub type EngineFactory = dyn Fn() -> Box<dyn Engine>;
