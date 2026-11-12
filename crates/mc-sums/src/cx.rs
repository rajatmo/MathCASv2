//! The context of one native call: the evaluator, the canonicaliser, a work budget, and term helpers.
use crate::rf::Rf;
use mc_api::{names, Store, View, T};
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use mc_poly::convert::{Conv, Heads, Ring};
use mc_poly::ratfun::{self, RatFun};
use mc_poly::work::{Halt, Meter, Work, WorkResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The work budget of one call: the stop flag (the evaluator's interrupt) and a time limit.
pub struct Budget {
    stop: Arc<AtomicBool>,
    deadline: Instant,
}

impl Budget {
    /// A budget of `secs` seconds.
    pub fn new(stop: &Arc<AtomicBool>, secs: u64) -> Budget {
        Budget { stop: stop.clone(), deadline: Instant::now() + Duration::from_secs(secs) }
    }
}

impl Work for Budget {
    fn charge(&mut self, _units: u64) -> WorkResult<()> {
        if self.stop.load(Ordering::Relaxed) || Instant::now() > self.deadline {
            return Err(Halt::Stopped);
        }
        Ok(())
    }
}

/// `Ok(v)` of a polynomial kernel, or (on a halt) a poll of the evaluator and `Ok(None)` from the caller.
#[macro_export]
macro_rules! w {
    ($sx:expr, $e:expr) => {
        match $e {
            Ok(v) => v,
            Err(_) => {
                $sx.cx.tick(1)?;
                return Ok(None);
            }
        }
    };
}

/// `Some(v)`, or `Ok(None)` from the caller.
#[macro_export]
macro_rules! some {
    ($e:expr) => {
        match $e {
            Some(v) => v,
            None => return Ok(None),
        }
    };
}

/// The shared state of one call.
pub struct Sx<'a, 'c, 'k, 'h> {
    /// the evaluator
    pub cx: &'a mut NativeCx<'c, 'k, 'h>,
    /// the canonicaliser
    pub ar: Arith<'h, Heap>,
    /// the heads of the polynomial conversion
    pub hd: Heads<'h>,
    /// the stop flag
    pub stop: Arc<AtomicBool>,
    /// why a numeric sum or product gave no value (for the message of the caller)
    pub verdict: Verdict,
    /// the method asked for by `method:`
    pub method: Method,
}

/// The `method:` of `n_sum` and `n_product`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// chosen from the terms
    Auto,
    /// the tail by the integral, the Abel-Plana (Euler-Maclaurin type) correction
    Tail,
    /// the Cohen-Villegas-Zagier acceleration of an alternating series
    Alternating,
    /// term by term
    Direct,
}

/// Why a numeric sum or product was not evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// no particular reason
    Open,
    /// the series or product diverges (its terms do not fall fast enough)
    Diverges,
    /// it may converge, but too slowly to be summed
    Slow,
    /// the terms are not numbers
    NotNumeric,
}

