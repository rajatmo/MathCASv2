//! The real roots of one equation in one unknown on a bounded real interval, numerically (package io-sym, decision
//! IOSYM.3): what `solve` adds when the equation is not polynomial and the conditions bound the unknown
//! (`solve(bessel_j(0, x) == 0 and 0 < x and x < 3, x)`, `solve(sin(x) == 0 and 0 < x and x < 7, x)`).
//!
//! `f = lhs - rhs` is sampled at [`SAMPLES`] + 1 equally spaced points; every sign change is narrowed by bisection to
//! machine precision and kept when `|f|` there is small (a sign change across a pole is not a root); a sample where
//! `f` is exactly 0 is a root. Each root is then matched against simple exact values (p/q and p pi/q with
//! q <= [`MAX_DEN`]): one at which `f` evaluates to exactly 0 replaces the machine number. Roots where `f` touches 0
//! without changing sign are found only when a sample lands on them.
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// Subintervals sampled.
const SAMPLES: usize = 1000;
/// The largest denominator of the exact values tried.
const MAX_DEN: i64 = 12;

/// `f(v)` as a machine real (`None` when not real).
fn value<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, v: f64) -> Res<Option<f64>> {
    let h = cx.heap();
    let rule = h.call(h.sym(names::RULE), &[x, h.real(v)]);
    let r = cx.eval(h.call(h.sym("rewrite"), &[f, rule]))?;
    let r = match h.view(r) {
        View::Real(_) | View::Int(_) => r,
        _ => cx.eval(h.call(h.sym("n"), &[r]))?,
    };
    Ok(match h.view(r) {
        View::Real(v) if v.is_finite() => Some(v),
        View::Int(k) => Some(k as f64),
        _ => None,
    })
}

/// The roots of `f` (an expression in `x`) in `[a, b]`, sorted, as terms (exact when matched, else machine reals).
pub fn roots<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, a: f64, b: f64) -> Res<Vec<T<'h>>> {
    let mut found: Vec<f64> = vec![];
    if !(a.is_finite() && b.is_finite() && a < b) {
        return Ok(vec![]);
    }
    let step = (b - a) / SAMPLES as f64;
    let mut prev: Option<(f64, f64)> = None;
    let mut scale = 0f64;
    let mut samples = Vec::with_capacity(SAMPLES + 1);
    for i in 0..=SAMPLES {
        cx.tick(1)?;
        let t = if i == SAMPLES { b } else { a + step * i as f64 };
        let v = value(cx, f, x, t)?;
        if let Some(v) = v {
            scale = scale.max(v.abs());
        }
        samples.push((t, v));
    }
    let tol = 1e-8 * scale.max(1.0);
    for &(t, v) in &samples {
        let Some(v) = v else {
            prev = None;
            continue;
        };
        if v == 0.0 {
            found.push(t);
        } else if let Some((pt, pv)) = prev {
            if pv != 0.0 && (pv < 0.0) != (v < 0.0) {
                if let Some(r) = bisect(cx, f, x, (pt, pv), (t, v), tol)? {
                    found.push(r);
                }
            }
        }
        prev = Some((t, v));
    }
    found.sort_by(f64::total_cmp);
    found.dedup_by(|p, q| (*p - *q).abs() <= 1e-12 * (1.0 + q.abs()));
    let mut out = Vec::with_capacity(found.len());
    for r in found {
        out.push(exact(cx, f, x, r)?);
    }
    Ok(out)
}

/// A root in `[l, r]` (signs differ) by bisection; `None` when `|f|` there is not small (a pole).
fn bisect<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, l: (f64, f64), r: (f64, f64), tol: f64) -> Res<Option<f64>> {
    let (mut lo, mut flo) = l;
    let (mut hi, _) = r;
    for _ in 0..200 {
        cx.tick(1)?;
        let mid = 0.5 * (lo + hi);
        if mid <= lo || mid >= hi {
            break;
        }
        let Some(fm) = value(cx, f, x, mid)? else { return Ok(None) };
        if fm == 0.0 {
            return Ok(Some(mid));
        }
        if (fm < 0.0) == (flo < 0.0) {
            lo = mid;
            flo = fm;
        } else {
            hi = mid;
        }
    }
    let root = if flo.abs() <= tol { lo } else { hi };
    let Some(fr) = value(cx, f, x, root)? else { return Ok(None) };
    Ok((fr.abs() <= tol).then_some(root))
}

/// The exact value p/q or p pi/q (q <= [`MAX_DEN`]) at which `f` is exactly 0 and which is within rounding of `r`;
/// else `r` as a machine real.
fn exact<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, r: f64) -> Res<T<'h>> {
    let h = cx.heap();
    let near = |c: f64| (c - r).abs() <= 1e-9 * (1.0 + r.abs());
    let mut tried = vec![];
    for q in 1..=MAX_DEN {
        for (pi, unit) in [(false, 1.0), (true, std::f64::consts::PI)] {
            let p = (r * q as f64 / unit).round();
            if p.abs() > 1e9 || !near(p * unit / q as f64) || tried.contains(&(p as i64, q, pi)) {
                continue;
            }
            // the reduced form only once
            let g = gcd(p as i64, q);
            let (pp, qq) = (p as i64 / g, q / g);
            if tried.contains(&(pp, qq, pi)) {
                continue;
            }
            tried.push((pp, qq, pi));
            let ar = mc_arith::Arith::new(h);
            let mut cand = ar.rational(h.int(pp), h.int(qq));
            if pi {
                cand = ar.mul(&[cand, h.sym_term(h.sym("pi"))]);
            }
            let rule = h.call(h.sym(names::RULE), &[x, cand]);
            let v = cx.eval(h.call(h.sym("rewrite"), &[f, rule]))?;
            let v = if v == h.int(0) { v } else { cx.eval(h.call(h.sym("simplify"), &[v]))? };
            if v == h.int(0) {
                return Ok(cand);
            }
        }
    }
    Ok(h.real(r))
}

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}
