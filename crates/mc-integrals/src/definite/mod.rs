//! Definite integrals (items 2C part 4 and 14D).
//!
//! `integrate(f, [x, a, b])` is one driver ([`integrate`]) over an ordered cascade ([`solve`]). The organisation
//! (convergence first, conditions accumulated, methods in a fixed order, Newton-Leibniz in the middle) follows the
//! study of how definite integration is usually organised; every method is implemented from the literature cited
//! in its module:
//!
//! 1. **trivial cases**: a zero integrand, equal bounds, reversed bounds (a sign), a constant integrand,
//!    `dirac_delta` factors ([`delta`]), functions defined by cases split at their switching points ([`split`]);
//! 2. **convergence** ([`conv`]): the behaviour of `f` at both ends and at its interior singular points from the
//!    critical-point search ([`points`]); a divergent integral is `failure(divergent, integrate, f)` at once; a
//!    parameter the convergence depends on gives conditions (`re(n) > -1`, `re(c) > 0`), recorded for the result,
//!    and the integral is computed with them as facts (decision 14D.2);
//! 3. **linearity**: constant factors, then a sum term by term when every term converges;
//! 4. **closed forms** ([`forms`]): Gaussian integrals with polynomial factors over the real line, `x^r log(x)^k
//!    exp(-a x^d)` over `(0, inf)` (Euler's gamma integral and its derivatives), Beta integrals of binomials;
//! 5. **residues** ([`residue`]): rational functions over the real line, Fourier-type integrands `R(x) exp(i a x)`
//!    (Jordan's lemma), rational functions of `sin` and `cos` over a period (`z = exp(i x)`);
//! 6. **symmetry**: an odd integrand over a symmetric range is 0;
//! 7. **Newton-Leibniz**: `F(b) - F(a)` with `F` from [`super::antiderivative`], made safe by splitting at every
//!    critical point of `f` and `F` in `[a, b]`: between consecutive critical points `F` is a continuous
//!    antiderivative, so the integral is the sum over the pieces of `F(right⁻) - F(left⁺)`; one-sided values at
//!    critical points and infinities are limits (the walk, then Gruntz's algorithm, [`mc_limit::limit_with`]).
//!    With `principal_value: true` an interior point where both sides diverge contributes the limit of
//!    `F(c - ε) - F(c + ε)`;
//! 8. the hook of the Meijer G methods ([`meijer_g`], added later).
//!
//! Every result with numeric data is checked against `n_integrate` before it is returned ([`check`]); one with
//! parameters is checked at sample values that satisfy its conditions. A value the check refutes is not returned.
//! When something cannot be decided (a comparison, the roots of a polynomial with symbolic coefficients, a limit, a
//! function the critical-point search does not know) the result is `None` and the call stays unevaluated: no answer
//! is better than a wrong one. Every step polls the evaluator (`tick`), and the cascade has a time budget.
//!
//! Symbolic bounds (symbols, complex numbers): `F(b) - F(a)` only when `f` and `F` are continuous for every real
//! `x`, else the call stays (decision 2C4.3).
//!
//! Decisions are numbered 2C4.1, ... (`notes/2c-definite.md`) and 14D.1, ... (`NOTES.md`, item 14D).
pub mod check;
mod conv;
mod meijer;
pub(crate) mod delta;
pub(crate) mod forms;
mod points;
mod residue;
mod split;
mod subst;
pub(crate) use mc_calculus::common::{call_of, is_failure};
pub(crate) use mc_limit::bounds::{cmp, real_constant, sign, subst, Pt};
use mc_limit::bounds::{inf, point};

use super::{antiderivative, apply, args_of, is_free};
use mc_limit::walk::Lim;
use mc_limit::{limit_with, Direction, Options, Outcome};
use mc_calculus::term::Build;
use conv::Conv;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The options of one definite integral.
#[derive(Clone, Copy, Debug)]
pub struct Opts {
    /// `principal_value: true`
    pub pv: bool,
    /// `generate_conditions` (default true): attach the conditions the result needs
    pub conditions: bool,
    /// an internal call under conditions already recorded by an outer one: no conditions, no numeric check
    pub inner: bool,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts { pv: false, conditions: true, inner: false }
    }
}

/// The options term of `integrate` read: `generate_conditions`, `principal_value`, `assumptions` (returned apart)
/// and the internal [`INNER`]; `None` when `t` is not an options term or holds another option.
pub fn options<'h>(h: &'h mc_core::Heap, t: T<'h>) -> Option<(Opts, Option<T<'h>>)> {
    let View::App { head, args } = h.view(t) else { return None };
    if head != h.sym_term(h.sym("options")) || args.is_empty() {
        return None;
    }
    let mut o = Opts::default();
    let mut assumptions = None;
    let falsity = h.sym_term(h.sym(names::FALSE));
    let truth = h.sym_term(h.sym(names::TRUE));
    for &r in args {
        let View::App { args: &[k, v], .. } = h.view(r) else { return None };
        let View::Sym(s) = h.view(k) else { return None };
        match h.sym_name(s).as_str() {
            "generate_conditions" => o.conditions = v != falsity,
            "principal_value" => o.pv = v == truth,
            "assumptions" => assumptions = Some(v),
            INNER => o.inner = v == truth,
            _ => return None,
        }
    }
    Some((o, assumptions))
}

/// The options term for `o` (without assumptions); `None` for the defaults.
pub fn options_term<'h>(h: &'h mc_core::Heap, o: &Opts) -> Option<T<'h>> {
    let rule = |k: &str, v: bool| {
        let v = h.sym_term(h.sym(if v { names::TRUE } else { names::FALSE }));
        h.call(h.sym(names::RULE), &[h.sym_term(h.sym(k)), v])
    };
    let mut rs = vec![];
    if !o.conditions {
        rs.push(rule("generate_conditions", false));
    }
    if o.pv {
        rs.push(rule("principal_value", true));
    }
    if o.inner {
        rs.push(rule(INNER, true));
    }
    (!rs.is_empty()).then(|| h.call(h.sym("options"), &rs))
}

/// The time the cascade may take before the remaining methods are skipped (each method polls the evaluator).
const BUDGET: Duration = Duration::from_secs(20);

/// The deepest nesting of [`solve`] (linearity, pieces, reflections).
const MAX_DEPTH: u32 = 10;

