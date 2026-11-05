//! The shared context of one native call: the canonicaliser, the evaluator's fuel as a work budget, term helpers
//! (heads, lists, substitution, the free-of test) and the zero test of terms.
use mc_api::{names, Store, View, T};
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::{Abort, NativeCx};
use mc_numeric::numeval::{Graph, Names};
use mc_numeric::fun::Mach;
use mc_poly::convert::{Conv, Heads, Ring};
use mc_poly::ratfun;
use mc_poly::work::{Halt, Meter, Work, WorkResult};
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// The evaluator's fuel and interrupt as a [`Work`] budget, also polling `mc-arith`'s stop flag.
pub(crate) struct Tick<'c, 'a, 'k, 'h> {
    pub cx: &'c mut NativeCx<'a, 'k, 'h>,
    pub stop: &'c AtomicBool,
    pub abort: Option<Abort>,
}

impl Work for Tick<'_, '_, '_, '_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        if self.stop.load(Ordering::Relaxed) {
            return Err(Halt::Stopped);
        }
        match self.cx.tick(units) {
            Ok(()) => Ok(()),
            Err(a) => {
                self.abort = Some(a);
                Err(Halt::Stopped)
            }
        }
    }
}

/// What a function gives back to the native wrapper.
pub(crate) enum Out<'h> {
    /// a normal form
    Done(T<'h>),
    /// a term to evaluate (it contains calls such as `cos(acos(1/3)/3)` or `log(2)`)
    Again(T<'h>),
    /// not this function's business: the call stays unevaluated
    Decline,
}

/// Shared state of one native call.
pub(crate) struct Env<'h> {
    pub ar: Arith<'h, Heap>,
    pub hd: Heads<'h>,
    pub nm: Names,
    pub name: &'static str,
}

