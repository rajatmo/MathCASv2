//! Convergence of a definite integral at its endpoints and at interior singular points (item 14D, step 2 of the
//! driver), decided before any method runs.
//!
//! The point is moved to `t → 0⁺` (`x = c ± t`, `x = ±1/t` with the Jacobian `t^-2`), and the integrand's behaviour
//! there is read factor by factor as `C t^P (log t)^L` times an exponential `exp(v)` and a bounded oscillating
//! factor (`sin`, `cos` of a real argument tending to infinity). The exponent `P` may hold parameters (`x^n`,
//! `x^(a - 1)`), the exponential's leading coefficient too (`exp(-c x^2)`). The integral of `t^P (log t)^L` near 0
//! converges iff `P > -1`, or `P = -1` and `L < -1` (Hardy's comparison scale, e.g. Hardy, *Orders of Infinity*,
//! and any text on improper integrals). An exponential that decays wins over every power, one that grows loses.
//!
//! The answer is [`Conv::Converges`], [`Conv::Diverges`], [`Conv::Needs`] (sufficient conditions on parameters,
//! such as `re(n) > -1` or `re(c) > 0`, each decided first by the decision service under the call's facts) or
//! [`Conv::Unknown`] (an oscillation that does not decay fast enough to be absolutely integrable, a function the
//! series kernels do not expand, a sum whose terms may cancel). Unknown never blocks: the methods and the limits
//! of Newton-Leibniz decide then.
use super::{sign, subst, Pt};
use crate::{apply, args_of, is_free};
use mc_limit::{limit_with, Direction, Options, Outcome};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

/// The verdict at one side of one point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Conv<'h> {
    /// converges, whatever the parameters
    Converges,
    /// converges when the first conditions hold (not decided under the facts of the call); the second ones were
    /// needed too and are implied by the facts
    Needs(Vec<T<'h>>, Vec<T<'h>>),
    /// diverges
    Diverges,
    /// not decided here
    Unknown,
}

/// The behaviour of a factor at `t → 0⁺`: `t^p (log t)^l`, times exponentials whose leading coefficients must have
/// negative real parts (`decay`), bounded oscillation (`bounded`), or exponential decay/growth decided.
#[derive(Clone, Debug)]
struct Beh<'h> {
    p: Vec<T<'h>>,
    l: i64,
    /// conditions for the exponentials to decay
    decay: Vec<T<'h>>,
    /// an exponential that decays for certain (it dominates every power)
    decays: bool,
    /// an exponential that grows for certain
    grows: bool,
    /// a bounded factor that may oscillate (only absolute convergence is concluded)
    bounded: bool,
    /// sufficient conditions for factors to have finite nonzero limits (`re(a) > 0` for `exp(x^a)` at 0)
    conds: Vec<T<'h>>,
    /// the orders `k` of the oscillating factors (`sin(c t^-k)`, `exp(i c t^-k)`)
    osc: Vec<(i64, i64)>,
    /// a power of `log t` that is not an integer (it matters only when `P + 1 = 0`)
    lfree: bool,
}

impl<'h> Beh<'h> {
    fn unit() -> Beh<'h> {
        Beh { p: vec![], l: 0, decay: vec![], decays: false, grows: false, bounded: false, conds: vec![], osc: vec![], lfree: false }
    }
    fn times(mut self, o: Beh<'h>) -> Beh<'h> {
        self.p.extend(o.p);
        self.l += o.l;
        self.decay.extend(o.decay);
        self.decays |= o.decays;
        self.grows |= o.grows;
        self.bounded |= o.bounded;
        self.conds.extend(o.conds);
        self.osc.extend(o.osc);
        self.lfree |= o.lfree;
        self
    }
}

/// The private variable of the moved point.
fn tvar<'h>(b: &Build<'h>) -> T<'h> {
    b.heap().sym_term(b.heap().sym("t$conv"))
}

/// The convergence of `∫ f dx` near `pt`, approached from `side` (1 from above, -1 from below; ignored at an
/// infinity).
pub(crate) fn at<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    stop: &Arc<AtomicBool>,
    f: T<'h>,
    x: T<'h>,
    pt: Pt<'h>,
    side: i8,
) -> Res<Conv<'h>> {
    let t = tvar(b);
    if !is_free(b, f, t) {
        return Ok(Conv::Unknown);
    }
    let (sub, jac) = match pt {
        Pt::Fin(c) => (if side >= 0 { b.add(&[c, t]) } else { b.sub(c, t) }, None),
        Pt::PosInf => (b.powi(t, -1), Some(b.powi(t, -2))),
        Pt::NegInf => (b.neg(b.powi(t, -1)), Some(b.powi(t, -2))),
    };
    let g = subst(cx, b, f, x, sub)?;
    if super::is_failure(b, g) {
        return Ok(Conv::Unknown);
    }
    let Some(mut beh) = behaviour(cx, b, stop, g, t, 0)? else { return Ok(Conv::Unknown) };
    if jac.is_some() {
        beh.p.push(b.int(-2));
    }
    verdict(cx, b, beh)
}