/// The state of one definite integral: its options, the conditions recorded, the deadline.
pub(crate) struct Job<'s, 'h> {
    pub(crate) stop: &'s Arc<AtomicBool>,
    pub(crate) opts: Opts,
    pub(crate) conds: Vec<T<'h>>,
    /// a value was computed with conditions on real parts given as real conditions (decision 14D.2)
    realified: bool,
    depth: u32,
    deadline: Instant,
}

impl<'h> Job<'_, 'h> {
    fn record(&mut self, c: T<'h>) {
        if !self.conds.contains(&c) {
            self.conds.push(c);
        }
    }
    /// Is the budget spent?
    pub(crate) fn late(&self) -> bool {
        Instant::now() > self.deadline
    }
}

/// The integral of `f` in `x` from `lo` to `hi` (either may be `inf` or `-inf`), else `None`: the cascade, the
/// numeric check, the conditions attached. A divergent integral gives `failure(divergent, integrate, f)`.
#[allow(clippy::too_many_arguments)]
pub fn integrate<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    stop: &Arc<AtomicBool>,
    f: T<'h>,
    x: T<'h>,
    lo: T<'h>,
    hi: T<'h>,
    opts: Opts,
) -> Res<Option<T<'h>>> {
    let mut job = Job { stop, opts, conds: vec![], realified: false, depth: 0, deadline: Instant::now() + BUDGET };
    let Some(v) = solve(&mut job, cx, b, f, x, lo, hi)? else { return Ok(None) };
    if is_failure(b, v) {
        return Ok(Some(v));
    }
    let v = cx.eval(v)?;
    if !is_free(b, v, x) || is_failure(b, v) {
        return Ok(None);
    }
    let mut v = tidy(cx, b, v)?;
    if !opts.inner {
        v = under_facts(cx, b, v)?;
    }
    if !opts.inner && check::verify(cx, b, f, x, lo, hi, v, &job.conds, job.realified)? == check::Verdict::Refuted {
        return Ok(None);
    }
    if job.conds.is_empty() || !opts.conditions {
        return Ok(Some(v));
    }
    let mut c = mc_decide::Conditions::new();
    for &k in &job.conds {
        c.record(k);
    }
    Ok(Some(c.finish(cx, stop, v)?))
}

/// The value simplified under the facts of the context (decision n14e.I2): a value with logarithms, roots, `abs`,
/// `sign` or inverse trigonometric functions of the parameters is refined and simplified once, and the smaller form
/// kept (`-(1/8) log(a/2) a^2 + (1/8) log(-a/2) a^2` is `(im/8) pi a^2` when `a > 0`).
fn under_facts<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>) -> Res<T<'h>> {
    if cx.assumptions().is_empty() || size(b, v) > 300 {
        return Ok(v);
    }
    let h = b.heap();
    let mut worth = false;
    let mut stack = vec![v];
    while let Some(u) = stack.pop() {
        if let Some((n, args)) = call_of(b, u) {
            worth |= match (n.as_str(), args) {
                ("log" | "abs" | "sign" | "arg" | "re" | "im_part" | "asin" | "acos" | "atan" | "conjugate", _) => true,
                ("pow", [_, e]) => !matches!(h.view(*e), View::Int(_)),
                _ => false,
            };
            stack.extend_from_slice(args);
        }
    }
    if !worth {
        return Ok(v);
    }
    let mut best = v;
    for f in ["refine", "simplify"] {
        let r = apply(cx, b, f, &[best])?;
        if !is_failure(b, r) && size(b, r) < size(b, best) {
            best = r;
        }
    }
    Ok(best)
}

/// The value tidied: expanded, then put over a common denominator, each when that is smaller (`1/3 + 3 (-1 +
/// e)` is `-8/3 + 3 e`); logarithms of rationals combined ([`tidy_logs`]).
fn tidy<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>) -> Res<T<'h>> {
    let mut v = v;
    if size(b, v) < 400 {
        for f in ["expand", "together"] {
            let e = apply(cx, b, f, &[v])?;
            if size(b, e) < size(b, v) {
                v = e;
            }
        }
    }
    tidy_logs(cx, b, v)
}

/// The number of nodes of a term (shared subterms counted each time), capped.
fn size<'h>(b: &Build<'h>, t: T<'h>) -> usize {
    let h = b.heap();
    let mut n = 0;
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

/// Logarithms of positive rationals with integer coefficients combined, in every sum of the value: two or more
/// become one logarithm of their product (`3 log(5) - 3 log(3) - log(2)` is `log(125/54)`), and inside a factor one
/// with a coefficient of at least 2 in size a logarithm of the power (`(-2 log(2) - γ) √π/4` is `(-log(4) - γ)
/// √π/4`, as tables write it; at the top level `-1 + 2 log(2)` is kept); other terms are kept.
fn tidy_logs<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>) -> Res<T<'h>> {
    let mut memo = FxHashMap::default();
    let r = combine_logs(cx, b, v, &mut memo, 0)?;
    if r == v {
        return Ok(v);
    }
    cx.eval(r)
}

fn combine_logs<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    t: T<'h>,
    memo: &mut FxHashMap<T<'h>, T<'h>>,
    depth: u32,
) -> Res<T<'h>> {
    let h = b.heap();
    if let Some(&r) = memo.get(&t) {
        return Ok(r);
    }
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    if depth > 60 {
        return Ok(t);
    }
    let mut out = Vec::with_capacity(args.len());
    for &a in args {
        out.push(combine_logs(cx, b, a, memo, depth + 1)?);
    }
    let mut r = if out.as_slice() == args { t } else { b.app(head, &out) };
    if b.sym_of(head) == Some(b.hd.add) {
        let (mut logs, mut rest, mut big) = (vec![], vec![], false);
        for &u in &out {
            let (k, l) = match args_of(b, u, b.hd.mul) {
                Some([k, l]) if matches!(h.view(*k), View::Int(_)) => (*k, *l),
                _ => (b.one(), u),
            };
            match call_of(b, l) {
                Some((n, [q])) if n == "log" && matches!(h.view(*q), View::Int(2..) | View::Rational { .. }) && sign(cx, b, *q)? == Some(1) => {
                    big |= !matches!(h.view(k), View::Int(-1..=1));
                    logs.push(b.pow(*q, k));
                }
                _ => rest.push(u),
            }
        }
        if logs.len() >= 2 || big && depth > 0 {
            let p = cx.eval(b.mul(&logs))?;
            // a power below 1 as minus the logarithm of its inverse
            let lp = match h.view(p) {
                View::Rational { num, .. } if num == b.one() => b.neg(b.call("log", &[cx.eval(b.powi(p, -1))?])),
                _ => b.call("log", &[p]),
            };
            rest.push(lp);
            r = b.add(&rest);
        }
    }
    memo.insert(t, r);
    Ok(r)
}

