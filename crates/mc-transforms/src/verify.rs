//! The numeric check of a transform found by rules or tables (decision n14e.3).
//!
//! A forward transform is compared with its defining integral (`n_integrate`, through the comparison of item
//! 14D's check: refuted only on clear evidence, a difference far above the error estimate) at a sample point of
//! the transform's variable and sample values of the parameters; an inverse transform is checked by the forward
//! integral of its value. A value with undefined functions, unevaluated transforms or distributions is not checked
//! (`Unverified`), as is one whose integral `n_integrate` cannot do.
pub(crate) use mc_integrals::definite::check::Verdict;
use mc_integrals::definite::check::{compare, complex_parts, symbols};
use mc_calculus::common::{apply, is_failure, is_free};
use mc_calculus::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// Positive sample values of the parameters (rationals away from the usual special points).
const SAMPLES: [(i64, i64); 4] = [(3, 2), (5, 7), (7, 3), (2, 5)];

/// Which integral defines the transform being checked.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Def {
    /// `integrate(f exp(-s t), [t, 0, inf])` at a real `s` to the right of the singularities
    Laplace,
    /// `integrate(f x^(s - 1), [x, 0, inf])` at a few real `s`
    Mellin,
    /// `sqrt(2/pi) integrate(f sin(w t), [t, 0, inf])`
    Sin,
    /// `sqrt(2/pi) integrate(f cos(w t), [t, 0, inf])`
    Cos,
}

/// Check `v` as the Laplace transform of `f` (variable `t`, transform variable `s`).
pub(crate) fn laplace<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, t: T<'h>, s: T<'h>, v: T<'h>) -> Res<Verdict> {
    run_in(cx, b, Def::Laplace, f, t, s, v, None)
}

/// Check `v` as the Mellin transform of `f`; with the strip's condition, at points inside the strip.
pub(crate) fn mellin<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, s: T<'h>, v: T<'h>, strip: Option<T<'h>>) -> Res<Verdict> {
    run_in(cx, b, Def::Mellin, f, x, s, v, strip)
}

/// Check `v` as the sine (`sin: true`) or cosine transform of `f`.
pub(crate) fn sin_cos<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, sin: bool, f: T<'h>, t: T<'h>, w: T<'h>, v: T<'h>) -> Res<Verdict> {
    run_in(cx, b, if sin { Def::Sin } else { Def::Cos }, f, t, w, v, None)
}

fn uncheckable<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    // unit_box: a narrow support the truncated oscillatory quadrature over a long range may miss
    for name in ["unit_box", "dirac_delta", "laplace_transform", "inverse_laplace_transform", "mellin_transform", "fourier_transform", "z_transform", "sum"] {
        if !is_free(b, t, h.sym_term(h.sym(name))) {
            return true;
        }
    }
    super::unknown_function(cx, b, t, h.sym_term(h.sym("transform·none"))) || has_undefined(cx, b, t)
}

