//! Gruntz's algorithm (item 13C): the limit of an exp-log function as its variable tends to `+inf`.
//!
//! References: D. Gruntz, *On Computing Limits in a Symbolic Manipulation System*, thesis, ETH Zürich (1996);
//! J. Shackell, *Symbolic Asymptotics* (Springer, 2004); B. Salvy and J. Shackell, "Symbolic asymptotics:
//! multiseries of inverse functions", JSC 27 (1999). The implementation is written from the thesis' description.
//!
//! For `e(x)`, `x → +inf`:
//!
//! 1. the set Ω of **most rapidly varying** subexpressions of `e` (`mrv`): `x`, or exponentials `exp(s)` with
//!    `s → ±inf`, compared by their **comparability classes** (`f ≻ g` when `log f / log g → ±inf`);
//! 2. when `x ∈ Ω`, the function is **moved up** (`x ← exp(x)`), so Ω holds exponentials only;
//! 3. one element `g = exp(h)` of Ω gives `ω = exp(-|h|) → 0⁺`, and every `f = exp(s)` in Ω is **rewritten** as
//!    `exp(s - c h) ω^(∓c)` with `c = lim s/h` (the rest is of a smaller class);
//! 4. the **leading term** `c₀ ω^e₀` of the expansion in ω (the series kernels of `mc-poly`, with the classes below
//!    Ω as constants; `log ω` replaced by `∓h`); `e₀ > 0`: the limit is 0, `e₀ < 0`: `±inf` by the sign of `c₀`,
//!    `e₀ = 0`: the limit of `c₀`, whose class is smaller (the recursion ends).
//!
//! Signs (`sign`) are the signs of leading coefficients, so limits and signs recurse into each other.
//!
//! Extensions (the thesis' chapter 7 and Salvy-Shackell): before the algorithm, [`Gruntz::prepare`] writes the
//! hyperbolic and inverse hyperbolic functions, `tan` and its relatives, `abs`, `sign`, `max`, `min` and `atan` at
//! an infinity in exp-log form, and special functions whose argument tends to an infinity as their asymptotic
//! expansions (`erf`, `erfc`, `erfi`, `gamma`, `log_gamma`, `factorial`, `binomial`, `pochhammer`, `beta`,
//! `exp_integral_ei`, `exp_integral_e`, `log_integral`, `bessel_i`, `bessel_k`, the Airy functions, `digamma`,
//! `polygamma`, `zeta`, `product_log`, `dawson_f`, the incomplete gamma function) truncated after `terms` terms.
//! The truncation is safe because every limit computed with one is computed again with more terms and the two must
//! agree. Other functions are accepted when their arguments have finite limits; the series kernels expand them
//! through their derivatives, and give up at a singular point.
//!
//! Beyond the thesis (decisions 13C.x in `NOTES.md`): per call memo tables for limits, signs, mrv sets and
//! leading terms (terms are hash-consed, so a repeated subproblem costs one lookup); a depth bound, a deadline and
//! the evaluator's interrupt polled at every step, with `None` (unknown) on expiry; leading terms with symbolic
//! exponents (`2^x / 3^x`) handled before the series kernels, which need rational ones.
use super::walk::Lim;
use crate::bounds::{sign as const_sign, subst};
use mc_calculus::common::{apply, args_of, call_of, is_failure, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

type R<X> = Res<Option<X>>;

/// The head of the private exponential: `e^u` is written `G(u)`, which the evaluator leaves as it is (it would turn
/// `e^(p log b)` back into `b^p`, recreating the subexpressions the rewriting removed; decision 13C.2).
const G: &str = "exp%gruntz";

/// The terms of a truncated asymptotic expansion in the first run and in the confirming one (decision 13C.3).
pub(crate) const TERMS: (usize, usize) = (4, 6);

/// The deepest recursion of limits, signs and leading terms.
const MAX_DEPTH: u32 = 64;

/// The leading behaviour of a function of x as x → +inf.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Lead<'h> {
    /// `c ω^e`: `c` nonzero of a smaller comparability class than ω, `e` free of x, `w` the term of ω in x (any
    /// term when `e` is 0)
    Term { c: T<'h>, e: T<'h>, w: T<'h> },
    /// zero to every order the expansion reached, which is positive: the limit is 0, its side unknown
    Small,
}

/// The leading term of a function of ω: `c ω^e`, or small (see [`Lead::Small`]).
#[derive(Clone, Copy, Debug)]
enum WLead<'h> {
    Term(T<'h>, T<'h>),
    Small,
}

/// Counters of one computation (for the notes and the tests).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stats {
    /// limits computed (not from the memo)
    pub limits: u64,
    /// answers taken from a memo table
    pub hits: u64,
    /// leading terms through the series kernels
    pub series: u64,
}

/// One run of the algorithm in the variable `x` (tending to +inf).
pub(crate) struct Gruntz<'a, 'h> {
    b: &'a Build<'h>,
    stop: Arc<AtomicBool>,
    x: T<'h>,
    deadline: Instant,
    /// the deadline passed, or the depth bound was reached: every answer from then on is unknown
    pub expired: bool,
    depth: u32,
    fresh: u32,
    /// how many terms of an asymptotic expansion stand for a special function
    terms: usize,
    /// an asymptotic expansion was used (the result must be confirmed with more terms)
    pub truncated: bool,
    lims: FxHashMap<T<'h>, Option<Lim<'h>>>,
    signs: FxHashMap<T<'h>, Option<i8>>,
    mrvs: FxHashMap<T<'h>, Option<Vec<T<'h>>>>,
    leads: FxHashMap<T<'h>, Option<Lead<'h>>>,
    /// the counters
    pub stats: Stats,
}

/// Functions accepted when their arguments have finite limits (the series kernels expand them through their
/// derivatives at the limit, and give up when that is a singular point).
const ANALYTIC: &[&str] = &[
    "sin", "cos", "atan", "asin", "acos", "erf", "erfc", "erfi", "gamma", "log_gamma", "polygamma", "digamma", "zeta",
    "polylog", "bessel_j", "bessel_y", "bessel_i", "bessel_k", "airy_ai", "airy_bi", "airy_ai_prime", "airy_bi_prime",
    "exp_integral_ei", "exp_integral_e", "log_integral", "sin_integral", "cos_integral", "sinh_integral",
    "cosh_integral", "fresnel_s", "fresnel_c", "dawson_f", "product_log", "elliptic_k", "elliptic_e", "lambert_w",
    "spherical_bessel_j", "spherical_bessel_y", "hankel_h1", "hankel_h2", "struve_h", "struve_l", "beta",
];

impl<'a, 'h> Gruntz<'a, 'h> {
    /// A run in `x` that gives up at `deadline`, with special functions at infinity truncated after `terms` terms.
    pub fn new(b: &'a Build<'h>, x: T<'h>, deadline: Instant, terms: usize) -> Gruntz<'a, 'h> {
        Gruntz {
            b,
            stop: Arc::new(AtomicBool::new(false)),
            x,
            deadline,
            expired: false,
            depth: 0,
            fresh: 0,
            terms,
            truncated: false,
            lims: FxHashMap::default(),
            signs: FxHashMap::default(),
            mrvs: FxHashMap::default(),
            leads: FxHashMap::default(),
            stats: Stats::default(),
        }
    }