/// The cascade (module comment), conditions recorded in `job`.
pub(crate) fn solve<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: T<'h>,
    hi: T<'h>,
) -> Res<Option<T<'h>>> {
    if job.depth >= MAX_DEPTH {
        return Ok(None);
    }
    job.depth += 1;
    let r = solve_in(job, cx, b, f, x, lo, hi);
    job.depth -= 1;
    r
}

fn solve_in<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: T<'h>,
    hi: T<'h>,
) -> Res<Option<T<'h>>> {
    cx.tick(1)?;
    if f == b.zero() || lo == hi {
        return Ok(Some(b.zero()));
    }
    if !is_free(b, lo, x) || !is_free(b, hi, x) {
        return Ok(None);
    }
    if let Some(r) = delta::definite(cx, b, f, x, lo, hi)? {
        return Ok(Some(r));
    }
    // the k-th derivative of an undefined function `g`: `g^(k-1)(hi) - g^(k-1)(lo)` (the fundamental theorem of
    // calculus, with `g` taken as smooth, as the unknown function of an integrand is)
    if let View::App { head, args: &[a] } = b.heap().view(f) {
        if a == x {
            if let Some((orders, g)) = b.as_derivative(head) {
                if let ([k], View::Sym(_)) = (orders.as_slice(), b.heap().view(g)) {
                    if *k >= 1 && is_free(b, lo, x) && is_free(b, hi, x) {
                        let prev = if *k == 1 { g } else { b.derivative(&[*k - 1], g) };
                        return Ok(Some(b.sub(b.app(prev, &[hi]), b.app(prev, &[lo]))));
                    }
                }
            }
        }
    }
    // a function defined by cases: piece by piece (decision IOSYM.2), else as a whole; step-like functions
    // (`unit_box`, `heaviside_theta`, `boole`, ...) in the forms the splitting knows
    let f = match mc_limit::switches::as_steps(b, f, x) {
        Some(g) => cx.eval(g)?,
        None => f,
    };
    if let Some(r) = split::by_cases(job, cx, b, f, x, lo, hi)? {
        return Ok(Some(r));
    }
    match (point(cx, b, lo)?, point(cx, b, hi)?) {
        (Some(p), Some(q)) => {
            if let Some(r) = real_range(job, cx, b, f, x, p, q)? {
                return Ok(Some(r));
            }
            // the integrand simplified on the range (`sqrt(x^2)` is `x` on (0, 1)), and tried again
            if let Some(g) = on_range(cx, b, f, x, p, q)? {
                return solve(job, cx, b, g, x, lo, hi);
            }
            // square roots of a (1 -+ cos u) by the half angle, and tried again
            match half_angle(cx, b, f, x)? {
                Some(g) => solve(job, cx, b, g, x, lo, hi),
                None => Ok(None),
            }
        }
        (p, q) => {
            if let Some(r) = symbolic_range(cx, b, f, x, lo, hi, p, q)? {
                return Ok(Some(r));
            }
            if p.is_some_and(|p| p != Pt::Fin(lo)) || q.is_some_and(|q| q != Pt::Fin(hi)) {
                return Ok(None);
            }
            by_sign_of_bounds(job, cx, b, f, x, Pt::Fin(lo), Pt::Fin(hi))
        }
    }
}

/// `f` refined with `x` real and between the bounds, when that changes it and `f` holds a function the facts can
/// simplify (`abs`, `sign`, roots and other non-integer powers, `log`, `arg`, `re`, `im_part`); else `None`.
fn on_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, p: Pt<'h>, q: Pt<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut simplifiable = false;
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if is_free(b, u, x) {
            continue;
        }
        if let Some((n, args)) = call_of(b, u) {
            simplifiable |= match (n.as_str(), args) {
                ("abs" | "sign" | "log" | "arg" | "re" | "im_part" | "real_abs" | "conjugate", _) => true,
                ("pow", [_, e]) => !matches!(h.view(*e), View::Int(_)),
                _ => false,
            };
            stack.extend_from_slice(args);
        }
    }
    if !simplifiable {
        return Ok(None);
    }
    let (lo, hi) = match cmp(cx, b, p, q)? {
        Some(Ordering::Less) => (p, q),
        Some(Ordering::Greater) => (q, p),
        _ => return Ok(None),
    };
    let mut facts = vec![h.call(h.sym("element"), &[x, h.sym_term(h.sym("reals"))])];
    if let Pt::Fin(a) = lo {
        facts.push(h.call(h.sym("greater"), &[x, a]));
    }
    if let Pt::Fin(c) = hi {
        facts.push(h.call(h.sym("less"), &[x, c]));
    }
    let g = cx.eval_assuming(&facts, b.call("refine", &[f]))?;
    Ok((g != f && !is_failure(b, g)).then_some(g))
}

