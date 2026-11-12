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
use mc_poly::Poly;
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
            // raised with the evaluator's interrupt, which the fuel poll has not seen yet: the stop is the abort (a
            // caller that sees no abort tries its next method with the number kernels returning arbitrary values)
            if self.cx.stopped() {
                self.abort = Some(mc_eval::Abort::Limit);
            }
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
    /// `solve`'s answer: the list of solutions evaluated, then put in the canonical order of solutions ([`crate::order`])
    Sorted(T<'h>),
    /// `n_solve`'s numeric answer: the term evaluated, then made numeric and ordered ([`crate::nsolve::finish`])
    Numeric(T<'h>),
}

/// The settings of one call of the solver (decision 11C.2): what the options and the calling function ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cfg {
    /// cubics by radicals (Cardano) rather than root objects, when the cubic has numeric coefficients
    pub cubics: bool,
    /// quartics by radicals (Ferrari) rather than root objects, when the quartic has numeric coefficients
    pub quartics: bool,
    /// all solutions of periodic functions (generated integer parameters), rather than the principal ones
    pub periodic: bool,
    /// over the reals: the inversions keep the real branches only
    pub reals: bool,
}

impl Default for Cfg {
    fn default() -> Cfg {
        Cfg { cubics: false, quartics: false, periodic: true, reals: false }
    }
}

/// Shared state of one native call.
pub(crate) struct Env<'h> {
    pub ar: Arith<'h, Heap>,
    pub hd: Heads<'h>,
    pub nm: Names,
    pub name: &'static str,
    /// the solver's settings
    pub cfg: Cfg,
    /// the head of generated parameters (`c`: `c(1)`, `c(2)`, ...)
    pub gen: T<'h>,
    /// the stop flag of the call
    pub stop: std::sync::Arc<AtomicBool>,
}

impl<'h> Env<'h> {
    pub fn new(h: &'h Heap, stop: std::sync::Arc<AtomicBool>, name: &'static str) -> Env<'h> {
        // the settings of the enclosing solving calls (decision 11C.1); roots gives radicals by default
        let s = crate::front::current();
        let cfg = s.cfg(name == "roots");
        let gen = h.sym_term(h.sym(s.gen.as_deref().unwrap_or("c")));
        Env { ar: Arith::new(h).with_stop(stop.clone()), hd: Heads::new(h), nm: Names::new(h), name, cfg, gen, stop }
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
    /// `failure(argument, name, message)` as a term.
    pub fn arg_failure_term(&self, message: &str) -> T<'h> {
        let h = self.h();
        self.call(names::FAILURE, &[self.sym("argument"), self.sym(self.name), h.string(message)])
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

    /// `t` as one fraction in lowest terms in its kernels, the positive rational content in front and the signs of the
    /// numerator's terms kept (`1 + (-1 - sqrt(3))/2` is `(1/2)(1 - sqrt(3))`); `t` itself
    /// when it has a zero denominator.
    pub fn tidy(&self, t: T<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
        // exact complex numbers as a + b j with a symbol j for i, so that their parts share the content
        let h = self.h();
        let mut rules = vec![];
        let mut stack = vec![t];
        let j = self.sym("solve`i");
        while let Some(u) = stack.pop() {
            if let Some(n) = self.ar.num(u) {
                if let mc_arith::Num::Complex(re, im) = n {
                    if re.kind() == mc_arith::Kind::Exact && im.kind() == mc_arith::Kind::Exact && !rules.iter().any(|r: &(T<'h>, T<'h>)| r.0 == u) {
                        let (a, b) = (self.ar.number(&mc_arith::Num::Real(re)), self.ar.number(&mc_arith::Num::Real(im)));
                        rules.push((u, self.ar.add(&[a, self.ar.mul(&[b, j])])));
                    }
                }
                continue;
            }
            if let View::App { args, .. } = h.view(u) {
                stack.extend_from_slice(args);
            }
        }
        if !rules.is_empty() {
            let u = self.subst(t, &rules, m)?;
            let v = self.tidy_real(u, m)?;
            return self.subst(v, &[(j, self.ar.number(&mc_arith::Num::i()))], m);
        }
        self.tidy_real(t, m)
    }

    fn tidy_real(&self, t: T<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
        let env = self;
        let c = env.rconv();
        let ring = env.ring(&[t], &[], m)?;
        let Ok(f) = ratfun::read(&c, t, &ring, m)? else { return Ok(t) };
        if f.num.is_zero() {
            return Ok(env.int(0));
        }
        let cn = f.num.content().abs();
        let mut num = f.num.div_exact_scalar(&cn);
        let (mut cd, den) = f.den.primitive();
        // a positive denominator: the sign goes into the numerator's terms
        if cd.is_neg() {
            cd = cd.neg();
            num = num.neg();
        }
        // the numerator as the sum of its monomials (the signs of its terms as they are)
        let nv = num.nvars();
        let mut monos = vec![];
        for (e, z) in num.terms() {
            monos.push(c.int_poly_term(&Poly::monomial(e, z.clone()), &ring, m)?);
        }
        let _ = nv;
        let n = env.ar.add(&monos);
        let q = env.ar.div(env.ar.integer(&cn.to_int()), env.ar.integer(&cd.to_int()));
        if den.as_constant().is_some() {
            let dq = env.ar.integer(&den.as_constant().expect("constant").to_int());
            return Ok(env.ar.mul(&[env.ar.div(q, dq), n]));
        }
        let d = c.int_poly_term(&den, &ring, m)?;
        Ok(env.ar.mul(&[q, n, env.ar.pow(d, env.int(-1))]))
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