    /// The variable (tending to +inf).
    pub fn variable(&self) -> T<'h> {
        self.x
    }

    /// The exponent of an exponential of the run (`G(s)`, `e^s`, `b^p`), or `None`.
    pub fn exp_argument(&self, t: T<'h>) -> Option<T<'h>> {
        self.exp_arg(t)
    }

    fn free(&self, t: T<'h>) -> bool {
        is_free(self.b, t, self.x)
    }

    /// One step: the interrupt, the deadline and the depth. `false` when the run must give up.
    fn step(&mut self, cx: &mut NativeCx<'_, '_, 'h>) -> Res<bool> {
        cx.tick(1)?;
        if self.expired || Instant::now() > self.deadline || self.depth > MAX_DEPTH {
            self.expired = true;
            return Ok(false);
        }
        Ok(true)
    }

    /// `exp(u)` in the private form the evaluator leaves alone (decision 13C.2).
    fn exp(&self, u: T<'h>) -> T<'h> {
        let b = self.b;
        if u == b.zero() {
            return b.one();
        }
        // terms c log(v) with a constant c leave as powers v^c (a symbolic c of unknown sign would make the
        // exponent's limit undecided, though v^c is in the class of v whatever c is)
        let terms: Vec<T<'h>> = args_of(b, u, b.hd.add).map_or_else(|| vec![u], |a| a.to_vec());
        let (mut powers, mut rest) = (vec![], vec![]);
        for &t in &terms {
            match self.const_log(t) {
                Some((c, v)) => powers.push(b.pow(v, c)),
                None => rest.push(t),
            }
        }
        if powers.is_empty() {
            return b.call(G, &[u]);
        }
        if !rest.is_empty() {
            powers.push(b.call(G, &[b.add(&rest)]));
        }
        b.mul(&powers)
    }

    /// `(c, v)` when `t` is `c log(v)` with `c` free of x and `v` depending on it.
    fn const_log(&self, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
        let b = self.b;
        let fs: Vec<T<'h>> = args_of(b, t, b.hd.mul).map_or_else(|| vec![t], |a| a.to_vec());
        let mut found = None;
        let mut c = vec![];
        for &f in &fs {
            match call_of(b, f) {
                Some((n, [v])) if n == "log" && !self.free(*v) && found.is_none() => found = Some(*v),
                _ if self.free(f) => c.push(f),
                _ => return None,
            }
        }
        found.map(|v| (b.mul(&c), v))
    }

    /// The argument of the private exponential `G(u)`.
    fn g_arg(&self, t: T<'h>) -> Option<T<'h>> {
        match call_of(self.b, t) {
            Some((n, [u])) if n == G => Some(*u),
            _ => None,
        }
    }

    /// `t` evaluated, with its exponentials in the private form.
    fn ev(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
        let v = cx.eval(t)?;
        Ok(self.to_g(v))
    }

    /// `t` with `e^u` and `b^p` (u, p depending on x) as `G(u)`, `G(p log b)`, and `G(u)^p` as `G(u p)`.
    pub(crate) fn to_g(&self, t: T<'h>) -> T<'h> {
        let mut memo = FxHashMap::default();
        self.to_g_in(t, &mut memo)
    }

    fn to_g_in(&self, t: T<'h>, memo: &mut FxHashMap<T<'h>, T<'h>>) -> T<'h> {
        let b = self.b;
        if self.free(t) {
            return t;
        }
        if let Some(&r) = memo.get(&t) {
            return r;
        }
        let r = match b.heap().view(t) {
            View::App { head, args } => {
                let na: Vec<T<'h>> = args.iter().map(|&a| self.to_g_in(a, memo)).collect();
                match b.sym_of(head) {
                    Some(s) if s == b.hd.add => b.add(&na),
                    Some(s) if s == b.hd.mul => {
                        // G(a) G(b) = G(a + b)
                        let (gs, rest): (Vec<T<'h>>, Vec<T<'h>>) = na.iter().partition(|&&f| self.g_arg(f).is_some());
                        if gs.len() > 1 {
                            let sum = b.add(&gs.iter().filter_map(|&f| self.g_arg(f)).collect::<Vec<_>>());
                            let mut fs = rest;
                            fs.push(self.exp(sum));
                            b.mul(&fs)
                        } else {
                            b.mul(&na)
                        }
                    }
                    Some(s) if s == b.hd.pow && na.len() == 2 => {
                        let (base, p) = (na[0], na[1]);
                        if let Some(u) = self.g_arg(base) {
                            self.exp(b.mul(&[u, p]))
                        } else if self.free(p) {
                            b.pow(base, p)
                        } else if b.sym_of(base) == Some(b.hd.e) {
                            self.exp(p)
                        } else {
                            self.exp(b.mul(&[p, self.log_of(base)]))
                        }
                    }
                    _ => b.app(head, &na),
                }
            }
            _ => t,
        };
        memo.insert(t, r);
        r
    }

    /// `t` with `G(u)` as `e^u` (for the series kernels and for results).
    pub(crate) fn unwrap_g(&self, t: T<'h>) -> T<'h> {
        let b = self.b;
        let g = b.heap().sym(G);
        if !has_head(b, t, g) {
            return t;
        }
        let mut memo: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
        from_g_in(b, g, t, &mut memo)
    }

    fn log(&self, u: T<'h>) -> T<'h> {
        self.b.call("log", &[u])
    }

    /// The exponent `s` of an exponential `exp(s)` (`e^s`, or `b^p` with `p` depending on x, as `p log b`), not
    /// evaluated.
    fn exp_arg(&self, t: T<'h>) -> Option<T<'h>> {
        let b = self.b;
        if let Some(u) = self.g_arg(t) {
            return (!self.free(u)).then_some(u);
        }
        let [base, p] = args_of(b, t, b.hd.pow)? else { return None };
        if b.sym_of(*base) == Some(b.hd.e) {
            return (!self.free(*p)).then_some(*p);
        }
        if self.free(*p) {
            return None;
        }
        Some(b.mul(&[*p, self.log_of(*base)]))
    }

    /// `log(u)`, with `log(e^s)` as `s` and `log(b^p)` as `p log(b)` (real values).
    fn log_of(&self, u: T<'h>) -> T<'h> {
        let b = self.b;
        if let Some(a) = self.g_arg(u) {
            return a;
        }
        if let Some([base, p]) = args_of(b, u, b.hd.pow) {
            if b.sym_of(*base) == Some(b.hd.e) {
                return *p;
            }
            if let Some(q) = b.ar.num(*p) {
                if q.is_exact() {
                    return b.mul(&[*p, self.log_of(*base)]);
                }
            }
        }
        self.log(u)
    }

    /// The limit of `e` as x → +inf, or `None`.
    pub fn limit(&mut self, cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>) -> R<Lim<'h>> {
        if self.free(e) {
            return Ok(Some(Lim::Fin(e, 0)));
        }
        if e == self.x {
            return Ok(Some(Lim::Inf(1)));
        }
        if let Some(&r) = self.lims.get(&e) {
            self.stats.hits += 1;
            return Ok(r);
        }
        if !self.step(cx)? {
            return Ok(None);
        }
        self.stats.limits += 1;
        self.depth += 1;
        let r = self.limit_inner(cx, e);
        self.depth -= 1;
        let r = r?;
        trace(self.b, self.depth, "limit", e, &format!("{r:?}"));
        if !self.expired {
            self.lims.insert(e, r);
        }
        Ok(r)
    }

    fn limit_inner(&mut self, cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>) -> R<Lim<'h>> {
        let Some(lead) = self.leadterm(cx, e)? else { return Ok(None) };
        let (c, ex) = match lead {
            Lead::Small => return Ok(Some(Lim::Fin(self.b.zero(), 0))),
            Lead::Term { c, e, .. } => (c, e),
        };
        let Some(se) = const_sign(cx, self.b, ex)? else { return Ok(None) };
        Ok(match se {
            1 => {
                let s = self.sign(cx, c)?.unwrap_or(0);
                Some(Lim::Fin(self.b.zero(), s))
            }
            -1 => match self.sign(cx, c)? {
                Some(s @ (1 | -1)) => Some(Lim::Inf(s)),
                _ => None,
            },
            _ => {
                if self.free(c) {
                    Some(Lim::Fin(cx.eval(self.unwrap_g(c))?, 0))
                } else {
                    self.limit(cx, c)?
                }
            }
        })
    }

    /// The sign of `e` for all large x (1, -1, or 0 when e is 0), or `None`.
    pub fn sign(&mut self, cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>) -> R<i8> {
        let b = self.b;
        if self.free(e) {
            let s = const_sign(cx, b, e)?;
            // abs(c) of a symbolic c: positive for generic values (an outer variable of an iterated limit)
            if s.is_none() && matches!(call_of(b, e), Some((n, [_])) if n == "abs" || n == "real_abs") {
                return Ok(Some(1));
            }
            return Ok(s);
        }
        if e == self.x {
            return Ok(Some(1));
        }
        if let Some(&r) = self.signs.get(&e) {
            self.stats.hits += 1;
            return Ok(r);
        }
        if !self.step(cx)? {
            return Ok(None);
        }
        self.depth += 1;
        let r = self.sign_inner(cx, e);
        self.depth -= 1;
        let r = r?;
        if !self.expired {
            self.signs.insert(e, r);
        }
        Ok(r)
    }

    fn sign_inner(&mut self, cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>) -> R<i8> {
        let b = self.b;
        if self.g_arg(e).is_some() {
            return Ok(Some(1));
        }
        if let Some(args) = args_of(b, e, b.hd.mul) {
            let mut s = 1i8;
            for &a in args {
                let Some(sa) = self.sign(cx, a)? else { return Ok(None) };
                s *= sa;
            }
            return Ok(Some(s));
        }
        if let Some([base, p]) = args_of(b, e, b.hd.pow) {
            if b.sym_of(*base) == Some(b.hd.e) {
                return Ok(Some(1));
            }
            let Some(sb) = self.sign(cx, *base)? else { return Ok(None) };
            if sb == 1 {
                return Ok(Some(1));
            }
            if let View::Int(k) = b.heap().view(*p) {
                return Ok(Some(if k % 2 == 0 { sb * sb } else { sb }));
            }
            return Ok(None);
        }
        Ok(match self.leadterm(cx, e)? {
            Some(Lead::Term { c, .. }) => self.sign(cx, c)?,
            _ => None,
        })
    }

    /// The most rapidly varying subexpressions of `e` (one comparability class; `[x]` for the class of x, `[]`
    /// when `e` is free of x), or `None` when `e` is not an exp-log function the algorithm takes.
    fn mrv(&mut self, cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>) -> R<Vec<T<'h>>> {
        if self.free(e) {
            return Ok(Some(vec![]));
        }
        if e == self.x {
            return Ok(Some(vec![self.x]));
        }
        if let Some(r) = self.mrvs.get(&e) {
            self.stats.hits += 1;
            return Ok(r.clone());
        }
        if !self.step(cx)? {
            return Ok(None);
        }
        self.depth += 1;
        let r = self.mrv_inner(cx, e);
        self.depth -= 1;
        let r = r?;
        trace(self.b, self.depth, "mrv", e, &match &r {
            Some(v) => v.iter().map(|&t| show(self.b, t)).collect::<Vec<_>>().join(", "),
            None => "None".into(),
        });
        if !self.expired {
            self.mrvs.insert(e, r.clone());
        }
        Ok(r)
    }

    fn mrv_inner(&mut self, cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>) -> R<Vec<T<'h>>> {
        let b = self.b;
        if let Some(args) = args_of(b, e, b.hd.add).or_else(|| args_of(b, e, b.hd.mul)) {
            let mut acc = vec![];
            for &a in args {
                let Some(m) = self.mrv(cx, a)? else { return Ok(None) };
                let Some(n) = self.max(cx, acc, m)? else { return Ok(None) };
                acc = n;
            }
            return Ok(Some(acc));
        }
        if let Some([base, p]) = args_of(b, e, b.hd.pow) {
            if self.free(*p) {
                // a fractional power of a negative function is not real (decision 13C.6)
                if !matches!(b.heap().view(*p), View::Int(_)) && self.sign(cx, *base)? != Some(1) {
                    return Ok(None);
                }
                return self.mrv(cx, *base);
            }
            // an exponential: e^p, or b^p as exp(p log b) with b > 0 for large x
            if b.sym_of(*base) != Some(b.hd.e) && self.sign(cx, *base)? != Some(1) {
                return Ok(None);
            }
            let Some(s) = self.exp_arg(e) else { return Ok(None) };
            let s = self.ev(cx, s)?;
            let Some(inner) = self.mrv(cx, s)? else { return Ok(None) };
            return Ok(match self.unbounded(cx, s)? {
                Some(true) => self.max(cx, vec![e], inner)?,
                Some(false) => Some(inner),
                None => None,
            });
        }
        if let Some(s) = self.g_arg(e) {
            let Some(inner) = self.mrv(cx, s)? else { return Ok(None) };
            return Ok(match self.unbounded(cx, s)? {
                Some(true) => self.max(cx, vec![e], inner)?,
                Some(false) => Some(inner),
                None => None,
            });
        }
        let Some((name, args)) = call_of(b, e) else { return Ok(None) };
        if name == "log" && args.len() == 1 {
            // real values only: the argument must be positive for large x (decision 13C.6)
            if self.sign(cx, args[0])? != Some(1) {
                return Ok(None);
            }
            return self.mrv(cx, args[0]);
        }
        if !ANALYTIC.contains(&name.as_str()) {
            return Ok(None);
        }
        let mut acc = vec![];
        for &a in args {
            if self.free(a) {
                continue;
            }
            // a function of an argument with a finite limit (an infinite one was rewritten by `prepare`)
            if !matches!(self.limit(cx, a)?, Some(Lim::Fin(..))) {
                return Ok(None);
            }
            let Some(m) = self.mrv(cx, a)? else { return Ok(None) };
            let Some(n) = self.max(cx, acc, m)? else { return Ok(None) };
            acc = n;
        }
        Ok(Some(acc))
    }

    /// Does `s` tend to an infinity (`Some(true)`), or to a finite limit? An infinity of unknown sign counts (a
    /// leading term `c x^k` with k > 0 and a parameter `c`): exp(c x) is in the class of exp(x) for every c ≠ 0.
    fn unbounded(&mut self, cx: &mut NativeCx<'_, '_, 'h>, s: T<'h>) -> R<bool> {
        match self.limit(cx, s)? {
            Some(Lim::Inf(_)) => return Ok(Some(true)),
            Some(Lim::Fin(..)) => return Ok(Some(false)),
            None if self.expired => return Ok(None),
            None => {}
        }
        Ok(match self.leadterm(cx, s)? {
            Some(Lead::Term { e, c, .. }) => match const_sign(cx, self.b, e)? {
                Some(-1) => Some(true),
                Some(1) => Some(false),
                Some(_) if self.free(c) => Some(false),
                _ => None,
            },
            Some(Lead::Small) => Some(false),
            None => None,
        })
    }

    /// The faster of two mrv sets (their union when they are of one class).
    fn max(&mut self, cx: &mut NativeCx<'_, '_, 'h>, a: Vec<T<'h>>, c: Vec<T<'h>>) -> R<Vec<T<'h>>> {
        if a.is_empty() {
            return Ok(Some(c));
        }
        if c.is_empty() {
            return Ok(Some(a));
        }
        let union = |mut a: Vec<T<'h>>, c: Vec<T<'h>>| {
            for t in c {
                if !a.contains(&t) {
                    a.push(t);
                }
            }
            a
        };
        if a.iter().any(|t| c.contains(t)) {
            return Ok(Some(union(a, c)));
        }
        Ok(match self.compare(cx, a[0], c[0])? {
            Some(Ordering::Greater) => Some(a),
            Some(Ordering::Less) => Some(c),
            Some(Ordering::Equal) => Some(union(a, c)),
            None => None,
        })
    }

    /// The order of the comparability classes of `f` and `g` (x or exponentials): by the limit of
    /// `log f / log g`.
    fn compare(&mut self, cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, g: T<'h>) -> R<Ordering> {
        let lf = if f == self.x { self.log(f) } else { match self.exp_arg(f) { Some(s) => s, None => return Ok(None) } };
        let lg = if g == self.x { self.log(g) } else { match self.exp_arg(g) { Some(s) => s, None => return Ok(None) } };
        let q = self.ev(cx, self.b.div(lf, lg))?;
        // against x the quotient has the form s / log(x), and the general limit would compare exp(s) with x again
        // after moving up (a cycle): the walk decides these (logarithms against algebraic growth) directly
        let mut l = None;
        if f == self.x || g == self.x {
            l = super::walk::limit(cx, self.b, q, self.x, crate::bounds::Pt::PosInf, 1)?;
        }
        if l.is_none() {
            l = self.limit(cx, q)?;
        }
        Ok(match l {
            Some(Lim::Fin(v, _)) if v == self.b.zero() => Some(Ordering::Less),
            Some(Lim::Fin(..)) => Some(Ordering::Equal),
            Some(Lim::Inf(_)) => Some(Ordering::Greater),
            None => None,
        })
    }

    /// The leading term of `e` as x → +inf.
    pub fn leadterm(&mut self, cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>) -> R<Lead<'h>> {
        let b = self.b;
        if e == b.zero() {
            return Ok(Some(Lead::Small));
        }
        if self.free(e) {
            return Ok(Some(Lead::Term { c: e, e: b.zero(), w: b.one() }));
        }
        if let Some(&r) = self.leads.get(&e) {
            self.stats.hits += 1;
            return Ok(r);
        }
        if !self.step(cx)? {
            return Ok(None);
        }
        self.depth += 1;
        let r = self.leadterm_inner(cx, e);
        self.depth -= 1;
        let r = r?;
        trace(self.b, self.depth, "lead", e, &match r {
            Some(Lead::Term { c, e, w }) => format!("{} * ({})^{}", show(self.b, c), show(self.b, w), show(self.b, e)),
            other => format!("{other:?}"),
        });
        if !self.expired {
            self.leads.insert(e, r);
        }
        Ok(r)
    }

    fn leadterm_inner(&mut self, cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>) -> R<Lead<'h>> {
        let b = self.b;
        let Some(omega) = self.mrv(cx, e)? else { return Ok(None) };
        if omega.is_empty() {
            return Ok(Some(Lead::Term { c: e, e: b.zero(), w: b.one() }));
        }
        if omega.contains(&self.x) {
            // move up: x ← exp(x), then the coefficient and ω back down (x ← log x)
            let up = self.moveup(cx, e)?;
            if up == e || is_failure(b, up) {
                return Ok(None);
            }
            return Ok(match self.leadterm(cx, up)? {
                Some(Lead::Term { c, e, w }) => {
                    let lx = self.log(self.x);
                    let c = subst(cx, b, self.unwrap_g(c), self.x, lx)?;
                    let w = subst(cx, b, self.unwrap_g(w), self.x, lx)?;
                    let (c, w) = (self.to_g(c), self.to_g(w));
                    Some(Lead::Term { c, e, w })
                }
                other => other,
            });
        }
        let rw = self.rewrite(cx, e, &omega)?;
        trace(b, self.depth, "rewrite", e, &match rw {
            Some((f, _, logw, wexpr)) => format!("{} ; log w = {} ; w = {}", show(b, f), show(b, logw), show(b, wexpr)),
            None => "None".into(),
        });
        let Some((f, w, logw, wexpr)) = rw else { return Ok(None) };
        Ok(match self.lead_w(cx, f, w, logw)? {
            Some(WLead::Term(c, ex)) => {
                if !self.free(ex) || !is_free(b, c, w) {
                    return Ok(None);
                }
                Some(Lead::Term { c, e: ex, w: wexpr })
            }
            Some(WLead::Small) => Some(Lead::Small),
            None => None,
        })
    }

    /// `e` with x replaced by `exp(x)`, `log(exp(s))` as `s` and `exp(s)^p` as `exp(s p)` (x real).
    fn moveup(&mut self, cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>) -> Res<T<'h>> {
        let mut memo = FxHashMap::default();
        let t = self.up(e, &mut memo);
        self.ev(cx, t)
    }

    fn up(&self, t: T<'h>, memo: &mut FxHashMap<T<'h>, T<'h>>) -> T<'h> {
        let b = self.b;
        if t == self.x {
            return self.exp(self.x);
        }
        if let Some(&r) = memo.get(&t) {
            return r;
        }
        let r = match b.heap().view(t) {
            View::App { head, args } if !self.free(t) => {
                let na: Vec<T<'h>> = args.iter().map(|&a| self.up(a, memo)).collect();
                match b.sym_of(head) {
                    Some(s) if s == b.hd.add => b.add(&na),
                    Some(s) if s == b.hd.mul => b.mul(&na),
                    Some(s) if s == b.hd.pow && na.len() == 2 => match self.g_arg(na[0]) {
                        Some(q) => self.exp(b.mul(&[q, na[1]])),
                        None => b.pow(na[0], na[1]),
                    },
                    Some(s) if b.heap().sym_name(s) == "log" && na.len() == 1 => self.up_log(na[0]),
                    _ => b.app(head, &na),
                }
            }
            _ => t,
        };
        memo.insert(t, r);
        r
    }

    /// `log(u)` for a moved-up `u`: `log(c e^s) = log(c) + s`.
    fn up_log(&self, u: T<'h>) -> T<'h> {
        let b = self.b;
        if let Some(s) = self.g_arg(u) {
            return s;
        }
        if let Some(fs) = args_of(b, u, b.hd.mul) {
            let at: Vec<usize> = (0..fs.len()).filter(|&i| !self.free(fs[i])).collect();
            if let [i] = at.as_slice() {
                if let Some(s) = self.g_arg(fs[*i]) {
                    let rest: Vec<T<'h>> = fs.iter().enumerate().filter(|&(j, _)| j != *i).map(|(_, &f)| f).collect();
                    return b.add(&[self.log(b.mul(&rest)), s]);
                }
            }
        }
        self.log(u)
    }

    /// A fresh symbol for ω.
    fn fresh_w(&mut self) -> T<'h> {
        self.fresh += 1;
        let h = self.b.heap();
        h.sym_term(h.sym(&format!("w%gruntz{}", self.fresh)))
    }

    /// `e` rewritten in ω (Gruntz's step 3): `(f, w, log ω, ω)` with `f` a function of the symbol `w` and of
    /// classes below Ω, `log ω` and `ω` as terms in x.
    fn rewrite(&mut self, cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>, omega: &[T<'h>]) -> R<(T<'h>, T<'h>, T<'h>, T<'h>)> {
        let b = self.b;
        // ω from the smallest element (its argument holds no other element of Ω) ...
        let Some(&g0) = omega.iter().min_by_key(|&&t| leaves(b, t)) else { return Ok(None) };
        let Some(h0) = self.exp_arg(g0) else { return Ok(None) };
        let h0 = self.ev(cx, h0)?;
        // ... then the slowest of the elements whose arguments hold no other element: the ratios c = lim s/h are then
        // at least 1 in size, so exp(-13/4 x) against exp(-3/4 x) gives ω^(13/3), not ω^(3/13) (fewer ramifications)
        let mut ratios: Vec<(T<'h>, T<'h>)> = vec![];
        for &fi in omega {
            let c = if fi == g0 {
                b.one()
            } else {
                let Some(si) = self.exp_arg(fi) else { return Ok(None) };
                let q = self.ev(cx, b.div(si, h0))?;
                match self.limit(cx, q)? {
                    Some(Lim::Fin(c, _)) if self.free(c) => c,
                    _ => return Ok(None),
                }
            };
            // a constant ratio in another form ((a - b)/(b - a)): its simplest one, so that s - c h cancels
            let c = if b.ar.num(c).is_some() {
                c
            } else {
                let sc = apply(cx, b, "simplify", &[c])?;
                super::tidy(cx, b, sc)?
            };
            ratios.push((fi, c));
        }
        let mut g = g0;
        let mut cg = b.one();
        for &(fi, c) in &ratios {
            let inner = self.exp_arg(fi).is_some_and(|s| omega.iter().all(|&o| o == fi || is_free(b, s, o)));
            if !inner || fi == g {
                continue;
            }
            let d = cx.eval(b.sub(b.call("abs", &[c]), b.call("abs", &[cg])))?;
            if const_sign(cx, b, d)? == Some(-1) {
                g = fi;
                cg = c;
            }
        }
        let Some(h1) = self.exp_arg(g) else { return Ok(None) };
        let hx = self.ev(cx, h1)?;
        let sg = match self.limit(cx, hx)? {
            Some(Lim::Inf(s)) => s,
            _ => return Ok(None),
        };
        let w = self.fresh_w();
        let logw = self.ev(cx, if sg > 0 { b.neg(hx) } else { hx })?;
        let wexpr = if sg > 0 { self.exp(b.neg(hx)) } else { g };
        let mut order: Vec<T<'h>> = omega.to_vec();
        order.sort_by_key(|&t| std::cmp::Reverse(leaves(b, t)));
        let mut f = e;
        for &fi in &order {
            if !self.step(cx)? {
                return Ok(None);
            }
            let c = if fi == g {
                b.one()
            } else {
                let Some(&(_, c0)) = ratios.iter().find(|(o, _)| *o == fi) else { return Ok(None) };
                cx.eval(b.div(c0, cg))?
            };
            let Some(si) = self.exp_arg(fi) else { return Ok(None) };
            let rest = self.ev(cx, b.sub(si, b.mul(&[c, hx])))?;
            // log(1 + x) (x + 1/2) - log(x) (x + 1/2) as (x + 1/2) log((1 + x)/x): otherwise the evaluator turns
            // exp(rest) back into the powers just rewritten
            let rest = self.combine_logs(cx, rest)?;
            // x log(4) - 2 x log(2) is 0: logarithms of integers over primes (13C.5)
            let rest = self.unwrap_g(rest);
            let rest = super::tidy(cx, b, rest)?;
            let rest = self.to_g(rest);
            let a = b.mul(&[self.exp(rest), b.pow(w, b.mul(&[b.int(-sg as i64), c]))]);
            f = replace(b, f, fi, a);
        }
        let f = self.ev(cx, f)?;
        if is_failure(b, f) || order.iter().any(|&o| !is_free(b, f, o)) {
            return Ok(None);
        }
        Ok(Some((f, w, logw, wexpr)))
    }

    /// `r` with its terms `a log(u)` grouped by the factor `a` (up to rational multiples) into `a log(prod u^q)`
    /// (the arguments are positive for large x: they are bases of exponentials).
    fn combine_logs(&mut self, cx: &mut NativeCx<'_, '_, 'h>, r: T<'h>) -> Res<T<'h>> {
        let b = self.b;
        if leaves(b, r) > 2000 {
            return Ok(r);
        }
        let r = apply(cx, b, "expand", &[r])?;
        let terms: Vec<T<'h>> = args_of(b, r, b.hd.add).map_or_else(|| vec![r], |a| a.to_vec());
        // (factor, [(q, u)]) groups, and the other terms
        let mut groups: Vec<(T<'h>, Vec<(T<'h>, T<'h>)>)> = vec![];
        let mut other = vec![];
        for &t in &terms {
            let fs: Vec<T<'h>> = args_of(b, t, b.hd.mul).map_or_else(|| vec![t], |a| a.to_vec());
            let at: Vec<usize> = (0..fs.len())
                .filter(|&i| matches!(call_of(b, fs[i]), Some((n, [u])) if n == "log" && !self.free(*u)))
                .collect();
            let [i] = at.as_slice() else {
                other.push(t);
                continue;
            };
            let Some((_, &[u])) = call_of(b, fs[*i]) else { unreachable!("a log call") };
            let a = b.mul(&fs.iter().enumerate().filter(|&(j, _)| j != *i).map(|(_, &f)| f).collect::<Vec<_>>());
            let mut placed = false;
            for (g, items) in groups.iter_mut() {
                let q = self.ev(cx, b.div(a, *g))?;
                if matches!(b.heap().view(q), View::Int(_) | View::Rational { .. }) {
                    items.push((q, u));
                    placed = true;
                    break;
                }
            }
            if !placed {
                groups.push((a, vec![(b.one(), u)]));
            }
        }
        if groups.iter().all(|(_, items)| items.len() < 2) {
            return Ok(r);
        }
        for (g, items) in groups {
            let prod = b.mul(&items.iter().map(|&(q, u)| b.pow(u, q)).collect::<Vec<_>>());
            let prod = apply(cx, b, "together", &[prod])?;
            other.push(b.mul(&[g, self.log(prod)]));
        }
        self.ev(cx, b.add(&other))
    }

    /// The leading term of `f` in the symbol `w` at 0⁺ (`log w` is `logw`): through the series kernels, and by
    /// the structure of `f` when `w` has symbolic exponents.
    fn lead_w(&mut self, cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, w: T<'h>, logw: T<'h>) -> R<WLead<'h>> {
        let b = self.b;
        if !self.step(cx)? {
            return Ok(None);
        }
        if f == b.zero() {
            return Ok(Some(WLead::Small));
        }
        if is_free(b, f, w) {
            return Ok(Some(WLead::Term(f, b.zero())));
        }
        if f == w {
            return Ok(Some(WLead::Term(b.one(), b.one())));
        }
        if !symbolic_powers(b, f, w) {
            return self.lead_series(cx, f, w, logw);
        }
        if let Some(args) = args_of(b, f, b.hd.mul) {
            let (mut c, mut ex) = (vec![], vec![]);
            for &a in args {
                match self.lead_w(cx, a, w, logw)? {
                    Some(WLead::Term(ca, ea)) => {
                        c.push(ca);
                        ex.push(ea);
                    }
                    _ => return Ok(None),
                }
            }
            return Ok(Some(WLead::Term(self.ev(cx, b.mul(&c))?, self.ev(cx, b.add(&ex))?)));
        }
        if let Some(p) = self.g_arg(f) {
            return Ok(match self.lead_w(cx, p, w, logw)? {
                Some(WLead::Small) => Some(WLead::Term(b.one(), b.zero())),
                Some(WLead::Term(c, ex)) => match const_sign(cx, b, ex)? {
                    Some(1) => Some(WLead::Term(b.one(), b.zero())),
                    Some(0) => Some(WLead::Term(self.exp(c), b.zero())),
                    _ => None,
                },
                None => None,
            });
        }
        if let Some([base, p]) = args_of(b, f, b.hd.pow) {
            if b.sym_of(*base) == Some(b.hd.e) {
                return Ok(match self.lead_w(cx, *p, w, logw)? {
                    Some(WLead::Small) => Some(WLead::Term(b.one(), b.zero())),
                    Some(WLead::Term(c, ex)) => match const_sign(cx, b, ex)? {
                        Some(1) => Some(WLead::Term(b.one(), b.zero())),
                        Some(0) => Some(WLead::Term(self.exp(c), b.zero())),
                        _ => None,
                    },
                    None => None,
                });
            }
            if is_free(b, *p, w) && !self.free(*p) {
                // base^p with p varying in x: exp(p log(base)), whose exponent has a leading term of its own
                let q = self.ev(cx, b.mul(&[*p, self.log_of(*base)]))?;
                return self.lead_w(cx, self.exp(q), w, logw);
            }
            if is_free(b, *p, w) {
                return Ok(match self.lead_w(cx, *base, w, logw)? {
                    Some(WLead::Term(c, ex)) => Some(WLead::Term(self.ev(cx, b.pow(c, *p))?, self.ev(cx, b.mul(&[ex, *p]))?)),
                    _ => None,
                });
            }
            return Ok(None);
        }
        if let Some(args) = args_of(b, f, b.hd.add) {
            let mut best: Option<(T<'h>, Vec<T<'h>>)> = None;
            for &a in args {
                let Some(WLead::Term(c, ex)) = self.lead_w(cx, a, w, logw)? else { return Ok(None) };
                best = match best {
                    None => Some((ex, vec![c])),
                    Some((be, mut bc)) => {
                        let d = self.ev(cx, b.sub(ex, be))?;
                        match const_sign(cx, b, d)? {
                            Some(-1) => Some((ex, vec![c])),
                            Some(0) => {
                                bc.push(c);
                                Some((be, bc))
                            }
                            Some(_) => Some((be, bc)),
                            None => return Ok(None),
                        }
                    }
                };
            }
            let Some((ex, cs)) = best else { return Ok(None) };
            let c = self.ev(cx, b.add(&cs))?;
            // a cancellation needs the next terms, which this form does not have
            return Ok(if c == b.zero() { None } else { Some(WLead::Term(c, ex)) });
        }
        if let Some((name, &[u])) = call_of(b, f).as_ref().map(|(n, a)| (n.as_str(), *a)) {
            if name == "log" {
                let Some(WLead::Term(c, ex)) = self.lead_w(cx, u, w, logw)? else { return Ok(None) };
                let r = self.ev(cx, b.add(&[self.log_of(c), b.mul(&[ex, logw])]))?;
                return Ok((r != b.zero()).then_some(WLead::Term(r, b.zero())));
            }
        }
        Ok(None)
    }

    /// The leading term of `f` in `w` from the series kernels (rational exponents).
    fn lead_series(&mut self, cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, w: T<'h>, logw: T<'h>) -> R<WLead<'h>> {
        let b = self.b;
        let h = b.heap();
        let logt = self.log(w);
        for n in [2i64, 6, 14] {
            if !self.step(cx)? {
                return Ok(None);
            }
            self.stats.series += 1;
            let fe = self.unwrap_g(f);
            let lw = self.unwrap_g(logw);
            let Some(ex) = mc_poly::expansion(cx, &self.stop, fe, w, n, true, Some(lw))? else { return Ok(None) };
            for (i, &c) in ex.coeffs.iter().enumerate() {
                if c == b.zero() || is_failure(b, c) {
                    if is_failure(b, c) {
                        return Ok(None);
                    }
                    continue;
                }
                let c = if is_free(b, c, w) { self.to_g(c) } else { self.ev(cx, replace(b, c, logt, logw))? };
                if !is_free(b, c, w) {
                    return Ok(None);
                }
                if c == b.zero() || self.zero_coefficient(cx, c)? {
                    continue;
                }
                let k = ex.val + i as i64;
                let e = b.rat(k, ex.den);
                let _ = h;
                return Ok(Some(WLead::Term(c, e)));
            }
            if ex.prec > 0 && n == 14 {
                return Ok(Some(WLead::Small));
            }
        }
        Ok(None)
    }

    /// Is the coefficient `c` (a function of x) zero, by a common denominator? (The kernels' zero test is
    /// polynomial in their kernels; quotients that cancel need more.)
    fn zero_coefficient(&mut self, cx: &mut NativeCx<'_, '_, 'h>, c: T<'h>) -> Res<bool> {
        if self.free(c) || leaves(self.b, c) > 400 {
            return Ok(false);
        }
        let t = apply(cx, self.b, "together", &[c])?;
        Ok(t == self.b.zero())
    }

    /// `e` in the exp-log form the algorithm takes (module comment), or `None` when a part has no such form here.
    pub fn prepare(&mut self, cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>) -> R<T<'h>> {
        let mut memo = FxHashMap::default();
        let Some(t) = self.prep(cx, e, &mut memo)? else { return Ok(None) };
        let t = self.ev(cx, t)?;
        // exponents expanded, so that their terms c log(v) with constant c become powers (see `exp`)
        let mut memo2 = FxHashMap::default();
        let t = self.expand_g(cx, t, &mut memo2)?;
        Ok(Some(self.ev(cx, t)?))
    }

    fn expand_g(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, memo: &mut FxHashMap<T<'h>, T<'h>>) -> Res<T<'h>> {
        let b = self.b;
        if self.free(t) {
            return Ok(t);
        }
        if let Some(&r) = memo.get(&t) {
            return Ok(r);
        }
        let View::App { head, args } = b.heap().view(t) else { return Ok(t) };
        let mut na = Vec::with_capacity(args.len());
        for &a in args {
            na.push(self.expand_g(cx, a, memo)?);
        }
        let r = if let (Some(_), [u]) = (self.g_arg(t), na.as_slice()) {
            let u = if leaves(b, *u) < 3000 { apply(cx, b, "expand", &[self.unwrap_g(*u)])? } else { *u };
            let u = self.to_g(u);
            self.exp(u)
        } else {
            match b.sym_of(head) {
                Some(s) if s == b.hd.add => b.add(&na),
                Some(s) if s == b.hd.mul => b.mul(&na),
                Some(s) if s == b.hd.pow && na.len() == 2 => b.pow(na[0], na[1]),
                _ => b.app(head, &na),
            }
        };
        memo.insert(t, r);
        Ok(r)
    }

    fn prep(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, memo: &mut FxHashMap<T<'h>, Option<T<'h>>>) -> R<T<'h>> {
        let b = self.b;
        if self.free(t) || t == self.x {
            return Ok(Some(t));
        }
        if let Some(&r) = memo.get(&t) {
            return Ok(r);
        }
        if !self.step(cx)? {
            return Ok(None);
        }
        let View::App { head, args } = b.heap().view(t) else { return Ok(None) };
        let mut na = Vec::with_capacity(args.len());
        for &a in args {
            let Some(p) = self.prep(cx, a, memo)? else {
                memo.insert(t, None);
                return Ok(None);
            };
            na.push(p);
        }
        let r = match b.sym_of(head) {
            Some(s) if s == b.hd.add => Some(b.add(&na)),
            Some(s) if s == b.hd.mul => Some(b.mul(&na)),
            Some(s) if s == b.hd.pow && na.len() == 2 => {
                let (base, p) = (na[0], na[1]);
                // u^p with u < 0 and p not an integer: (-1)^p (-u)^p, a constant times a real power (principal
                // values), so a limit 0 is still found and an infinite one is left undecided (complex sign)
                let fractional = self.free(p) && !matches!(b.heap().view(p), View::Int(_)) && !self.free(base);
                if fractional && self.sign(cx, base)? == Some(-1) {
                    Some(b.mul(&[b.pow(b.int(-1), p), b.pow(b.neg(base), p)]))
                } else {
                    Some(b.pow(base, p))
                }
            }
            Some(s) => {
                let name = b.heap().sym_name(s);
                let u = cx.eval(b.app(head, &na))?;
                if u != t && !matches!(call_of(b, u), Some((n, _)) if n == name) {
                    // the evaluator rewrote the call (a special value, a simpler form): take that
                    let r = self.prep(cx, u, memo)?;
                    memo.insert(t, r);
                    return Ok(r);
                }
                self.prep_call(cx, &name, &na)?
            }
            None => None,
        };
        memo.insert(t, r);
        Ok(r)
    }

    /// The exp-log form of the call `name(args)` (arguments prepared).
    fn prep_call(&mut self, cx: &mut NativeCx<'_, '_, 'h>, name: &str, args: &[T<'h>]) -> R<T<'h>> {
        let b = self.b;
        let h = b.heap();
        let pi = || b.sym(h.sym("pi"));
        let half = b.rat(1, 2);
        let ex = |u: T<'h>| b.call(G, &[u]);
        let inv = |u: T<'h>| b.powi(u, -1);
        let call = |n: &str, a: &[T<'h>]| b.call(n, a);
        Ok(Some(match (name, args) {
            ("exp", [u]) => ex(*u),
            ("log", [u]) => self.log(*u),
            ("sqrt", [u]) => b.pow(*u, half),
            ("sinh", [u]) => b.mul(&[half, b.sub(ex(*u), ex(b.neg(*u)))]),
            ("cosh", [u]) => b.mul(&[half, b.add(&[ex(*u), ex(b.neg(*u))])]),
            ("tanh", [u]) => {
                let e2 = ex(b.mul(&[b.int(2), *u]));
                b.div(b.sub(e2, b.one()), b.add(&[e2, b.one()]))
            }
            ("coth", [u]) => {
                let e2 = ex(b.mul(&[b.int(2), *u]));
                b.div(b.add(&[e2, b.one()]), b.sub(e2, b.one()))
            }
            ("sech", [u]) => b.div(b.int(2), b.add(&[ex(*u), ex(b.neg(*u))])),
            ("csch", [u]) => b.div(b.int(2), b.sub(ex(*u), ex(b.neg(*u)))),
            ("asinh", [u]) => self.log(b.add(&[*u, b.pow(b.add(&[b.powi(*u, 2), b.one()]), half)])),
            ("acosh", [u]) => self.log(b.add(&[*u, b.pow(b.sub(b.powi(*u, 2), b.one()), half)])),
            ("atanh", [u]) => b.mul(&[half, self.log(b.div(b.add(&[b.one(), *u]), b.sub(b.one(), *u)))]),
            ("tan", [u]) => b.div(call("sin", &[*u]), call("cos", &[*u])),
            ("cot", [u]) => b.div(call("cos", &[*u]), call("sin", &[*u])),
            ("sec", [u]) => inv(call("cos", &[*u])),
            ("csc", [u]) => inv(call("sin", &[*u])),
            ("abs" | "real_abs", [u]) => match self.sign(cx, *u)? {
                Some(s) => b.mul(&[b.int(s.into()), *u]),
                None => return Ok(None),
            },
            ("sign", [u]) => match self.sign(cx, *u)? {
                Some(s) => b.int(s.into()),
                None => return Ok(None),
            },
            ("unit_step", [u]) => match self.sign(cx, *u)? {
                Some(s) => b.int(if s >= 0 { 1 } else { 0 }),
                None => return Ok(None),
            },
            (m @ ("max" | "min"), [p, q]) => match self.sign(cx, b.sub(*p, *q))? {
                Some(s) => {
                    if (s >= 0) == (m == "max") {
                        *p
                    } else {
                        *q
                    }
                }
                None => return Ok(None),
            },
            ("atan", [u]) => match self.limit(cx, *u)? {
                Some(Lim::Inf(s)) => b.sub(b.mul(&[b.rat(s.into(), 2), pi()]), call("atan", &[inv(*u)])),
                Some(Lim::Fin(..)) => call("atan", &[*u]),
                None => return Ok(None),
            },
            ("acot", [u]) => return self.prep_call(cx, "atan", &[inv(*u)]),
            ("factorial", [u]) => return self.prep_call(cx, "gamma", &[b.add(&[*u, b.one()])]),
            ("binomial", [n, k]) => {
                let g = |s: &mut Self, cx: &mut NativeCx<'_, '_, 'h>, a: T<'h>| s.prep_call(cx, "gamma", &[a]);
                let (Some(a), Some(c), Some(d)) = (
                    g(self, cx, b.add(&[*n, b.one()]))?,
                    g(self, cx, b.add(&[*k, b.one()]))?,
                    g(self, cx, b.add(&[*n, b.neg(*k), b.one()]))?,
                ) else {
                    return Ok(None);
                };
                b.div(a, b.mul(&[c, d]))
            }
            ("pochhammer", [a, n]) => {
                let (Some(p), Some(q)) = (self.prep_call(cx, "gamma", &[b.add(&[*a, *n])])?, self.prep_call(cx, "gamma", &[*a])?) else {
                    return Ok(None);
                };
                b.div(p, q)
            }
            ("factorial_power", [n, k]) => {
                let (Some(p), Some(q)) = (
                    self.prep_call(cx, "gamma", &[b.add(&[*n, b.one()])])?,
                    self.prep_call(cx, "gamma", &[b.add(&[*n, b.neg(*k), b.one()])])?,
                ) else {
                    return Ok(None);
                };
                b.div(p, q)
            }
            ("beta", [p, q]) if !self.free(*p) || !self.free(*q) => {
                let (Some(gp), Some(gq), Some(gs)) = (
                    self.prep_call(cx, "gamma", &[*p])?,
                    self.prep_call(cx, "gamma", &[*q])?,
                    self.prep_call(cx, "gamma", &[b.add(&[*p, *q])])?,
                ) else {
                    return Ok(None);
                };
                b.div(b.mul(&[gp, gq]), gs)
            }
            ("gamma", [u]) if !self.free(*u) => match self.limit(cx, *u)? {
                Some(Lim::Inf(1)) => {
                    self.truncated = true;
                    ex(super::asym::log_gamma(b, *u, self.terms))
                }
                Some(Lim::Fin(v, _)) => {
                    // a pole at -k (k = 0, 1, ...): gamma(u) = gamma(u + k + 1) / (u (u + 1) ... (u + k))
                    match b.heap().view(v) {
                        View::Int(k) if k <= 0 && k > -10_000 => {
                            let k = -k;
                            let mut den = vec![];
                            for j in 0..=k {
                                den.push(b.add(&[*u, b.int(j)]));
                            }
                            b.div(call("gamma", &[b.add(&[*u, b.int(k + 1)])]), b.mul(&den))
                        }
                        _ => call("gamma", &[*u]),
                    }
                }
                _ => return Ok(None),
            },
            ("log_gamma", [u]) if !self.free(*u) => match self.limit(cx, *u)? {
                Some(Lim::Inf(1)) => {
                    self.truncated = true;
                    super::asym::log_gamma(b, *u, self.terms)
                }
                Some(Lim::Fin(..)) => call("log_gamma", &[*u]),
                _ => return Ok(None),
            },
            (f, all) => {
                // the remaining functions of one varying argument (the last): at an infinity their asymptotic
                // expansion, at a finite limit the function itself
                let Some((&u, params)) = all.split_last() else { return Ok(None) };
                if params.iter().any(|&p| !self.free(p)) || self.free(u) {
                    return Ok(if ANALYTIC.contains(&f) && params.iter().all(|&p| self.free(p)) {
                        Some(call(f, all))
                    } else {
                        None
                    });
                }
                match self.limit(cx, u)? {
                    Some(Lim::Inf(s)) => match super::asym::at_infinity(b, f, params, u, s, self.terms) {
                        Some(r) => {
                            self.truncated = true;
                            r
                        }
                        None => return Ok(None),
                    },
                    Some(Lim::Fin(..)) if ANALYTIC.contains(&f) => call(f, all),
                    _ => return Ok(None),
                }
            }
        }))
    }
}

