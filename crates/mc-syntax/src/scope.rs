//! Lexical bindings, shared by lowering and the printer.
//!
//! Lowering needs them for one decision: whether `x = v` assigns a `var` local (`assign(x, v)`), is an error (`x` is
//! bound by `let`, a parameter or a pattern), or defines the symbol `x` (`set(x, v)`) (04 section 3.5). The printer
//! has to make the same decision in reverse, so both compute bound names with the functions here, from the
//! **lowered terms** of the binding constructs: the two sides cannot drift apart.
use crate::heads::{as_sym, Head, Heads};
use mc_api::{Store, Sym, View, T};
use std::collections::HashSet;

/// How a name is bound.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bind {
    /// Immutable: `let`, parameters, pattern variables, loop variables.
    Let,
    /// A mutable `var` local.
    Var,
}

/// A stack of binding frames.
#[derive(Default)]
pub struct Scopes {
    entries: Vec<(Sym, Bind)>,
    frames: Vec<usize>,
}

impl Scopes {
    pub fn push(&mut self) {
        self.frames.push(self.entries.len());
    }
    /// A new frame holding `syms`, all bound immutably.
    pub fn push_lets(&mut self, syms: impl IntoIterator<Item = Sym>) {
        self.push();
        self.entries.extend(syms.into_iter().map(|s| (s, Bind::Let)));
    }
    pub fn pop(&mut self) {
        let n = self.frames.pop().unwrap_or(0);
        self.entries.truncate(n);
    }
    pub fn bind(&mut self, s: Sym, b: Bind) {
        self.entries.push((s, b));
    }
    /// The innermost binding of `s`.
    pub fn lookup(&self, s: Sym) -> Option<Bind> {
        self.entries.iter().rev().find(|e| e.0 == s).map(|e| e.1)
    }
}

/// The variables a pattern binds: `x` of `any(x)`, `seq1(x)`, `seq0(x)` and `named(x, p)`, anywhere in `t`.
pub fn pattern_vars<'h, S: Store + ?Sized>(store: &'h S, heads: &Heads, t: T<'h>) -> Vec<Sym> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![t];
    // Each distinct subterm is visited once, so the walk is linear in the size of the (shared) term.
    while let Some(t) = stack.pop() {
        if !seen.insert(t) {
            continue;
        }
        if let Some((h, args)) = heads.classify(store, t) {
            let var = match (h, args) {
                (Head::Any | Head::Seq1 | Head::Seq0, [x]) => as_sym(store, *x),
                (Head::Named, [x, _]) => as_sym(store, *x),
                _ => None,
            };
            if let Some(v) = var {
                if !out.contains(&v) {
                    out.push(v);
                }
            }
        }
        if let View::App { head, args } = store.view(t) {
            stack.push(head);
            stack.extend(args.iter().rev());
        }
    }
    out
}

/// The names bound by a lambda parameter list: the symbol under `held`, `optional`, `typed` and `rest`.
pub fn lambda_param_names<'h, S: Store + ?Sized>(store: &'h S, heads: &Heads, params: &[T<'h>]) -> Vec<Sym> {
    let mut out = Vec::new();
    for &p in params {
        let mut p = p;
        // At most four wrappers (held, optional, typed or rest, symbol); the bound keeps odd terms finite.
        for _ in 0..4 {
            if let Some(s) = as_sym(store, p) {
                out.push(s);
                break;
            }
            match heads.classify(store, p) {
                Some((Head::Held | Head::Rest, [x])) => p = *x,
                Some((Head::Optional | Head::Typed, [x, _])) => p = *x,
                _ => break,
            }
        }
    }
    out
}

/// The binding a block or top-level statement introduces: `let(x, v)`, `let(typed(x, T), v)`, `var(...)`.
pub fn statement_binding<'h, S: Store + ?Sized>(store: &'h S, heads: &Heads, t: T<'h>) -> Option<(Sym, Bind)> {
    let (h, args) = heads.classify(store, t)?;
    let bind = match h {
        Head::Let => Bind::Let,
        Head::Var => Bind::Var,
        _ => return None,
    };
    let [lhs, _] = args else { return None };
    let x = match heads.is(store, *lhs, Head::Typed, 2) {
        Some(ta) => as_sym(store, ta[0])?,
        None => as_sym(store, *lhs)?,
    };
    Some((x, bind))
}
