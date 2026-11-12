//! Oscillatory integrals over half-lines (decision quad-2.3): Longman's method (Longman 1956) with Wynn's
//! ε-algorithm (Wynn 1956) as QUADPACK's QAWF does it (Piessens et al. 1983), but with the half-periods found from
//! the integrand itself rather than from a given sin(ωx) or cos(ωx) weight, so chirps such as `exp(im x^2)` and
//! integrands such as `sin(x)/x` work alike.
//!
//! The zeros of the real part of f on [a, inf) (or (-inf, a]) are found by sampling at a step a fraction of the
//! last gap between zeros and refining each sign change by bisection; the integral between consecutive zeros is
//! computed by adaptive Gauss–Kronrod, and the partial sums are extrapolated by the ε-algorithm. The error
//! estimate is the change of the extrapolated value over the last terms, plus the errors of the terms. The search
//! is bounded: at most `MAX_TERMS` intervals and `MAX_SAMPLES` samples.
//!
//! Wynn's ε-algorithm also serves `epsilon`, the public sequence accelerator of this crate's tests.
// index loops over several parallel arrays read more plainly than zipped iterators here
#![allow(clippy::needless_range_loop)]
use crate::gk::{self, Limits};
use crate::machine::{Integrand, Map, Outcome, Piece};
use crate::options::Goals;
use mc_arith::Num;
use mc_eval::{NativeCx, Res};
use mc_numfn::Scalar;

/// The most intervals between zeros.
const MAX_TERMS: usize = 400;

/// The most samples of the zero search.
const MAX_SAMPLES: usize = 200_000;

/// The fewest terms before the extrapolation may be accepted.
const MIN_TERMS: usize = 8;

/// The real part of a scalar as a machine real.
fn re<S: Scalar>(v: S) -> f64 {
    match v.to_num() {
        Num::Real(r) => r.to_f64(),
        Num::Complex(r, _) => r.to_f64(),
    }
}

/// The ε-algorithm on the sequence `s`: the extrapolated limit and an error estimate (the distance between the two
/// last diagonal estimates); `None` for fewer than three terms.
pub(crate) fn epsilon<S: Scalar>(s: &[S]) -> Option<(S, f64)> {
    let n = s.len();
    if n < 3 {
        return None;
    }
    // e[k] holds column k for the current anti-diagonal; we build the table column by column
    let mut prev: Vec<S> = vec![S::real(0.0); n + 1]; // column -1 (zeros), length n+1
    let mut cur: Vec<S> = s.to_vec(); // column 0
    let mut estimates: Vec<S> = vec![s[n - 1]];
    let mut k = 0;
    while cur.len() > 1 {
        let mut next = Vec::with_capacity(cur.len() - 1);
        for i in 0..cur.len() - 1 {
            let d = cur[i + 1] - cur[i];
            if d.abs() == 0.0 || !d.is_finite() {
                // the table breaks down: the sequence has converged in this column
                return Some((*estimates.last().expect("one"), err_of(&estimates)));
            }
            next.push(prev[i + 1] + S::real(1.0) / d);
        }
        prev = cur;
        cur = next;
        k += 1;
        if k % 2 == 0 {
            estimates.push(*cur.last().expect("nonempty"));
        }
    }
    Some((*estimates.last().expect("one"), err_of(&estimates)))
}

fn err_of<S: Scalar>(e: &[S]) -> f64 {
    match e {
        [.., a, b] => (*b - *a).abs(),
        _ => f64::INFINITY,
    }
}

/// The integral of f over the half-line from `a` in direction `dir` (±1) by the zeros of its real part; `None`
/// when the real part shows no sign changes (not oscillatory) or the search limits are reached first.
pub(crate) fn half_line<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn Integrand<'h, S>,
    a: f64,
    dir: f64,
    goals: Goals,
) -> Res<Option<Outcome<S>>> {
    let x_of = |u: f64| a + dir * u;
    // which parts oscillate: a complex integrand with both parts alive is integrated part by part, each between
    // its own zeros (the zeros of cos(x^2) are not half-periods of sin(x^2))
    let probe: Vec<f64> = (0..64).map(|k| x_of(0.37 * (k as f64 + 0.5) * a.abs().max(1.0))).collect();
    let mut py = vec![S::real(0.0); probe.len()];
    let mut pe = vec![0.0; probe.len()];
    if f.eval(cx, &probe, &mut py, &mut pe)?.is_err() {
        return Ok(None);
    }
    let (mr, mi) = py.iter().fold((0.0f64, 0.0f64), |(r, i), &v| (r.max(re(v).abs()), i.max(im(v).abs())));
    if mi > 1e3 * mr {
        return one_part(cx, f, a, dir, goals, true);
    }
    if mr > 1e3 * mi {
        return one_part(cx, f, a, dir, goals, false);
    }
    let r = one_part(cx, &mut Proj { f: &mut *f, imag: false }, a, dir, goals, false)?;
    let Some(Outcome::Done { value: vr, err: er, converged: cr }) = r else { return Ok(r) };
    let i = one_part(cx, &mut Proj { f: &mut *f, imag: true }, a, dir, goals, true)?;
    let Some(Outcome::Done { value: vi, err: ei, converged: ci }) = i else { return Ok(i) };
    Ok(Some(Outcome::Done { value: vr + vi, err: er + ei, converged: cr && ci }))
}

