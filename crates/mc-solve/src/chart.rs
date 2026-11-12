//! The sign chart of a condition on one real unknown whose atoms mix `x` with `exp`, `log`, `atan` and powers
//! (round 25, B23-15: `x exp(x) > 1`, `exp(-x^2) > 1/2`, `exp(x) + exp(-x) < 5`, `exp(x) - x > 1`, `atan(x) < 1/100`),
//! for the conditions the kernel algorithm of [`crate::reduce_fn`] does not take.
//!
//! The real zeros of every atom are found by `solve` over the reals (closed forms: `product_log`, logarithms,
//! square roots), and by a numerical scan of [-40, 40] (sign changes refined by bisection, local minima of |d| that
//! touch 0: the double zero of `exp(x) - x - 1`) whose zeros must be exact rationals (a rational with a small
//! denominator at which the atom is exactly 0); a zero that cannot be written exactly gives up. The truth of the
//! condition between consecutive zeros and at each zero comes from machine evaluation of the atoms (an atom that is
//! not a real number at a point is false there, which restricts `log` to its domain). The claim is then checked
//! against the condition itself at 400 points around the zeros and at both sides of every zero; a mismatch (a pole or
//! a zero the search missed) gives up, and `reduce` returns the failure it gave before.
use crate::cx::{Env, Out};
use crate::reduce::{Cond, Rel};
use crate::reduce_fn::{neg_inf, normal, pos_inf, same, value, write, End, Iv};
use mc_api::{Store, T};
use mc_poly::work::{Meter, WorkResult};

/// The truth of the condition given the value of every atom (`None`: not a real number there, so the atom is false)
/// and which atoms vanish.
fn truth(c: &Cond, vals: &[Option<f64>], zero: &[bool]) -> bool {
    match c {
        Cond::Const(b) => *b,
        Cond::Atom(i, r) => match vals[*i] {
            None => false,
            Some(v) => {
                let z = zero[*i];
                match r {
                    Rel::Lt => !z && v < 0.0,
                    Rel::Le => z || v < 0.0,
                    Rel::Gt => !z && v > 0.0,
                    Rel::Ge => z || v > 0.0,
                    Rel::Eq => z,
                    Rel::Ne => !z,
                }
            }
        },
        Cond::And(cs) => cs.iter().all(|c| truth(c, vals, zero)),
        Cond::Or(cs) => cs.iter().any(|c| truth(c, vals, zero)),
        Cond::Not(c) => !truth(c, vals, zero),
    }
}

/// The real branches of the Lambert W function by Halley's iteration (Corless et al. 1996, section 5): `k` = 0 for
/// z >= -1/e, `k` = -1 for -1/e <= z < 0.
fn lambert(k: i64, z: f64) -> Option<f64> {
    let e1 = -(-1f64).exp();
    if !z.is_finite() || z < e1 - 1e-15 || (k == -1 && z >= 0.0) || !(k == 0 || k == -1) {
        return None;
    }
    let z = z.max(e1);
    let p2 = 2.0 * ((1.0f64).exp() * z + 1.0);
    let mut w = if k == 0 {
        if z < -0.25 {
            let p = p2.max(0.0).sqrt();
            -1.0 + p - p * p / 3.0
        } else if z < 3.0 {
            (1.0 + z).ln()
        } else {
            z.ln() - z.ln().ln()
        }
    } else if z < -0.25 {
        let p = -p2.max(0.0).sqrt();
        -1.0 + p - p * p / 3.0
    } else {
        let (l1, l2) = ((-z).ln(), (-(-z).ln()).ln());
        l1 - l2 + l2 / l1
    };
    for _ in 0..60 {
        let ew = w.exp();
        let f = w * ew - z;
        let d = ew * (w + 1.0) - (w + 2.0) * f / (2.0 * w + 2.0);
        if d == 0.0 || !d.is_finite() {
            break;
        }
        let step = f / d;
        w -= step;
        if step.abs() <= 1e-16 * w.abs().max(1e-300) {
            break;
        }
    }
    (w * w.exp() - z).abs().le(&1e-9).then_some(w)
}

/// The machine value of a real term, with the Lambert W leaves (`product_log(z)`, `product_log(k, z)`) evaluated here
/// when the numerical evaluator does not know them.
fn mach<'h>(env: &Env<'h>, t: T<'h>, m: &mut Meter) -> WorkResult<Option<f64>> {
    if let Some(v) = value(env, t) {
        return Ok(Some(v));
    }
    let mut leaves = vec![];
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        let Some((name, args)) = env.head_name(u) else { continue };
        if name == "product_log" {
            leaves.push(u);
        } else {
            stack.extend(args.iter().copied());
        }
    }
    let mut rules = vec![];
    for u in leaves {
        let Some((_, args)) = env.head_name(u) else { return Ok(None) };
        let (k, z) = match args {
            [z] => (Some(0), *z),
            [k, z] => (env.ar.num(*k).and_then(|n| n.as_rat().and_then(|q| q.num.to_i64())), *z),
            _ => return Ok(None),
        };
        let (Some(k), Some(zv)) = (k, value(env, z)) else { return Ok(None) };
        let Some(w) = lambert(k, zv) else { return Ok(None) };
        rules.push((u, env.h().real(w)));
    }
    if rules.is_empty() {
        return Ok(None);
    }
    let t = env.subst(t, &rules, m)?;
    Ok(value(env, t))
}

