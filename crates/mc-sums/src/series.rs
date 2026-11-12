//! Machine-precision series: the convergence verdict, and two accelerations of a convergent series.
//!
//! **Verdict** ([`classify`]). The size of the terms is sampled in windows of 24 consecutive terms far out
//! (indices `lo + 2^10`, `lo + 2^13`, `lo + 2^16`) and a power-law exponent `p` fitted to the window maxima,
//! `|f(k)| ~ k^-p`. Terms that do not tend to zero, or that are of one sign and fall no faster than `1/k`,
//! diverge (the integral test); alternating or oscillating terms with `p > 0` converge (Leibniz, Dirichlet).
//! The verdict is a numerical one: a series that only a logarithm separates from `1/k` (`1/(k log k)`) is
//! reported as too slowly convergent rather than given a number.
//!
//! **Alternating series** ([`cvz`]): Algorithm 1 of Cohen, Rodriguez Villegas and Zagier, "Convergence
//! acceleration of alternating series", Experimental Mathematics 9 (2000); error about `5.83^-n` for totally
//! monotone terms, so 24 terms give machine precision.
//!
//! **Other convergent series** ([`abel_plana`]): the Abel-Plana summation formula (Olver, *Asymptotics and
//! Special Functions*, 1974, ch. 8 sec. 3; DLMF 2.10.2),
//! `sum_{k>=N} f(k) = int_N^inf f + f(N)/2 + i int_0^inf (f(N + i t) - f(N - i t)) / (e^(2 pi t) - 1) dt`,
//! with the second integral by 32-point Gauss-Legendre rules on `[0, 1.5]`, `[1.5, 4]`, `[4, 7.5]` (the poles of
//! the weight at `t = i n` are at distance 1 from the first panel, a Bernstein ellipse of parameter 2.9, so
//! the rule is exact to 1e-25) and the first by the closed form of `integrate` or by `n_integrate`. The sum is computed with two
//! splits `N`; their agreement is the test that `f` is analytic enough for the formula (a summand with `floor`,
//! `mod` or `unit_step` is refused).
use crate::cx::{Sx, Verdict};
use crate::numsum::{term, value};
use mc_api::{Store, View, T};
use mc_arith::{Num, Real};
use mc_eval::Res;
use std::sync::OnceLock;

/// What the far terms of a series show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Class {
    /// terms below 1e-200 (or zero) far out: the direct sum ends
    Fast,
    /// alternating terms tending to zero
    Alternating,
    /// terms of one sign or oscillating, falling as a power
    Power,
    /// the series diverges
    Diverges,
    /// convergent or divergent, but not decidable in machine arithmetic
    Slow,
    /// nothing could be decided (non-numeric samples)
    Unsure,
}

/// The pattern of signs of a window.
#[derive(Clone, Copy, PartialEq)]
enum Signs {
    Same,
    Alt,
    Mixed,
}

/// The term at (large) `j`: in machine arithmetic with a machine real index first (cheap for any `j`), an exact
/// integer index when that gives no finite number.
fn probe<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, j: i64, log: bool) -> Res<Option<(f64, f64)>> {
    let jr = sx.h().real(j as f64);
    let t = sx.subst(f, k, jr)?;
    if let Some(v) = value(sx, t)? {
        if v.0.is_finite() && v.1.is_finite() {
            return log_of(v, log);
        }
    }
    if j <= 1 << 14 {
        return term(sx, f, k, j, log);
    }
    Ok(None)
}

/// The principal logarithm of `v` when `log`.
fn log_of(v: (f64, f64), log: bool) -> Res<Option<(f64, f64)>> {
    if !log {
        return Ok(Some(v));
    }
    let r = (v.0 * v.0 + v.1 * v.1).sqrt();
    if r == 0.0 || !r.is_finite() {
        return Ok(None);
    }
    Ok(Some((r.ln(), v.1.atan2(v.0))))
}