/// The real or imaginary part of an integrand (as a scalar of the same type: Re f, or i Im f).
struct Proj<'a, 'h, S> {
    f: &'a mut dyn Integrand<'h, S>,
    imag: bool,
}

impl<'h, S: Scalar> Integrand<'h, S> for Proj<'_, 'h, S> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, xs: &[f64], out: &mut [S], err: &mut [f64]) -> Res<Result<(), Vec<f64>>> {
        let r = self.f.eval(cx, xs, out, err)?;
        if r.is_ok() {
            for v in out.iter_mut() {
                *v = if self.imag { S::real(im(*v)) * im_unit::<S>() } else { S::real(re(*v)) };
            }
        }
        Ok(r)
    }
}

/// The imaginary part of a scalar as a machine real.
fn im<S: Scalar>(v: S) -> f64 {
    match v.to_num() {
        Num::Real(_) => 0.0,
        Num::Complex(_, i) => i.to_f64(),
    }
}

/// The half-line integral between the zeros of the real (or imaginary, `imag`) part.
fn one_part<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn Integrand<'h, S>,
    a: f64,
    dir: f64,
    goals: Goals,
    imag: bool,
) -> Res<Option<Outcome<S>>> {
    let lim = Limits { max_subdivisions: 200, max_recursion: 40 };
    // the terms and the extrapolation aim near machine precision: a term costs little, and the extrapolated limit
    // is only as good as the terms
    let tight = Goals { abs: goals.abs.min(1e-15), rel: goals.rel.min(1e-13) };
    let term_goals = Goals { abs: tight.abs * 0.01, rel: tight.rel * 0.01 };
    let x_of = |u: f64| a + dir * u;
    // the sign of the oscillating part
    let part = |v: S| if imag { im(v) } else { re(v) };
    // the sample step: start at a scale of the end, adapted to the gaps found
    let mut step = 0.05 * a.abs().max(1.0);
    let mut u = 0.0;
    // the sign at the start (just inside the half-line: the end itself may be 0/0, as sin(x)/x at 0)
    let mut g0 = match value(cx, f, x_of(u))? {
        Some(v) => part(v),
        None => match value(cx, f, x_of(1e-9 * a.abs().max(1.0)))? {
            Some(v) => part(v),
            None => return Ok(None),
        },
    };
    let mut zeros: Vec<f64> = vec![0.0];
    let mut sums: Vec<S> = Vec::new();
    let mut total = S::real(0.0);
    let mut term_err = 0.0;
    let mut samples = 0usize;
    let mut best: Option<(S, f64, f64)> = None;
    let mut history: Vec<S> = Vec::new();
    while zeros.len() <= MAX_TERMS && samples < MAX_SAMPLES {
        cx.tick(1)?;
        // the next sign change of the real part, sampled in blocks of 16
        let mut found = None;
        'search: while samples < MAX_SAMPLES {
            let xs: Vec<f64> = (1..=16).map(|k| x_of(u + step * k as f64)).collect();
            let mut ys = vec![S::real(0.0); 16];
            let mut es = vec![0.0; 16];
            if f.eval(cx, &xs, &mut ys, &mut es)?.is_err() {
                return Ok(None);
            }
            samples += 16;
            for k in 0..16 {
                let g = part(ys[k]);
                let uk = u + step * (k + 1) as f64;
                if g == 0.0 || (g < 0.0) != (g0 < 0.0) {
                    found = Some((uk - step, uk));
                    u = uk;
                    g0 = g;
                    break 'search;
                }
            }
            u += 16.0 * step;
            // no sign change for long: not oscillatory at this scale
            if zeros.len() == 1 && u > 1e6 * a.abs().max(1.0) {
                return Ok(None);
            }
            if zeros.len() > 1 && u - zeros[zeros.len() - 1] > 1e4 * (zeros[zeros.len() - 1] - zeros[zeros.len() - 2]).max(1e-300) {
                return Ok(None);
            }
            step *= 1.5;
        }
        let Some((lo0, hi0)) = found else { break };
        // the Illinois variant of regula falsi on the bracket: the partition point need not be an exact zero
        // (any point gives a valid sum), close to one keeps the partial sums alternating for the extrapolation
        let (mut lo, mut hi) = (lo0, hi0);
        let (Some(vl), Some(vh)) = (value(cx, f, x_of(lo))?, value(cx, f, x_of(hi))?) else { return Ok(None) };
        let (mut glo, mut ghi) = (part(vl), part(vh));
        let mut side = 0i8;
        for _ in 0..12 {
            if glo == ghi || ghi == 0.0 {
                break;
            }
            let m = hi - ghi * (hi - lo) / (ghi - glo);
            if !(m > lo && m < hi) || (hi - lo) <= 1e-13 * hi.abs().max(1.0) {
                break;
            }
            let Some(gm) = value(cx, f, x_of(m))? else { return Ok(None) };
            let gm = part(gm);
            if gm == 0.0 {
                lo = m;
                hi = m;
                break;
            }
            if (gm < 0.0) == (glo < 0.0) {
                lo = m;
                glo = gm;
                if side == -1 {
                    ghi *= 0.5;
                }
                side = -1;
            } else {
                hi = m;
                ghi = gm;
                if side == 1 {
                    glo *= 0.5;
                }
                side = 1;
            }
        }
        let z = 0.5 * (lo + hi);
        let last = *zeros.last().expect("one");
        if z <= last {
            continue;
        }
        // the integral from the last zero to this one
        let piece = Piece { map: Map::Id, t0: x_of(last), t1: x_of(z) };
        let o = gk::integrate(cx, f, &[piece], term_goals, lim)?;
        let (v, e) = match o {
            Outcome::Done { value, err, .. } => (value, err),
            Outcome::NotNumeric(p) => return Ok(Some(Outcome::NotNumeric(p))),
        };
        total = total + v;
        term_err += e;
        sums.push(total);
        zeros.push(z);
        // the next step: an eighth of the last gap
        step = (z - last) / 8.0;
        u = z;
        // g0 stays the sign beyond the zero (the sample that showed the change); a fresh sample just past an
        // inexact z could still show the old sign and find the same zero again as a sliver of an interval
        if sums.len() >= MIN_TERMS {
            // a window of the last 20 partial sums (deep tables amplify the terms' rounding), the error the
            // spread of the last three extrapolations
            let window = &sums[sums.len().saturating_sub(20)..];
            if let Some((est, _)) = epsilon(window) {
                history.push(est);
                let k = history.len();
                if k < 3 {
                    continue;
                }
                let de = (history[k - 1] - history[k - 2]).abs().max((history[k - 1] - history[k - 3]).abs());
                // the extrapolation has settled when its own change is at the tight goal; the reported error adds
                // the terms' error estimates (each at least the Gauss-Kronrod roundoff floor)
                let err = de + term_err;
                if best.as_ref().is_none_or(|b| de < b.2) {
                    best = Some((est, err, de));
                }
                if de <= tight.tol(est.abs()) || (de <= goals.tol(est.abs()) && sums.len() >= 4 * MIN_TERMS) {
                    return Ok(Some(Outcome::Done { value: est, err, converged: err <= goals.tol(est.abs()) }));
                }
            }
        }
    }
    Ok(best.map(|(value, err, _)| Outcome::Done { value, err, converged: err <= goals.tol(value.abs()) }))
}

