//! Extend selection (08-notebook.md section 3.4): the smallest syntactic unit strictly larger than a selection.
//!
//! The units of a text are its tokens, the expressions of its syntax tree (statements that do not parse are left
//! out), argument and element sequences (the contents of every bracket pair), parenthesised groups with their
//! parentheses, statements, and the whole content. From a caret the first unit follows the observed rules of
//! section 3.4: a caret inside or at the edge of a name or literal selects it, except that a caret between a name
//! and an operator or bracket that follows it selects the expression the name belongs to (`f|(x)` selects `f(x)`,
//! `b|*c` selects `b*c`); a caret touching only an operator, bracket or comma selects the smallest unit containing
//! it (`a +| b` selects `a + b`, `(a, | b)` selects the sequence `a, b`).
use crate::ast::{Arg, Expr, ExprKind, Param, ParamKind, Program, Stmt, StmtKind};
use crate::lexer::{lex, Tok, Token, P};
use crate::parser::parse_tokens;
use crate::span::Span;
use std::ops::Range;

/// The next selection after `start..end` (byte offsets; `start == end` is a caret), or `None` when the selection
/// already covers the whole content (the editor then selects the cell).
pub fn extend_selection(src: &str, start: usize, end: usize) -> Option<Range<usize>> {
    let (start, end) = (start.min(end).min(src.len()), end.max(start).min(src.len()));
    crate::stack::on_big_stack(|| {
        let lexed = lex(src);
        let parsed = parse_tokens(&lexed.tokens, Vec::new());
        let toks: Vec<Token> =
            lexed.tokens.into_iter().filter(|t| !matches!(t.tok, Tok::Newline | Tok::Juxt | Tok::Eof)).collect();
        let mut units = Units { out: Vec::new() };
        units.program(&parsed.program);
        units.brackets(&toks);
        for t in &toks {
            units.span(t.span);
        }
        let content = content_range(src, &toks, &lexed.comments);
        units.out.push(content.clone());
        let units = units.out;

        if start == end {
            if let Some(r) = from_caret(&toks, &units, start) {
                return Some(r);
            }
        }
        smallest(&units, start, end)
    })
}

/// The text from its first to its last token or comment.
fn content_range(src: &str, toks: &[Token], comments: &[Span]) -> Range<usize> {
    let first = toks.first().map(|t| t.span.start).into_iter().chain(comments.first().map(|c| c.start)).min();
    let last = toks.last().map(|t| t.span.end).into_iter().chain(comments.last().map(|c| c.end)).max();
    match (first, last) {
        (Some(a), Some(b)) => a as usize..b as usize,
        _ => 0..src.len(),
    }
}

/// The smallest unit containing `start..end` and strictly larger than it.
fn smallest(units: &[Range<usize>], start: usize, end: usize) -> Option<Range<usize>> {
    units
        .iter()
        .filter(|u| u.start <= start && u.end >= end && u.len() > end - start)
        .min_by_key(|u| u.len())
        .cloned()
}

/// Whether a token is a name or a literal (an atom of an expression).
fn is_atom(t: &Tok) -> bool {
    matches!(
        t,
        Tok::Ident(_) | Tok::Num(_) | Tok::Str(_) | Tok::Pattern { .. } | Tok::Slot(_) | Tok::Placeholder
    ) || matches!(t, Tok::Kw("true" | "false" | "null"))
}

/// The first unit from a caret.
fn from_caret(toks: &[Token], units: &[Range<usize>], at: usize) -> Option<Range<usize>> {
    let span = |t: &Token| t.span.start as usize..t.span.end as usize;
    // inside a token
    if let Some(t) = toks.iter().find(|t| (t.span.start as usize) < at && at < t.span.end as usize) {
        let r = span(t);
        return if is_atom(&t.tok) { Some(r) } else { smallest(units, r.start, r.end) };
    }
    let left = toks.iter().find(|t| t.span.end as usize == at && t.span.end > t.span.start);
    let right = toks.iter().find(|t| t.span.start as usize == at && t.span.end > t.span.start);
    match (left, right) {
        // a name followed directly by an operator, bracket or another atom: the expression it belongs to
        (Some(l), Some(_)) if is_atom(&l.tok) => smallest(units, span(l).start, span(l).end),
        (Some(l), None) if is_atom(&l.tok) => Some(span(l)),
        (_, Some(r)) if is_atom(&r.tok) => Some(span(r)),
        (Some(l), _) => smallest(units, span(l).start, span(l).end),
        (None, Some(r)) => smallest(units, span(r).start, span(r).end),
        (None, None) => None,
    }
}

struct Units {
    out: Vec<Range<usize>>,
}

impl Units {
    fn span(&mut self, s: Span) {
        if s.end > s.start {
            self.out.push(s.start as usize..s.end as usize);
        }
    }