/// `f` with each square root `(a + c cos(u))^(1/2)`, `u` depending on `x` and the expanded radicand of that form with
/// numbers `a > 0` and `c = -a` or `c = a`, written by the half-angle formulas as `(2 a)^(1/2) abs(sin(u/2))` or
/// `(2 a)^(1/2) abs(cos(u/2))` (1 -+ cos u = 2 sin^2(u/2), 2 cos^2(u/2)), valid for real `u` (round 36: the
/// arc length of the astroid, the integral of sqrt(9/8 (1 - cos 4 t)) over [0, 2 pi], stayed unevaluated); `None`
/// when there is no such root.
fn half_angle<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let half = b.rat(1, 2);
    let mut rules = vec![];
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if is_free(b, u, x) {
            continue;
        }
        let Some((n, args)) = call_of(b, u) else { continue };
        stack.extend_from_slice(args);
        let ("pow", &[base, e]) = (n.as_str(), args) else { continue };
        if e != half {
            continue;
        }
        let ex = cx.eval(b.call("expand", &[base]))?;
        let Some(("add", &[t1, t2])) = call_of(b, ex).as_ref().map(|(n, a)| (n.as_str(), *a)) else { continue };
        let (a, rest) = if is_free(b, t1, x) { (t1, t2) } else { (t2, t1) };
        if !is_free(b, a, x) || is_free(b, rest, x) {
            continue;
        }
        let (c, cosine) = match call_of(b, rest) {
            Some((m, &[k, g])) if m == "mul" && is_free(b, k, x) => (k, g),
            _ => (b.one(), rest),
        };
        let Some(("cos", &[arg])) = call_of(b, cosine).as_ref().map(|(n, a)| (n.as_str(), *a)) else { continue };
        let positive = cx.eval(b.call("greater", &[a, b.zero()]))?;
        if positive != h.sym_term(h.sym(names::TRUE)) {
            continue;
        }
        let trig = if cx.eval(b.add(&[a, c]))? == b.zero() {
            "sin"
        } else if cx.eval(b.sub(a, c))? == b.zero() {
            "cos"
        } else {
            continue;
        };
        let factor = b.pow(b.mul(&[b.int(2), a]), half);
        let new = b.mul(&[factor, b.call("abs", &[b.call(trig, &[b.mul(&[half, arg])])])]);
        rules.push(b.call("rule", &[u, new]));
    }
    if rules.is_empty() {
        return Ok(None);
    }
    let g = cx.eval(b.call("rewrite", &[f, b.call("list", &rules)]))?;
    Ok((g != f).then_some(g))
}

/// The integral over real bounds: order, convergence, linearity, methods (module comment).
pub(crate) fn real_range<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    p: Pt<'h>,
    q: Pt<'h>,
) -> Res<Option<T<'h>>> {
    if job.depth >= MAX_DEPTH {
        return Ok(None);
    }
    job.depth += 1;
    let r = real_range_in(job, cx, b, f, x, p, q);
    job.depth -= 1;
    r
}

fn real_range_in<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    p: Pt<'h>,
    q: Pt<'h>,
) -> Res<Option<T<'h>>> {
    cx.tick(1)?;
    let (lo, hi) = match cmp(cx, b, p, q)? {
        None => {
            if let Some(r) = unordered_bounds(cx, b, f, x, p, q)? {
                return Ok(Some(r));
            }
            return by_sign_of_bounds(job, cx, b, f, x, p, q);
        }
        Some(Ordering::Equal) => return Ok(Some(b.zero())),
        Some(Ordering::Less) => (p, q),
        Some(Ordering::Greater) => {
            return Ok(real_range(job, cx, b, f, x, q, p)?.map(|v| if is_failure(b, v) { v } else { b.neg(v) }))
        }
    };
    // a constant integrand
    if is_free(b, f, x) {
        return Ok(match (lo, hi) {
            (Pt::Fin(a), Pt::Fin(c)) => Some(b.mul(&[f, b.sub(c, a)])),
            _ if sign(cx, b, f)?.is_some_and(|s| s != 0) => Some(divergent(b, f)),
            _ => None,
        });
    }
    // a rational f with a real pole in [lo, hi] diverges, whatever the degree of the pole's factor and whether an
    // antiderivative is known (decision 2C4.4); a principal value may still exist
    if !job.opts.pv && points::rational_pole(cx, b, f, x, lo, hi)? == Some(true) {
        return Ok(Some(divergent(b, f)));
    }
    // convergence first (decision 14D.2)
    let inner = singular_points(cx, b, f, x, lo, hi)?;
    let mut verdict = conv::at(cx, b, job.stop, f, x, lo, 1)?;
    verdict = conv::join(verdict, conv::at(cx, b, job.stop, f, x, hi, -1)?);
    if !job.opts.pv {
        for &c in &inner {
            verdict = conv::join(verdict, conv::at(cx, b, job.stop, f, x, c, -1)?);
            verdict = conv::join(verdict, conv::at(cx, b, job.stop, f, x, c, 1)?);
        }
    }
    // the closed forms and the symmetry rule hold only for integrals known to converge; the residue methods check
    // their own conditions (Jordan's lemma, degrees), Newton-Leibniz its limits
    let known = !matches!(verdict, Conv::Unknown);
    match verdict {
        Conv::Diverges => return Ok(Some(divergent(b, f))),
        // conditions implied by the facts are not recorded, but those on real parts are still given to the inner
        // call as real conditions (decision 14D.2)
        Conv::Needs(cs, held) if !job.opts.inner && (!cs.is_empty() || realify_conditions(cx, b, &held)?.1) => {
            let mut all = cs.clone();
            all.extend(held.iter().copied());
            if let Some(v) = under_conditions(job, cx, b, f, x, lo, hi, all)? {
                // recorded also without generate_conditions: the numeric check samples only where they hold
                // (decision n14e.I3); they are attached only when asked for
                for c in cs {
                    job.record(c);
                }
                return Ok(Some(v));
            }
            if !cs.is_empty() {
                return Ok(None);
            }
        }
        _ => {}
    }
    // linearity
    if let Some(r) = linear(job, cx, b, f, x, lo, hi)? {
        return Ok(Some(r));
    }
    if job.late() {
        return Ok(None);
    }
    if known {
        if let Some(r) = forms::closed(job, cx, b, f, x, lo, hi)? {
            return Ok(Some(r));
        }
    } else if lo == Pt::NegInf && hi == Pt::PosInf {
        if let Some(r) = forms::dominated_gaussian(job, cx, b, f, x)? {
            return Ok(Some(r));
        }
    } else if lo == Pt::Fin(b.zero()) && hi == Pt::PosInf {
        // conditionally convergent: the formula carries its own conditions
        if let Some(r) = forms::bessel_j_integral(cx, b, f, x)? {
            return Ok(Some(r));
        }
    }
    if let Some(r) = forms::self_checked(cx, b, f, x, lo, hi)? {
        return Ok(Some(r));
    }
    if let Some(r) = residue::by_residues(job, cx, b, f, x, lo, hi)? {
        return Ok(Some(r));
    }
    if let Some(r) = subst::exp_rational(job, cx, b, f, x, lo, hi)? {
        return Ok(Some(r));
    }
    if known {
        if let Some(r) = subst::log_to_exp(job, cx, b, f, x, lo, hi)? {
            return Ok(Some(r));
        }
        if let Some(r) = subst::inversion(cx, b, f, x, lo, hi)? {
            return Ok(Some(r));
        }
    }
    if known {
        if let Some(r) = odd(cx, b, f, x, lo, hi)? {
            return Ok(Some(r));
        }
    }
    if let Some(r) = newton_leibniz(job, cx, b, f, x, lo, hi)? {
        return Ok(Some(r));
    }
    meijer_g(job, cx, b, f, x, lo, hi)
}