/// The maximum magnitude and the sign pattern of the 24 terms from `j0`; `None` when a term is not a number.
fn window<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, j0: i64, log: bool) -> Res<Option<(f64, Signs)>> {
    let mut vs = Vec::with_capacity(24);
    for j in j0..j0 + 24 {
        sx.cx.tick(1)?;
        vs.push(crate::some!(probe(sx, f, k, j, log)?));
    }
    let max = vs.iter().map(|v| (v.0 * v.0 + v.1 * v.1).sqrt()).fold(0.0, f64::max);
    // the component that carries the terms
    let sr: f64 = vs.iter().map(|v| v.0.abs()).sum();
    let si: f64 = vs.iter().map(|v| v.1.abs()).sum();
    let comp: Vec<f64> = vs.iter().map(|v| if sr >= si { v.0 } else { v.1 }).filter(|x| *x != 0.0).collect();
    let signs = if comp.len() < 2 || comp.windows(2).all(|w| (w[0] < 0.0) == (w[1] < 0.0)) {
        Signs::Same
    } else if comp.windows(2).all(|w| (w[0] < 0.0) != (w[1] < 0.0)) {
        Signs::Alt
    } else {
        Signs::Mixed
    };
    Ok(Some((max, signs)))
}

/// The largest modulus of the sums of 12 consecutive pairs of terms from `j0`; `None` when a term is not a number.
fn pair_window<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, j0: i64, log: bool) -> Res<Option<f64>> {
    let mut best = 0.0f64;
    for j in (j0..j0 + 24).step_by(2) {
        sx.cx.tick(1)?;
        let a = crate::some!(probe(sx, f, k, j, log)?);
        let b = crate::some!(probe(sx, f, k, j + 1, log)?);
        best = best.max(((a.0 + b.0).powi(2) + (a.1 + b.1).powi(2)).sqrt());
    }
    Ok(Some(best))
}

/// The term of a real index `x` (any size a machine real holds): `None` when it is not a finite number.
fn probe_at<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, x: f64, log: bool) -> Res<Option<f64>> {
    let t = sx.subst(f, k, sx.h().real(x))?;
    let v = crate::some!(value(sx, t)?);
    if !(v.0.is_finite() && v.1.is_finite()) {
        return Ok(None);
    }
    let v = crate::some!(log_of(v, log)?);
    let m = (v.0 * v.0 + v.1 * v.1).sqrt();
    Ok((m.is_finite() && m > 1e-290).then_some(m))
}

/// The verdict on a series of one-signed terms that fall about like `1/k` (the windows gave an exponent within
/// 0.1 of 1). The local exponent `p(x) = -ln(|f(2x)| / |f(x)|) / ln 2` is measured at `x = 1e8, 1e16, ..., 1e128,
/// 1e250` (as far out as the terms stay representable) and `d = p - 1` examined:
/// - `d` constant and positive: a power law `k^-(1+d)`, convergent (a tail estimate follows; `d = 1e-4` is as
///   good as `d = 1/2`);
/// - `d ln x` constant: `f ~ 1/(k ln^q k)`, convergent for `q > 1`, divergent for `q < 1` (Cauchy's condensation
///   test: `sum 1/(k ln^q k)` behaves as `sum 1/n^q`); `q` within 5% of 1 is undecidable here;
/// - `d` bounded away from 0 and not falling (`ln(k)/k^1.5`): convergent;
/// - `d` negative or `d ln x` falling away to 0 (`1/k + 1/k^1.5`): divergent;
/// - anything else (the exponent drifting, as for `1/(k ln k ln ln k)`, which diverges, or
///   `1/(k ln k (ln ln k)^2)`, which converges, both too slowly to be told apart numerically): too slow.
///
/// Round 23: 1/k^1.0001 converges (it was called divergent), 1/(k ln^1.5 k) is summed instead of returning a
/// value 2.6% off.
fn boundary<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, log: bool) -> Res<Class> {
    let mut xs: Vec<(f64, f64)> = vec![];
    let mut x = 1e8f64;
    while x < 1e100 {
        let (a, b) = (probe_at(sx, f, k, x, log)?, probe_at(sx, f, k, 2.0 * x, log)?);
        let (Some(a), Some(b)) = (a, b) else { break };
        xs.push((x, -(b / a).ln() / std::f64::consts::LN_2 - 1.0));
        x = x * x.min(1e16);
    }
    if xs.len() < 3 {
        return Ok(Class::Slow);
    }
    let last = &xs[xs.len() - 3..];
    let d = last[2].1;
    // p = 1 to machine precision at the far scales: the terms are 1/k, plus a part that vanishes
    if d.abs() < 1e-12 && last[1].1.abs() < 1e-9 {
        return Ok(Class::Diverges);
    }
    let near = |a: f64, b: f64, rel: f64| (a - b).abs() <= rel * b.abs().max(1e-300);
    if last.iter().all(|&(_, v)| near(v, d, 0.02)) {
        return Ok(if d >= 1e-9 { Class::Power } else if d <= -1e-9 { Class::Diverges } else { Class::Slow });
    }
    if last.iter().any(|&(_, v)| v < 0.0) {
        return Ok(if last.iter().all(|&(_, v)| v < 0.0) { Class::Diverges } else { Class::Slow });
    }
    let q: Vec<f64> = last.iter().map(|&(x, v)| v * x.ln()).collect();
    if q.iter().all(|&v| near(v, q[2], 0.03)) {
        return Ok(if q[2] >= 1.05 {
            Class::Power
        } else if q[2] <= 0.95 {
            Class::Diverges
        } else {
            Class::Slow
        });
    }
    if last.iter().all(|&(_, v)| v > 0.02) && last[2].1 >= last[0].1 {
        return Ok(Class::Power);
    }
    if q[2] < 0.5 * q[0] {
        return Ok(Class::Diverges);
    }
    Ok(Class::Slow)
}

