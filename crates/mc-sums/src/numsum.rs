//! Numeric sums and products in machine precision (`n_sum`, `n_product`, and `sum` / `product` with inexact
//! summands, which is what `n` makes of an unevaluated sum).
//!
//! The terms are evaluated one by one. A series whose terms fall below 1e-17 of the partial sum stops there;
//! an alternating series is accelerated with Wynn's epsilon algorithm on its partial sums; a slowly converging
//! one (term ratio tending to 1) by Richardson extrapolation of the partial sums at `N = 16, 32, ..., 2048`,
//! assuming an error expansion in powers of `1/N` (Euler-Maclaurin for a rational or algebraic summand).
//! Products are summed as logarithms. The number of terms is bounded and the loop polls the interrupt.
use crate::cx::{Method, Sx, Verdict};
use crate::series::{self, Class};
use mc_api::{names, Store, View, T};
use mc_numfn::{NumFn, Var};
use mc_eval::Res;

/// The most terms evaluated directly.
const MAX_TERMS: i64 = 100_000;

/// The most terms of a finite range summed one by one when the tails (a difference of two infinite sums) cannot be used.
const MAX_FINITE: i64 = 3_000_000;

/// The most terms of a finite range summed with the compiled summand (about 20 ns a term).
const MAX_FAST: i64 = 400_000_000;

/// The most terms of a finite range summed directly with the compiled summand before the tails of a convergent series
/// are tried (0.5 s).
const MAX_DIRECT: i64 = 20_000_000;

/// `prod_{k=lo}^{hi} f(k)` as a machine complex product; `None` when a factor is not numeric, or the running
/// product overflows or reaches zero (the caller then sums logarithms).
fn multiply<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, hi: i64) -> Res<Option<(f64, f64)>> {
    let mut z = (1.0f64, 0.0f64);
    for j in lo..=hi {
        sx.cx.tick(1)?;
        let t = crate::some!(term(sx, f, k, j, false)?);
        z = (z.0 * t.0 - z.1 * t.1, z.0 * t.1 + z.1 * t.0);
        if !(z.0.is_finite() && z.1.is_finite()) || (z.0 == 0.0 && z.1 == 0.0) {
            return Ok(None);
        }
    }
    Ok(Some(z))
}

/// `sum_{k=lo}^{hi} f(k)` term by term with compensated (Neumaier) summation.
fn add_up<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, hi: i64, log: bool) -> Res<Option<(f64, f64)>> {
    let (mut s, mut c) = ((0.0f64, 0.0f64), (0.0f64, 0.0f64));
    for j in lo..=hi {
        sx.cx.tick(1)?;
        let t = crate::some!(term(sx, f, k, j, log)?);
        if !(t.0.is_finite() && t.1.is_finite()) {
            return Ok(None);
        }
        for (i, ti) in [t.0, t.1].into_iter().enumerate() {
            let (si, ci) = if i == 0 { (&mut s.0, &mut c.0) } else { (&mut s.1, &mut c.1) };
            let sum = *si + ti;
            *ci += if si.abs() >= ti.abs() { (*si - sum) + ti } else { (ti - sum) + *si };
            *si = sum;
        }
    }
    Ok(Some((s.0 + c.0, s.1 + c.1)))
}

/// Does `t` contain an inexact number?
pub fn inexact<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    let h = sx.h();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Real(_) | View::Ball { .. } => return true,
            View::App { head, args } => {
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    false
}

/// A machine complex number `(re, im)` of a numeric term.
pub(crate) fn value<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> Res<Option<(f64, f64)>> {
    let h = sx.h();
    let x = match h.view(t) {
        View::Real(_) | View::Int(_) => t,
        _ => sx.ev("n", &[t])?,
    };
    match h.view(x) {
        View::Real(v) => Ok(Some((v, 0.0))),
        View::Int(v) => Ok(Some((v as f64, 0.0))),
        View::App { head, args: &[a, b] } if head == sx.sym(names::COMPLEX) => {
            let re = match h.view(a) {
                View::Real(v) => v,
                View::Int(v) => v as f64,
                _ => return Ok(None),
            };
            let im = match h.view(b) {
                View::Real(v) => v,
                View::Int(v) => v as f64,
                _ => return Ok(None),
            };
            Ok(Some((re, im)))
        }
        _ => {
            // n of a rational or ball: through a real conversion
            let r = sx.ev("n", &[x])?;
            match h.view(r) {
                View::Real(v) => Ok(Some((v, 0.0))),
                _ => Ok(None),
            }
        }
    }
}

/// The term at `k`.
pub(crate) fn term<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, j: i64, log: bool) -> Res<Option<(f64, f64)>> {
    let t = sx.subst(f, k, sx.int(j))?;
    let v = crate::some!(value(sx, t)?);
    if !log {
        return Ok(Some(v));
    }
    // the principal logarithm
    let (re, im) = v;
    let r = (re * re + im * im).sqrt();
    if r == 0.0 {
        return Ok(None);
    }
    Ok(Some((r.ln(), im.atan2(re))))
}

/// Wynn's epsilon algorithm on the partial sums `s` (one coordinate): the last even-column estimate.
fn wynn(s: &[f64]) -> f64 {
    let n = s.len();
    let mut e0 = vec![0.0; n + 1];
    let mut e1: Vec<f64> = s.to_vec();
    let mut best = s[n - 1];
    let mut col = 1;
    while e1.len() > 1 {
        let mut e2 = Vec::with_capacity(e1.len() - 1);
        for i in 0..e1.len() - 1 {
            let d = e1[i + 1] - e1[i];
            let v = if d == 0.0 { f64::INFINITY } else { e0[i + 1] + 1.0 / d };
            e2.push(v);
        }
        e0 = e1;
        e1 = e2;
        col += 1;
        if col % 2 == 1 {
            if let Some(&v) = e1.last() {
                if v.is_finite() {
                    best = v;
                }
            }
        }
    }
    best
}

/// Richardson extrapolation of `s(N)` at `N = n0 2^j` assuming `s(N) = s + a1/N + a2/N^2 + ...`.
fn richardson(s: &[f64]) -> f64 {
    let mut t = s.to_vec();
    let mut p = 1.0;
    for _ in 1..s.len() {
        p *= 2.0;
        t = (0..t.len() - 1).map(|i| (p * t[i + 1] - t[i]) / (p - 1.0)).collect();
    }
    t[0]
}

