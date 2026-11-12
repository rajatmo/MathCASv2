//! The numeric check of a definite integral before it is returned (item 14D, decision 14D.3).
//!
//! A value with numeric data is compared with `n_integrate(f, [x, a, b], error_estimate: true)`: it is *refuted*
//! when it differs from the numerical integral by more than the larger of 50 times the error estimate and
//! `10^-6 (1 + |I|)` while the estimate itself is small; *confirmed* when it agrees; *unverified* when the
//! numerical integral is not available (no convergence, a function without numeric values, oscillation on an
//! infinite range). A value with parameters is checked at up to two sample points that satisfy its conditions and
//! the facts of the call (and, when conditions on real parts were turned into real ones, at one complex point).
use super::{call_of, is_failure};
use crate::{apply, is_free};
use mc_calculus::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The verdict of the check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// the check confirms the value
    Confirmed,
    /// the check refutes the value
    Refuted,
    /// the check does not decide
    Unverified,
}

/// The parameter values tried, in order (rationals that avoid the usual special points).
const SAMPLES: [(i64, i64); 8] = [(3, 2), (5, 7), (7, 3), (1, 3), (5, 2), (-1, 2), (9, 4), (-3, 2)];

/// Check `v` as the integral of `f` in `x` over `[lo, hi]` under the conditions `conds` (module comment).
#[allow(clippy::too_many_arguments)]
pub(crate) fn verify<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: T<'h>,
    hi: T<'h>,
    v: T<'h>,
    conds: &[T<'h>],
    complex: bool,
) -> Res<Verdict> {
    // distributions have no numerical integral
    let h = b.heap();
    if !is_free(b, f, h.sym_term(h.sym("dirac_delta"))) {
        return Ok(Verdict::Unverified);
    }
    let mut params = vec![];
    for t in [f, lo, hi, v] {
        symbols(b, t, x, &mut params);
    }
    if params.is_empty() {
        return compare(cx, b, f, x, lo, hi, v);
    }
    if params.len() > 3 {
        return Ok(Verdict::Unverified);
    }
    let facts: Vec<T<'h>> = cx.assumptions().to_vec();
    let mut checked = 0;
    let mut confirmed = false;
    let im = h.sym_term(h.sym("im"));
    let rounds = SAMPLES.len() + usize::from(complex);
    let mut complex_done = !complex;
    for k in 0..rounds {
        if checked >= 2 && complex_done {
            break;
        }
        cx.tick(1)?;
        // the last round (when asked): complex values, for a value found with real conditions (decision 14D.2)
        let z = k == SAMPLES.len();
        let rules: Vec<T<'h>> = params
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                let (n, d) = SAMPLES[(k + 2 * i) % SAMPLES.len()];
                let v = if z { b.add(&[b.rat(n, d), b.mul(&[b.rat(1, 3 + i as i64), im])]) } else { b.rat(n, d) };
                h.call(h.sym(names::RULE), &[p, v])
            })
            .collect();
        if checked >= 2 && !z {
            continue;
        }
        let rl = h.call(b.hd.list, &rules);
        // the conditions and the facts must hold at the sample
        let mut ok = true;
        for &c in conds.iter().chain(facts.iter()) {
            let s = apply(cx, b, "rewrite", &[c, rl])?;
            if !cx.is_true(s) {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        let at = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| apply(cx, b, "rewrite", &[t, rl]);
        let (fs, ls, hs, vs) = (at(cx, f)?, at(cx, lo)?, at(cx, hi)?, at(cx, v)?);
        if is_failure(b, vs) {
            continue;
        }
        checked += 1;
        complex_done |= z;
        match compare(cx, b, fs, x, ls, hs, vs)? {
            Verdict::Refuted => return Ok(Verdict::Refuted),
            Verdict::Confirmed => confirmed = true,
            Verdict::Unverified => {}
        }
    }
    Ok(if confirmed { Verdict::Confirmed } else { Verdict::Unverified })
}

/// The symbols of `t` other than `x` and the constants, in order of appearance.
pub fn symbols<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>, out: &mut Vec<T<'h>>) {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) if u != x => {
                let name = h.sym_name(s);
                if !matches!(name.as_str(), "pi" | "e" | "euler_gamma" | "catalan" | "golden_ratio" | "im" | "inf" | "glaisher" | "khinchin")
                    && !out.contains(&u)
                {
                    out.push(u);
                }
            }
            // the variables of a pure function (`root_sum(t => ..., t => ...)`) are bound, not parameters
            View::App { head, args } if args.len() == 2 && b.sym_of(head) == Some(h.sym("fn")) => {
                let mut bound = vec![];
                if let View::App { args: vs, .. } = h.view(args[0]) {
                    bound.extend_from_slice(vs);
                }
                let mut inner = vec![];
                symbols(b, args[1], x, &mut inner);
                for v in inner {
                    if !bound.contains(&v) && !out.contains(&v) {
                        out.push(v);
                    }
                }
            }
            // heads are functions, not parameters
            View::App { args, .. } => stack.extend_from_slice(args),
            _ => {}
        }
    }
}

