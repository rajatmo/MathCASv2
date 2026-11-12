//! [`Bld`]: the term builder of the decision service (canonical sums, products and powers through `mc-arith`,
//! plain calls for everything else), with the views of terms the deciders use.
use mc_api::{names, Store, Sym, View, T};
use mc_arith::Arith;
use mc_core::Heap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The heads the deciders look at, interned once per call.
pub struct Heads {
    /// `add`
    pub add: Sym,
    /// `mul`
    pub mul: Sym,
    /// `pow`
    pub pow: Sym,
    /// `list`
    pub list: Sym,
    /// `complex`, the head of a complex number
    pub complex: Sym,
    /// the constant `e`
    pub e: Sym,
    /// `exp`
    pub exp: Sym,
    /// `log`
    pub log: Sym,
    /// `sin`
    pub sin: Sym,
    /// `cos`
    pub cos: Sym,
}

/// The builder: the heap, its canonical arithmetic and the interned heads.
pub struct Bld<'h> {
    /// the canonical arithmetic
    pub ar: Arith<'h, Heap>,
    /// the heads
    pub hd: Heads,
    h: &'h Heap,
}

impl<'h> Bld<'h> {
    /// A builder on `h`; big exact numbers poll `stop`.
    pub fn new(h: &'h Heap, stop: &Arc<AtomicBool>) -> Bld<'h> {
        let hd = Heads {
            add: h.sym(names::ADD),
            mul: h.sym(names::MUL),
            pow: h.sym(names::POW),
            list: h.sym(names::LIST),
            complex: h.sym(names::COMPLEX),
            e: h.sym("e"),
            exp: h.sym("exp"),
            log: h.sym("log"),
            sin: h.sym("sin"),
            cos: h.sym("cos"),
        };
        Bld { ar: Arith::new(h).with_stop(stop.clone()), hd, h }
    }

    /// The heap.
    pub fn heap(&self) -> &'h Heap {
        self.h
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
            [] => self.int(0),
            [a] => *a,
            _ => self.ar.add(args),
        }
    }
    /// A canonical product.
    pub fn mul(&self, args: &[T<'h>]) -> T<'h> {
        match args {
            [] => self.int(1),
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
    /// The call `s(args...)`, evaluated later.
    pub fn call(&self, s: Sym, args: &[T<'h>]) -> T<'h> {
        self.h.call(s, args)
    }
    /// The call `name(args...)`, evaluated later.
    pub fn call_named(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(name), args)
    }
    /// The symbol term `name`.
    pub fn sym_named(&self, name: &str) -> T<'h> {
        self.h.sym_term(self.h.sym(name))
    }
    /// The symbol of a symbol term.
    pub fn sym_of(&self, t: T<'h>) -> Option<Sym> {
        match self.h.view(t) {
            View::Sym(s) => Some(s),
            _ => None,
        }
    }
    /// `(s, args)` when `t` is a call with symbol head `s`.
    pub fn call_of(&self, t: T<'h>) -> Option<(Sym, &'h [T<'h>])> {
        match self.h.view(t) {
            View::App { head, args } => self.sym_of(head).map(|s| (s, args)),
            _ => None,
        }
    }
    /// The name of the head of a call with a symbol head, and its arguments.
    pub fn named_call(&self, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
        let (s, args) = self.call_of(t)?;
        Some((self.h.sym_name(s), args))
    }
    /// The terms of a sum (`t` itself when it is not one).
    pub fn terms(&self, t: T<'h>) -> Vec<T<'h>> {
        match self.call_of(t) {
            Some((s, args)) if s == self.hd.add => args.to_vec(),
            _ => vec![t],
        }
    }
    /// Is `t` an exact or inexact number (complex numbers included)?
    pub fn is_number(&self, t: T<'h>) -> bool {
        self.ar.num(t).is_some()
    }
    /// Is `t` an exact rational number (an integer or a fraction)?
    pub fn is_rational(&self, t: T<'h>) -> bool {
        matches!(self.h.view(t), View::Int(_) | View::BigInt { .. } | View::Rational { .. })
    }
    /// Is `t` an integer number?
    pub fn is_integer(&self, t: T<'h>) -> bool {
        matches!(self.h.view(t), View::Int(_) | View::BigInt { .. })
    }
    /// Is `t` free of `x` (no subterm equal to `x`)?
    pub fn free_of(&self, t: T<'h>, x: T<'h>) -> bool {
        if t == x {
            return false;
        }
        let h = self.h;
        if let View::Sym(s) = h.view(x) {
            // the symbol mask is a quick, conservative test
            if h.mask(t) & h.mask(h.sym_term(s)) == 0 {
                return true;
            }
        }
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if u == x {
                return false;
            }
            if let View::App { head, args } = h.view(u) {
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
        true
    }
    /// The symbols of `t` in argument positions (not heads), each once, in order of first appearance; `None` when
    /// there are more than `max`.
    pub fn symbols(&self, t: T<'h>, max: usize) -> Option<Vec<T<'h>>> {
        let h = self.h;
        let mut out: Vec<T<'h>> = vec![];
        let mut stack = vec![t];
        let mut seen = 0usize;
        while let Some(u) = stack.pop() {
            seen += 1;
            if seen > 1 << 14 {
                return None;
            }
            match h.view(u) {
                View::Sym(_) => {
                    if !out.contains(&u) {
                        if out.len() == max {
                            return None;
                        }
                        out.push(u);
                    }
                }
                View::App { args, .. } => stack.extend(args.iter().rev().copied()),
                _ => {}
            }
        }
        Some(out)
    }
}