/// Does `t` call a function without a definition?
fn has_undefined<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            match h.view(head) {
                View::Sym(s) if !cx.is_defined(s) => return true,
                View::App { .. } => return true,
                _ => {}
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

/// Real points tried for a Mellin integral with a known strip (the first two inside it are used).
const MELLIN_POINTS: [(i64, i64); 12] = [(1, 2), (1, 3), (3, 2), (5, 4), (5, 2), (7, 2), (-1, 2), (-1, 3), (9, 2), (-3, 2), (11, 2), (-5, 2)];

#[allow(clippy::too_many_arguments)]
fn run_in<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, def: Def, f: T<'h>, t: T<'h>, s: T<'h>, v: T<'h>, strip: Option<T<'h>>) -> Res<Verdict> {
    let h = b.heap();
    if uncheckable(cx, b, f) || uncheckable(cx, b, v) {
        return Ok(Verdict::Unverified);
    }
    let mut params = vec![];
    symbols(b, f, t, &mut params);
    symbols(b, v, t, &mut params);
    params.retain(|&p| p != s);
    if params.len() > 3 {
        return Ok(Verdict::Unverified);
    }
    let mut confirmed = false;
    let mut refuted = 0;
    for round in 0..2 {
        cx.tick(1)?;
        let mut rules = vec![];
        let mut total = 0.0;
        for (i, &p) in params.iter().enumerate() {
            let (n, d) = SAMPLES[(round + i) % SAMPLES.len()];
            total += n as f64 / d as f64;
            rules.push(h.call(h.sym(names::RULE), &[p, b.rat(n, d)]));
        }
        // the facts must hold at the sample
        let rl = h.call(b.hd.list, &rules);
        let mut ok = true;
        for &c in cx.assumptions().to_vec().iter() {
            let r = apply(cx, b, "rewrite", &[c, rl])?;
            if !cx.is_true(r) {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        let fs = apply(cx, b, "rewrite", &[f, rl])?;
        let vs = apply(cx, b, "rewrite", &[v, rl])?;
        let points: Vec<T<'h>> = match def {
            Def::Laplace => vec![b.rat((6.0 + 4.0 * total).ceil() as i64 * 2 + 1, 2)],
            Def::Mellin => mellin_points(cx, b, strip, rl, s)?,
            Def::Sin | Def::Cos => vec![b.rat(7, 10)],
        };
        for p in points {
            let vp = apply(cx, b, "rewrite", &[vs, h.call(b.hd.list, &[h.call(h.sym(names::RULE), &[s, p])])])?;
            if is_failure(b, vp) {
                continue;
            }
            let nv = apply(cx, b, "n", &[vp])?;
            if complex_parts(cx, b, nv)?.is_none() {
                continue;
            }
            let inf = h.sym_term(h.sym("inf"));
            let pi = h.sym_term(h.sym("pi"));
            let (g, lo, scale) = match def {
                Def::Laplace => (b.mul(&[fs, b.call("exp", &[b.neg(b.mul(&[p, t]))])]), b.zero(), b.one()),
                Def::Mellin => (b.mul(&[fs, b.pow(t, b.sub(p, b.one()))]), b.zero(), b.one()),
                Def::Sin => (b.mul(&[fs, b.call("sin", &[b.mul(&[p, t])])]), b.zero(), b.pow(b.div(pi, b.int(2)), b.rat(1, 2))),
                Def::Cos => (b.mul(&[fs, b.call("cos", &[b.mul(&[p, t])])]), b.zero(), b.pow(b.div(pi, b.int(2)), b.rat(1, 2))),
            };
            let g = cx.eval(g)?;
            // a delayed step factor heaviside_theta(t - a), a > 0: the integral from a without it (n_integrate
            // misses the jump: n_integrate(heaviside_theta(t - 1) exp(-t), [t, 0, inf]) is 0)
            let (g, lo) = delayed(cx, b, g, t, lo)?;
            // the value of the integral itself: v times the normalisation
            let target = cx.eval(b.mul(&[vp, scale]))?;
            match compare(cx, b, g, t, lo, inf, target)? {
                Verdict::Confirmed => confirmed = true,
                Verdict::Refuted => refuted += 1,
                Verdict::Unverified => {}
            }
            if confirmed {
                break;
            }
        }
        if confirmed {
            return Ok(Verdict::Confirmed);
        }
    }
    // a Mellin integral may be evaluated outside its strip, where n_integrate's value means nothing: two refutations
    Ok(if refuted >= if def == Def::Mellin { 2 } else { 1 } { Verdict::Refuted } else { Verdict::Unverified })
}

/// Check `f` as the inverse Laplace transform of `big` (the forward integral of `f` against `big`).
pub(crate) fn inverse_laplace<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, big: T<'h>, s: T<'h>, t: T<'h>, f: T<'h>) -> Res<Verdict> {
    run_in(cx, b, Def::Laplace, f, t, s, big, None)
}

/// Check `f` as the inverse Mellin transform of `big`.
pub(crate) fn inverse_mellin<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, big: T<'h>, s: T<'h>, x: T<'h>, f: T<'h>) -> Res<Verdict> {
    run_in(cx, b, Def::Mellin, f, x, s, big, None)
}