/// `sum_{k=lo}^{hi} f(k)` numerically (`hi = None` for infinity); `log` sums the logarithms of the terms.
pub fn sum_values<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, hi: Option<i64>, log: bool) -> Res<Option<(f64, f64)>> {
    let mut s = (0.0, 0.0);
    let mut partial: Vec<(f64, f64)> = vec![];
    let mut small = 0;
    let mut j = lo;
    let end = hi.unwrap_or(i64::MAX);
    let mut last = (0.0f64, 0.0f64);
    let mut r32 = 0.0f64;
    while j <= end {
        if j - lo > MAX_TERMS {
            return Ok(None);
        }
        sx.cx.tick(1)?;
        let t = crate::some!(term(sx, f, k, j, log)?);
        if !(t.0.is_finite() && t.1.is_finite()) {
            return Ok(None);
        }
        s = (s.0 + t.0, s.1 + t.1);
        partial.push(s);
        let mag = (s.0.abs() + s.1.abs()).max(1e-300);
        // zero terms before the first nonzero one (unit_step(n - 3) cuts off the start) are not convergence: up to
        // 1000 of them are passed over
        let all_zero = s.0 == 0.0 && s.1 == 0.0 && j - lo < 1000;
        if hi.is_none() && !all_zero && t.0.abs() + t.1.abs() <= 1e-17 * mag {
            small += 1;
            if small >= 3 {
                return Ok(Some(s));
            }
        } else {
            small = 0;
        }
        if hi.is_none() && partial.len() == 32 {
            r32 = ((t.0 * t.0 + t.1 * t.1) / (last.0 * last.0 + last.1 * last.1).max(1e-300)).sqrt();
        }
        if hi.is_none() && partial.len() == 64 {
            // the far terms decide: divergent series and products give no number, an alternating series is
            // accelerated (Cohen-Villegas-Zagier), other slowly converging ones summed by the Abel-Plana formula
            let class = match sx.method {
                Method::Auto => series::classify(sx, f, k, lo, log)?,
                Method::Tail => Class::Power,
                Method::Alternating => Class::Alternating,
                Method::Direct => Class::Unsure,
            };
            match class {
                Class::Diverges => {
                    sx.verdict = Verdict::Diverges;
                    return Ok(None);
                }
                Class::Slow => {
                    sx.verdict = Verdict::Slow;
                    return Ok(None);
                }
                Class::Alternating => {
                    if let Some(v) = series::alternating_sum(sx, f, k, lo, log)? {
                        return Ok(Some((v, 0.0)));
                    }
                }
                Class::Power => {
                    if let Some(v) = series::abel_plana(sx, f, k, lo, log)? {
                        return Ok(Some(v));
                    }
                    // an oscillating series that Abel-Plana did not sum: no estimate (the heuristics below gave a value 6e-6 off
                    // for the logarithms of 1 + e^(2 i n/3)/n^2)
                    if t.0 * last.0 < 0.0 || t.1 * last.1 < 0.0 {
                        sx.verdict = Verdict::Slow;
                        return Ok(None);
                    }
                }
                Class::Fast => {
                    last = t;
                    j += 1;
                    continue;
                }
                Class::Unsure => {}
            }
            // the asymptotic behaviour of the terms
            let r = ((t.0 * t.0 + t.1 * t.1) / (last.0 * last.0 + last.1 * last.1).max(1e-300)).sqrt();
            let alternating = t.0 * last.0 < 0.0;
            if alternating && r > 0.5 {
                let re: Vec<f64> = partial.iter().map(|p| p.0).collect();
                let im: Vec<f64> = partial.iter().map(|p| p.1).collect();
                let w = (wynn(&re[24..]), wynn(&im[24..]));
                // an alternating part with a slowly converging monotone one: Richardson on the even partial sums
                let r = crate::some!(even_partials(sx, f, k, lo, log, &partial)?);
                let close = (w.0 - r.0).abs() + (w.1 - r.1).abs() <= 1e-11 * (w.0.abs() + w.1.abs()).max(1e-300);
                return Ok(Some(if close { w } else { r }));
            }
            if r > 0.9 && (r - r32 > 1e-4 || r > 0.9999) {
                return slow(sx, f, k, lo, log);
            }
        }
        last = t;
        j += 1;
    }
    Ok(Some(s))
}

/// A slowly converging series by Euler-Maclaurin: the first 40 terms, then the integral of the tail from `N` (by
/// `n_integrate`) with `f(N)/2 - f'(N)/12 + f3(N)/720 - f5(N)/30240` (`f3`, `f5` the third and fifth derivatives).
/// Richardson extrapolation of the partial sums when the integral is not available.
fn slow<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, log: bool) -> Res<Option<(f64, f64)>> {
    let n0 = lo + 40;
    let mut s = (0.0, 0.0);
    for j in lo..n0 {
        sx.cx.tick(1)?;
        let t = crate::some!(term(sx, f, k, j, log)?);
        s = (s.0 + t.0, s.1 + t.1);
    }
    let g = if log { sx.ev("log", &[f])? } else { f };
    let x = sx.sym("numsum$x");
    let gx = sx.subst(g, k, x)?;
    let nr = sx.h().real(n0 as f64);
    // the tail integral: the closed form when there is one, else the integral in ln k (series::tail_integral)
    if let Some(iv) = crate::series::tail_integral(sx, g, k, n0)? {
        if iv.0.is_finite() && iv.1.is_finite() {
            let mut acc = (s.0 + iv.0, s.1 + iv.1);
            let g0t = sx.subst(gx, x, nr)?;
            let g0 = crate::some!(value(sx, g0t)?);
            acc = (acc.0 + g0.0 / 2.0, acc.1 + g0.1 / 2.0);
            let mut d = gx;
            for (times, c) in [(1, -1.0 / 12.0), (2, 1.0 / 720.0), (2, -1.0 / 30240.0)] {
                let sp = sx.list(&[x, sx.int(times)]);
                d = sx.ev("d", &[d, sp])?;
                let dv = sx.subst(d, x, nr)?;
                let Some(v) = value(sx, dv)? else { return Ok(Some(acc)) };
                acc = (acc.0 + c * v.0, acc.1 + c * v.1);
            }
            return Ok(Some(acc));
        }
    }
    let mut s = (0.0, 0.0);
    let mut marks = vec![];
    let mut next = 16i64;
    let mut j = lo;
    while marks.len() < 8 {
        sx.cx.tick(1)?;
        let t = crate::some!(term(sx, f, k, j, log)?);
        s = (s.0 + t.0, s.1 + t.1);
        if j - lo + 1 == next {
            marks.push(s);
            next *= 2;
        }
        j += 1;
    }
    let re: Vec<f64> = marks.iter().map(|p| p.0).collect();
    let im: Vec<f64> = marks.iter().map(|p| p.1).collect();
    // Richardson extrapolation assumes an error series in powers of 1/N: the estimate must not move when the first
    // mark is left out (a tail like N^-0.01 or 1/ln N gives numbers 10% off that do not agree)
    let (a, b) = ((richardson(&re), richardson(&im)), (richardson(&re[1..]), richardson(&im[1..])));
    if (a.0 - b.0).abs() + (a.1 - b.1).abs() > 1e-6 * (a.0.abs() + a.1.abs()).max(1e-300) {
        sx.verdict = Verdict::Slow;
        return Ok(None);
    }
    Ok(Some(a))
}