/// The hook of the Meijer G methods (roadmap 14D, added later): products of two G functions over `(0, inf)`,
/// finite ranges through a step function written as a G function, with each formula's validity conditions recorded
/// in `job`. Not implemented in this crate: `None`.
fn meijer_g<'h>(
    _job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<T<'h>>> {
    meijer::integrate(cx, b, f, x, lo, hi)
}

/// The interior points of `(lo, hi)` where `f` may be singular (the critical-point search on `f` alone); empty
/// when they cannot be located (convergence is then decided at the ends only, and Newton-Leibniz splits again).
fn singular_points<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Vec<Pt<'h>>> {
    let mut zs = Vec::new();
    if !points::collect_with(b, f, x, &mut zs, true) {
        return Ok(vec![]);
    }
    let Some(mut cs) = points::critical(cx, b, x, &zs, lo, hi)? else { return Ok(vec![]) };
    let Some(trig) = points::trig_poles(cx, b, f, x, lo, hi)? else { return Ok(vec![]) };
    for c in trig {
        if !points::insert_point(cx, b, &mut cs, c)? {
            return Ok(vec![]);
        }
    }
    let mut out = vec![];
    for c in cs {
        if cmp(cx, b, c, lo)? == Some(Ordering::Greater) && cmp(cx, b, c, hi)? == Some(Ordering::Less) {
            out.push(c);
        }
    }
    Ok(out)
}

/// The integral computed with the convergence conditions `cs` as facts (decision 14D.2): an inner call of
/// `integrate` under `assuming`. Conditions on real parts (`re(n) > -1`) are given to the inner call as the
/// conditions on the parameters themselves (`n > -1`); the value then holds for complex parameters by analytic
/// continuation when it is an analytic expression in them (no `abs`, `re`, `sign`, ... of a parameter), which is
/// checked, and the numeric check also samples a complex parameter value.
#[allow(clippy::too_many_arguments)]
fn under_conditions<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
    cs: Vec<T<'h>>,
) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let (facts, realified) = realify_conditions(cx, b, &cs)?;
    let rule = |k: &str, v: T<'h>| h.call(h.sym(names::RULE), &[h.sym_term(h.sym(k)), v]);
    let opts = h.call(
        h.sym("options"),
        &[
            rule("generate_conditions", cx.bool(false)),
            rule("principal_value", cx.bool(job.opts.pv)),
            rule(INNER, cx.bool(true)),
        ],
    );
    let spec = h.call(b.hd.list, &[x, term(b, lo), term(b, hi)]);
    let call = h.call(h.sym("integrate"), &[f, spec, opts]);
    let r = cx.eval_assuming(&facts, call)?;
    if is_failure(b, r) {
        return Ok(None);
    }
    if !is_free(b, r, h.sym_term(h.sym("integrate"))) || !is_free(b, r, x) {
        return Ok(None);
    }
    if realified && !analytic(b, r) {
        return Ok(None);
    }
    job.realified |= realified;
    Ok(Some(r))
}

/// The internal option of an inner call (decision 14D.2).
pub const INNER: &str = "definite$inner";

/// The conditions with `re(u) > c` (and `<`, `>=`, `<=`) given as `u > c`; whether one was changed.
fn realify_conditions<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, cs: &[T<'h>]) -> Res<(Vec<T<'h>>, bool)> {
    let h = b.heap();
    let mut out = vec![];
    let mut changed = false;
    for &c in cs {
        let mut d = c;
        if let Some((name, [l, r])) = call_of(b, c) {
            if matches!(name.as_str(), "greater" | "less" | "greater_equal" | "less_equal") {
                if let Some((inner, [u])) = call_of(b, *l) {
                    if inner == "re" && is_free(b, *u, h.sym_term(h.sym("im"))) {
                        d = cx.eval(h.call(h.sym(&name), &[*u, *r]))?;
                        changed = true;
                    }
                }
            }
        }
        out.push(d);
    }
    Ok((out, changed))
}

/// Is `t` built without the non-analytic functions (`abs`, `re`, `im`, `sign`, `conjugate`, `arg`, steps, cases)?
fn analytic<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if let Some(s) = b.sym_of(head) {
                if matches!(
                    h.sym_name(s).as_str(),
                    "abs"
                        | "re"
                        | "im_part"
                        | "sign"
                        | "conjugate"
                        | "arg"
                        | "unit_step"
                        | "heaviside_theta"
                        | "floor"
                        | "ceiling"
                        | "max"
                        | "min"
                        | "piecewise"
                        | "if"
                        | "real_abs"
                        | "real_sign"
                        | "boole"
                ) {
                    return false;
                }
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    true
}

/// Linearity: constant factors out; a sum term by term when every term has a finite value (a divergent term sends
/// the sum back to the methods as a whole, and the conditions its terms recorded are dropped).
#[allow(clippy::too_many_arguments)]
fn linear<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<T<'h>>> {
    if let Some(fs) = args_of(b, f, b.hd.mul) {
        let (free, dep): (Vec<T<'h>>, Vec<T<'h>>) = fs.iter().partition(|&&g| is_free(b, g, x));
        if !free.is_empty() && !dep.is_empty() {
            let g = b.mul(&dep);
            return Ok(real_range(job, cx, b, g, x, lo, hi)?
                .map(|v| if is_failure(b, v) { divergent(b, f) } else { b.mul(&[b.mul(&free), v]) }));
        }
        return Ok(None);
    }
    let Some(terms) = args_of(b, f, b.hd.add) else { return Ok(None) };
    let mark = job.conds.len();
    let mut out = Vec::with_capacity(terms.len());
    for &t in terms {
        match real_range(job, cx, b, t, x, lo, hi)? {
            Some(v) if !is_failure(b, v) => out.push(v),
            _ => {
                job.conds.truncate(mark);
                return Ok(None);
            }
        }
    }
    Ok(Some(b.add(&out)))
}