/// The verdict from a behaviour (module comment).
fn verdict<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, mut beh: Beh<'h>) -> Res<Conv<'h>> {
    let conds = std::mem::take(&mut beh.conds);
    let v = verdict_powers(cx, b, beh)?;
    if conds.is_empty() || matches!(v, Conv::Diverges | Conv::Unknown) {
        return Ok(v);
    }
    // the conditions of the factors are sufficient, not necessary: refuted, the verdict is unknown
    Ok(join(v, decide(cx, b, conds, true)?))
}

fn verdict_powers<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, beh: Beh<'h>) -> Res<Conv<'h>> {
    if beh.grows {
        return Ok(if beh.bounded || beh.decays { Conv::Unknown } else { Conv::Diverges });
    }
    if beh.decays {
        return Ok(Conv::Converges);
    }
    if !beh.decay.is_empty() {
        // an exponential whose rate is not decided to decay may still oscillate (`exp(i x^2)`): not divergent
        return decide(cx, b, beh.decay, true);
    }
    let p1 = cx.eval(b.add(&[b.add(&beh.p), b.one()]))?;
    if b.ar.num(p1).is_some() || super::real_constant(cx, b, p1)? && is_free(b, p1, tvar(b)) && closed(b, p1) {
        let s = sign(cx, b, p1)?;
        // one oscillating factor of order k: Dirichlet's test (the amplitude t^P is monotone near 0) gives
        // convergence for P + 1 > -k, and the integral has no limit for P + 1 <= -k
        if let (true, [(kn, kd)]) = (beh.bounded, beh.osc.as_slice()) {
            if s != Some(1) {
                let m = cx.eval(b.add(&[p1, b.rat(*kn, *kd)]))?;
                return Ok(match sign(cx, b, m)? {
                    Some(1) => Conv::Converges,
                    Some(_) => Conv::Diverges,
                    None => Conv::Unknown,
                });
            }
        }
        return Ok(match s {
            Some(1) => Conv::Converges,
            Some(-1) if !beh.bounded => Conv::Diverges,
            Some(0) if beh.lfree => Conv::Unknown,
            Some(0) if beh.l <= -2 => Conv::Converges,
            Some(0) if !beh.bounded => Conv::Diverges,
            _ => Conv::Unknown,
        });
    }
    // a parameter in the exponent: re(P + 1) > 0, normalised
    let c = re_condition(cx, b, p1, b.zero())?;
    decide(cx, b, vec![c], beh.bounded)
}

/// The condition `re(u) > c`, written with the numeric terms of `u` moved to the right and the sign of the rest
/// made positive (`re(1 - s) > 0` is `re(s) < 1`, `re(-a) > 1` is `re(a) < -1`).
pub(crate) fn re_condition<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, c: T<'h>) -> Res<T<'h>> {
    let u = apply(cx, b, "expand", &[u])?;
    let ts = match args_of(b, u, b.hd.add) {
        Some(ts) => ts.to_vec(),
        None => vec![u],
    };
    let (num, rest): (Vec<T<'h>>, Vec<T<'h>>) = ts.iter().partition(|&&t| b.ar.num(t).is_some());
    let mut c = cx.eval(b.sub(c, b.add(&num)))?;
    let mut r = b.add(&rest);
    let negative = |t: T<'h>| match args_of(b, t, b.hd.mul) {
        Some(fs) => fs.first().and_then(|&k| b.ar.num(k)).is_some_and(|_| {
            matches!(b.heap().view(fs[0]), View::Int(..=-1)) || matches!(b.heap().view(fs[0]), View::Rational { num, .. } if matches!(b.heap().view(num), View::Int(..=-1)))
        }),
        None => false,
    };
    // one term i k u (k real): re(i k u) = -k im_part(u)
    let mut part = "re";
    if let [t] = rest.as_slice() {
        if let Some(fs) = args_of(b, *t, b.hd.mul) {
            if let Some(&k) = fs.first().filter(|&&k| b.ar.num(k).is_some()) {
                let kr = apply(cx, b, "re", &[k])?;
                if kr == b.zero() {
                    let ki = apply(cx, b, "im_part", &[k])?;
                    r = cx.eval(b.mul(&[b.neg(ki), b.mul(&fs[1..])]))?;
                    part = "im_part";
                }
            }
        }
    }
    let mut op = "greater";
    let rest_terms = match args_of(b, r, b.hd.add) {
        Some(ts) => ts.to_vec(),
        None => vec![r],
    };
    if rest_terms.iter().all(|&t| negative(t)) {
        r = cx.eval(b.neg(r))?;
        c = cx.eval(b.neg(c))?;
        op = "less";
    }
    cx.eval(b.call(op, &[b.call(part, &[r]), c]))
}