/// The number of symbols of `t` other than the constants (`x` counted too).
pub fn symbols_count<'h>(b: &Build<'h>, t: T<'h>) -> usize {
    let h = b.heap();
    let mut out = vec![];
    symbols(b, t, h.sym_term(h.sym("check·none")), &mut out);
    out.len()
}

/// The numerical comparison for closed data.
#[allow(clippy::too_many_arguments)]
pub fn compare<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: T<'h>,
    hi: T<'h>,
    v: T<'h>,
) -> Res<Verdict> {
    let h = b.heap();
    let nv = apply(cx, b, "n", &[v])?;
    let Some((vr, vi)) = complex_parts(cx, b, nv)? else { return Ok(Verdict::Unverified) };
    // an oscillating integrand over an infinite range: the truncated integrals at once (adaptive quadrature over
    // the whole range spends seconds before it gives up)
    let inf = h.sym_term(h.sym("inf"));
    if (lo == b.neg(inf) || hi == inf) && slowest_frequency(cx, b, f, x)?.is_some() {
        return oscillatory(cx, b, f, x, lo, hi, (vr, vi));
    }
    // the real line in two halves (the integrand is not evaluated at 0, where it may be removable: sin(x)/x)
    let pieces = if lo == b.neg(inf) && hi == inf { vec![(lo, b.zero()), (b.zero(), hi)] } else { vec![(lo, hi)] };
    let (mut ir, mut ii, mut e) = (0.0, 0.0, Some(0.0));
    for (l, r) in pieces {
        let Some((pr, pi, pe)) = numerical(cx, b, f, x, l, r)? else {
            return oscillatory(cx, b, f, x, lo, hi, (vr, vi));
        };
        ir += pr;
        ii += pi;
        e = match (e, pe) {
            (Some(a), Some(c)) => Some(a + c),
            _ => None,
        };
    }
    let mag = (ir * ir + ii * ii).sqrt();
    let diff = ((vr - ir).powi(2) + (vi - ii).powi(2)).sqrt();
    let tol = (50.0 * e.unwrap_or(0.0)).max(1e-6 * (1.0 + mag));
    if diff <= tol {
        return Ok(Verdict::Confirmed);
    }
    match e {
        // a large estimate: the numerical value is not evidence
        Some(e) if e > 1e-4 * (1.0 + mag) => Ok(Verdict::Unverified),
        Some(_) => Ok(Verdict::Refuted),
        // no estimate: only a gross disagreement refutes
        None if diff > 1e-3 * (1.0 + mag) => Ok(Verdict::Refuted),
        None => Ok(Verdict::Unverified),
    }
}

/// The real and imaginary parts of a numerical value (a machine real, a ball, a complex number of them).
pub fn complex_parts<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<Option<(f64, f64)>> {
    let h = b.heap();
    let t = match h.view(t) {
        View::Ball { .. } => apply(cx, b, "n", &[t])?,
        _ => t,
    };
    match h.view(t) {
        View::Real(r) if r.is_finite() => Ok(Some((r, 0.0))),
        View::Int(k) => Ok(Some((k as f64, 0.0))),
        View::App { .. } => {
            if let Some(("complex", [re, im])) = call_of(b, t).as_ref().map(|(n, a)| (n.as_str(), *a)) {
                let (Some((r, _)), Some((i, _))) = (complex_parts(cx, b, *re)?, complex_parts(cx, b, *im)?) else {
                    return Ok(None);
                };
                return Ok(Some((r, i)));
            }
            if !is_free(b, t, h.sym_term(h.sym("im"))) {
                return Ok(None);
            }
            Ok(None)
        }
        _ => {
            let r = apply(cx, b, "n", &[t])?;
            match h.view(r) {
                View::Real(r) if r.is_finite() => Ok(Some((r, 0.0))),
                _ => Ok(None),
            }
        }
    }
}