/// The machine value of `d` at the real point `p` (`None` where it is not a finite real number).
fn at<'h>(env: &Env<'h>, d: T<'h>, x: T<'h>, p: f64, m: &mut Meter) -> WorkResult<Option<f64>> {
    let t = env.subst(d, &[(x, env.h().real(p))], m)?;
    Ok(match env.machine(t) {
        Some((v, im)) if im == 0.0 && v.is_finite() => Some(v),
        _ => None,
    })
}

/// A rational with a small denominator near `r` at which `d` is exactly zero.
fn exact_root<'h>(env: &Env<'h>, d: T<'h>, x: T<'h>, r: f64, m: &mut Meter) -> WorkResult<Option<T<'h>>> {
    for q in 1..=6i64 {
        let n = (r * q as f64).round();
        if (n / q as f64 - r).abs() > 1e-9 || n.abs() > 1e6 {
            continue;
        }
        let c = env.ar.div(env.int(n as i64), env.int(q));
        let v = env.subst(d, &[(x, c)], m)?;
        if env.ar.num(v).is_some_and(|z| z.is_zero()) {
            return Ok(Some(c));
        }
    }
    Ok(None)
}

/// The real zeros of `d` (see the module comment); `None` when a zero cannot be written exactly.
fn zeros<'h>(env: &Env<'h>, renv: &Env<'h>, d: T<'h>, x: T<'h>, m: &mut Meter) -> WorkResult<Option<Vec<(T<'h>, f64)>>> {
    let mut out: Vec<(T<'h>, f64)> = vec![];
    if let Ok(sols) = crate::solve::solve_eqs(renv, &[d], &[x], m, 0)? {
        for s in sols {
            let Some(&(_, v)) = s.first() else { continue };
            let v = crate::solve::strip_condition(renv, v);
            if let Some(val) = mach(env, v, m)? {
                // a root that is an exact rational is written as one (`-1 - product_log(-1, -1/e)` is 0)
                let v = match exact_root(env, d, x, val, m)? {
                    Some(c) => c,
                    None => v,
                };
                if !out.iter().any(|&(_, w)| same(val, w)) {
                    out.push((v, val));
                }
            }
        }
    }
    let (lo, hi, n) = (-40.0f64, 40.0f64, 1600usize);
    let step = (hi - lo) / n as f64;
    let mut vals: Vec<Option<f64>> = Vec::with_capacity(n + 1);
    for k in 0..=n {
        vals.push(at(env, d, x, lo + step * k as f64, m)?);
    }
    let mut cand: Vec<f64> = vec![];
    for k in 0..=n {
        let p = lo + step * k as f64;
        let Some(v) = vals[k] else { continue };
        if v == 0.0 {
            cand.push(p);
        }
        if k == 0 {
            continue;
        }
        if let Some(pv) = vals[k - 1] {
            if pv * v < 0.0 {
                let (mut a, mut b, mut fa) = (p - step, p, pv);
                for _ in 0..80 {
                    let mid = 0.5 * (a + b);
                    let Some(fm) = at(env, d, x, mid, m)? else { break };
                    if fm * fa <= 0.0 {
                        b = mid;
                    } else {
                        a = mid;
                        fa = fm;
                    }
                }
                cand.push(0.5 * (a + b));
            } else if k >= 2 {
                // a local minimum of |d| that is very small: a double zero
                if let (Some(v0), Some(v1)) = (vals[k - 2], vals[k - 1]) {
                    if v1.abs() < v0.abs() && v1.abs() < v.abs() && v1 * v0 > 0.0 && v1 * v > 0.0 {
                        let (mut a, mut b) = (lo + step * (k - 2) as f64, p);
                        for _ in 0..100 {
                            let (m1, m2) = (a + (b - a) / 3.0, b - (b - a) / 3.0);
                            let (Some(f1), Some(f2)) = (at(env, d, x, m1, m)?, at(env, d, x, m2, m)?) else { break };
                            if f1.abs() < f2.abs() {
                                b = m2;
                            } else {
                                a = m1;
                            }
                        }
                        let best = 0.5 * (a + b);
                        if at(env, d, x, best, m)?.is_some_and(|f| f.abs() < 1e-9) {
                            cand.push(best);
                        }
                    }
                }
            }
        }
    }
    for r in cand {
        if out.iter().any(|&(_, w)| (r - w).abs() <= 1e-6 * r.abs().max(1.0)) {
            continue;
        }
        match exact_root(env, d, x, r, m)? {
            Some(t) => {
                let v = mach(env, t, m)?.unwrap_or(r);
                if !out.iter().any(|&(_, w)| same(v, w)) {
                    out.push((t, v));
                }
            }
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// The sign chart (see the module comment).
pub(crate) fn chart<'h>(env: &Env<'h>, cond: &Cond, diffs: &[T<'h>], x: T<'h>, m: &mut Meter) -> WorkResult<Option<Out<'h>>> {
    let mut renv = Env::new(env.h(), env.stop.clone(), "solve");
    renv.cfg.reals = true;
    renv.cfg.periodic = false;
    let mut roots: Vec<(T<'h>, f64, Vec<bool>)> = vec![];
    for (i, &d) in diffs.iter().enumerate() {
        let Some(zs) = zeros(env, &renv, d, x, m)? else { return Ok(None) };
        for (t, v) in zs {
            match roots.iter_mut().find(|r| same(r.1, v)) {
                Some(r) => r.2[i] = true,
                None => {
                    let mut mask = vec![false; diffs.len()];
                    mask[i] = true;
                    roots.push((t, v, mask));
                }
            }
        }
    }
    roots.sort_by(|a, b| a.1.total_cmp(&b.1));
    if roots.len() > 24 {
        return Ok(None);
    }
    let eval_all = |p: f64, m: &mut Meter| -> WorkResult<Vec<Option<f64>>> {
        let mut v = Vec::with_capacity(diffs.len());
        for &d in diffs {
            v.push(at(env, d, x, p, m)?);
        }
        Ok(v)
    };
    let none = vec![false; diffs.len()];
    let mut seen = vec![false; diffs.len()];
    let n = roots.len();
    let mut set: Vec<Iv<'h>> = vec![];
    let mut region_truth: Vec<bool> = vec![];
    for k in 0..=n {
        let (lo, hi) = (if k == 0 { None } else { Some(k - 1) }, if k == n { None } else { Some(k) });
        let p = match (lo, hi) {
            (Some(a), Some(b)) => 0.5 * (roots[a].1 + roots[b].1),
            (Some(a), None) => roots[a].1 + 1.0,
            (None, Some(b)) => roots[b].1 - 1.0,
            (None, None) => 0.0,
        };
        let v = eval_all(p, m)?;
        for (s, w) in seen.iter_mut().zip(&v) {
            *s |= w.is_some();
        }
        let t = truth(cond, &v, &none);
        region_truth.push(t);
        if t {
            let end = |r: Option<usize>, inf: End<'h>| match r {
                Some(j) => End { term: Some(roots[j].0), val: roots[j].1, closed: false },
                None => inf,
            };
            set.push(Iv { lo: end(lo, neg_inf()), hi: end(hi, pos_inf()) });
        }
    }
    for (term, val, mask) in &roots {
        let v = eval_all(*val, m)?;
        for (s, w) in seen.iter_mut().zip(&v) {
            *s |= w.is_some();
        }
        let mut zero = mask.clone();
        for (i, z) in zero.iter_mut().enumerate() {
            if !*z && v[i].is_some_and(|w| w.abs() < 1e-12) {
                *z = true;
            }
        }
        if truth(cond, &v, &zero) {
            let e = End { term: Some(*term), val: *val, closed: true };
            set.push(Iv { lo: e, hi: e });
        }
    }
    // the check against the condition at points near and between the zeros
    let (first, last) = (roots.first().map_or(0.0, |r| r.1), roots.last().map_or(0.0, |r| r.1));
    let span = (last - first).max(2.0);
    let mut pts: Vec<f64> = (0..400).map(|k| first - 3.0 * span + 7.0 * span * (k as f64 + 0.5) / 400.0).collect();
    for r in &roots {
        for dlt in [1e-3, 1e-6] {
            pts.push(r.1 - dlt * span);
            pts.push(r.1 + dlt * span);
        }
    }
    for p in pts {
        if roots.iter().any(|r| same(r.1, p)) {
            continue;
        }
        let k = roots.iter().take_while(|r| r.1 < p).count();
        let v = eval_all(p, m)?;
        for (s, w) in seen.iter_mut().zip(&v) {
            *s |= w.is_some();
        }
        if truth(cond, &v, &none) != region_truth[k] {
            return Ok(None);
        }
    }
    // an atom that is not a real number anywhere (a symbolic parameter, a constant that does not evaluate) is no
    // information: the claim would be vacuous
    if seen.iter().any(|s| !*s) {
        return Ok(None);
    }
    Ok(Some(Out::Again(write(env, &normal(set), x))))
}