    /// The contents of every bracket pair (argument and element sequences), and grouping parentheses with their
    /// parentheses (a `(` not directly after a name, a closing bracket or a prime).
    fn brackets(&mut self, toks: &[Token]) {
        let mut stack: Vec<usize> = Vec::new();
        // open brackets of each kind on the stack: a closer with none of its kind costs no scan, and every opener a
        // scan passes is removed, so the pass is linear
        let mut open = [0usize; 3];
        let kind = |p: P| match p {
            P::LParen | P::RParen => 0,
            P::LBracket | P::RBracket => 1,
            _ => 2,
        };
        for (i, t) in toks.iter().enumerate() {
            match t.tok {
                Tok::Punct(p @ (P::LParen | P::LBracket | P::LBrace)) => {
                    stack.push(i);
                    open[kind(p)] += 1;
                }
                Tok::Punct(c @ (P::RParen | P::RBracket | P::RBrace)) => {
                    let want = match c {
                        P::RParen => P::LParen,
                        P::RBracket => P::LBracket,
                        _ => P::LBrace,
                    };
                    if open[kind(want)] == 0 {
                        continue;
                    }
                    let Some(pos) = stack.iter().rposition(|&j| toks[j].tok == Tok::Punct(want)) else { continue };
                    let o = stack[pos];
                    // open brackets left inside: their contents run to this closer
                    for &j in &stack[pos + 1..] {
                        self.open_contents(toks, j, i);
                    }
                    for &j in &stack[pos..] {
                        if let Tok::Punct(q) = toks[j].tok {
                            open[kind(q)] -= 1;
                        }
                    }
                    stack.truncate(pos);
                    if o + 1 < i {
                        self.out.push(toks[o + 1].span.start as usize..toks[i - 1].span.end as usize);
                    }
                    let callee = o > 0
                        && matches!(
                            toks[o - 1].tok,
                            Tok::Ident(_) | Tok::Prime(_) | Tok::Punct(P::RParen | P::RBracket)
                        );
                    if want != P::LParen || !callee {
                        self.out.push(toks[o].span.start as usize..t.span.end as usize);
                    }
                }
                _ => {}
            }
        }
        for j in stack {
            self.open_contents(toks, j, toks.len());
        }
    }

    /// The contents of an open bracket at `o` with no partner, up to the token `end` (exclusive).
    fn open_contents(&mut self, toks: &[Token], o: usize, end: usize) {
        if o + 1 < end {
            self.out.push(toks[o + 1].span.start as usize..toks[end - 1].span.end as usize);
        }
    }

    fn program(&mut self, p: &Program) {
        self.stmts(&p.stmts);
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        self.span(s.span);
        match &s.kind {
            StmtKind::Expr(e) => self.expr(e),
            StmtKind::Let { value, .. } => self.expr(value),
            StmtKind::Fn { decorators, params, options, condition, body, .. } => {
                for d in decorators {
                    self.span(d.span);
                    if let Some(args) = &d.args {
                        self.args(args);
                    }
                }
                self.params(params);
                self.params(options);
                if let Some(c) = condition {
                    self.expr(c);
                }
                self.expr(body);
            }
            StmtKind::Rule { decorators, lhs, rhs, .. } => {
                for d in decorators {
                    self.span(d.span);
                }
                self.expr(lhs);
                self.expr(rhs);
            }
            StmtKind::Module { body, .. } => self.stmts(body),
            StmtKind::Pub(inner) => self.stmt(inner),
            StmtKind::Sym { .. } | StmtKind::Import { .. } => {}
        }
    }

    fn params(&mut self, ps: &[Param]) {
        for p in ps {
            self.span(p.span);
            if let ParamKind::Pattern(e) = &p.kind {
                self.expr(e);
            }
            if let Some(d) = &p.default {
                self.expr(d);
            }
        }
    }

    fn args(&mut self, args: &[Arg]) {
        for a in args {
            if let Some((_, s)) = &a.name {
                self.span(s.to(a.value.span));
            }
            self.expr(&a.value);
        }
    }

    fn expr(&mut self, e: &Expr) {
        self.span(e.span);
        match &e.kind {
            ExprKind::Num(_)
            | ExprKind::Str(_)
            | ExprKind::Name(_)
            | ExprKind::Pattern { .. }
            | ExprKind::Slot(_) => {}
            ExprKind::Typed(a, _) | ExprKind::Named(_, a) | ExprKind::Neg(a) | ExprKind::Not(a) => self.expr(a),
            ExprKind::Prime(a, _) | ExprKind::SlotFn(a) => self.expr(a),
            ExprKind::Add(items) => items.iter().for_each(|(_, x)| self.expr(x)),
            ExprKind::Mul(items) => items.iter().for_each(|(_, x)| self.expr(x)),
            ExprKind::Pow(a, b) | ExprKind::Binary(_, a, b) | ExprKind::Span(a, b) | ExprKind::While(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Nary(_, items) | ExprKind::List(items) => items.iter().for_each(|x| self.expr(x)),
            ExprKind::Call(f, args) => {
                self.expr(f);
                self.args(args);
            }
            ExprKind::Index(a, idx) => {
                self.expr(a);
                idx.iter().for_each(|x| self.expr(x));
            }
            ExprKind::Block(stmts) => self.stmts(stmts),
            ExprKind::Lambda(params, body) => {
                self.params(params);
                self.expr(body);
            }
            ExprKind::If(c, a, b) => {
                self.expr(c);
                self.expr(a);
                if let Some(b) = b {
                    self.expr(b);
                }
            }
            ExprKind::For(_, it, body) => {
                self.expr(it);
                self.expr(body);
            }
            ExprKind::Match(e, arms) => {
                self.expr(e);
                for a in arms {
                    self.span(a.pattern.span.to(a.body.span));
                    self.expr(&a.pattern);
                    self.expr(&a.body);
                }
            }
        }
    }
}