/// The imaginary unit as a scalar (zero for reals, which never take the imaginary path).
fn im_unit<S: Scalar>() -> S {
    S::of_num(&Num::Complex(mc_arith::Real::Machine(0.0), mc_arith::Real::Machine(1.0))).unwrap_or(S::real(1.0))
}

/// Does the integrand oscillate without decaying on the half-line from `a` in direction `dir`: at least 8 sign
/// changes of its real (or imaginary) part over 400 samples spanning 100 units (or 100 |a|), the last quarter
/// still above 10^-6 of the largest value? Such tails go to `half_line` first (the adaptive rules spend their
/// whole budget on them).
pub(crate) fn oscillating_tail<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn Integrand<'h, S>,
    a: f64,
    dir: f64,
) -> Res<bool> {
    let sc = a.abs().max(1.0);
    let xs: Vec<f64> = (1..=400).map(|k| a + dir * 0.25 * sc * k as f64).collect();
    let mut ys = vec![S::real(0.0); xs.len()];
    let mut es = vec![0.0; xs.len()];
    if f.eval(cx, &xs, &mut ys, &mut es)?.is_err() {
        return Ok(false);
    }
    let parts = |imag: bool| -> Vec<f64> {
        ys.iter().map(|&v| if imag { im(v) } else { re(v) }).collect()
    };
    for imag in [false, true] {
        let g = parts(imag);
        let changes = g.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        let big = g.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let tail = g[300..].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        if changes >= 8 && big > 0.0 && tail > 1e-6 * big {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The value at one point, `None` when not numeric.
fn value<'h, S: Scalar>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut dyn Integrand<'h, S>, x: f64) -> Res<Option<S>> {
    let mut y = [S::real(0.0)];
    let mut e = [0.0];
    Ok(f.eval(cx, &[x], &mut y, &mut e)?.is_ok().then_some(y[0]))
}

#[cfg(test)]
mod tests {
    use super::epsilon;

    #[test]
    fn epsilon_accelerates_alternating_series() {
        // log 2 = 1 - 1/2 + 1/3 - ...
        let mut s = Vec::new();
        let mut t = 0.0;
        for k in 1..=20 {
            t += if k % 2 == 1 { 1.0 } else { -1.0 } / k as f64;
            s.push(t);
        }
        let (v, e) = epsilon(&s).unwrap();
        assert!((v - std::f64::consts::LN_2).abs() < 1e-13, "{v} {e}");
    }
}