/// Richardson extrapolation of the partial sums `S(2N)`, `N = 8, 16, ..., 1024` (the first `known` reused).
fn even_partials<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, log: bool, known: &[(f64, f64)]) -> Res<Option<(f64, f64)>> {
    let mut marks = vec![];
    let mut s = *known.last().unwrap_or(&(0.0, 0.0));
    for (i, p) in known.iter().enumerate() {
        if [16usize, 32, 64].contains(&(i + 1)) {
            marks.push(*p);
        }
    }
    let mut j = lo + known.len() as i64;
    let mut next = 128i64;
    while marks.len() < 8 {
        sx.cx.tick(1)?;
        let t = crate::some!(term(sx, f, k, j, log)?);
        s = (s.0 + t.0, s.1 + t.1);
        if j - lo + 1 == next {
            marks.push(s);
            next *= 2;
        }
        j += 1;
    }
    let re: Vec<f64> = marks.iter().map(|p| p.0).collect();
    let im: Vec<f64> = marks.iter().map(|p| p.1).collect();
    Ok(Some((richardson(&re), richardson(&im))))
}

/// The term of a machine complex number.
pub fn number<'h>(sx: &mut Sx<'_, '_, '_, 'h>, v: (f64, f64)) -> Res<T<'h>> {
    let h = sx.h();
    // an overflowed real is the symbol inf (as n(exp(1000)) is)
    if v.1 == 0.0 && v.0.is_infinite() {
        let inf = sx.sym("inf");
        return if v.0 > 0.0 { Ok(inf) } else { sx.mul(&[sx.int(-1), inf]) };
    }
    if v.1 == 0.0 || v.1.abs() < 1e-15 * v.0.abs() {
        return Ok(h.real(v.0));
    }
    let im = sx.ev("sqrt", &[sx.int(-1)])?;
    let b = sx.mul(&[h.real(v.1), im])?;
    sx.add(&[h.real(v.0), b])
}

/// Which fast loop to run over a finite range.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// the sum of the terms
    Sum,
    /// the sum of the (principal) logarithms of the terms
    Log,
    /// the product of the terms
    Prod,
}

/// A machine function of `k` compiled from the summand (the policy `Compiled`), for loops of many terms.
fn fast_fn<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>) -> Res<Option<Box<dyn NumFn<'h, f64> + 'h>>> {
    let View::Sym(s) = sx.h().view(k) else { return Ok(None) };
    let opts = mc_numfn::Options { policy: mc_numfn::Policy::Compiled, ..mc_numfn::Options::default() };
    Ok(Some(mc_numfn::compile_expr(sx.cx, f, &[Var::scalar(s)], &opts)?))
}

/// The sum, the sum of the logarithms, or the product of the real terms `f(lo) .. f(hi)` with the summand compiled
/// (some 20 ns a term against 10 us for the evaluator); `None` when a term is not a finite real (the caller then
/// takes the general loop), or a product leaves the range of machine reals. Neumaier summation.
fn fast_loop<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, hi: i64, mode: Mode) -> Res<Option<(f64, f64)>> {
    let Some(mut nf) = fast_fn(sx, f, k)? else { return Ok(None) };
    let mut out = [0.0f64];
    let (mut s, mut c, mut z, mut negs) = (0.0f64, 0.0f64, 1.0f64, 0u64);
    for j in lo..=hi {
        if !nf.eval(sx.cx, &[j as f64], &mut out)? {
            return Ok(None);
        }
        let t = out[0];
        if mode == Mode::Prod {
            if t == 0.0 {
                return Ok(Some((0.0, 0.0)));
            }
            z *= t;
            if !z.is_finite() || z == 0.0 {
                return Ok(None);
            }
            continue;
        }
        let t = if mode == Mode::Log {
            if t == 0.0 {
                return Ok(None);
            }
            negs += (t < 0.0) as u64;
            t.abs().ln()
        } else {
            t
        };
        let sum = s + t;
        c += if s.abs() >= t.abs() { (s - sum) + t } else { (t - sum) + s };
        s = sum;
    }
    Ok(Some(match mode {
        Mode::Prod => (z, 0.0),
        Mode::Sum => (s + c, 0.0),
        Mode::Log => (s + c, if negs % 2 == 1 { std::f64::consts::PI } else { 0.0 }),
    }))
}

/// The machine value of a real numeric bound.
fn bound_value<'h>(sx: &mut Sx<'_, '_, '_, 'h>, b: T<'h>) -> Res<Option<f64>> {
    Ok(match sx.h().view(b) {
        View::Int(v) => Some(v as f64),
        View::Real(v) => Some(v),
        _ => match value(sx, b)? {
            Some((re, 0.0)) => Some(re),
            _ => None,
        },
    })
}

/// Is `t` minus infinity (`-inf`, written `times(-1, inf)`)?
fn is_minus_inf<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    match sx.args_of(t, names::MUL) {
        Some(&[a, b]) => b == sx.sym("inf") && matches!(sx.h().view(a), View::Int(-1)),
        _ => false,
    }
}

/// The numeric sum or product `op(f, [k, lo, hi])`; `None` when the bounds are not numbers or the terms not
/// numeric. The bounds may be non-integers (the terms are `f(lo), f(lo + 1), ...` up to `hi`: the summand is shifted
/// to an integer index) and `lo` may be `-inf` (a sum over the integers up to `hi`: two one-sided sums).
pub fn numeric<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, hi: T<'h>, product: bool) -> Res<Option<T<'h>>> {
    let inf = sx.sym("inf");
    if is_minus_inf(sx, lo) {
        // k = -m: the terms k >= 0 (to infinity) and the mirrored ones m >= 1; below a finite top t, m >= -t
        let neg_k = sx.mul(&[sx.int(-1), k])?;
        let fm = sx.subst(f, k, neg_k)?;
        let (first, from) = if hi == inf {
            (numeric(sx, f, k, sx.int(0), inf, product)?, 1)
        } else {
            let t = crate::some!(bound_value(sx, hi)?);
            if !t.is_finite() || t.abs() > 1e15 {
                return Ok(None);
            }
            (Some(sx.int(if product { 1 } else { 0 })), -(t.floor() as i64))
        };
        let second = numeric(sx, fm, k, sx.int(from), inf, product)?;
        let (Some(a), Some(b)) = (first, second) else { return Ok(None) };
        return Ok(Some(if product { sx.mul(&[a, b])? } else { sx.add(&[a, b])? }));
    }
    // lo = lo0 + frac with lo0 an integer: k = m + frac, m from lo0 to the last integer with m + frac <= hi
    let lov = crate::some!(bound_value(sx, lo)?);
    if !lov.is_finite() || lov.abs() > 1e15 {
        return Ok(None);
    }
    let frac = lov - lov.floor();
    let mut f = f;
    if frac != 0.0 {
        let shifted = sx.add(&[k, sx.h().real(frac)])?;
        f = sx.subst(f, k, shifted)?;
    }
    let hi = if hi == inf {
        None
    } else {
        let hv = crate::some!(bound_value(sx, hi)?);
        if !hv.is_finite() || hv.abs() > 1e15 {
            return Ok(None);
        }
        Some((hv - frac).floor() as i64)
    };
    numeric_int(sx, f, k, lov.floor() as i64, hi, product)
}

