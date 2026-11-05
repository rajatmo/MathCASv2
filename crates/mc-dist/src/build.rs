//! A small builder of unevaluated terms: the symbolic results are written as source-like terms (`add`, `mul`,
//! `pow`, calls) and handed back to the evaluator, which brings them to canonical form.
use mc_api::{names, Store, T};
use mc_core::Heap;

/// The term builder over one heap.
#[derive(Clone, Copy)]
pub struct B<'h> {
    /// the heap the terms live in
    pub h: &'h Heap,
}

impl<'h> B<'h> {
    /// A builder on `h`.
    pub fn new(h: &'h Heap) -> B<'h> {
        B { h }
    }

    /// An integer.
    pub fn int(&self, v: i64) -> T<'h> {
        self.h.int(v)
    }

    /// The rational p/q (canonicalized by the evaluator).
    pub fn rat(&self, p: i64, q: i64) -> T<'h> {
        self.div(self.int(p), self.int(q))
    }

    /// A symbol.
    pub fn sym(&self, name: &str) -> T<'h> {
        self.h.sym_term(self.h.sym(name))
    }

    /// `name(args)`.
    pub fn call(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(name), args)
    }

    /// `a + b + ...`.
    pub fn add(&self, args: &[T<'h>]) -> T<'h> {
        self.call(names::ADD, args)
    }

    /// `a * b * ...`.
    pub fn mul(&self, args: &[T<'h>]) -> T<'h> {
        self.call(names::MUL, args)
    }

    /// `a^b`.
    pub fn pow(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.call(names::POW, &[a, b])
    }

    /// `a^k` for an integer k.
    pub fn powi(&self, a: T<'h>, k: i64) -> T<'h> {
        self.pow(a, self.int(k))
    }

    /// `-a`.
    pub fn neg(&self, a: T<'h>) -> T<'h> {
        self.mul(&[self.int(-1), a])
    }

    /// `a - b`.
    pub fn sub(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.add(&[a, self.neg(b)])
    }

    /// `a / b`.
    pub fn div(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.mul(&[a, self.powi(b, -1)])
    }

    /// `sqrt(a)`.
    pub fn sqrt(&self, a: T<'h>) -> T<'h> {
        self.call("sqrt", &[a])
    }

    /// `exp(a)`.
    pub fn exp(&self, a: T<'h>) -> T<'h> {
        self.call("exp", &[a])
    }

    /// `log(a)`.
    pub fn log(&self, a: T<'h>) -> T<'h> {
        self.call("log", &[a])
    }

    /// `[a, ...]`.
    pub fn list(&self, items: &[T<'h>]) -> T<'h> {
        self.call(names::LIST, items)
    }

    /// `a < b`.
    pub fn lt(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.call("less", &[a, b])
    }

    /// `a <= b`.
    pub fn le(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.call("less_equal", &[a, b])
    }

    /// `c1 and c2`.
    pub fn and(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.call("and", &[a, b])
    }

    /// `piecewise([[v1, c1], ...], default)`.
    pub fn piecewise(&self, cases: &[(T<'h>, T<'h>)], default: T<'h>) -> T<'h> {
        let cs: Vec<T<'h>> = cases.iter().map(|&(v, c)| self.list(&[v, c])).collect();
        self.call("piecewise", &[self.list(&cs), default])
    }

    /// `inf`.
    pub fn inf(&self) -> T<'h> {
        self.sym("inf")
    }

    /// `pi`.
    pub fn pi(&self) -> T<'h> {
        self.sym("pi")
    }

    /// `failure(argument, f, message)`.
    pub fn bad(&self, f: &str, message: &str) -> T<'h> {
        self.call(names::FAILURE, &[self.sym("argument"), self.sym(f), self.h.string(message)])
    }
}
