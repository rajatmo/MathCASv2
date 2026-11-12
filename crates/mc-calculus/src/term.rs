//! [`Build`]: canonical construction of the terms derivatives are made of.
//!
//! Sums, products and powers go through `mc-arith`'s canonicaliser, so a derivative is built collected and without
//! `0*...` terms as it is assembled; calls of other functions are built as plain calls and evaluated when the result
//! is handed back to the evaluator (`cos(0)` and the like simplify there).
use mc_api::{Store, Sym, View, T};
use mc_arith::Arith;
use mc_core::Heap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The symbols this crate looks at, interned once per call of `d`.
pub struct Heads {
    /// `add`
    pub add: Sym,
    /// `mul`
    pub mul: Sym,
    /// `pow`
    pub pow: Sym,
    /// `list`
    pub list: Sym,
    /// `derivative`, the head of the derivative operator `derivative(n1, ..., nk)(f)`
    pub derivative: Sym,
    /// the constant `e` (a power `e^u` differentiates without a `log(e)` factor)
    pub e: Sym,
}

/// The term builder of one call of `d`: the heap, its canonical arithmetic and the interned heads.
pub struct Build<'h> {
    /// the canonical arithmetic
    pub ar: Arith<'h, Heap>,
    /// the heads
    pub hd: Heads,
    h: &'h Heap,
    zero: T<'h>,
    one: T<'h>,
    stop: Arc<AtomicBool>,
}

impl<'h> Build<'h> {
    /// A builder on `h`; big exact numbers poll `stop`.
    pub fn new(h: &'h Heap, stop: &Arc<AtomicBool>) -> Build<'h> {
        let hd = Heads {
            add: h.sym(mc_api::names::ADD),
            mul: h.sym(mc_api::names::MUL),
            pow: h.sym(mc_api::names::POW),
            list: h.sym(mc_api::names::LIST),
            derivative: h.sym("derivative"),
            e: h.sym("e"),
        };
        Build { ar: Arith::new(h).with_stop(stop.clone()), hd, h, zero: h.int(0), one: h.int(1), stop: stop.clone() }
    }

    /// The stop flag the arithmetic polls (parallel tasks make their own builders with it).
    pub fn stop(&self) -> &Arc<AtomicBool> {
        &self.stop
    }

    /// The heap.
    pub fn heap(&self) -> &'h Heap {
        self.h
    }
    /// `0`
    pub fn zero(&self) -> T<'h> {
        self.zero
    }
    /// `1`
    pub fn one(&self) -> T<'h> {
        self.one
    }
    /// Is `t` the integer 0?
    pub fn is_zero(&self, t: T<'h>) -> bool {
        t == self.zero
    }
    /// A machine integer.
    pub fn int(&self, v: i64) -> T<'h> {
        self.h.int(v)
    }
    /// The rational n/d (canonical).
    pub fn rat(&self, n: i64, d: i64) -> T<'h> {
        self.ar.rational(self.h.int(n), self.h.int(d))
    }
    /// A canonical sum.
    pub fn add(&self, args: &[T<'h>]) -> T<'h> {
        match args {
            [] => self.zero,
            [a] => *a,
            _ => self.ar.add(args),
        }
    }
    /// A canonical product.
    pub fn mul(&self, args: &[T<'h>]) -> T<'h> {
        match args {
            [] => self.one,
            [a] => *a,
            _ => self.ar.mul(args),
        }
    }
    /// A canonical power.
    pub fn pow(&self, b: T<'h>, e: T<'h>) -> T<'h> {
        self.ar.pow(b, e)
    }
    /// `b^k` for a machine integer k.
    pub fn powi(&self, b: T<'h>, k: i64) -> T<'h> {
        self.ar.pow(b, self.h.int(k))
    }
    /// `-a`
    pub fn neg(&self, a: T<'h>) -> T<'h> {
        self.ar.neg(a)
    }
    /// `a - b`
    pub fn sub(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.ar.sub(a, b)
    }
    /// `a / b`
    pub fn div(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.ar.div(a, b)
    }
    /// The call `name(args...)`, evaluated later.
    pub fn call(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(name), args)
    }
    /// The term of symbol `s`.
    pub fn sym(&self, s: Sym) -> T<'h> {
        self.h.sym_term(s)
    }
    /// `head(args...)` with canonical construction when the head is `add`, `mul` or `pow`.
    pub fn app(&self, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        match (self.sym_of(head), args) {
            (Some(s), _) if s == self.hd.add => self.add(args),
            (Some(s), _) if s == self.hd.mul => self.mul(args),
            (Some(s), [b, e]) if s == self.hd.pow => self.pow(*b, *e),
            _ => self.h.app(head, args),
        }
    }
    /// The symbol of a symbol term.
    pub fn sym_of(&self, t: T<'h>) -> Option<Sym> {
        match self.h.view(t) {
            View::Sym(s) => Some(s),
            _ => None,
        }
    }
    /// The operator `derivative(orders...)(f)`.
    pub fn derivative(&self, orders: &[i64], f: T<'h>) -> T<'h> {
        let ns: Vec<T<'h>> = orders.iter().map(|&n| self.h.int(n)).collect();
        let op = self.h.call(self.hd.derivative, &ns);
        self.h.app(op, &[f])
    }
    /// The orders and the function of an operator `derivative(n1, ..., nk)(f)` with non-negative machine-integer
    /// orders.
    pub fn as_derivative(&self, t: T<'h>) -> Option<(Vec<i64>, T<'h>)> {
        let View::App { head, args: [f] } = self.h.view(t) else { return None };
        let View::App { head: d, args: ns } = self.h.view(head) else { return None };
        if self.sym_of(d) != Some(self.hd.derivative) || ns.is_empty() {
            return None;
        }
        let orders = ns
            .iter()
            .map(|&n| match self.h.view(n) {
                View::Int(n @ 0..) => Some(n),
                _ => None,
            })
            .collect::<Option<Vec<i64>>>()?;
        Some((orders, *f))
    }
}