/// The verdict on the series `sum_{k>=lo} f(k)` (`log`: of the logarithms of the terms).
pub fn classify<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, log: bool) -> Res<Class> {
    let mut ws: Vec<(f64, f64, Signs)> = vec![];
    for e in [10, 13, 16] {
        let n = lo.max(0) + (1i64 << e);
        let Some((m, s)) = window(sx, f, k, n, log)? else {
            return Ok(if ws.is_empty() { Class::Unsure } else { Class::Slow });
        };
        if m == f64::INFINITY {
            // an overflow far out: the terms grow
            return Ok(Class::Diverges);
        }
        if !m.is_finite() {
            return Ok(Class::Unsure);
        }
        if m < 1e-200 {
            return Ok(Class::Fast);
        }
        ws.push((n as f64, m, s));
        // very small and falling fast: no need to look further out
        if ws.len() >= 2 {
            let (a, b) = (ws[ws.len() - 2], ws[ws.len() - 1]);
            if b.1 < 1e-30 && b.1 < a.1 * 1e-3 {
                return Ok(Class::Fast);
            }
        }
    }
    let [.., (n1, m1, _), (n2, m2, s2)] = ws.as_slice() else { return Ok(Class::Unsure) };
    let p = (m1 / m2).ln() / (n2 / n1).ln();
    let fall = (m1 - m2) / m1;
    if p <= 0.02 {
        // not tending to zero, or so slowly that no number can be found
        return Ok(if fall < 1e-3 || p < 0.0 { Class::Diverges } else { Class::Slow });
    }
    match s2 {
        Signs::Same => {
            if p <= 0.9 {
                return Ok(Class::Diverges);
            }
            if p < 1.1 {
                // an exponent near 1 cannot be told from the windows: the local exponent at far larger indices decides
                return boundary(sx, f, k, log);
            }
            Ok(Class::Power)
        }
        Signs::Alt => {
            // terms that alternate in sign but whose consecutive pairs do not cancel to a faster decay
            // ((-1)^k/sqrt(k) - 1/(2k)) have the divergent series of their pair sums behind them
            let mut pairs = vec![];
            for e in [13, 16] {
                let n = lo.max(0) + (1i64 << e);
                match pair_window(sx, f, k, n, log)? {
                    Some(v) => pairs.push((n as f64, v)),
                    None => return Ok(Class::Alternating),
                }
            }
            let [(n1, b1), (n2, b2)] = pairs[..] else { return Ok(Class::Alternating) };
            // pair sums below 1e-9 of the terms are rounding noise (the logarithms of 1 + (-1)^k/k cancel to 1/k^4 and
            // the product converges to 1): no verdict from them
            if b2 > 1e-9 * ws[ws.len() - 1].1 && b1 > 1e-9 * ws[ws.len() - 2].1 {
                let q = (b1 / b2).ln() / (n2 / n1).ln();
                if q <= 1.003 {
                    return Ok(Class::Diverges);
                }
                if q < 1.1 {
                    return Ok(Class::Slow);
                }
            }
            Ok(Class::Alternating)
        }
        Signs::Mixed => Ok(Class::Power),
    }
}