/// A trace line on standard error when `MC_GRUNTZ_TRACE` is set (development aid).
fn trace<'h>(b: &Build<'h>, depth: u32, what: &str, e: T<'h>, r: &str) {
    if std::env::var_os("MC_GRUNTZ_TRACE").is_some() {
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        let t0 = *START.get_or_init(Instant::now);
        let ms = t0.elapsed().as_millis();
        eprintln!("{ms:>6} {:indent$}{what} {} => {r}", "", show(b, e), indent = 2 * depth as usize);
    }
}

/// A term as text for traces (prefix form).
fn show<'h>(b: &Build<'h>, t: T<'h>) -> String {
    let h = b.heap();
    match h.view(t) {
        View::Int(k) => k.to_string(),
        View::Sym(s) => h.sym_name(s),
        View::Rational { .. } | View::BigInt { .. } => match b.ar.num(t) {
            Some(mc_arith::Num::Real(r)) => format!("{:.6}", r.to_f64()),
            _ => "?".into(),
        },
        View::App { head, args } => {
            let a: Vec<String> = args.iter().map(|&a| show(b, a)).collect();
            format!("{}({})", show(b, head), a.join(", "))
        }
        _ => "#".into(),
    }
}

/// Does `f` hold a power of a term in `w` with an exponent that is not a rational number (or an exponential
/// with `w` in the exponent of a base other than e)?
fn symbolic_powers<'h>(b: &Build<'h>, f: T<'h>, w: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![f];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(t) = stack.pop() {
        if !seen.insert(t) {
            continue;
        }
        let View::App { head, args } = h.view(t) else { continue };
        if b.sym_of(head) == Some(b.hd.pow) && args.len() == 2 && !is_free(b, args[0], w) {
            let rational = matches!(h.view(args[1]), View::Int(_) | View::Rational { .. });
            if !rational && is_free(b, args[1], w) {
                return true;
            }
        }
        stack.extend_from_slice(args);
    }
    false
}

