//! The canonical order (semantics 8), behind a trait so that `mc-arith` can supply the real one (with exact number
//! comparison) without this crate depending on it. The evaluator uses it to sort the arguments of associative and
//! commutative heads (6.2, 7.3), which fixes the enumeration order of AC matching.
//!
//! [`StructuralOrder`] is the stand-in used until then (decision F.9): numbers first, then strings by code point, then
//! symbols by name, then compound terms by head, number of arguments and arguments. Numbers are ordered by an `f64`
//! approximation of their value, then by kind (exact before inexact), then exactly within a kind; a structural
//! tie-break keeps the order total. It is iterative (no recursion on term depth).
use crate::conc::SymTable;
use mc_core::{Heap, Store, Sym, View, T};
use std::cmp::Ordering;
use std::sync::OnceLock;

/// A total order on terms.
pub trait TermOrder: Send + Sync {
    /// Compare two terms of `h`.
    fn cmp<'h>(&self, h: &'h Heap, a: T<'h>, b: T<'h>) -> Ordering;
}

/// The structural stand-in for the canonical order (see the module documentation).
#[derive(Default)]
pub struct StructuralOrder {
    /// Symbol names, cached: the store hands out owned strings.
    names: SymTable<OnceLock<Box<str>>>,
}

/// Rank of a term's kind: numbers, strings, symbols, compound terms (semantics 8).
fn rank(v: &View) -> u8 {
    match v {
        View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. } => 0,
        View::Str(_) => 1,
        View::Sym(_) => 2,
        View::App { .. } => 3,
    }
}

/// An `f64` approximation of a number's value (exactness is restored by the tie-breaks).
fn approx<'h>(h: &'h Heap, v: &View<'h>) -> f64 {
    let big = |neg: bool, mag: &[u64]| {
        let m = mag.iter().rev().fold(0f64, |acc, &w| acc * 18_446_744_073_709_551_616.0 + w as f64);
        if neg {
            -m
        } else {
            m
        }
    };
    match *v {
        View::Int(i) => i as f64,
        View::BigInt { neg, mag } => big(neg, mag),
        View::Rational { num, den } => approx(h, &h.view(num)) / approx(h, &h.view(den)),
        View::Real(r) => r,
        // the midpoint of a ball: words are [exponent, radius mantissa, radius exponent, mantissa limbs...]
        View::Ball { neg, words } => {
            let m = big(neg, &words[3..]);
            m * 2f64.powi((words[0] as i64).clamp(-2000, 2000) as i32)
        }
        _ => 0.0,
    }
}

/// Kind order among numbers of equal approximate value: exact before inexact.
fn num_kind(v: &View) -> u8 {
    match v {
        View::Int(_) | View::BigInt { .. } => 0,
        View::Rational { .. } => 1,
        View::Real(_) => 2,
        _ => 3,
    }
}

/// Exact comparison of two integers given as views.
fn int_cmp(a: &View, b: &View) -> Ordering {
    let key = |v: &View| -> (i8, Vec<u64>) {
        match *v {
            View::Int(i) => (i.signum() as i8, vec![i.unsigned_abs()]),
            View::BigInt { neg, mag } => (if neg { -1 } else { 1 }, mag.to_vec()),
            _ => (0, vec![]),
        }
    };
    let ((sa, ma), (sb, mb)) = (key(a), key(b));
    sa.cmp(&sb).then_with(|| {
        let m = ma.len().cmp(&mb.len()).then_with(|| ma.iter().rev().cmp(mb.iter().rev()));
        if sa < 0 {
            m.reverse()
        } else {
            m
        }
    })
}

enum Item<'h> {
    Pair(T<'h>, T<'h>),
    Arity(usize, usize),
}

impl StructuralOrder {
    fn name(&self, h: &Heap, s: Sym) -> &str {
        self.names.get_or_init(s.0).get_or_init(|| h.sym_name(s).into_boxed_str())
    }

    /// Two atoms (or an atom and anything) of the same rank.
    fn atoms<'h>(&self, h: &'h Heap, a: &View<'h>, b: &View<'h>) -> Ordering {
        match (a, b) {
            (View::Str(x), View::Str(y)) => x.cmp(y),
            (View::Sym(x), View::Sym(y)) => self.name(h, *x).cmp(self.name(h, *y)).then(x.0.cmp(&y.0)),
            _ => {
                let (fa, fb) = (approx(h, a), approx(h, b));
                fa.total_cmp(&fb).then(num_kind(a).cmp(&num_kind(b))).then_with(|| match (a, b) {
                    (View::Real(x), View::Real(y)) => x.total_cmp(y),
                    (View::Int(_) | View::BigInt { .. }, View::Int(_) | View::BigInt { .. }) => int_cmp(a, b),
                    (View::Rational { num: n1, den: d1 }, View::Rational { num: n2, den: d2 }) => {
                        // n1/d1 vs n2/d2 for the common case of word-sized parts; else structurally
                        match (h.view(*n1), h.view(*d1), h.view(*n2), h.view(*d2)) {
                            (View::Int(a1), View::Int(b1), View::Int(a2), View::Int(b2)) => {
                                (a1 as i128 * b2 as i128).cmp(&(a2 as i128 * b1 as i128))
                            }
                            (x1, y1, x2, y2) => int_cmp(&x1, &x2).then(int_cmp(&y1, &y2)),
                        }
                    }
                    (View::Ball { neg: n1, words: w1 }, View::Ball { neg: n2, words: w2 }) => {
                        n2.cmp(n1).then(w1.len().cmp(&w2.len())).then_with(|| w1.cmp(w2))
                    }
                    _ => Ordering::Equal,
                })
            }
        }
    }
}

impl TermOrder for StructuralOrder {
    fn cmp<'h>(&self, h: &'h Heap, a: T<'h>, b: T<'h>) -> Ordering {
        // The first unequal comparison in the sequence decides; compound terms push head, arity and arguments.
        let mut stack = vec![Item::Pair(a, b)];
        while let Some(it) = stack.pop() {
            match it {
                Item::Arity(x, y) => {
                    if x != y {
                        return x.cmp(&y);
                    }
                }
                Item::Pair(x, y) => {
                    if x == y {
                        continue;
                    }
                    let (vx, vy) = (h.view(x), h.view(y));
                    let (rx, ry) = (rank(&vx), rank(&vy));
                    if rx != ry {
                        return rx.cmp(&ry);
                    }
                    match (vx, vy) {
                        (View::App { head: h1, args: a1 }, View::App { head: h2, args: a2 }) => {
                            for (p, q) in a1.iter().zip(a2).rev() {
                                stack.push(Item::Pair(*p, *q));
                            }
                            stack.push(Item::Arity(a1.len(), a2.len()));
                            stack.push(Item::Pair(h1, h2));
                        }
                        (va, vb) => {
                            let c = self.atoms(h, &va, &vb);
                            if c != Ordering::Equal {
                                return c;
                            }
                        }
                    }
                }
            }
        }
        Ordering::Equal
    }
}