/// Symmetry: over `(-c, c)` an odd integrand gives 0 (convergence was established before).
fn odd<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, lo: Pt<'h>, hi: Pt<'h>) -> Res<Option<T<'h>>> {
    let symmetric = match (lo, hi) {
        (Pt::NegInf, Pt::PosInf) => true,
        (Pt::Fin(a), Pt::Fin(c)) => cx.eval(b.add(&[a, c]))? == b.zero(),
        _ => false,
    };
    if !symmetric {
        return Ok(None);
    }
    let g = subst(cx, b, f, x, b.neg(x))?;
    let s = apply(cx, b, "together", &[b.add(&[f, g])])?;
    if s == b.zero() || apply(cx, b, "simplify", &[s])? == b.zero() {
        return Ok(Some(b.zero()));
    }
    Ok(None)
}

/// Symbolic (or complex) bounds, or real ones whose kind is not decided (decision 2C4.3).
#[allow(clippy::too_many_arguments)]
fn symbolic_range<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: T<'h>,
    hi: T<'h>,
    p: Option<Pt<'h>>,
    q: Option<Pt<'h>>,
) -> Res<Option<T<'h>>> {
    // decision 2C4.3: symbolic (or complex) bounds only for antiderivatives continuous on the whole line
    let infinite = |p: Option<Pt<'h>>| matches!(p, Some(Pt::NegInf | Pt::PosInf));
    let Some(big) = antiderivative(cx, b, f, x, 0)? else { return Ok(None) };
    let big = cx.eval(big)?;
    if is_failure(b, big) {
        return Ok(None);
    }
    if infinite(p) || infinite(q) {
        // one infinite bound: F continuous on the whole line, with a finite limit there (decision 14D.6)
        if infinite(p) && infinite(q) || !continuous(b, f, x) || !continuous(b, big, x) {
            return Ok(None);
        }
        let at = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, pt: Option<Pt<'h>>| -> Res<Option<T<'h>>> {
            match pt {
                Some(pt @ (Pt::NegInf | Pt::PosInf)) => Ok(match value(cx, b, big, x, pt, if pt == Pt::PosInf { -1 } else { 1 }, true)? {
                    Some(Lim::Fin(v, _)) => Some(v),
                    _ => None,
                }),
                _ => {
                    let v = subst(cx, b, big, x, t)?;
                    Ok((!is_failure(b, v)).then_some(v))
                }
            }
        };
        let (Some(a), Some(z)) = (at(cx, lo, p)?, at(cx, hi, q)?) else { return Ok(None) };
        return Ok(Some(b.sub(z, a)));
    }
    if !continuous(b, f, x) || !continuous(b, big, x) {
        // no critical point on the whole real line will do as well (`1/(1 + x^2)`)
        let mut zs = Vec::new();
        if !points::collect(b, f, x, &mut zs) || !points::collect(b, big, x, &mut zs) {
            return Ok(None);
        }
        if !points::critical(cx, b, x, &zs, Pt::NegInf, Pt::PosInf)?.is_some_and(|c| c.is_empty()) {
            return Ok(None);
        }
    }
    let a = subst(cx, b, big, x, lo)?;
    let z = subst(cx, b, big, x, hi)?;
    if is_failure(b, a) || is_failure(b, z) {
        return Ok(None);
    }
    Ok(Some(b.sub(z, a)))
}

/// Two finite real bounds whose order is not decided (symbols under assumptions): `F(q) - F(p)` when every
/// critical point of `f` and `F` on the real line is decided to lie on the same side of both bounds (decision 2C4.5).
fn unordered_bounds<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    p: Pt<'h>,
    q: Pt<'h>,
) -> Res<Option<T<'h>>> {
    let (Pt::Fin(lo), Pt::Fin(hi)) = (p, q) else { return Ok(None) };
    let Some(big) = antiderivative(cx, b, f, x, 0)? else { return Ok(None) };
    let big = cx.eval(big)?;
    if is_failure(b, big) {
        return Ok(None);
    }
    let mut zs = Vec::new();
    if !points::collect(b, f, x, &mut zs) || !points::collect(b, big, x, &mut zs) {
        return Ok(None);
    }
    let Some(crit) = points::critical(cx, b, x, &zs, Pt::NegInf, Pt::PosInf)? else { return Ok(None) };
    for c in crit {
        let Pt::Fin(c) = c else { continue };
        let (s1, s2) = (sign(cx, b, b.sub(c, lo))?, sign(cx, b, b.sub(c, hi))?);
        if !matches!((s1, s2), (Some(1), Some(1)) | (Some(-1), Some(-1))) {
            return Ok(None);
        }
    }
    let a = subst(cx, b, big, x, lo)?;
    let z = subst(cx, b, big, x, hi)?;
    if is_failure(b, a) || is_failure(b, z) {
        return Ok(None);
    }
    Ok(Some(b.sub(z, a)))
}

/// Bounds whose order depends on the sign of one parameter (`integrate(sqrt(x (x - a)), [x, 0, a])`): the integral
/// under `a > 0` and under `a < 0`, joined (decision n14e.I1): one value when they agree, `sign(a)` times the first
/// when they are opposite (`a abs(a)` for `a^2`), else a `piecewise` of the two; the result holds for real `a`,
/// recorded as the condition `element(a, reals)`.
fn by_sign_of_bounds<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    p: Pt<'h>,
    q: Pt<'h>,
) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let (Pt::Fin(lo), Pt::Fin(hi)) = (p, q) else { return Ok(None) };
    if job.opts.inner || job.late() {
        return Ok(None);
    }
    let d = apply(cx, b, "expand", &[b.sub(hi, lo)])?;
    let mut syms = vec![];
    check::symbols(b, d, x, &mut syms);
    let [a] = syms.as_slice() else { return Ok(None) };
    let a = *a;
    // the sign of d must follow the sign of a: d = c a with c a nonzero real constant
    let c = apply(cx, b, "together", &[b.div(d, a)])?;
    if !is_free(b, c, a) || sign(cx, b, c)?.is_none_or(|s| s == 0) {
        return Ok(None);
    }
    let range = h.call(b.hd.list, &[x, lo, hi]);
    let opts = job.opts;
    let mut vals = vec![];
    let mut conds = vec![];
    for rel in ["greater", "less"] {
        let fact = h.call(h.sym(rel), &[a, b.zero()]);
        let mut args = vec![f, range];
        if let Some(o) = options_term(h, &opts) {
            args.push(o);
        }
        let v = cx.eval_assuming(&[fact], b.call("integrate", &args))?;
        if is_failure(b, v) || !is_free(b, v, h.sym_term(h.sym("integrate"))) {
            return Ok(None);
        }
        let v = match call_of(b, v) {
            Some((n, [v, c])) if n == "conditional_expression" => {
                conds.push(*c);
                *v
            }
            _ => v,
        };
        vals.push(v);
    }
    let (vp, vm) = (vals[0], vals[1]);
    let zero = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<bool> {
        let s = apply(cx, b, "simplify", &[t])?;
        Ok(b.is_zero(s))
    };
    let v = if zero(cx, b.sub(vp, vm))? {
        vp
    } else if zero(cx, b.add(&[vp, vm]))? {
        odd_extension(cx, b, vp, a)?
    } else {
        let gt = h.call(h.sym("greater"), &[a, b.zero()]);
        let lt = h.call(h.sym("less"), &[a, b.zero()]);
        cx.eval(b.call("piecewise", &[h.call(b.hd.list, &[h.call(b.hd.list, &[vp, gt]), h.call(b.hd.list, &[vm, lt])]), b.zero()]))?
    };
    if job.opts.conditions {
        job.record(h.call(h.sym("element"), &[a, h.sym_term(h.sym("reals"))]));
        for c in conds {
            job.record(c);
        }
    }
    Ok(Some(v))
}