/// Does `t` contain a call with head `g`?
fn has_head<'h>(b: &Build<'h>, t: T<'h>, g: mc_api::Sym) -> bool {
    let h = b.heap();
    let mut stack = vec![t];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            if b.sym_of(head) == Some(g) {
                return true;
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

/// `t` with `G(u)` as `e^u`.
fn from_g_in<'h>(b: &Build<'h>, g: mc_api::Sym, t: T<'h>, memo: &mut FxHashMap<T<'h>, T<'h>>) -> T<'h> {
    if let Some(&r) = memo.get(&t) {
        return r;
    }
    let r = match b.heap().view(t) {
        View::App { head, args } => {
            let na: Vec<T<'h>> = args.iter().map(|&a| from_g_in(b, g, a, memo)).collect();
            match b.sym_of(head) {
                Some(s) if s == g && na.len() == 1 => b.pow(b.sym(b.hd.e), na[0]),
                Some(s) if s == b.hd.add => b.add(&na),
                Some(s) if s == b.hd.mul => b.mul(&na),
                Some(s) if s == b.hd.pow && na.len() == 2 => b.pow(na[0], na[1]),
                _ => b.app(head, &na),
            }
        }
        _ => t,
    };
    memo.insert(t, r);
    r
}

/// The number of nodes of `t` (as a tree, capped).
pub(crate) fn leaves<'h>(b: &Build<'h>, t: T<'h>) -> usize {
    let h = b.heap();
    let mut n = 0usize;
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        n += 1;
        if n > 100_000 {
            break;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    n
}

/// `t` with every occurrence of the subterm `from` replaced by `to` (sums, products and powers rebuilt with the
/// canonical arithmetic, other calls as they are; not evaluated).
pub(crate) fn replace<'h>(b: &Build<'h>, t: T<'h>, from: T<'h>, to: T<'h>) -> T<'h> {
    let mut memo: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    replace_in(b, t, from, to, &mut memo)
}

fn replace_in<'h>(b: &Build<'h>, t: T<'h>, from: T<'h>, to: T<'h>, memo: &mut FxHashMap<T<'h>, T<'h>>) -> T<'h> {
    if t == from {
        return to;
    }
    if let Some(&r) = memo.get(&t) {
        return r;
    }
    let r = match b.heap().view(t) {
        View::App { head, args } if !is_free(b, t, from) => {
            let na: Vec<T<'h>> = args.iter().map(|&a| replace_in(b, a, from, to, memo)).collect();
            match b.sym_of(head) {
                Some(s) if s == b.hd.add => b.add(&na),
                Some(s) if s == b.hd.mul => b.mul(&na),
                Some(s) if s == b.hd.pow && na.len() == 2 => b.pow(na[0], na[1]),
                _ => b.app(head, &na),
            }
        }
        _ => t,
    };
    memo.insert(t, r);
    r
}

/// The limit of `g` as `x` tends to `pt` from `side` (1 from above, -1 from below; ignored at an infinity) by
/// Gruntz's algorithm, giving up at `deadline`; `None` when not decided.
pub(crate) fn limit<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    x: T<'h>,
    pt: crate::bounds::Pt<'h>,
    side: i8,
    deadline: Instant,
) -> R<Lim<'h>> {
    use crate::bounds::Pt;
    let h = b.heap();
    let t = ["t%gruntz", "t%gruntz2"].into_iter().map(|n| h.sym_term(h.sym(n))).find(|&t| is_free(b, g, t));
    let Some(t) = t else { return Ok(None) };
    let at = match pt {
        Pt::Fin(c) => b.add(&[c, b.mul(&[b.int(side.into()), b.powi(t, -1)])]),
        Pt::PosInf => t,
        Pt::NegInf => b.neg(t),
    };
    let gt = subst(cx, b, g, x, at)?;
    if is_failure(b, gt) {
        return Ok(None);
    }
    at_infinity(cx, b, gt, t, deadline)
}

/// The limit of `e` as `t → +inf` (see [`limit`]). A result that used truncated asymptotic expansions is computed
/// again with more terms and kept only when both agree (decision 13C.3).
pub(crate) fn at_infinity<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, e: T<'h>, t: T<'h>, deadline: Instant) -> R<Lim<'h>> {
    let mut g = Gruntz::new(b, t, deadline, TERMS.0);
    let Some(p) = g.prepare(cx, e)? else { return Ok(None) };
    let r = g.limit(cx, p)?;
    if r.is_none() || !g.truncated {
        return Ok(r);
    }
    let mut g2 = Gruntz::new(b, t, deadline, TERMS.1);
    let Some(p2) = g2.prepare(cx, e)? else { return Ok(None) };
    let r2 = g2.limit(cx, p2)?;
    Ok(match (r, r2) {
        (Some(Lim::Fin(u, s)), Some(Lim::Fin(v, _))) if u == v => Some(Lim::Fin(u, s)),
        (Some(Lim::Inf(s)), Some(Lim::Inf(q))) if s == q => Some(Lim::Inf(s)),
        _ => None,
    })
}
