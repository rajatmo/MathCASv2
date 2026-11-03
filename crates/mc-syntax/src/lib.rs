//! The concrete syntax of MathCAS v2 (docs/design/04-syntax.md): lexer, parser, lowering to core terms, and a
//! printer back to text. Generic over [`mc_api::Store`].
//!
//! ```
//! use mc_api::RefStore;
//! let store = RefStore::new();
//! let terms = mc_syntax::read(&store, "f(x) = 2x^2 - 1").unwrap();
//! let printer = mc_syntax::Printer::new(&store);
//! assert_eq!(printer.program(&terms), "f(x) = 2*x^2 - 1");
//! ```
//!
//! - [`parse`] gives the syntax tree ([`ast`]) and every lexical and syntax error with its position.
//! - [`lower_program`] turns the tree into terms (shapes documented on [`Head`]).
//! - [`read`] does both and fails if anything is wrong; nothing of an erroneous input should be evaluated.
//! - [`Printer`] writes terms back; for every term except balls and non-finite machine reals, the text reads back
//!   to the identical term.
pub mod ast;
mod heads;
mod ident;
mod lexer;
mod lower;
mod nat;
mod parser;
mod prec;
mod print;
mod scope;
mod span;
mod stack;

pub use heads::Head;
pub use lower::lower_program;
pub use parser::{parse, Parsed, MAX_DEPTH};
pub use print::{DefaultNumberFormat, NumberFormat, Printer};
pub use span::{render_all, Diagnostic, LineCol, LineIndex, Span};

use mc_api::{Store, T};

/// Parses and lowers a source text: one term per statement, or all errors (syntax errors first; lowering runs only
/// on input without syntax errors).
pub fn read<'h, S: Store + ?Sized>(store: &'h S, src: &str) -> Result<Vec<T<'h>>, Vec<Diagnostic>> {
    // Both phases on one reserved stack (see `stack.rs`); the syntax tree is also dropped there.
    stack::on_big_stack(|| {
        let parsed = parser::parse_here(src);
        if !parsed.errors.is_empty() {
            return Err(parsed.errors);
        }
        lower::lower_here(store, &parsed.program)
    })
}