/// [`numeric`] with integer bounds (`hi = None`: infinity).
fn numeric_int<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, hi: Option<i64>, product: bool) -> Res<Option<T<'h>>> {
    // a summand that is not a number at the first index (a free symbol): n_sum cannot sum it
    {
        let t = sx.subst(f, k, sx.int(lo))?;
        match value(sx, t)? {
            None => {
                sx.verdict = Verdict::NotNumeric;
                return Ok(None);
            }
            // a product with a zero factor is zero (1 - 1/k at k = 1)
            Some((re, im)) if product && re == 0.0 && im == 0.0 => return Ok(Some(number(sx, (0.0, 0.0))?)),
            _ => {}
        }
    }
    // a long finite range with the summand compiled: the product or sum of the terms themselves (a finite range is
    // summed as it is, where the tails of an infinite series lose digits to their difference)
    if let Some(u) = hi {
        if u - lo > 2_000 && u - lo <= MAX_DIRECT {
            if let Some(v) = fast_loop(sx, f, k, lo, u, if product { Mode::Prod } else { Mode::Sum })? {
                return Ok(Some(number(sx, v)?));
            }
            if product {
                if let Some(v) = fast_loop(sx, f, k, lo, u, Mode::Log)? {
                    return Ok(Some(number(sx, polar(v.0, v.1))?));
                }
            }
        }
    }
    let v = match hi {
        Some(u) if u < lo => (0.0, 0.0),
        // a finite product of numbers: multiplied (an exact integer product such as 10! is then exact), the logarithms
        // only when that overflows or vanishes
        Some(u) if product && u - lo <= MAX_TERMS => match multiply(sx, f, k, lo, u)? {
            Some(z) => return Ok(Some(number(sx, z)?)),
            None => crate::some!(sum_values(sx, f, k, lo, hi, product)?),
        },
        // a long finite range: the difference of two tails when the series converges; a divergent (or too slowly
        // convergent) one is summed term by term with the summand compiled (the difference of two divergent tails is
        // no number: n_product(1 + (-1)^k/sqrt(k), [k, 2, 10^6]) is not 0.0212, B23-8)
        Some(u) if u - lo > 20_000 => {
            let tails = match sum_values(sx, f, k, lo, None, product)? {
                Some(a) => sum_values(sx, f, k, u + 1, None, product)?.map(|b| (a.0 - b.0, a.1 - b.1)),
                None => None,
            };
            match tails {
                Some(v) if sx.verdict == Verdict::Open => v,
                _ if u - lo <= MAX_FAST => {
                    sx.verdict = Verdict::Open;
                    if product {
                        if let Some(z) = fast_loop(sx, f, k, lo, u, Mode::Prod)? {
                            return Ok(Some(number(sx, z)?));
                        }
                    }
                    if let Some(v) = fast_loop(sx, f, k, lo, u, if product { Mode::Log } else { Mode::Sum })? {
                        v
                    } else if u - lo <= MAX_FINITE {
                        crate::some!(add_up(sx, f, k, lo, u, product)?)
                    } else {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            }
        }
        _ => crate::some!(sum_values(sx, f, k, lo, hi, product)?),
    };
    if product {
        return Ok(Some(number(sx, polar(v.0, v.1))?));
    }
    Ok(Some(number(sx, v)?))
}

// ---------------------------------------------------------------- arbitrary precision (the precision contract)

/// The least ball precision (in whole decimal digits, at least 17) among the numbers of `ts`, when there is a ball
/// and no machine real (precision contagion: what `n(sum(...), d)` leaves of a sum without a closed form).
pub fn ball_digits<'h>(sx: &Sx<'_, '_, '_, 'h>, ts: &[T<'h>]) -> Option<u64> {
    let h = sx.h();
    let mut bits: Option<u64> = None;
    let mut stack: Vec<T<'h>> = ts.to_vec();
    let mut seen = 0usize;
    while let Some(u) = stack.pop() {
        seen += 1;
        if seen > 100_000 {
            return None;
        }
        match h.view(u) {
            View::Real(_) => return None,
            View::Ball { .. } => {
                if let Some(mc_arith::Num::Real(mc_arith::Real::Ball(_, p))) = sx.ar.num(u) {
                    bits = Some(bits.map_or(p, |b| b.min(p)));
                }
            }
            View::App { head, args } => {
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    let d = (bits? as f64 * std::f64::consts::LOG10_2).floor() as u64;
    (d > 16).then_some(d)
}

/// Is `t` a number (exact or a ball, real or complex)?
fn is_number<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    sx.ar.num(t).is_some()
}

/// `sum_{k=lo}^{N-1} f(k)` with each term at `w` digits; `None` when a term is not a number.
fn direct<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, n: i64, w: u64) -> Res<Option<T<'h>>> {
    let mut parts = Vec::with_capacity((n - lo).max(0) as usize);
    for j in lo..n {
        sx.cx.tick(1)?;
        let t = sx.subst(f, k, sx.int(j))?;
        let v = sx.ev("n", &[t, sx.int(w as i64)])?;
        if !is_number(sx, v) {
            return Ok(None);
        }
        parts.push(v);
    }
    Ok(Some(sx.add(&parts)?))
}