/// Algorithm 1 of Cohen-Rodriguez Villegas-Zagier: `sum_k (-1)^k a_k` from `a_0 .. a_{n-1}` (positive, totally
/// monotone).
pub fn cvz(a: &[f64]) -> f64 {
    let n = a.len() as f64;
    let mut d = (3.0 + 8f64.sqrt()).powf(n);
    d = (d + 1.0 / d) / 2.0;
    let (mut b, mut c, mut s) = (-1.0f64, -d, 0.0f64);
    for (k, ak) in a.iter().enumerate() {
        let k = k as f64;
        c = b - c;
        s += c * ak;
        b = (k + n) * (k - n) * b / ((k + 0.5) * (k + 1.0));
    }
    s / d
}

/// The sum of the alternating series by [`cvz`] from the first `n` terms, checked against `n - 6` terms; `None`
/// when the terms do not alternate in sign over those, or the two disagree.
fn alternating_from<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, log: bool) -> Res<Option<f64>> {
    let n = 30;
    let mut a = Vec::with_capacity(n);
    for j in 0..n as i64 {
        sx.cx.tick(1)?;
        let t = crate::some!(term(sx, f, k, lo + j, log)?);
        if t.1 != 0.0 || !t.0.is_finite() || t.0 == 0.0 {
            return Ok(None);
        }
        a.push(if j % 2 == 0 { t.0 } else { -t.0 });
    }
    // the sign of the first term, so that a_k > 0
    let s0 = if a[0] < 0.0 { -1.0 } else { 1.0 };
    let a: Vec<f64> = a.iter().map(|x| x * s0).collect();
    if a.iter().any(|x| *x <= 0.0) || a.windows(2).any(|w| w[1] > w[0] * (1.0 + 1e-12)) {
        return Ok(None);
    }
    let big = s0 * cvz(&a);
    let small = s0 * cvz(&a[..n - 6]);
    if !big.is_finite() || (big - small).abs() > 1e-11 * big.abs().max(1e-300) {
        return Ok(None);
    }
    Ok(Some(big))
}

/// Gauss-Legendre nodes and weights on `[-1, 1]` (Newton iteration on the Legendre polynomial).
fn gauss_legendre(n: usize) -> &'static (Vec<f64>, Vec<f64>) {
    static GL: OnceLock<(Vec<f64>, Vec<f64>)> = OnceLock::new();
    GL.get_or_init(|| {
        let mut xs = vec![0.0; n];
        let mut ws = vec![0.0; n];
        for i in 0..n {
            let mut x = (std::f64::consts::PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
            let mut dp = 1.0;
            for _ in 0..100 {
                let (mut p0, mut p1) = (1.0, x);
                for j in 2..=n {
                    let jf = j as f64;
                    let p2 = ((2.0 * jf - 1.0) * x * p1 - (jf - 1.0) * p0) / jf;
                    p0 = p1;
                    p1 = p2;
                }
                dp = n as f64 * (x * p1 - p0) / (x * x - 1.0);
                let dx = p1 / dp;
                x -= dx;
                if dx.abs() < 1e-16 {
                    break;
                }
            }
            xs[i] = x;
            ws[i] = 2.0 / ((1.0 - x * x) * dp * dp);
        }
        (xs, ws)
    })
}

/// The summand at the complex point `(re, im)`: a machine complex number.
fn at<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, re: f64, im: f64, log: bool) -> Res<Option<(f64, f64)>> {
    let z = sx.ar.number(&Num::Complex(Real::Machine(re), Real::Machine(im)));
    let t = sx.subst(f, k, z)?;
    let v = crate::some!(value(sx, t)?);
    if !(v.0.is_finite() && v.1.is_finite()) {
        return Ok(None);
    }
    log_of(v, log)
}