/// Is `t` free of symbols other than the known constants (`pi`, `e`, ...)? A rough test: its `n` is a number.
fn closed<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) => {
                if !matches!(h.sym_name(s).as_str(), "pi" | "e" | "euler_gamma" | "catalan" | "golden_ratio" | "im") {
                    return false;
                }
            }
            View::App { head, args } => {
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    true
}

/// Conditions decided by the service under the facts of the call: all true converges, one false diverges (unless
/// only bounded oscillation was seen), else the undecided ones are needed.
fn decide<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, cs: Vec<T<'h>>, bounded: bool) -> Res<Conv<'h>> {
    let mut need = vec![];
    let mut held = vec![];
    for c in cs {
        let c = cx.eval(c)?;
        if cx.is_true(c) {
            continue;
        }
        if cx.is_false(c) {
            return Ok(if bounded { Conv::Unknown } else { Conv::Diverges });
        }
        let r = apply(cx, b, "refine", &[c])?;
        if cx.is_true(r) {
            held.push(c);
            continue;
        }
        if cx.is_false(r) {
            return Ok(if bounded { Conv::Unknown } else { Conv::Diverges });
        }
        if !need.contains(&c) {
            need.push(c);
        }
    }
    Ok(if need.is_empty() && held.is_empty() { Conv::Converges } else { Conv::Needs(need, held) })
}

/// The deepest nesting followed.
const MAX_DEPTH: u32 = 40;