/// The tail `sum_{k>=N} f(k)` by the Abel-Plana formula (Olver 1974, ch. 8 section 3; DLMF 2.10.2):
/// `int_N^inf f + f(N)/2 + i int_0^inf (f(N + i t) - f(N - i t)) / (e^(2 pi t) - 1) dt`, both integrals by
/// `n_integrate` at `w` digits. It needs f analytic for Re z >= N and o(e^(2 pi |Im z|)) there.
fn abel_plana<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, n: i64, w: u64) -> Res<Option<T<'h>>> {
    let wp = sx.call(names::RULE, &[sx.sym("working_precision"), sx.int(w as i64)]);
    let opts = sx.call("options", &[wp]);
    let inf = sx.sym("inf");
    let nn = sx.int(n);
    let spec = sx.list(&[k, nn, inf]);
    // the tail integral: in closed form when one is found quickly (powers such as k^(-51/50), on which the
    // double-exponential rule is slow), else numerically; each step under a time limit, so a hard summand falls back
    // to machine precision instead of running long
    let exact = sx.call("integrate", &[f, spec]);
    let ne = sx.call("n", &[exact, sx.int(w as i64)]);
    let i1 = sx.ev("time_constrained", &[ne, sx.int(2)])?;
    let i1 = if is_number(sx, i1) {
        i1
    } else {
        let ni = sx.call("n_integrate", &[f, spec, opts]);
        sx.ev("time_constrained", &[ni, sx.int(10)])?
    };
    if !is_number(sx, i1) {
        return Ok(None);
    }
    let fn0 = sx.subst(f, k, nn)?;
    let fnn = sx.ev("n", &[fn0, sx.int(w as i64)])?;
    if !is_number(sx, fnn) {
        return Ok(None);
    }
    let half = sx.div(fnn, sx.int(2))?;
    // g(t) = i (f(N + i t) - f(N - i t)) / (exp(2 pi t) - 1), in a fresh variable
    let t = sx.sym("numsum$t");
    let im = sx.sym("im");
    let it = sx.mul(&[im, t])?;
    let up = sx.add(&[nn, it])?;
    let mit = sx.mul(&[sx.int(-1), it])?;
    let down = sx.add(&[nn, mit])?;
    let fu = sx.subst(f, k, up)?;
    let fd = sx.subst(f, k, down)?;
    let diff = sx.sub(fu, fd)?;
    let pi = sx.sym("pi");
    let arg = sx.mul(&[sx.int(2), pi, t])?;
    // e^(2 pi t) - 1 without cancellation near t = 0
    let den = sx.ev("expm1", &[arg])?;
    let num = sx.mul(&[im, diff])?;
    let g = sx.div(num, den)?;
    // the integrand falls like e^(-2 pi t): the range [0, T] with e^(-2 pi T) below 10^-(w + 4), in unit pieces
    // by the tanh-sinh rule (the poles of 1/(e^(2 pi t) - 1) at t = i n lie on the edge of the double-exponential
    // strip of a half-line, which makes that rule unreliable beyond about 50 digits; a unit piece keeps them at
    // twice its half-length)
    let big = (((w + 4) as f64) * std::f64::consts::LN_10 / (2.0 * std::f64::consts::PI)).ceil() as i64 + 1;
    let mut ends = vec![t];
    for j in 0..=big {
        ends.push(sx.int(j));
    }
    let spec2 = sx.list(&ends);
    let ni2 = sx.call("n_integrate", &[g, spec2, opts]);
    let i2 = sx.ev("time_constrained", &[ni2, sx.int(20)])?;
    if !is_number(sx, i2) {
        return Ok(None);
    }

    Ok(Some(sx.add(&[i1, half, i2])?))
}

/// The tail `sum_{k>=N} f(k)` by the Euler-Maclaurin formula (Olver 1974, ch. 8 section 1; DLMF 2.10.1):
/// `int_N^inf f + f(N)/2 - sum_j B_(2j)/(2j)! f^(2j-1)(N)`, an asymptotic series whose terms fall until `2 j` is about
/// `2 pi N`. The derivatives need no symbolic differentiation: with `c_m` the Taylor coefficients of f at N, which
/// are the trapezoid sums of f on the circle of radius `r = N/4` (`m! c_m = f^(m)(N)`, aliasing error about
/// `4^-M` for f analytic in |z - N| < N), the correction is `(1/M) sum_j f(N + r z_j) W(z_j^-1 / r)` with
/// `W(q) = sum_(m odd) B_(m+1)/(m+1) q^m` and `z_j = exp(2 pi i j/M)`. Only M values of f are needed, whatever the
/// order. The correction is computed with `J` and with `J - 6` terms, which must agree to `10^-w`; the integral is
/// the closed form when `integrate` finds one quickly (`cos(k)/k`: `-Ci(N)`). `None` when f is not a number on the
/// circle or the series has not converged by `J` (the caller falls back to the Abel-Plana tail).
fn euler_maclaurin<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, n: i64, w: u64) -> Res<Option<T<'h>>> {
    let wp = w + 40 + (w as f64 * 0.65).ceil() as u64;
    let wpi = sx.int(wp as i64);
    let inf = sx.sym("inf");
    let nn = sx.int(n);
    let spec = sx.list(&[k, nn, inf]);
    let exact = sx.call("integrate", &[f, spec]);
    let ne = sx.call("n", &[exact, wpi]);
    let i1 = sx.ev("time_constrained", &[ne, sx.int(2)])?;
    if !is_number(sx, i1) || sx.contains_any(i1, &["integrate", "failure"]) {
        return Ok(None);
    }
    let fn0 = sx.subst(f, k, nn)?;
    let fnn = sx.ev("n", &[fn0, wpi])?;
    if !is_number(sx, fnn) {
        return Ok(None);
    }
    let real = real_summand(sx, f);
    // M odd, so that for a real f (f(conj z) = conj f(z)) half the circle is enough
    let m = (((w + 12) as f64 / 0.6).ceil() as usize + 8) | 1;
    let jmax = (w as f64 * 0.65).ceil() as usize + 12;
    let im = sx.sym("im");
    let pi = sx.sym("pi");
    // z[t] = exp(2 pi i t / M) at working precision
    let mut z = Vec::with_capacity(m);
    let inv_m = sx.pow(sx.int(m as i64), sx.int(-1))?;
    for t in 0..m {
        sx.cx.tick(1)?;
        let a = sx.mul(&[sx.int(2 * t as i64), pi, im, inv_m])?;
        let e = sx.ev("exp", &[a])?;
        let v = sx.ev("n", &[e, wpi])?;
        if !is_number(sx, v) {
            return Ok(None);
        }
        z.push(v);
    }
    let half = sx.div(nn, sx.int(4))?;
    // the coefficients B_(m+1)/(m+1) of W at odd m, as numbers: with 2n = m + 1, B_(2n) = (-1)^(n+1) 2 (2n)! zeta(2n)
    // / (2 pi)^(2n) (DLMF 24.12.2)
    let mut beta = Vec::with_capacity(jmax);
    let two_pi = sx.mul(&[sx.int(2), pi])?;
    for i in 0..jmax {
        let n2 = 2 * (i as i64 + 1);
        let zt = sx.ev("zeta", &[sx.int(n2)])?;
        let fact = sx.ev("factorial", &[sx.int(n2 - 1)])?;
        let den = sx.pow(two_pi, sx.int(-n2))?;
        let sign = if i % 2 == 0 { 2 } else { -2 };
        let c = sx.mul(&[sx.int(sign), fact, zt, den])?;
        let v = sx.ev("n", &[c, wpi])?;
        if !is_number(sx, v) {
            return Ok(None);
        }
        beta.push(v);
    }
    let inv_r = sx.div(sx.int(4), nn)?;
    let inv_r = sx.ev("n", &[inv_r, wpi])?;
    let inv_r2 = sx.mul(&[inv_r, inv_r])?;
    let (mut e_full, mut e_short) = (sx.int(0), sx.int(0));
    let last = if real { (m - 1) / 2 } else { m - 1 };
    for j in 0..=last {
        sx.cx.tick(1)?;
        // f(N + r z_j)
        let zj = sx.mul(&[half, z[j]])?;
        let pt = sx.add(&[nn, zj])?;
        let fv0 = sx.subst(f, k, pt)?;
        let fv = sx.ev("n", &[fv0, wpi])?;
        if !is_number(sx, fv) {
            return Ok(None);
        }
        // q = z_j^-1 / r, y = q^2; W = q P(y)
        let q = sx.mul(&[z[(m - j) % m], inv_r])?;
        let y = sx.mul(&[z[(2 * (m - j)) % m], inv_r2])?;
        for (len, acc) in [(jmax, &mut e_full), (jmax - 6, &mut e_short)] {
            let mut p = sx.int(0);
            for b in beta[..len].iter().rev() {
                let py = sx.mul(&[p, y])?;
                p = sx.add(&[py, *b])?;
            }
            let wj = sx.mul(&[q, p])?;
            let mut term = sx.mul(&[fv, wj])?;
            if real {
                // the conjugate half of the circle doubles the real part
                term = sx.ev("re", &[term])?;
                if j > 0 {
                    term = sx.mul(&[sx.int(2), term])?;
                }
            }
            *acc = sx.add(&[*acc, term])?;
        }
    }
    let mi = sx.int(m as i64);
    let e_full = sx.div(e_full, mi)?;
    let e_short = sx.div(e_short, mi)?;
    let diff = sx.sub(e_full, e_short)?;
    let ad = size(sx, diff)?;
    let tol = sx.pow(sx.int(10), sx.int(-(w as i64) - 2))?;
    let scale = size(sx, i1)?;
    let scale = sx.add(&[scale, sx.int(1)])?;
    let bound = sx.mul(&[tol, scale])?;
    let ok = sx.ev("less_equal", &[ad, bound])?;
    if !sx.is_true(ok) {
        return Ok(None);
    }
    let fh = sx.div(fnn, sx.int(2))?;
    let t = sx.sub(fh, e_full)?;
    Ok(Some(sx.add(&[i1, t])?))
}