/// `int_n^inf g(k) dk` for the tail of a slowly falling summand: the closed form of `integrate` when it has one
/// (under a short time limit), else `n_integrate` of the integrand in the variable `u = ln k`,
/// `int_{ln n}^inf g(e^u) e^u du`. The substitution is what makes a tail that falls like `k^-1.01` computable: in
/// `u` it falls like `e^(-0.01 u)` (a rate the exponential-sinh rule resolves), where in `k` the part of the
/// integral beyond the largest machine real is 0.09 of the 97.27 (the double-exponential rule stops at 1e308). A
/// tail `1/(k ln^q k)` becomes `u^-q`, which the rule integrates to full accuracy for `q` above 1. `None` when
/// no number comes out.
pub(crate) fn tail_integral<'h>(sx: &mut Sx<'_, '_, '_, 'h>, g: T<'h>, k: T<'h>, n: i64) -> Res<Option<(f64, f64)>> {
    // the integrand in u = ln k: log(k) is u itself, and the powers of exp(u) are combined (exp(u)^-1.01 exp(u) is
    // exp(-0.01 u), which does not overflow where exp(u) does)
    let u = sx.sym("numsum$u");
    let eu = sx.call("exp", &[u]);
    let logk = sx.call("log", &[k]);
    let g1 = mc_numfn::replace(sx.h(), g, &mut |t| (t == logk).then_some(u));
    let gu = sx.subst(g1, k, eu)?;
    let h = sx.mul(&[gu, eu])?;
    let h = sx.ev("power_expand", &[h])?;
    let lo = sx.h().real((n as f64).ln());
    let uspec = sx.list(&[u, lo, sx.sym("inf")]);
    let x = sx.sym("numsum$x");
    let gx = sx.subst(g, k, x)?;
    let xspec = sx.list(&[x, sx.int(n), sx.sym("inf")]);
    // a closed form: for a summand with a non-integer power (k^-1.01, 1/(k ln^1.5 k)) in u first (a power or an
    // exponential there; the symbolic integrator would try x^-1.01 numerically, with a warning of its own), for the
    // others (rational functions) in k first, where the closed form is exact; an inexact summand has none in k
    let mut closed = vec![];
    if !crate::numsum::inexact(sx, g) {
        if has_fractional_power(sx, g)? {
            closed.extend([(h, uspec), (gx, xspec)]);
        } else {
            closed.extend([(gx, xspec), (h, uspec)]);
        }
    } else {
        closed.push((h, uspec));
    }
    for (body, spec) in closed {
        let exact = sx.call("integrate", &[body, spec]);
        let ne = sx.call("n", &[exact]);
        let i = sx.ev("time_constrained", &[ne, sx.int(2)])?;
        if !sx.contains_any(i, &["integrate", "failure", "time_constrained", "conditional_expression"]) {
            if let Some(v) = value(sx, i)? {
                return Ok(Some(v));
            }
        }
    }
    // numerically, in u and then in k. A one-signed integrand (a power law with a slow tail) goes first through u
    // with an explicit precision goal (a usable estimate is then accepted without the warning of the default goals);
    // an oscillating one through k with the default goals, as the oscillatory method of n_integrate needs
    let mut signs = [false; 2];
    for j in 0..40 {
        let t = sx.subst(g, k, sx.h().real(1000.0 + 0.9 * j as f64))?;
        if let Some((re, im)) = value(sx, t)? {
            if re != 0.0 || im != 0.0 {
                signs[((re < 0.0) || (re == 0.0 && im < 0.0)) as usize] = true;
            }
        }
    }
    let oscillates = signs[0] && signs[1];
    let prec = sx.call(mc_api::names::RULE, &[sx.sym("precision_goal"), sx.int(6)]);
    let attempts: Vec<(T<'h>, T<'h>, Vec<T<'h>>)> = if oscillates {
        // tight first (the two splits of Abel-Plana are compared to 1e-11), the default goals when those fail
        let tight = sx.call(mc_api::names::RULE, &[sx.sym("precision_goal"), sx.int(13)]);
        vec![(gx, xspec, vec![tight]), (gx, xspec, vec![]), (h, uspec, vec![prec])]
    } else {
        vec![(h, uspec, vec![prec]), (gx, xspec, vec![prec])]
    };
    for (body, spec, goals) in attempts {
        let mut args = vec![body, spec];
        args.extend(goals);
        let ni = sx.call("n_integrate", &args);
        let i = sx.ev("time_constrained", &[ni, sx.int(8)])?;
        if !sx.contains_any(i, &["n_integrate", "failure", "time_constrained"]) {
            if let Some(v) = value(sx, i)? {
                return Ok(Some(v));
            }
        }
    }
    Ok(None)
}