/// `sign(a) v` for a value `v` of the case `a > 0`, with an even power `a^(2k)` of it written `a^(2k-1) abs(a)`.
fn odd_extension<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>, a: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let fs = match args_of(b, v, b.hd.mul) {
        Some(fs) => fs.to_vec(),
        None => vec![v],
    };
    let mut out = vec![];
    let mut done = false;
    for g in fs {
        if !done {
            let k = if g == a {
                None
            } else {
                match args_of(b, g, b.hd.pow) {
                    Some(&[base, e]) if base == a => match h.view(e) {
                        View::Int(k) if k > 0 && k % 2 == 0 => Some(k),
                        _ => None,
                    },
                    _ => None,
                }
            };
            if let Some(k) = k {
                out.push(b.powi(a, k - 1));
                out.push(b.call("abs", &[a]));
                done = true;
                continue;
            }
        }
        out.push(g);
    }
    if !done {
        out.push(b.call("sign", &[a]));
    }
    cx.eval(b.mul(&out))
}

/// Newton-Leibniz over ordered real bounds `lo < hi` (module comment, step 7).
#[allow(clippy::too_many_arguments)]
fn newton_leibniz<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<T<'h>>> {
    let Some(big) = antiderivative(cx, b, f, x, 0)? else { return Ok(None) };
    let big = cx.eval(big)?;
    if is_failure(b, big) {
        return Ok(None);
    }
    // the critical points of f and F
    let mut zs = Vec::new();
    if !points::collect_with(b, f, x, &mut zs, true) || !points::collect_with(b, big, x, &mut zs, true) {
        return Ok(None);
    }
    let Some(mut crit) = points::critical(cx, b, x, &zs, lo, hi)? else { return Ok(None) };
    // the poles of tan, sec, cot, csc of linear arguments in f and F (the jumps of `atan(tan(x/2)/c)`-type
    // antiderivatives)
    for g in [f, big] {
        let Some(trig) = points::trig_poles(cx, b, g, x, lo, hi)? else { return Ok(None) };
        for c in trig {
            if !points::insert_point(cx, b, &mut crit, c)? {
                return Ok(None);
            }
        }
    }
    // the cut points with whether each is critical; a critical point equal in value to a bound marks the bound
    let mut cuts = vec![(lo, false)];
    let mut hi_crit = false;
    for &c in &crit {
        match (cmp(cx, b, c, lo)?, cmp(cx, b, c, hi)?) {
            (Some(Ordering::Equal), _) => cuts[0].1 = true,
            (_, Some(Ordering::Equal)) => hi_crit = true,
            (Some(Ordering::Greater), Some(Ordering::Less)) => cuts.push((c, true)),
            _ => return Ok(None),
        }
    }
    cuts.push((hi, hi_crit));
    let logs = points::log_args(b, big, x);
    let mut pieces = Vec::with_capacity(cuts.len());
    for w in cuts.windows(2) {
        cx.tick(1)?;
        // on a piece, an additive log(u) with u < 0 becomes log(-u): F changes by a constant, and the value is real
        // (decision 2C4.5)
        pieces.push(realify(cx, b, big, x, &logs, w[0].0, w[1].0)?);
    }
    let mut parts = Vec::with_capacity(2 * cuts.len());
    let mut unknown = false;
    let n = pieces.len();
    // the left end, the interior cut points (the two pieces meeting there), the right end
    match value(cx, b, pieces[0], x, cuts[0].0, 1, cuts[0].1)? {
        Some(Lim::Inf(_)) => return Ok(Some(divergent(b, f))),
        Some(Lim::Fin(v, _)) => parts.push(b.neg(v)),
        None => unknown = true,
    }
    for k in 1..n {
        if job.late() {
            return Ok(None);
        }
        let (c, critical) = cuts[k];
        let below = value(cx, b, pieces[k - 1], x, c, -1, critical)?;
        let above = value(cx, b, pieces[k], x, c, 1, critical)?;
        match (below, above) {
            (Some(Lim::Fin(u, _)), Some(Lim::Fin(v, _))) => {
                parts.push(u);
                parts.push(b.neg(v));
            }
            (Some(Lim::Inf(s)), Some(Lim::Inf(t))) if job.opts.pv && s == t => {
                // a principal value: lim F(c - ε) - F(c + ε)
                match principal(cx, b, pieces[k - 1], pieces[k], x, c)? {
                    Some(v) => parts.push(v),
                    None => unknown = true,
                }
            }
            (Some(Lim::Inf(_)), _) | (_, Some(Lim::Inf(_))) => return Ok(Some(divergent(b, f))),
            _ => unknown = true,
        }
    }
    match value(cx, b, pieces[n - 1], x, cuts[n].0, -1, cuts[n].1)? {
        Some(Lim::Inf(_)) => return Ok(Some(divergent(b, f))),
        Some(Lim::Fin(v, _)) => parts.push(v),
        None => unknown = true,
    }
    if unknown {
        return Ok(None);
    }
    Ok(Some(b.add(&parts)))
}