/// `sum_{k>=lo} f(k)` at `w` digits by the Euler-Maclaurin tail from N = lo + 1.2 w + 16, checked against N + 8
/// (they must agree to `d` digits); `None` when either tail declines.
fn euler_maclaurin_sum<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, w: u64, d: u64) -> Res<Option<T<'h>>> {
    let n1 = lo + (w as f64 * 1.2).ceil() as i64 + 16;
    let mut vals = Vec::new();
    for n in [n1, n1 + 8] {
        let head = crate::some!(direct(sx, f, k, lo, n, w + 40)?);
        let tail = crate::some!(euler_maclaurin(sx, f, k, n, w)?);
        vals.push(sx.add(&[head, tail])?);
    }
    if !agree(sx, vals[0], vals[1], d, w)? {
        return Ok(None);
    }
    Ok(Some(vals[1]))
}

/// |re t| + |im t| (abs of a complex ball near the real axis may not decide).
fn size<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let r = sx.ev("re", &[t])?;
    let i = sx.ev("im_part", &[t])?;
    let ar = sx.ev("abs", &[r])?;
    let ai = sx.ev("abs", &[i])?;
    sx.add(&[ar, ai])
}

/// Do two estimates agree to `d` digits (or to 10^-w absolutely)?
fn agree<'h>(sx: &mut Sx<'_, '_, '_, 'h>, a: T<'h>, b: T<'h>, d: u64, w: u64) -> Res<bool> {
    let diff = sx.sub(a, b)?;
    let ad = size(sx, diff)?;
    let aa = size(sx, a)?;
    let rel = sx.pow(sx.int(10), sx.int(-(d as i64)))?;
    let bound = sx.mul(&[aa, rel])?;
    let abs_tol = sx.pow(sx.int(10), sx.int(-(w as i64)))?;
    let bound = sx.add(&[bound, abs_tol])?;
    let ok = sx.ev("less_equal", &[ad, bound])?;
    Ok(sx.is_true(ok))
}

/// Do the real terms alternate in sign with decreasing magnitudes over k = lo .. lo + 24 (machine arithmetic)?
fn alternating<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64) -> Res<bool> {
    let mut prev: Option<f64> = None;
    for j in lo..lo + 24 {
        let Some((re, im)) = term(sx, f, k, j, false)? else { return Ok(false) };
        if im != 0.0 || !re.is_finite() || re == 0.0 {
            return Ok(false);
        }
        if let Some(p) = prev {
            if (p < 0.0) == (re < 0.0) || re.abs() > p.abs() * (1.0 + 1e-12) {
                return Ok(false);
            }
        }
        prev = Some(re);
    }
    Ok(true)
}

/// sum_{k>=0} f(lo + k) for alternating terms by Algorithm 1 of Cohen, Rodriguez Villegas and Zagier, "Convergence
/// acceleration of alternating series" (Experimental Math. 9, 2000), with n terms: a_k = (-1)^k f(lo + k),
/// d = T_n(3) (the Chebyshev polynomial, an integer), b = -1, c = -d, then for k < n: c = b - c,
/// s = s + c a_k, b = b (k + n)(k - n) / ((k + 1/2)(k + 1)); the sum is s/d (error about 5.83^-n for totally
/// monotone a_k). The weights are exact rationals.
fn cvz<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, n: i64, w: u64) -> Res<Option<T<'h>>> {
    // T_n(3) by T_(j+1) = 6 T_j - T_(j-1)
    let (mut t0, mut t1) = (sx.int(1), sx.int(3));
    for _ in 1..n {
        let six = sx.mul(&[sx.int(6), t1])?;
        let nt0 = sx.mul(&[sx.int(-1), t0])?;
        let t2 = sx.add(&[six, nt0])?;
        t0 = t1;
        t1 = t2;
    }
    let d = t1;
    let mut b = sx.int(-1);
    let mut c = sx.mul(&[sx.int(-1), d])?;
    let mut s = sx.int(0);
    for j in 0..n {
        sx.cx.tick(1)?;
        c = sx.sub(b, c)?;
        let t = sx.subst(f, k, sx.int(lo + j))?;
        let t = if j % 2 == 1 { sx.mul(&[sx.int(-1), t])? } else { t };
        let a = sx.ev("n", &[t, sx.int(w as i64)])?;
        if !is_number(sx, a) {
            return Ok(None);
        }
        let ca = sx.mul(&[c, a])?;
        s = sx.add(&[s, ca])?;
        // b (j + n)(j - n) / ((j + 1/2)(j + 1)) = b 2 (j + n)(j - n) / ((2j + 1)(j + 1))
        let num = sx.int(2 * (j + n) * (j - n));
        let den = sx.int((2 * j + 1) * (j + 1));
        let q = sx.div(num, den)?;
        b = sx.mul(&[b, q])?;
    }
    Ok(Some(sx.div(s, d)?))
}

