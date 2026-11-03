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
//! - [`scope`] and [`Heads`] let later passes over lowered terms (name resolution in `mc-load`) see local bindings
//!   exactly as lowering and the printer do; [`identifiers`] gives the position of every name in a text.
pub mod ast;
mod heads;
mod ident;
mod lexer;
mod lower;
mod nat;
mod parser;
mod prec;
mod print;
pub mod scope;
mod span;
mod stack;

pub use heads::{as_sym, Head, Heads};
/// The reserved words of the language (04 section 2), for editor support (completion, grammars).
pub use ident::KEYWORDS;
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

/// One statement read by [`read_spanned`]: its term and its span in the source text, and for a `module`
/// statement the spans of the statements of its body (in order, one per element of the body list).
#[derive(Clone, Debug)]
pub struct Spanned<'h> {
    /// the lowered statement
    pub term: T<'h>,
    /// the statement's text
    pub span: Span,
    /// the statements of a module body (empty for other statements)
    pub body: Vec<Span>,
}

/// [`read`], keeping the source span of every statement (and of the statements of a module body), for passes that
/// report errors about whole statements.
pub fn read_spanned<'h, S: Store + ?Sized>(store: &'h S, src: &str) -> Result<Vec<Spanned<'h>>, Vec<Diagnostic>> {
    stack::on_big_stack(|| {
        let parsed = parser::parse_here(src);
        if !parsed.errors.is_empty() {
            return Err(parsed.errors);
        }
        let terms = lower::lower_here(store, &parsed.program)?;
        // lowering succeeded, so it produced one term per statement, and one body element per module statement
        Ok(parsed
            .program
            .stmts
            .iter()
            .zip(terms)
            .map(|(s, term)| {
                let body = match &s.kind {
                    ast::StmtKind::Module { body, .. } => body.iter().map(|b| b.span).collect(),
                    _ => vec![],
                };
                Spanned { term, span: s.span, body }
            })
            .collect())
    })
}

/// Every name token of a source text (plain, qualified `M.f` or backquoted), with its span, in text order. Used to
/// point diagnostics of later passes, which see only lowered terms, at the name they are about.
pub fn identifiers(src: &str) -> Vec<(String, Span)> {
    lexer::lex(src)
        .tokens
        .into_iter()
        .filter_map(|t| match t.tok {
            lexer::Tok::Ident(n) => Some((n, t.span)),
            _ => None,
        })
        .collect()
}
