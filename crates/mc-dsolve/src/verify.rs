//! Every solution is checked before it is returned: substituted into the equation, the residual simplified to 0,
//! or, when simplification does not decide, evaluated at random points (random values for the constants `c(i)` and
//! for every parameter as well) to 30 digits.
use super::{Cx, Ode};
use mc_api::{Store, View, T};
use mc_eval::Res;

/// The outcome of a check.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Verdict {
    /// the residual is 0 (exactly or at every sample point)
    Ok,
    /// the residual is not 0 at a sample point
    Fail,
    /// neither could be shown (the residual does not evaluate to numbers)
    Unknown,
}

/// The residual of `s` in `ode`: `diff` with `y^(k)` replaced by the k-th derivative of `s`.
pub(crate) fn residual<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ode: &Ode<'h>, s: T<'h>) -> Res<T<'h>> {
    let mut rules = vec![];
    let mut dk = s;
    for k in 0..=ode.n {
        if k > 0 {
            dk = c.d(dk, ode.x)?;
        }
        rules.push(c.rule(ode.ys[k], dk));
    }
    c.subst(ode.diff, &rules)
}

/// Check `s` against `ode`.
pub(crate) fn check<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ode: &Ode<'h>, s: T<'h>) -> Res<Verdict> {
    let r = residual(c, ode, s)?;
    zero(c, r)
}

/// Is the expression `r` zero: symbolically, else numerically at random points?
pub(crate) fn zero<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, r: T<'h>) -> Res<Verdict> {
    let b = c.b;
    if r == b.zero() {
        return Ok(Verdict::Ok);
    }
    // numbers first: cheap, and they decide a wrong solution at once
    let v = numeric(c, r)?;
    if v == Verdict::Fail {
        return Ok(Verdict::Fail);
    }
    if !c.late() {
        let s = c.simp(r)?;
        if s == b.zero() {
            return Ok(Verdict::Ok);
        }
    }
    Ok(v)
}

/// The symbols (not protected) and constants `c(i)` of `t`: what gets random values.
fn unknowns<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>) -> Vec<T<'h>> {
    let h = c.b.heap();
    let mut out: Vec<T<'h>> = vec![];
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) => {
                if !c.cx.protected(s) && !out.contains(&u) {
                    out.push(u);
                }
            }
            View::App { head, args } => {
                if c.constant_index(u).is_some() {
                    if !out.contains(&u) {
                        out.push(u);
                    }
                    continue;
                }
                if !matches!(h.view(head), View::Sym(_)) {
                    stack.push(head);
                }
                stack.extend(args.iter().copied());
            }
            _ => {}
        }
    }
    out
}

/// The residual at three random points.
fn numeric<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, r: T<'h>) -> Res<Verdict> {
    let b = c.b;
    let vars = unknowns(c, r);
    // a fixed sequence of "random" rationals in (0.3, 1.9): reproducible results
    const PTS: [(i64, i64); 12] = [(37, 100), (113, 100), (71, 50), (59, 100), (167, 100), (43, 50), (31, 25), (91, 100), (139, 100), (53, 100), (181, 100), (77, 100)];
    let mut ok = 0;
    let mut tried = 0;
    let mut k = 0usize;
    while ok < 3 && tried < 6 {
        if c.late() {
            break;
        }
        tried += 1;
        let rules: Vec<T<'h>> = vars
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let (p, q) = PTS[(k + 5 * i) % PTS.len()];
                c.rule(v, b.rat(p, q))
            })
            .collect();
        k += 1;
        let at = c.subst(r, &rules)?;
        let Some(m) = magnitude(c, at)? else { continue };
        if !m.is_finite() {
            continue;
        }
        // relative to the size of the terms
        let mut scale: f64 = 1.0;
        for t in c.terms(at) {
            if let Some(s) = magnitude(c, t)? {
                if s.is_finite() {
                    scale = scale.max(s);
                }
            }
        }
        if m > 1e-9 * scale {
            // confirmed to 30 digits before a solution is rejected
            match magnitude30(c, at)? {
                Some(m30) if m30 > 1e-20 * scale => return Ok(Verdict::Fail),
                Some(_) => {}
                None => continue,
            }
        }
        ok += 1;
    }
    Ok(if ok >= 3 { Verdict::Ok } else { Verdict::Unknown })
}

/// The absolute value of the numeric value of `t` (machine precision), if it is a number.
pub(crate) fn magnitude<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let v = c.ev("n", &[t])?;
    Ok(abs(c, v))
}

/// `|v|` of a number.
fn abs<'h>(c: &Cx<'_, '_, '_, '_, 'h>, v: T<'h>) -> Option<f64> {
    match c.b.ar.num(v) {
        Some(mc_arith::Num::Real(r)) => Some(r.to_f64().abs()),
        Some(mc_arith::Num::Complex(a, bb)) => Some(a.to_f64().hypot(bb.to_f64())),
        None => None,
    }
}

/// `|t|` to 30 digits: 0 when the evaluation cannot leave 0 (`precision_not_reached` around 0).
fn magnitude30<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let b = c.b;
    let v = c.ev("n", &[t, b.int(30)])?;
    if let Some(m) = abs(c, v) {
        return Ok(Some(m));
    }
    let h = b.heap();
    if let View::App { head, args } = h.view(v) {
        if b.sym_of(head) == Some(h.sym(mc_api::names::FAILURE)) && args.first().is_some_and(|&a| b.sym_of(a) == Some(h.sym("precision_not_reached"))) {
            return Ok(Some(0.0));
        }
    }
    Ok(None)
}