/// Do the terms fall at least geometrically: |f(k + 1)/f(k)| < 0.9 in machine arithmetic at k = lo + 40 and
/// lo + 41?
fn geometric<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64) -> Res<bool> {
    let mut mags = [0.0f64; 3];
    for (i, m) in mags.iter_mut().enumerate() {
        let Some((re, im)) = term(sx, f, k, lo + 40 + i as i64, false)? else { return Ok(false) };
        *m = (re * re + im * im).sqrt();
    }
    // terms that have already vanished in machine arithmetic (exp(-k^5)) fall faster than any geometric series
    if mags[0] == 0.0 && mags[1] == 0.0 && mags[2] == 0.0 {
        return Ok(true);
    }
    Ok(mags.iter().all(|m| m.is_finite() && *m > 0.0) && mags[1] < 0.9 * mags[0] && mags[2] < 0.9 * mags[1])
}

/// `sum_{k>=lo} f(k)` term by term at `w` digits until three terms in a row are below 10^-w of the partial sum
/// (at most 100000 terms).
fn until_small<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, w: u64) -> Res<Option<T<'h>>> {
    let mut s = sx.int(0);
    let mut small = 0;
    let tol = sx.pow(sx.int(10), sx.int(-(w as i64)))?;
    for j in lo..lo + MAX_TERMS {
        sx.cx.tick(1)?;
        let t = sx.subst(f, k, sx.int(j))?;
        let v = sx.ev("n", &[t, sx.int(w as i64)])?;
        if !is_number(sx, v) {
            return Ok(None);
        }
        s = sx.add(&[s, v])?;
        let av = sx.ev("abs", &[v])?;
        let asum = sx.ev("abs", &[s])?;
        let bound = sx.mul(&[asum, tol])?;
        let c = sx.ev("less_equal", &[av, bound])?;
        if sx.is_true(c) {
            small += 1;
            if small >= 3 {
                return Ok(Some(s));
            }
        } else {
            small = 0;
        }
    }
    Ok(None)
}

/// Does `f` contain no complex number and no `im`?
fn real_summand<'h>(sx: &Sx<'_, '_, '_, 'h>, f: T<'h>) -> bool {
    let h = sx.h();
    let im = sx.sym("im");
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if u == im || matches!(sx.ar.num(u), Some(mc_arith::Num::Complex(..))) {
            return false;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    true
}

/// `sum_{k>=lo} f(k)` at `w` digits: terms up to N directly, the tail by Abel-Plana.
fn tail_sum<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, n: i64, w: u64) -> Res<Option<T<'h>>> {
    let head = crate::some!(direct(sx, f, k, lo, n, w)?);
    let tail = crate::some!(abel_plana(sx, f, k, n, w)?);
    Ok(Some(sx.add(&[head, tail])?))
}

/// The symbolic `sum` or `product` of an exact summand, when it has a closed form that `n` evaluates at `d`
/// digits (`n_sum(1/k^2, ..., working_precision: 30)` is `n(pi^2/6, 30)`, instantly): tried under a 2 s limit before
/// the numerical methods.
fn closed_form<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, hi: T<'h>, product: bool, d: u64) -> Res<Option<T<'h>>> {
    if inexact(sx, f) || inexact(sx, lo) || inexact(sx, hi) {
        return Ok(None);
    }
    let spec = sx.list(&[k, lo, hi]);
    let call = sx.call(if product { "product" } else { "sum" }, &[f, spec]);
    let r = sx.ev("time_constrained", &[call, sx.int(2)])?;
    if sx.contains_any(r, &["sum", "product", "failure", "time_constrained", "integrate", "indeterminate", "complex_infinity", "inf", "undefined"]) {
        return Ok(None);
    }
    let mut v = sx.ev("n", &[r, sx.int(d as i64)])?;
    if !is_number(sx, v) {
        return Ok(None);
    }
    // a real summand: an imaginary part that is a zero ball is rounding
    if real_summand(sx, f) {
        let i = sx.ev("im_part", &[v])?;
        let ai = sx.ev("abs", &[i])?;
        let tol = sx.pow(sx.int(10), sx.int(-(d as i64)))?;
        let small = sx.ev("less_equal", &[ai, tol])?;
        if sx.is_true(small) {
            v = sx.ev("re", &[v])?;
        }
    }
    Ok(Some(v))
}