/// `lim_{ε → 0⁺} below(c - ε) - above(c + ε)`: the principal value's contribution at an interior point.
fn principal<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    below: T<'h>,
    above: T<'h>,
    x: T<'h>,
    c: Pt<'h>,
) -> Res<Option<T<'h>>> {
    let Pt::Fin(c) = c else { return Ok(None) };
    let h = b.heap();
    let eps = h.sym_term(h.sym("eps$pv"));
    let l = subst(cx, b, below, x, b.sub(c, eps))?;
    let r = subst(cx, b, above, x, b.add(&[c, eps]))?;
    let d = cx.eval(b.sub(l, r))?;
    match limit_with(cx, b, d, eps, b.zero(), Direction::FromAbove, &limit_options())? {
        Outcome::Value(v) if is_free(b, v, inf(b)) => Ok(Some(v)),
        _ => Ok(None),
    }
}

/// The limits of Newton-Leibniz: a budget and the numeric check.
fn limit_options() -> Options {
    Options { budget: Duration::from_secs(3), check: true }
}

/// The value of `g` at `pt` approached from the side `side` (1: from above, -1: from below): substitution at a
/// finite point that is not critical, else the limit (the walk, then Gruntz's algorithm).
fn value<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    x: T<'h>,
    pt: Pt<'h>,
    side: i8,
    critical: bool,
) -> Res<Option<Lim<'h>>> {
    let h = b.heap();
    if let (Pt::Fin(c), false) = (pt, critical) {
        let v = subst(cx, b, g, x, c)?;
        if !is_failure(b, v) && is_free(b, v, inf(b)) && is_free(b, v, h.sym_term(h.sym("undefined"))) {
            return Ok(Some(Lim::Fin(v, 0)));
        }
    }
    if let Some(v) = mc_limit::walk::limit(cx, b, g, x, pt, side)? {
        return Ok(Some(v));
    }
    let dir = match (pt, side) {
        (Pt::Fin(_), 1) => Direction::FromAbove,
        (Pt::Fin(_), _) => Direction::FromBelow,
        _ => Direction::TwoSided,
    };
    let i = inf(b);
    Ok(match limit_with(cx, b, g, x, term(b, pt), dir, &limit_options())? {
        Outcome::Value(v) if v == i => Some(Lim::Inf(1)),
        Outcome::Value(v) if v == b.neg(i) => Some(Lim::Inf(-1)),
        Outcome::Value(v) if is_free(b, v, i) && !is_failure(b, v) && is_free(b, v, h.sym_term(h.sym("limit"))) => {
            Some(Lim::Fin(v, 0))
        }
        _ => None,
    })
}

/// A point as a term.
pub(crate) fn term<'h>(b: &Build<'h>, p: Pt<'h>) -> T<'h> {
    match p {
        Pt::NegInf => b.neg(inf(b)),
        Pt::Fin(t) => t,
        Pt::PosInf => inf(b),
    }
}

/// `g` with `log(u)` replaced by `log(-u)` for the additive logarithms ([`points::log_args`]) whose arguments `u`
/// are negative on the piece `(l, r)`, which holds no zero of `u` (they are critical points). The sign is taken at
/// one inner point.
fn realify<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    x: T<'h>,
    logs: &[(T<'h>, T<'h>)],
    l: Pt<'h>,
    r: Pt<'h>,
) -> Res<T<'h>> {
    if logs.is_empty() {
        return Ok(g);
    }
    let m = match (l, r) {
        (Pt::Fin(a), Pt::Fin(c)) => b.mul(&[b.rat(1, 2), b.add(&[a, c])]),
        (Pt::Fin(a), _) => b.add(&[a, b.one()]),
        (_, Pt::Fin(c)) => b.sub(c, b.one()),
        _ => b.zero(),
    };
    let mut map = FxHashMap::default();
    for &(call, u) in logs {
        let v = subst(cx, b, u, x, m)?;
        if sign(cx, b, v)? == Some(-1) {
            map.insert(call, b.call("log", &[b.neg(u)]));
        }
    }
    if map.is_empty() {
        return Ok(g);
    }
    let mut memo = FxHashMap::default();
    match replace(b, g, &map, &mut memo, 0) {
        Some(t) => cx.eval(t),
        None => Ok(g),
    }
}

/// `t` with the subterms that are keys of `map` replaced (sums, products and powers rebuilt canonically); `None`
/// beyond a nesting depth.
fn replace<'h>(
    b: &Build<'h>,
    t: T<'h>,
    map: &FxHashMap<T<'h>, T<'h>>,
    memo: &mut FxHashMap<T<'h>, T<'h>>,
    depth: u32,
) -> Option<T<'h>> {
    if let Some(&r) = map.get(&t).or_else(|| memo.get(&t)) {
        return Some(r);
    }
    if depth > 200 {
        return None;
    }
    let r = match b.heap().view(t) {
        View::App { head, args } => {
            let mut out = Vec::with_capacity(args.len());
            for &a in args {
                out.push(replace(b, a, map, memo, depth + 1)?);
            }
            if out.as_slice() == args {
                t
            } else {
                b.app(head, &out)
            }
        }
        _ => t,
    };
    memo.insert(t, r);
    Some(r)
}

/// `failure(divergent, integrate, f)` (decision 2C4.2).
fn divergent<'h>(b: &Build<'h>, f: T<'h>) -> T<'h> {
    let h = b.heap();
    h.call(h.sym(names::FAILURE), &[h.sym_term(h.sym("divergent")), h.sym_term(h.sym("integrate")), f])
}

/// Is `t` continuous for every real `x` (and every real value of the other symbols)? Polynomials, and `exp`,
/// `sin`, `cos`, `sinh`, `cosh`, `tanh`, `atan` of such, sums, products and non-negative integer powers.
fn continuous<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if is_free(b, u, x) || u == x {
            continue;
        }
        if let Some(args) = args_of(b, u, b.hd.add).or_else(|| args_of(b, u, b.hd.mul)) {
            stack.extend_from_slice(args);
            continue;
        }
        let Some((name, args)) = call_of(b, u) else { return false };
        match (name.as_str(), args) {
            ("pow", [base, e]) if b.sym_of(*base) == Some(b.hd.e) => stack.push(*e),
            ("pow", [base, n]) if matches!(b.heap().view(*n), View::Int(0..)) => stack.push(*base),
            ("exp" | "sin" | "cos" | "sinh" | "cosh" | "tanh" | "atan", [a]) => stack.push(*a),
            _ => return false,
        }
    }
    true
}