/// Check `v` as the Z transform of `f`: `n_sum(f(n) z^-n, [n, 0, inf])` at `z = 3` (and sample parameters).
pub(crate) fn z<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, n: T<'h>, z: T<'h>, v: T<'h>) -> Res<Verdict> {
    let h = b.heap();
    if uncheckable(cx, b, f) || uncheckable(cx, b, v) {
        return Ok(Verdict::Unverified);
    }
    let mut params = vec![];
    symbols(b, f, n, &mut params);
    symbols(b, v, n, &mut params);
    params.retain(|&p| p != z);
    if params.len() > 3 {
        return Ok(Verdict::Unverified);
    }
    let mut rules = vec![];
    let mut total = 1.0f64;
    for (i, &p) in params.iter().enumerate() {
        let (a, d) = SAMPLES[(i + 1) % SAMPLES.len()];
        total = total.max(a as f64 / d as f64);
        rules.push(h.call(h.sym(names::RULE), &[p, b.rat(a, d)]));
    }
    let z0 = b.int((3.0 * total).ceil() as i64 + 2);
    rules.push(h.call(h.sym(names::RULE), &[z, z0]));
    let rl = h.call(b.hd.list, &rules);
    let fs = apply(cx, b, "rewrite", &[f, rl])?;
    let vs = apply(cx, b, "rewrite", &[v, rl])?;
    let nv = apply(cx, b, "n", &[vs])?;
    let Some((vr, vi)) = complex_parts(cx, b, nv)? else { return Ok(Verdict::Unverified) };
    // the partial sum to 60 terms (z0 is at least 3 times every sample rate, so the tail is below 1e-20)
    let g = cx.eval(b.mul(&[fs, b.pow(z0, b.neg(n))]))?;
    let (mut sr, mut si) = (0.0, 0.0);
    // at least 60 terms, more while the terms are still large (n^50 2^n z^-n peaks past n = 50)
    let mut recent = [f64::INFINITY; 8];
    for k in 0..2000 {
        cx.tick(1)?;
        let term = mc_limit::bounds::subst(cx, b, g, n, b.int(k))?;
        let nt = apply(cx, b, "n", &[term])?;
        let Some((r, i)) = complex_parts(cx, b, nt)? else { return Ok(Verdict::Unverified) };
        sr += r;
        si += i;
        recent[(k % 8) as usize] = (r * r + i * i).sqrt();
        if k >= 59 && recent.iter().sum::<f64>() < 1e-14 * (1.0 + (sr * sr + si * si).sqrt()) {
            break;
        }
    }
    let mag = (sr * sr + si * si).sqrt();
    let diff = ((vr - sr).powi(2) + (vi - si).powi(2)).sqrt();
    Ok(if diff <= 1e-6 * (1.0 + mag) {
        Verdict::Confirmed
    } else if diff > 1e-3 * (1.0 + mag) {
        Verdict::Refuted
    } else {
        Verdict::Unverified
    })
}

/// The sample points of a Mellin integral: the first two of [`MELLIN_POINTS`] where the strip's condition (at the
/// parameters' sample values `rl`) holds, else 1/2 and 1/3.
fn mellin_points<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, strip: Option<T<'h>>, rl: T<'h>, s: T<'h>) -> Res<Vec<T<'h>>> {
    let h = b.heap();
    let mut out = vec![];
    if let Some(c) = strip {
        let c = apply(cx, b, "rewrite", &[c, rl])?;
        for (n, d) in MELLIN_POINTS {
            cx.tick(1)?;
            let p = b.rat(n, d);
            let cp = apply(cx, b, "rewrite", &[c, h.call(b.hd.list, &[h.call(h.sym(names::RULE), &[s, p])])])?;
            if cx.is_true(cp) {
                out.push(p);
                if out.len() == 2 {
                    break;
                }
            }
        }
    }
    if out.is_empty() {
        out = vec![b.rat(1, 2), b.rat(1, 3)];
    }
    Ok(out)
}

/// `g = heaviside_theta(t - a) r` (or `unit_step`) with a number a > 0: `(r, a)`; otherwise `(g, lo)`.
fn delayed<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, t: T<'h>, lo: T<'h>) -> Res<(T<'h>, T<'h>)> {
    let fs = super::factors(b, g);
    for (i, &f) in fs.iter().enumerate() {
        let Some((n, [u])) = super::call_of(b, f) else { continue };
        if n != "heaviside_theta" && n != "unit_step" {
            continue;
        }
        let Some((k, d)) = super::linear(cx, b, *u, t)? else { continue };
        if k != b.one() {
            continue;
        }
        let a = cx.eval(b.neg(d))?;
        if mc_limit::bounds::sign(cx, b, a)? != Some(1) {
            continue;
        }
        let mut rest = fs.clone();
        rest.remove(i);
        return Ok((cx.eval(b.mul(&rest))?, a));
    }
    Ok((g, lo))
}