/// The behaviour of `g` at `t → 0⁺`, or `None` when it is not found.
fn behaviour<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    stop: &Arc<AtomicBool>,
    g: T<'h>,
    t: T<'h>,
    depth: u32,
) -> Res<Option<Beh<'h>>> {
    cx.tick(1)?;
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    if is_free(b, g, t) {
        // a constant factor (taken nonzero: a zero integrand is a trivial case of the driver)
        return Ok(Some(Beh::unit()));
    }
    if g == t {
        let mut u = Beh::unit();
        u.p.push(b.one());
        return Ok(Some(u));
    }
    if let Some(fs) = args_of(b, g, b.hd.mul) {
        let mut acc = Beh::unit();
        for &u in fs {
            let Some(bu) = behaviour(cx, b, stop, u, t, depth + 1)? else { return Ok(None) };
            acc = acc.times(bu);
        }
        return Ok(Some(acc));
    }
    let Some((name, args)) = super::call_of(b, g) else { return Ok(None) };
    match (name.as_str(), args) {
        ("pow", [base, e]) if b.sym_of(*base) == Some(b.hd.e) => return exponential(cx, b, stop, *e, t),
        ("exp", [e]) => return exponential(cx, b, stop, *e, t),
        ("pow", [base, e]) if is_free(b, *e, t) => {
            if *base == t {
                let mut u = Beh::unit();
                u.p.push(*e);
                return Ok(Some(u));
            }
            // the base's own behaviour, scaled; a numeric exponent falls back to the series of the power
            let symbolic = b.ar.num(*e).is_none();
            if let Some(bb) = behaviour(cx, b, stop, *base, t, depth + 1)? {
                let es = if symbolic { None } else { sign(cx, b, *e)? };
                if (bb.decays || bb.grows) && !bb.bounded && bb.decay.is_empty() && bb.conds.is_empty() && es.is_some_and(|s| s != 0) {
                    // an exponential: a positive power keeps it, a negative one inverts it
                    let mut u = Beh::unit();
                    (u.decays, u.grows) = if es == Some(1) { (bb.decays, bb.grows) } else { (bb.grows, bb.decays) };
                    return Ok(Some(u));
                }
                let pure = !(bb.decays || bb.grows || !bb.decay.is_empty() || bb.bounded);
                let (lpow, lfree) = match b.heap().view(*e) {
                    View::Int(k) => (Some(bb.l * k), bb.lfree),
                    _ if bb.l == 0 => (Some(0), bb.lfree),
                    _ => (Some(0), true),
                };
                if let (true, Some(l)) = (pure, lpow) {
                    let p = cx.eval(b.add(&bb.p))?;
                    let mut u = Beh::unit();
                    u.l = l;
                    u.lfree = lfree;
                    u.conds = bb.conds;
                    if p != b.zero() {
                        u.p.push(b.mul(&[p, *e]));
                    }
                    return Ok(Some(u));
                }
            }
            if symbolic {
                return Ok(None);
            }
        }
        ("log", [a]) => {
            // log(u) with u ~ c t^p: log(t) when p ≠ 0, a constant when u tends to a constant other than 1
            if let Some(ba) = behaviour(cx, b, stop, *a, t, depth + 1)? {
                if !(ba.decays || ba.grows || !ba.decay.is_empty() || ba.bounded) && ba.l == 0 {
                    let p = cx.eval(b.add(&ba.p))?;
                    if p != b.zero() {
                        let mut u = Beh::unit();
                        u.l = 1;
                        u.conds = ba.conds;
                        return Ok(Some(u));
                    }
                }
            }
        }
        ("sin" | "cos", [a]) => {
            // bounded for a real argument that tends to an infinity; else a power series
            if let Some(Outcome::Value(v)) = lim(cx, b, *a, t)? {
                let i = b.heap().sym_term(b.heap().sym("inf"));
                if (v == i || v == b.neg(i)) && real_coefficients(cx, b, *a)? {
                    let mut u = Beh::unit();
                    u.bounded = true;
                    if let Some(ex) = mc_poly::expansion(cx, stop, *a, t, 1, true, None)? {
                        if let Some((k, _)) = ex.coeffs.iter().enumerate().find(|(_, c)| **c != b.zero()) {
                            let val = ex.val + k as i64;
                            if val < 0 {
                                u.osc.push((-val, ex.den));
                            }
                        }
                    }
                    if u.osc.is_empty() {
                        u.osc.push((0, 1));
                    }
                    return Ok(Some(u));
                }
            }
        }
        _ => {}
    }
    // a sum with one exponentially growing term (and powers otherwise) behaves as that term
    if let Some(ts) = args_of(b, g, b.hd.add) {
        let mut grow = None;
        let mut others_powers = true;
        for &u in ts {
            match behaviour(cx, b, stop, u, t, depth + 1)? {
                Some(bu) if bu.grows && !bu.bounded && bu.decay.is_empty() => {
                    if grow.is_some() {
                        others_powers = false;
                    }
                    grow = Some(bu);
                }
                Some(bu) if !(bu.decays || bu.grows || !bu.decay.is_empty() || bu.bounded) => {}
                _ => others_powers = false,
            }
        }
        if let (Some(gb), true) = (grow, others_powers) {
            return Ok(Some(gb));
        }
    }
    // a sum of terms with parameters in their exponents: each term must be read (the smallest exponent is not known)
    if !numeric_exponents(b, g, t) {
        return Ok(None);
    }
    series_behaviour(cx, b, stop, g, t)
}

/// Are the coefficients of `a` real (its value at `t = 1/2` decided real)? A rough test for the oscillation rule.
fn real_coefficients<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, a: T<'h>) -> Res<bool> {
    let at = subst(cx, b, a, tvar(b), b.rat(1, 2))?;
    let im = cx.eval(b.call("im_part", &[at]))?;
    if im == b.zero() {
        return Ok(true);
    }
    let r = apply(cx, b, "refine", &[b.call("element", &[at, b.heap().sym_term(b.heap().sym("reals"))])])?;
    Ok(cx.is_true(r))
}

/// Are all exponents of powers of terms in `t` numbers?
fn numeric_exponents<'h>(b: &Build<'h>, g: T<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![g];
    while let Some(u) = stack.pop() {
        if is_free(b, u, t) {
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            if b.sym_of(head) == Some(b.hd.pow) && args.len() == 2 && b.sym_of(args[0]) != Some(b.hd.e) && b.ar.num(args[1]).is_none() {
                return false;
            }
            stack.extend_from_slice(args);
        }
    }
    true
}