/// A numeric sum or product at `d` digits (the `working_precision:` of `n_sum` and `n_product`, or the precision of
/// the balls in an unevaluated sum): finite ranges term by term, infinite ones by the Abel-Plana formula for the
/// tail from N = lo + 16, checked against N = lo + 24 (the two must agree to d digits; this rejects summands that
/// are not analytic, such as those with `floor` or `mod`). Products as the exponential of the sum of logarithms.
/// `None` when the method does not apply; the caller falls back to machine precision.
#[allow(clippy::too_many_arguments)]
pub fn numeric_digits<'h>(
    sx: &mut Sx<'_, '_, '_, 'h>,
    f: T<'h>,
    k: T<'h>,
    lo: T<'h>,
    hi: T<'h>,
    product: bool,
    d: u64,
) -> Res<Option<T<'h>>> {
    let h = sx.h();
    let int_of = |t: T<'h>| match h.view(t) {
        View::Int(v) => Some(v),
        View::Real(v) if v.fract() == 0.0 && v.abs() < 1e15 => Some(v as i64),
        _ => None,
    };
    // an integer, or a ball bound with an integer value
    let bound = |sx: &Sx<'_, '_, '_, 'h>, t: T<'h>| {
        int_of(t).or_else(|| match sx.ar.num(t) {
            Some(mc_arith::Num::Real(r)) => {
                let v = r.to_f64();
                (v.fract() == 0.0 && v.abs() < 1e15).then_some(v as i64)
            }
            _ => None,
        })
    };
    let Some(lo) = bound(sx, lo) else { return Ok(None) };
    let inf = sx.sym("inf");
    let hi = if hi == inf {
        None
    } else {
        match bound(sx, hi) {
            Some(v) => Some(v),
            None => return Ok(None),
        }
    };
    if let Some(v) = closed_form(sx, f, k, sx.int(lo), if let Some(u) = hi { sx.int(u) } else { inf }, product, d)? {
        return Ok(Some(v));
    }
    let w = d + 15;
    // exp(u) is written pow(e, u)
    let exponent = if product {
        sx.args_of(f, "exp").filter(|a| a.len() == 1).map(|a| a[0]).or_else(|| sx.args_of(f, names::POW).filter(|a| a.len() == 2 && a[0] == sx.sym("e")).map(|a| a[1]))
    } else {
        None
    };
    let g = if let Some(u) = exponent {
        // a product of exponentials is the exponential of the sum of the exponents (whatever the branch of the logarithm)
        u
    } else if product {
        // log(exp(u)) and log of a power: simplified under a short limit (the summand of a product of exponentials)
        let l = sx.call("log", &[f]);
        let sm = sx.call("simplify", &[l]);
        let r = sx.ev("time_constrained", &[sm, sx.int(1)])?;
        if sx.contains_any(r, &["failure", "time_constrained"]) { sx.ev("log", &[f])? } else { r }
    } else {
        f
    };
    if hi.is_none() && matches!(series::classify(sx, g, k, lo, false)?, Class::Diverges | Class::Slow) {
        return Ok(None);
    }
    // a product of exponentials (exp(u_k)): exp of the sum of the exponents, which has its own closed form
    if product && g != f && !inexact(sx, g) {
        let spec = sx.list(&[k, sx.int(lo), if let Some(u) = hi { sx.int(u) } else { inf }]);
        let wp = sx.call(names::RULE, &[sx.sym("working_precision"), sx.int(d as i64 + 10)]);
        let opts = sx.call("options", &[wp]);
        let call = sx.call("n_sum", &[g, spec, opts]);
        let sum = sx.ev("time_constrained", &[call, sx.int(20)])?;
        if is_number(sx, sum) {
            let e = sx.ev("exp", &[sum])?;
            let v = sx.ev("n", &[e, sx.int(d as i64)])?;
            if is_number(sx, v) {
                return Ok(Some(v));
            }
        }
    }
    let v = match hi {
        Some(u) if u < lo => sx.int(0),
        Some(u) if u - lo < 20_000 => crate::some!(direct(sx, g, k, lo, u + 1, w)?),
        // terms that fall geometrically (or faster): summed until they are below the working precision
        None if geometric(sx, g, k, lo)? || fast_decay(sx, g, k, lo, w)? => crate::some!(until_small(sx, g, k, lo, w)?),
        // an alternating series with decreasing terms (after a few leading ones that may rise first: log(k + 1)/sqrt(k)
        // peaks near k = 5): the leading terms added, the rest by the Cohen-Villegas-Zagier acceleration, checked by
        // a second run with more terms
        None if alt_offset(sx, g, k, lo)?.is_some() => {
            let m = alt_offset(sx, g, k, lo)?.unwrap_or(0);
            let head = if m > 0 { crate::some!(direct(sx, g, k, lo, lo + m, w)?) } else { sx.int(0) };
            let n1 = (1.31 * w as f64).ceil() as i64 + 4;
            let a = crate::some!(cvz(sx, g, k, lo + m, n1, w)?);
            let b = crate::some!(cvz(sx, g, k, lo + m, n1 + 12, w)?);
            if !agree(sx, a, b, d, w)? {
                return Ok(None);
            }
            sx.add(&[head, b])?
        }
        _ if hi.is_none() && !product => match euler_maclaurin_sum(sx, g, k, lo, w, d)? {
            Some(v) => v,
            None => {
                let a = crate::some!(tail_sum(sx, g, k, lo, lo + 16, w)?);
                let b = crate::some!(tail_sum(sx, g, k, lo, lo + 24, w)?);
                if !agree(sx, a, b, d, w)? {
                    return Ok(None);
                }
                a
            }
        },
        _ => {
            let a = crate::some!(tail_sum(sx, g, k, lo, lo + 16, w)?);
            let b = crate::some!(tail_sum(sx, g, k, lo, lo + 24, w)?);
            // the two splits agree to d digits
            if !agree(sx, a, b, d, w)? {
                return Ok(None);
            }
            match hi {
                None => a,
                Some(u) => {
                    let c = crate::some!(tail_sum(sx, g, k, u + 1, u + 17, w)?);
                    sx.sub(a, c)?
                }
            }
        }
    };
    let v = if product { sx.ev("exp", &[v])? } else { v };
    // a real summand: an imaginary part below the precision (from the complex integral of Abel-Plana) is rounding
    let v = if real_summand(sx, f) {
        let i = sx.ev("im_part", &[v])?;
        let ai = sx.ev("abs", &[i])?;
        let r = sx.ev("re", &[v])?;
        let ar = sx.ev("abs", &[r])?;
        let rel = sx.pow(sx.int(10), sx.int(-(w as i64) + 4))?;
        let tol = sx.mul(&[ar, rel])?;
        let small = sx.ev("less_equal", &[ai, tol])?;
        if sx.is_true(small) {
            r
        } else {
            v
        }
    } else {
        v
    };
    let r = sx.ev("n", &[v, sx.int(d as i64)])?;
    Ok(is_number(sx, r).then_some(r))
}

/// Is `b` an exact number that is not an integer (a rational such as 3/2, or a constant such as `pi`)? A sum over
/// `k = a, a + 1, ...` with such a bound has no closed form of the integer-bounded kind.
pub fn non_integer_bound<'h>(sx: &mut Sx<'_, '_, '_, 'h>, b: T<'h>) -> Res<bool> {
    if inexact(sx, b) || matches!(sx.h().view(b), View::Int(_)) || b == sx.sym("inf") {
        return Ok(false);
    }
    Ok(bound_value(sx, b)?.is_some_and(|v| v.is_finite() && v.fract() != 0.0))
}

/// The first of `0, 16, 32, 64, 128` at which the terms from `lo + m` alternate with falling magnitudes.
fn alt_offset<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64) -> Res<Option<i64>> {
    for m in [0, 16, 32, 64, 128] {
        if alternating(sx, f, k, lo + m)? {
            return Ok(Some(m));
        }
    }
    Ok(None)
}

/// Are the terms already below 10^-(w + 3) at `lo + 50000` and falling there (exp(-sqrt(k)): 1e-45 at k = 10^4)? Then
/// summing them term by term to the working precision is cheap (at most 10^5 terms).
fn fast_decay<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: i64, w: u64) -> Res<bool> {
    if w + 3 > 290 {
        return Ok(false);
    }
    let mut mags = [0.0f64; 3];
    for (m, off) in mags.iter_mut().zip([25_000i64, 37_500, 50_000]) {
        let Some((re, im)) = term(sx, f, k, lo + off, false)? else { return Ok(false) };
        *m = (re * re + im * im).sqrt();
    }
    Ok(mags.iter().all(|m| m.is_finite()) && mags[2] < 10f64.powi(-(w as i32 + 3)) && mags[2] <= mags[1] && mags[1] <= mags[0])
}

/// `exp(lr + i li)` as a machine complex number; a real product that overflows is `inf` (not `inf * 0`).
fn polar(lr: f64, li: f64) -> (f64, f64) {
    let r = lr.exp();
    if li == 0.0 {
        return (r, 0.0);
    }
    (r * li.cos(), r * li.sin())
}
