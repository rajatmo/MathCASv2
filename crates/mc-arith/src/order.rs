//! The canonical order (semantics 8): a total order on terms, used to sort the arguments of `add` and `mul`.
//!
//! 1. numbers first, by value (exact before inexact at equal value), complex numbers by real then imaginary part;
//! 2. then strings, by code point;
//! 3. then symbols, by name, then module path;
//! 4. then compound terms: by head, then by number of arguments, then argument by argument.
//!
//! The comparison walks both terms with an explicit stack (decision 1.6), stops at the first difference, and
//! skips equal subterms in O(1) by handle identity. It reads only views and symbol names, never handle values, so
//! it does not depend on creation order or the store implementation (stability).
use crate::term::{num_of_raw, Names};
use mc_api::{Store, Sym, View, T};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

/// The order on terms of one store, with a cache of split symbol names. Not shared between threads.
pub struct Order<'h, S: Store + ?Sized> {
    st: &'h S,
    names: Names<'h>,
    syms: RefCell<FxHashMap<Sym, Rc<(String, String)>>>,
}

/// One step of the comparison: a pair of terms, or the check of argument counts that follows equal heads.
enum Step<'h> {
    Pair(T<'h>, T<'h>),
    Count(usize, usize),
}

impl<'h, S: Store + ?Sized> Order<'h, S> {
    /// The order for a store.
    pub fn new(st: &'h S) -> Order<'h, S> {
        Order { st, names: Names::new(st), syms: RefCell::default() }
    }

    /// (name, module path) of a symbol: the qualified name split at its last dot.
    fn split(&self, s: Sym) -> Rc<(String, String)> {
        if let Some(v) = self.syms.borrow().get(&s) {
            return v.clone();
        }
        let full = self.st.sym_name(s);
        let v = Rc::new(match full.rfind('.') {
            Some(i) => (full[i + 1..].to_string(), full[..i].to_string()),
            None => (full, String::new()),
        });
        self.syms.borrow_mut().insert(s, v.clone());
        v
    }

    /// The class of a term in rule 1 to 4: number, string, symbol, compound.
    fn class(&self, t: T<'h>, v: &View<'h>) -> u8 {
        match v {
            View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. } => 0,
            View::Str(_) => 1,
            View::Sym(_) => 2,
            View::App { .. } => {
                if num_of_raw(self.st, &self.names, t).is_some() {
                    0
                } else {
                    3
                }
            }
        }
    }

    /// Two numbers. Distinct number terms never compare equal by value and kind unless one of them is not in
    /// canonical form; for those the payload decides, so that the order stays antisymmetric on any input.
    fn numbers(&self, a: T<'h>, b: T<'h>) -> Ordering {
        let na = num_of_raw(self.st, &self.names, a).expect("a number");
        let nb = num_of_raw(self.st, &self.names, b).expect("a number");
        na.cmp_canonical(&nb).then_with(|| self.payloads(a, b))
    }

    fn payloads(&self, a: T<'h>, b: T<'h>) -> Ordering {
        match (self.st.view(a), self.st.view(b)) {
            (View::Ball { neg: n1, words: w1 }, View::Ball { neg: n2, words: w2 }) => (n1, w1).cmp(&(n2, w2)),
            (View::Rational { den: d1, .. }, View::Rational { den: d2, .. }) => self.numbers(d1, d2),
            (View::App { args: x, .. }, View::App { args: y, .. }) => {
                x.iter().zip(y).map(|(&p, &q)| if p == q { Ordering::Equal } else { self.numbers(p, q) }).fold(Ordering::Equal, Ordering::then)
            }
            _ => Ordering::Equal,
        }
    }

    /// The canonical order of two terms.
    pub fn cmp(&self, a: T<'h>, b: T<'h>) -> Ordering {
        let mut stack = vec![Step::Pair(a, b)];
        while let Some(step) = stack.pop() {
            let (a, b) = match step {
                Step::Count(m, n) => match m.cmp(&n) {
                    Ordering::Equal => continue,
                    o => return o,
                },
                Step::Pair(a, b) => (a, b),
            };
            if a == b {
                continue;
            }
            let (va, vb) = (self.st.view(a), self.st.view(b));
            let (ca, cb) = (self.class(a, &va), self.class(b, &vb));
            let o = match (ca.cmp(&cb), &va, &vb) {
                (Ordering::Equal, View::Str(x), View::Str(y)) => x.cmp(y),
                (Ordering::Equal, View::Sym(x), View::Sym(y)) => self.split(*x).cmp(&self.split(*y)),
                (Ordering::Equal, View::App { head: h1, args: a1 }, View::App { head: h2, args: a2 }) if ca == 3 => {
                    // the head first, then the count, then the arguments left to right; the stack runs them in
                    // that order and returns at the first difference
                    for (&x, &y) in a1.iter().zip(a2.iter()).rev() {
                        stack.push(Step::Pair(x, y));
                    }
                    stack.push(Step::Count(a1.len(), a2.len()));
                    stack.push(Step::Pair(*h1, *h2));
                    continue;
                }
                (Ordering::Equal, ..) => self.numbers(a, b),
                (o, ..) => o,
            };
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    }
}