/// A one-sided limit at `t → 0⁺` with a small budget and no numeric check.
fn lim<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, t: T<'h>) -> Res<Option<Outcome<'h>>> {
    let o = limit_with(cx, b, g, t, b.zero(), Direction::FromAbove, &Options { budget: Duration::from_millis(600), check: false })?;
    Ok(match o {
        Outcome::Value(_) => Some(o),
        _ => None,
    })
}

/// `exp(v)` at `t → 0⁺`: a finite limit of `v` is a constant factor; `v ~ c t^k` with `k < 0` decays when
/// `re(c) < 0` (decided, or a condition), grows when `re(c) > 0`.
fn exponential<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    stop: &Arc<AtomicBool>,
    v: T<'h>,
    t: T<'h>,
) -> Res<Option<Beh<'h>>> {
    if is_free(b, v, t) {
        return Ok(Some(Beh::unit()));
    }
    if !numeric_exponents(b, v, t) {
        // `exp(c t^P)` with a symbolic P: a finite limit when re(P) > 0
        let Some(bv) = behaviour(cx, b, stop, v, t, 1)? else { return Ok(None) };
        if bv.decays || bv.grows || !bv.decay.is_empty() || bv.bounded || bv.l != 0 {
            return Ok(None);
        }
        let p = cx.eval(b.add(&bv.p))?;
        let mut u = Beh::unit();
        u.conds = bv.conds;
        if p != b.zero() {
            u.conds.push(re_condition(cx, b, p, b.zero())?);
        }
        return Ok(Some(u));
    }
    let Some(ex) = mc_poly::expansion(cx, stop, v, t, 2, true, None)? else { return Ok(None) };
    // the first nonzero coefficient
    let Some((k, c)) = ex.coeffs.iter().enumerate().find(|(_, c)| **c != b.zero()) else {
        return Ok(Some(Beh::unit()));
    };
    let val = ex.val + k as i64;
    if val >= 0 {
        return Ok(Some(Beh::unit()));
    }
    if !is_free(b, *c, t) {
        // a logarithm in the leading coefficient
        return Ok(None);
    }
    let re = cx.eval(b.call("re", &[*c]))?;
    let mut u = Beh::unit();
    match sign(cx, b, re)? {
        Some(-1) => u.decays = true,
        Some(1) => u.grows = true,
        Some(_) => {
            u.bounded = true;
            u.osc.push((-val, ex.den));
        }
        None => {
            let cond = re_condition(cx, b, b.neg(*c), b.zero())?;
            u.decay.push(cond);
        }
    }
    Ok(Some(u))
}

/// The leading term of a generalized series: `t^(val/den)` with the power of `log t` in its coefficient.
fn series_behaviour<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    stop: &Arc<AtomicBool>,
    g: T<'h>,
    t: T<'h>,
) -> Res<Option<Beh<'h>>> {
    for n in [3, 8] {
        let Some(ex) = mc_poly::expansion(cx, stop, g, t, n, true, None)? else { return Ok(None) };
        let Some((k, c)) = ex.coeffs.iter().enumerate().find(|(_, c)| **c != b.zero()) else { continue };
        let val = ex.val + k as i64;
        let mut u = Beh::unit();
        u.p.push(b.rat(val, ex.den));
        if !is_free(b, *c, t) {
            // the power of log(t) in the coefficient, through a symbol standing for it
            let lt = cx.eval(b.call("log", &[t]))?;
            let ls = b.heap().sym_term(b.heap().sym("l$conv"));
            let cl = subst(cx, b, *c, lt, ls)?;
            if !is_free(b, cl, t) {
                return Ok(None);
            }
            let e = apply(cx, b, "exponent", &[cl, ls])?;
            match b.heap().view(e) {
                View::Int(l) => u.l = l,
                _ => return Ok(None),
            }
        }
        return Ok(Some(u));
    }
    Ok(None)
}

/// Conditions joined: `Needs` collects, `Diverges` wins, `Unknown` next.
pub(crate) fn join<'h>(a: Conv<'h>, c: Conv<'h>) -> Conv<'h> {
    match (a, c) {
        (Conv::Diverges, _) | (_, Conv::Diverges) => Conv::Diverges,
        (Conv::Unknown, _) | (_, Conv::Unknown) => Conv::Unknown,
        (Conv::Converges, o) | (o, Conv::Converges) => o,
        (Conv::Needs(mut x, mut xh), Conv::Needs(y, yh)) => {
            for c in y {
                if !x.contains(&c) {
                    x.push(c);
                }
            }
            for c in yh {
                if !xh.contains(&c) {
                    xh.push(c);
                }
            }
            Conv::Needs(x, xh)
        }
    }
}