impl<'h> Env<'h> {
    pub fn new(h: &'h Heap, stop: std::sync::Arc<AtomicBool>, name: &'static str) -> Env<'h> {
        Env { ar: Arith::new(h).with_stop(stop), hd: Heads::new(h), nm: Names::new(h), name }
    }
    pub fn h(&self) -> &'h Heap {
        self.ar.store()
    }
    /// The term of the symbol `name`.
    pub fn sym(&self, name: &str) -> T<'h> {
        let h = self.h();
        h.sym_term(h.sym(name))
    }
    /// `name(args...)` (not canonicalised: for heads other than add, mul, pow).
    pub fn call(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        let h = self.h();
        h.call(h.sym(name), args)
    }
    pub fn int(&self, v: i64) -> T<'h> {
        self.ar.int(v)
    }
    pub fn list(&self, items: &[T<'h>]) -> T<'h> {
        self.h().app(self.hd.list, items)
    }
    /// `failure(argument, name, message)`.
    pub fn arg_failure(&self, message: &str) -> Out<'h> {
        let h = self.h();
        Out::Done(self.call(names::FAILURE, &[self.sym("argument"), self.sym(self.name), h.string(message)]))
    }
    /// `failure(tag, name, message)`.
    pub fn failure(&self, tag: &str, message: &str) -> Out<'h> {
        let h = self.h();
        Out::Done(self.call(names::FAILURE, &[self.sym(tag), self.sym(self.name), h.string(message)]))
    }
    /// The arguments of `t` when its head is the symbol `name`.
    pub fn args_of(&self, t: T<'h>, name: &str) -> Option<&'h [T<'h>]> {
        match self.h().view(t) {
            View::App { head, args } if head == self.sym(name) => Some(args),
            _ => None,
        }
    }
    /// The elements of a list term.
    pub fn as_list(&self, t: T<'h>) -> Option<&'h [T<'h>]> {
        match self.h().view(t) {
            View::App { head, args } if head == self.hd.list => Some(args),
            _ => None,
        }
    }
    /// The name of the symbol head of `t`, with its arguments.
    pub fn head_name(&self, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
        let h = self.h();
        match h.view(t) {
            View::App { head, args } => match h.view(head) {
                View::Sym(s) => Some((h.sym_name(s), args)),
                _ => None,
            },
            _ => None,
        }
    }
    /// Is `t` the symbol `name`?
    pub fn is_sym(&self, t: T<'h>, name: &str) -> bool {
        t == self.sym(name)
    }
    /// A polynomial converter (negative powers are kernels).
    pub fn conv(&self) -> Conv<'_, 'h, Heap> {
        Conv::new(&self.ar, &self.hd)
    }
    /// A rational-function converter (negative powers are denominators).
    pub fn rconv(&self) -> Conv<'_, 'h, Heap> {
        Conv::rational(&self.ar, &self.hd)
    }

    // ------------------------------------------------------------------ term walks

    /// Does `t` contain one of the terms `xs` (as a subterm)?
    pub fn contains_any(&self, t: T<'h>, xs: &[T<'h>]) -> bool {
        let h = self.h();
        let mut seen: FxHashMap<T<'h>, ()> = FxHashMap::default();
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if xs.contains(&u) {
                return true;
            }
            if seen.insert(u, ()).is_some() {
                continue;
            }
            if let View::App { head, args } = h.view(u) {
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
        false
    }

    /// `t` with every occurrence of `rules[i].0` replaced by `rules[i].1`, rebuilt canonically (sums, products and
    /// powers through the canonicaliser; other calls as they are, evaluated later by the evaluator).
    pub fn subst(&self, t: T<'h>, rules: &[(T<'h>, T<'h>)], m: &mut Meter) -> WorkResult<T<'h>> {
        let h = self.h();
        let mut done: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
        for &(a, b) in rules {
            done.insert(a, b);
        }
        let mut stack = vec![(t, false)];
        while let Some((u, ready)) = stack.pop() {
            m.step(1)?;
            if done.contains_key(&u) {
                continue;
            }
            let View::App { head, args } = h.view(u) else {
                done.insert(u, u);
                continue;
            };
            if !ready {
                stack.push((u, true));
                stack.push((head, false));
                stack.extend(args.iter().map(|&a| (a, false)));
                continue;
            }
            let nh = done[&head];
            let na: Vec<T<'h>> = args.iter().map(|a| done[a]).collect();
            let v = if nh == head && na.as_slice() == args {
                u
            } else {
                self.rebuild(nh, &na)
            };
            done.insert(u, v);
        }
        Ok(done[&t])
    }

    /// `head(args)` canonicalised when the head is add, mul or pow.
    pub fn rebuild(&self, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        if head == self.hd.add {
            self.ar.add(args)
        } else if head == self.hd.mul {
            self.ar.mul(args)
        } else if head == self.hd.pow && args.len() == 2 {
            self.ar.pow(args[0], args[1])
        } else {
            self.h().app(head, args)
        }
    }

    // ------------------------------------------------------------------ polynomials

    /// The ring of `terms` (rational reading) with the variables `extra` first.
    pub fn ring(&self, terms: &[T<'h>], extra: &[T<'h>], m: &mut Meter) -> WorkResult<Ring<'h>> {
        Ok(self.rconv().ring(terms, extra, m)?.0)
    }

    /// The expanded form of `t` as one fraction in lowest terms (in its kernels); `t` itself when it has a zero
    /// denominator.
    pub fn together(&self, t: T<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
        if self.ar.num(t).is_some() {
            return Ok(t);
        }
        let c = self.rconv();
        let ring = self.ring(&[t], &[], m)?;
        match ratfun::read(&c, t, &ring, m)? {
            Ok(f) => ratfun::to_term(&c, &f, &ring, m),
            Err(_) => Ok(t),
        }
    }

    // ------------------------------------------------------------------ zero tests and values

    /// Is `t` zero? Decided exactly by expansion as a rational function in its kernels (with the canonicaliser
    /// combining interacting kernels such as `(2^(1/2))^2`), then for closed numeric terms by complex balls: a term
    /// whose enclosure at 1024 bits still contains zero counts as zero (a heuristic: exact algebraic relations
    /// between radicals that the canonicaliser does not see are still found). Terms with unknown functions or
    /// symbols that do not expand to zero count as nonzero (the generic case).
    pub fn is_zero(&self, t: T<'h>, m: &mut Meter) -> WorkResult<bool> {
        if let Some(x) = self.ar.num(t) {
            return Ok(x.is_zero());
        }
        let c = self.rconv();
        let ring = self.ring(&[t], &[], m)?;
        match ratfun::read(&c, t, &ring, m)? {
            Ok(f) if f.num.is_zero() => return Ok(true),
            Ok(f) if ring.interacting() => {
                let n = c.int_poly_term(&f.num, &ring, m)?;
                if self.ar.num(n).is_some_and(|x| x.is_zero()) {
                    return Ok(true);
                }
            }
            _ => {}
        }
        Ok(self.numeric_zero(t).unwrap_or(false))
    }

    /// `Some(is zero)` for a closed numeric term (complex balls, or machine numbers with a tolerance); `None`
    /// otherwise.
    pub fn numeric_zero(&self, t: T<'h>) -> Option<bool> {
        let h = self.h();
        let g = Graph::build(h, &self.ar, &self.nm, t);
        let is = g.island(g.node(t)?);
        if is.is_machine() {
            let v = match g.eval_machine(&is) {
                Mach::Real(x) => x.abs(),
                Mach::Complex(z) => z.re.hypot(z.im),
            };
            return (!v.is_nan()).then_some(v < 1e-9);
        }
        for p in [256u64, 1024] {
            match g.eval_complex(&is, p) {
                Some(z) if !z.contains_zero() => return Some(false),
                Some(_) => {}
                None => return None,
            }
        }
        Some(true)
    }

    /// The machine value of a closed numeric term (for ordering solutions).
    pub fn machine(&self, t: T<'h>) -> Option<(f64, f64)> {
        let h = self.h();
        let g = Graph::build(h, &self.ar, &self.nm, t);
        let is = g.island(g.node(t)?);
        match g.eval_machine(&is) {
            Mach::Real(x) if x.is_finite() => Some((x, 0.0)),
            Mach::Complex(z) if z.re.is_finite() && z.im.is_finite() => Some((z.re, z.im)),
            _ => None,
        }
    }
}