/// `n_integrate` over `[l, r]`: the real and imaginary parts and the error estimate (none for complex integrands).
#[allow(clippy::type_complexity)]
fn numerical<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    l: T<'h>,
    r: T<'h>,
) -> Res<Option<(f64, f64, Option<f64>)>> {
    let h = b.heap();
    let rule = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("error_estimate")), cx.bool(true)]);
    let spec = h.call(b.hd.list, &[x, l, r]);
    let ni = apply(cx, b, "n_integrate", &[f, spec, h.call(h.sym("options"), &[rule])])?;
    let (val, err) = match call_of(b, ni).as_ref().map(|(n, a)| (n.as_str(), *a)) {
        Some(("around", [val, err])) => (*val, Some(*err)),
        _ => (ni, None),
    };
    let Some((vr, vi)) = complex_parts(cx, b, val)? else { return Ok(None) };
    let e = match err {
        Some(err) => match complex_parts(cx, b, err)? {
            Some((e, _)) => Some(e.abs()),
            None => return Ok(None),
        },
        None => None,
    };
    Ok(Some((vr, vi, e)))
}

/// The number of periods of the slowest oscillation the truncated ranges of [`oscillatory`] span.
const PERIODS: i64 = 40;

/// The check of an integral over an infinite range whose integrand oscillates with linear frequencies (`sin(a x +
/// b)`, `cos`, `exp(i a x)`), when `n_integrate` gives no value: the integrals truncated at `N P` and `N P + P/2`
/// (`P` the period of the slowest frequency) are averaged, which cancels the leading oscillating term of the tail
/// (`cos(L)/L` for `sin(x)/x`). Only a gross disagreement refutes (`10^-2` relative); agreement within `10^-3`
/// confirms.
fn oscillatory<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: T<'h>,
    hi: T<'h>,
    v: (f64, f64),
) -> Res<Verdict> {
    let h = b.heap();
    let inf = h.sym_term(h.sym("inf"));
    let (lo_inf, hi_inf) = (lo == b.neg(inf), hi == inf);
    if !lo_inf && !hi_inf {
        return Ok(Verdict::Unverified);
    }
    let Some(w) = slowest_frequency(cx, b, f, x)? else { return Ok(Verdict::Unverified) };
    let period = 2.0 * std::f64::consts::PI / w;
    let mut acc = (0.0, 0.0);
    for shift in [0.0, 0.5] {
        let l = (PERIODS as f64 + shift) * period;
        let (a, z) = (if lo_inf { h.real(-l) } else { lo }, if hi_inf { h.real(l) } else { hi });
        let pieces = if lo_inf && hi_inf { vec![(a, b.zero()), (b.zero(), z)] } else { vec![(a, z)] };
        for (pa, pz) in pieces {
            let spec = h.call(b.hd.list, &[x, pa, pz]);
            let ni = apply(cx, b, "n_integrate", &[f, spec])?;
            let Some((r, i)) = complex_parts(cx, b, ni)? else { return Ok(Verdict::Unverified) };
            acc = (acc.0 + r / 2.0, acc.1 + i / 2.0);
        }
    }
    let mag = (acc.0 * acc.0 + acc.1 * acc.1).sqrt();
    let diff = ((v.0 - acc.0).powi(2) + (v.1 - acc.1).powi(2)).sqrt();
    Ok(if diff <= 1e-3 * (1.0 + mag) {
        Verdict::Confirmed
    } else if diff > 1e-2 * (1.0 + mag) {
        Verdict::Refuted
    } else {
        Verdict::Unverified
    })
}

/// The smallest positive frequency `|a|` of the `sin(a x + b)`, `cos(a x + b)` and `exp(i a x + b)` in `f`; `None`
/// when one has a nonlinear argument or there is none.
fn slowest_frequency<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<f64>> {
    let h = b.heap();
    let im = h.sym_term(h.sym("im"));
    let mut stack = vec![f];
    let mut best: Option<f64> = None;
    while let Some(t) = stack.pop() {
        if is_free(b, t, x) {
            continue;
        }
        let Some((n, args)) = call_of(b, t) else { continue };
        let arg = match (n.as_str(), args) {
            ("sin" | "cos", [u]) => Some(*u),
            ("exp", [u]) => Some(apply(cx, b, "expand", &[b.mul(&[b.neg(im), *u])])?),
            ("pow", [c, u]) if b.sym_of(*c) == Some(b.hd.e) => Some(apply(cx, b, "expand", &[b.mul(&[b.neg(im), *u])])?),
            _ => None,
        };
        match arg {
            Some(u) => {
                let a = apply(cx, b, "d", &[u, x])?;
                if !is_free(b, a, x) {
                    return Ok(None);
                }
                let a = apply(cx, b, "n", &[a])?;
                match complex_parts(cx, b, a)? {
                    // a real frequency (a decaying exponential has an imaginary one, and is not an oscillation)
                    Some((r, i)) if i.abs() < 1e-12 && r.abs() > 1e-9 => best = Some(best.map_or(r.abs(), |w| w.min(r.abs()))),
                    Some(_) => {}
                    None => return Ok(None),
                }
            }
            None => stack.extend_from_slice(args),
        }
    }
    Ok(best)
}