/// `sum_{k>=n} f(k)` by the Abel-Plana formula; `g` is `f` (or `log f`, as a term, for products).
fn tail<'h>(sx: &mut Sx<'_, '_, '_, 'h>, g: T<'h>, k: T<'h>, n: i64) -> Res<Option<(f64, f64)>> {
    // int_n^inf g: the closed form or the integral in ln k ([`tail_integral`])
    let v1 = crate::some!(tail_integral(sx, g, k, n)?);
    let fnn = crate::some!(at(sx, g, k, n as f64, 0.0, false)?);
    let (xs, ws) = gauss_legendre(32);
    let (mut s0, mut s1) = (0.0, 0.0);
    for (a, b) in [(0.0, 1.5), (1.5, 4.0), (4.0, 7.5)] {
        let (c, r) = (0.5 * (a + b), 0.5 * (b - a));
        for (x, w) in xs.iter().zip(ws) {
            sx.cx.tick(1)?;
            let t = c + r * x;
            let up = crate::some!(at(sx, g, k, n as f64, t, false)?);
            let dn = crate::some!(at(sx, g, k, n as f64, -t, false)?);
            // i (f(n + i t) - f(n - i t)) / (e^(2 pi t) - 1)
            let d = (up.0 - dn.0, up.1 - dn.1);
            let den = (2.0 * std::f64::consts::PI * t).exp_m1();
            s0 += w * r * (-d.1) / den;
            s1 += w * r * d.0 / den;
        }
    }
    Ok(Some((v1.0 + fnn.0 / 2.0 + s0, v1.1 + fnn.1 / 2.0 + s1)))
}

/// The sum of the convergent series `sum_{k>=lo} f(k)` (`log`: of the logarithms) by the Abel-Plana formula with
/// the splits `lo + 16` and `lo + 24`; `None` when they disagree or a step fails.
pub fn abel_plana<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, log: bool) -> Res<Option<(f64, f64)>> {
    let g = if log { sx.ev("log", &[f])? } else { f };
    let mut sums = vec![];
    let (mut oscillates, mut tail_size) = (false, 0.0f64);
    let mut prev: Option<(f64, f64)> = None;
    for n in [lo + 16, lo + 24] {
        let mut s = (0.0, 0.0);
        for j in lo..n {
            sx.cx.tick(1)?;
            let t = crate::some!(term(sx, f, k, j, log)?);
            s = (s.0 + t.0, s.1 + t.1);
            if let Some(p) = prev {
                oscillates |= p.0 * t.0 < 0.0 || p.1 * t.1 < 0.0;
            }
            prev = Some(t);
        }
        let tl = tail(sx, g, k, n)?;
        let tl = crate::some!(tl);
        tail_size = tail_size.max(tl.0.abs() + tl.1.abs());
        sums.push((s.0 + tl.0, s.1 + tl.1));
    }
    let (a, b) = (sums[0], sums[1]);
    let scale = (b.0.abs() + b.1.abs()).max(1e-300);
    // the splits agree to the accuracy of the tail integral; for an oscillating summand that is the integrator's
    // 1e-8 of the tail (a one-signed one has a closed form or a smooth integral, and keeps 1e-11 of the sum)
    let tol = 1e-11 * scale + if oscillates { 1e-7 * tail_size } else { 0.0 };
    if (a.0 - b.0).abs() + (a.1 - b.1).abs() > tol {
        sx.verdict = Verdict::Open;
        return Ok(None);
    }
    Ok(Some(b))
}

/// The sum of an alternating series of real terms: [`cvz`] from the first index at which 30 terms alternate with
/// falling magnitudes (`lo`, then `lo` + 16, 32, 64, 128: log(k + 1)/sqrt(k) rises until k = 5, and the
/// acceleration of the whole series gave -0.348 for a sum of -0.398), the leading terms added directly.
pub fn alternating_sum<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, log: bool) -> Res<Option<f64>> {
    for m in [0i64, 16, 32, 64, 128] {
        if let Some(tail) = alternating_from(sx, f, k, lo + m, log)? {
            let mut head = 0.0;
            for j in lo..lo + m {
                sx.cx.tick(1)?;
                let t = crate::some!(term(sx, f, k, j, log)?);
                if t.1 != 0.0 || !t.0.is_finite() {
                    return Ok(None);
                }
                head += t.0;
            }
            return Ok(Some(head + tail));
        }
    }
    Ok(None)
}

/// Does `g` contain a power with a numeric exponent that is not an integer?
fn has_fractional_power<'h>(sx: &mut Sx<'_, '_, '_, 'h>, g: T<'h>) -> Res<bool> {
    let h = sx.h();
    let mut stack = vec![g];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if let (true, &[_, e]) = (head == sx.sym(mc_api::names::POW), args) {
                if let Some((v, 0.0)) = value(sx, e)? {
                    if v.fract() != 0.0 {
                        return Ok(true);
                    }
                }
            }
            stack.extend_from_slice(args);
        }
    }
    Ok(false)
}