impl<'a, 'c, 'k, 'h> Sx<'a, 'c, 'k, 'h> {
    /// A context.
    pub fn new(cx: &'a mut NativeCx<'c, 'k, 'h>, stop: &Arc<AtomicBool>) -> Self {
        let h = cx.heap();
        Sx { ar: Arith::new(h).with_stop(stop.clone()), hd: Heads::new(h), cx, stop: stop.clone(), verdict: Verdict::Open, method: Method::Auto }
    }
    /// The heap.
    pub fn h(&self) -> &'h Heap {
        self.ar.store()
    }
    /// The symbol term `name`.
    pub fn sym(&self, name: &str) -> T<'h> {
        let h = self.h();
        h.sym_term(h.sym(name))
    }
    /// An integer.
    pub fn int(&self, v: i64) -> T<'h> {
        self.h().int(v)
    }
    /// `name(args)` unevaluated.
    pub fn call(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        let h = self.h();
        h.call(h.sym(name), args)
    }
    /// A list term.
    pub fn list(&self, xs: &[T<'h>]) -> T<'h> {
        self.h().app(self.hd.list, xs)
    }
    /// `name(args)` evaluated.
    pub fn ev(&mut self, name: &str, args: &[T<'h>]) -> Res<T<'h>> {
        let t = self.call(name, args);
        self.cx.eval(t)
    }
    /// Evaluate.
    pub fn eval(&mut self, t: T<'h>) -> Res<T<'h>> {
        self.cx.eval(t)
    }
    /// The evaluated sum.
    pub fn add(&mut self, xs: &[T<'h>]) -> Res<T<'h>> {
        self.ev(names::ADD, xs)
    }
    /// The evaluated product.
    pub fn mul(&mut self, xs: &[T<'h>]) -> Res<T<'h>> {
        self.ev(names::MUL, xs)
    }
    /// The evaluated power.
    pub fn pow(&mut self, b: T<'h>, e: T<'h>) -> Res<T<'h>> {
        self.ev(names::POW, &[b, e])
    }
    /// `a - b`.
    pub fn sub(&mut self, a: T<'h>, b: T<'h>) -> Res<T<'h>> {
        let nb = self.mul(&[self.int(-1), b])?;
        self.add(&[a, nb])
    }
    /// `a / b`.
    pub fn div(&mut self, a: T<'h>, b: T<'h>) -> Res<T<'h>> {
        let ib = self.pow(b, self.int(-1))?;
        self.mul(&[a, ib])
    }
    /// `a + i`.
    pub fn plus(&mut self, a: T<'h>, i: i64) -> Res<T<'h>> {
        if i == 0 {
            return Ok(a);
        }
        self.add(&[a, self.int(i)])
    }
    /// `e` with `x` replaced by `v`, evaluated. The replacement is structural and `e` is not evaluated first (an
    /// inner `n_sum(f, [j, ...])` with the outer index still symbolic would print its "terms are not numbers"
    /// message once for every outer value, D23-19).
    pub fn subst(&mut self, e: T<'h>, x: T<'h>, v: T<'h>) -> Res<T<'h>> {
        let r = mc_numfn::replace(self.h(), e, &mut |t| (t == x).then_some(v));
        self.cx.eval(r)
    }
    /// `e` with the rules applied, evaluated.
    pub fn subst_all(&mut self, e: T<'h>, rules: &[T<'h>]) -> Res<T<'h>> {
        let l = self.list(rules);
        self.ev("rewrite", &[e, l])
    }
    /// A rule.
    pub fn rule(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.call(names::RULE, &[a, b])
    }
    /// Is `t` the symbol `true`?
    pub fn is_true(&self, t: T<'h>) -> bool {
        t == self.sym(names::TRUE)
    }
    /// Is `t` free of `x` (structurally)?
    pub fn free(&self, t: T<'h>, x: T<'h>) -> bool {
        let h = self.h();
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
    /// Does `t` contain a call of one of `names` (or one of the symbols)?
    pub fn contains_any(&self, t: T<'h>, names: &[&str]) -> bool {
        let syms: Vec<T<'h>> = names.iter().map(|n| self.sym(n)).collect();
        let h = self.h();
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if syms.contains(&u) {
                return true;
            }
            if let View::App { head, args } = h.view(u) {
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
        false
    }
    /// Is the result unusable (an infinity, an indeterminate, a failure)?
    pub fn bad(&self, t: T<'h>) -> bool {
        self.contains_any(t, &["complex_infinity", "indeterminate", names::FAILURE, "inf"])
    }
    /// The arguments when `t` is a call of `name`.
    pub fn args_of(&self, t: T<'h>, name: &str) -> Option<&'h [T<'h>]> {
        match self.h().view(t) {
            View::App { head, args } if head == self.sym(name) => Some(args),
            _ => None,
        }
    }
    /// The elements of a list.
    pub fn as_list(&self, t: T<'h>) -> Option<&'h [T<'h>]> {
        self.args_of(t, names::LIST)
    }
    /// The integer `t` is.
    pub fn as_int(&self, t: T<'h>) -> Option<i64> {
        match self.h().view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }
    /// A rational-function converter.
    pub fn conv(&self) -> Conv<'_, 'h, Heap> {
        Conv::rational(&self.ar, &self.hd)
    }
    /// The ring of the kernels of `terms` with `vars` first.
    pub fn ring(&self, terms: &[T<'h>], vars: &[T<'h>], m: &mut Meter) -> WorkResult<Ring<'h>> {
        Ok(self.conv().ring(terms, vars, m)?.0)
    }
    /// `t` as a rational function in `ring` (`None` for a division by zero).
    pub fn read(&self, t: T<'h>, ring: &Ring<'h>, m: &mut Meter) -> WorkResult<Option<Rf>> {
        let c = self.conv();
        Ok(match ratfun::read(&c, t, ring, m)? {
            Ok(r) => Some(Rf { num: r.num, den: r.den }),
            Err(_) => None,
        })
    }
    /// The term of a rational function (not evaluated beyond the canonicaliser).
    pub fn term(&self, f: &Rf, ring: &Ring<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
        let c = self.conv();
        ratfun::to_term(&c, &RatFun { num: f.num.clone(), den: f.den.clone() }, ring, m)
    }
    /// The term of a polynomial.
    pub fn pterm(&self, p: &mc_poly::Poly, ring: &Ring<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
        self.conv().int_poly_term(p, ring, m)
    }
    /// Is every kernel of `ring` other than `vars` free of every one of `vars`?
    pub fn clean(&self, ring: &Ring<'h>, vars: &[T<'h>]) -> bool {
        ring.kernels.iter().all(|&kk| vars.contains(&kk) || vars.iter().all(|&v| self.free(kk, v)))
    }
    /// Print `what` and `t` to standard error when `MC_SUMS_TRACE` is set (development aid).
    pub fn trace(&mut self, what: &str, t: T<'h>) {
        if std::env::var_os("MC_SUMS_TRACE").is_none() {
            return;
        }
        let s = self.ev("to_string", &[t]).ok();
        let txt = match s.map(|s| self.h().view(s)) {
            Some(View::Str(x)) => x.to_string(),
            _ => "?".into(),
        };
        eprintln!("[mc-sums] {what}: {txt}");
    }
    /// The size of a term by the simplifier's complexity measure.
    pub fn cost(&mut self, t: T<'h>) -> u64 {
        mc_simplify::cost::Cost::new(self.h()).size(t)
    }
    /// The simplest of the candidates (by the complexity measure; the first on ties).
    pub fn smallest(&mut self, cands: &[T<'h>]) -> T<'h> {
        let mut best = cands[0];
        let mut bc = self.cost(best);
        for &c in &cands[1..] {
            let cc = self.cost(c);
            if cc < bc {
                best = c;
                bc = cc;
            }
        }
        best
    }
}
