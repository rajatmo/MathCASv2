//! Mathieu functions, solutions of y'' + (a - 2 q cos 2z) y = 0 (DLMF 28.2.1):
//!
//! - `mathieu_c(a, q, z)`: the even Floquet solution ce_nu(z) = sum c_k cos((nu + 2k) z), `mathieu_s(a, q, z)` the odd
//!   one se_nu(z) = sum c_k sin((nu + 2k) z), with sum c_k^2 = 1 (DLMF 28.12); `mathieu_s_prime(a, q, z)`: the derivative
//!   of `mathieu_s`, `mathieu_c_prime(a, q, z)` that of `mathieu_c` (round 37: y' of the same integration). They are ce(0) w_I(z) and se'(0) w_II(z) for the solutions w_I, w_II of DLMF 28.2(ii);
//! - `mathieu_characteristic_a(r, q)`: the characteristic value a_r(q), for which the even solution is periodic (period
//!   pi for even r, 2 pi for odd r) at integers r >= 0, and for other real r the value with characteristic exponent r:
//!   cos(pi r) = w_I(pi; a, q) (DLMF 28.2.16), continuous in r with a_r(0) = r^2; a_(-r) = a_r.
//!
//! Method: Taylor series of the equation with the coefficient expanded at the current point, cos(2 z0 + 2 t) = sum
//! (2t)^i/i! cos(2 z0 + i pi/2), and re-expansion along the segment from 0 (steps at most min(1/2, 1/sqrt(1 + |a| +
//! 2|q|))), on midpoints at w and w + 48 bits, the radius twice their difference plus first-order effects of the
//! arguments' radii (as for the Heun functions). The characteristic values at integer r start from the eigenvalues of
//! the truncated tridiagonal matrices of the Fourier coefficients (DLMF 28.4.5-28.4.8, bisection by Sturm counts in
//! machine numbers) and are refined by the secant method on y'(pi/2) (even r) or y(pi/2) (odd r) of `mathieu_c`.
use crate::cover::{mag, mid_term, numeric, parts, work};
use mc_api::T;
use mc_arith::{Num, Real};
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;

/// A ball number carried at w bits (other terms unchanged).
fn retag<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, w: u64) -> T<'h> {
    let g = |x: Real| match x {
        Real::Ball(b, _) => Real::Ball(b, w),
        Real::Exact(q) => Real::Exact(q).to_kind(mc_arith::Kind::Ball, w),
        x => x,
    };
    match fx.num(t) {
        Some(Num::Real(x)) => fx.ar.number(&Num::Real(g(x))),
        Some(Num::Complex(x, y)) => fx.ar.number(&Num::Complex(g(x), g(y))),
        None => t,
    }
}

/// (y, y') at z for the solution with y(0) = y0, y'(0) = d0, at w bits on midpoints.
#[allow(clippy::too_many_arguments)]
fn solve<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: T<'h>, q: T<'h>, z: T<'h>, y0: i64, d0: i64, w: u64) -> Res<Option<(T<'h>, T<'h>)>> {
    let (Some(zf), Some(af), Some(qf)) = (parts(fx, z), parts(fx, a), parts(fx, q)) else { return Ok(None) };
    let size = af.0.hypot(af.1) + 2.0 * qf.0.hypot(qf.1);
    let hmax = 0.5f64.min(1.0 / (1.0 + size).sqrt());
    let len = zf.0.hypot(zf.1);
    let n = ((len / hmax).ceil() as usize).max(1);
    if n > 100_000 {
        return Ok(None);
    }
    let eps = 2f64.powi(-(w.min(1000) as i32));
    let mut y = fx.int(y0);
    let mut dy = fx.int(d0);
    let nt = fx.int(n as i64);
    let h = fx.eval(fx.div(z, nt))?;
    let two_q = fx.eval(fx.mul(&[fx.int(2), q]))?;
    let mh = mag(fx, h);
    for s in 0..n {
        if s % 8 == 7 {
            fx.tick(1)?;
        }
        let z0 = fx.eval(fx.mul(&[fx.int(s as i64), h]))?;
        let z2 = fx.mul(&[fx.int(2), z0]);
        let c = fx.eval(fx.call("cos", &[z2]))?;
        let sn = fx.eval(fx.call("sin", &[z2]))?;
        if fx.num(c).is_none() || fx.num(sn).is_none() {
            return Ok(None);
        }
        // c_i = 2^i/i! trig_i, trig = cos, -sin, -cos, sin
        let mut cs: Vec<T<'h>> = vec![];
        let mut ys: Vec<T<'h>> = vec![y, dy];
        let mut val = fx.int(0);
        let mut der = fx.int(0);
        let mut hk = fx.int(1);
        let mut small = 0;
        let mut ok = false;
        let mut pk = fx.int(1);
        for k in 0..4000usize {
            // c_k
            let trig = match k % 4 {
                0 => c,
                1 => fx.neg(sn),
                2 => fx.neg(c),
                _ => sn,
            };
            // 2^k/k!, exact
            if k > 0 {
                pk = fx.eval(fx.mul(&[pk, fx.rat(2, k as i64)]))?;
            }
            let fk = fx.eval(fx.mul(&[pk, trig]))?;
            cs.push(fk);
            if k >= ys.len() {
                // y_k from the equation of t^(k-2)
                let m = k - 2;
                let mut conv = vec![];
                for i in 0..=m {
                    conv.push(fx.mul(&[cs[i], ys[m - i]]));
                }
                let rhs = fx.sub(fx.mul(&[two_q, fx.add(&conv)]), fx.mul(&[a, ys[m]]));
                let yk = fx.eval(fx.div(rhs, fx.int((k * (k - 1)) as i64)))?;
                if fx.num(yk).is_none() {
                    return Ok(None);
                }
                ys.push(yk);
            }
            let t = fx.eval(fx.mul(&[ys[k], hk]))?;
            val = fx.eval(fx.add(&[val, t]))?;
            if k > 0 {
                let hk1 = fx.div(hk, h);
                der = fx.eval(fx.add(&[der, fx.mul(&[fx.int(k as i64), ys[k], hk1])]))?;
            }
            hk = fx.eval(fx.mul(&[hk, h]))?;
            let mt = mag(fx, t) * (1.0 + k as f64 / mh.max(1e-300));
            if k >= 4 && mt <= eps * (mag(fx, val) + mag(fx, der) * mh).max(1e-300) {
                small += 1;
                if small >= 3 {
                    ok = true;
                    break;
                }
            } else {
                small = 0;
            }
        }
        if !ok {
            return Ok(None);
        }
        y = mid_term(fx, val);
        dy = mid_term(fx, der);
    }
    Ok(Some((y, dy)))
}

/// Which value.
#[derive(Clone, Copy, PartialEq)]
enum Which {
    C,
    S,
    SPrime,
    CPrime,
}

impl Which {
    /// The even solution (mathieu_c and its derivative).
    fn even(self) -> bool {
        matches!(self, Which::C | Which::CPrime)
    }
}

/// The normalization of the Floquet solutions (DLMF 28.12): with the characteristic exponent nu, cos(pi nu) = w_I(pi)
/// (28.2.16), the Fourier coefficients c_k of ce_nu = sum c_k cos((nu + 2k) z) solve (a - (nu + 2k)^2) c_k = q (c_(k-1) +
/// c_(k+1)) (the minimal solutions in both directions by continued fractions, c_0 = 1) and are scaled to sum c_k^2 = 1:
/// the even function is then ce(0) w_I(z) with ce(0) = sum c_k, the odd one se'(0) w_II(z) with se'(0) = sum (nu + 2k)
/// c_k.
fn floquet_norm<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: T<'h>, q: T<'h>, w: u64, even: bool) -> R<'h> {
    let pi = crate::cover::pi_at(fx, w)?;
    let pi = retag(fx, pi, w);
    let Some((y_pi, _)) = solve(fx, a, q, pi, 1, 0, w)? else { return Ok(None) };
    let ac = fx.eval(fx.call("acos", &[y_pi]))?;
    if fx.num(ac).is_none() {
        return Ok(None);
    }
    let nu1 = fx.eval(fx.div(ac, pi))?;
    // the branch +-nu1 + 2m nearest to sqrt(a) (the exponent continuous from q = 0, where se_nu = sin(nu z)): the sign
    // of the odd function depends on it
    let sa = fx.eval(fx.call("sqrt", &[a]))?;
    let (Some(n1), Some(saf)) = (parts(fx, nu1), parts(fx, sa)) else { return Ok(None) };
    let mut best = (f64::INFINITY, 1i64, 0i64);
    for sg in [1i64, -1] {
        let m = ((saf.0 - sg as f64 * n1.0) / 2.0).round();
        let dist = (sg as f64 * n1.0 + 2.0 * m - saf.0).abs();
        if dist < best.0 {
            best = (dist, sg, m as i64);
        }
    }
    let nu = fx.eval(fx.add(&[fx.mul(&[fx.int(best.1), nu1]), fx.int(2 * best.2)]))?;
    let qm = mag(fx, q);
    let kmax = 20 + (w / 8) as i64 + (qm.sqrt() * 3.0) as i64;
    let d = |fx: &mut Fx<'_, '_, '_, 'h>, k: i64| -> Res<T<'h>> {
        let m = fx.add(&[nu, fx.int(2 * k)]);
        fx.eval(fx.sub(a, fx.mul(&[m, m])))
    };
    // ratios r_k = c_k/c_(k-1) (k > 0) and s_k = c_k/c_(k+1) (k < 0), backward from the ends
    let mut r = vec![fx.int(0); (kmax + 2) as usize];
    for k in (1..=kmax).rev() {
        let dk = d(fx, k)?;
        let den = fx.eval(fx.sub(dk, fx.mul(&[q, r[(k + 1) as usize]])))?;
        if fx.num(den).is_none_or(|v| v.is_zero()) {
            return Ok(None);
        }
        r[k as usize] = fx.eval(fx.div(q, den))?;
    }
    let mut sr = vec![fx.int(0); (kmax + 2) as usize];
    for k in (1..=kmax).rev() {
        let dk = d(fx, -k)?;
        let den = fx.eval(fx.sub(dk, fx.mul(&[q, sr[(k + 1) as usize]])))?;
        if fx.num(den).is_none_or(|v| v.is_zero()) {
            return Ok(None);
        }
        sr[k as usize] = fx.eval(fx.div(q, den))?;
    }
    let mut sum2 = vec![fx.int(1)];
    let mut sum1 = vec![fx.int(1)];
    let mut sumd = vec![nu];
    for (sign, ratios) in [(1i64, &r), (-1i64, &sr)] {
        let mut c = fx.int(1);
        for k in 1..=kmax {
            c = fx.eval(fx.mul(&[c, ratios[k as usize]]))?;
            sum2.push(fx.mul(&[c, c]));
            sum1.push(c);
            sumd.push(fx.mul(&[fx.add(&[nu, fx.int(2 * sign * k)]), c]));
        }
    }
    let s2 = fx.eval(fx.add(&sum2))?;
    let root = fx.eval(fx.call("sqrt", &[s2]))?;
    let num = if even { fx.add(&sum1) } else { fx.add(&sumd) };
    let v = fx.eval(fx.div(num, root))?;
    Ok(fx.num(v).map(|_| v))
}

/// The value at w bits from midpoint arguments.
fn value_at<'h>(fx: &mut Fx<'_, '_, '_, 'h>, xs: &[T<'h>], which: Which, w: u64) -> R<'h> {
    let (y0, d0) = if which.even() { (1, 0) } else { (0, 1) };
    let Some((y, d)) = solve(fx, xs[0], xs[1], xs[2], y0, d0, w)? else { return Ok(None) };
    let Some(c) = floquet_norm(fx, xs[0], xs[1], w, which.even())? else { return Ok(None) };
    let v = if matches!(which, Which::SPrime | Which::CPrime) { d } else { y };
    Ok(Some(fx.eval(fx.mul(&[c, v]))?))
}

/// The value with its estimated radius: midpoints at w and w + 48 bits, and one perturbed run per argument with a
/// radius.
fn estimated<'h>(fx: &mut Fx<'_, '_, '_, 'h>, xs: &[T<'h>], w: u64, f: &mut dyn FnMut(&mut Fx<'_, '_, '_, 'h>, &[T<'h>], u64) -> R<'h>) -> R<'h> {
    let mids: Vec<T<'h>> = xs.iter().map(|&t| retag(fx, mid_term(fx, t), w)).collect();
    let Some(v1) = f(fx, &mids, w)? else { return Ok(None) };
    let w2 = w + 48;
    let mids2: Vec<T<'h>> = mids.iter().map(|&t| retag(fx, t, w2)).collect();
    let Some(v2) = f(fx, &mids2, w2)? else { return Ok(None) };
    let diff = fx.eval(fx.sub(v1, v2))?;
    let mut err = 2.0 * mag(fx, diff) + mag(fx, v2) * 2f64.powi(-(w.min(1000) as i32));
    let k = 65536.0;
    for i in 0..xs.len() {
        let r = match fx.num(xs[i]) {
            Some(Num::Real(Real::Ball(b, _))) => crate::ball::f64_of(&b.rad.to_float()),
            Some(Num::Complex(x, y)) => {
                let rr = |x: &Real| match x {
                    Real::Ball(b, _) => crate::ball::f64_of(&b.rad.to_float()),
                    _ => 0.0,
                };
                rr(&x).max(rr(&y))
            }
            _ => 0.0,
        };
        if r == 0.0 {
            continue;
        }
        let mut ys = mids.clone();
        let dx = ball_of(fx, fx.real(r * k), w);
        ys[i] = fx.eval(fx.add(&[mids[i], dx]))?;
        let Some(v3) = f(fx, &ys, w)? else { return Ok(None) };
        let d = fx.eval(fx.sub(v3, v1))?;
        err += 2.0 * mag(fx, d) / k;
    }
    if !err.is_finite() {
        return Ok(None);
    }
    let m = mc_arith::num::Mag::from_float_up(&crate::ball::float_of(err.max(1e-300)));
    Ok(Some(crate::cover::map_balls(fx, v2, |b| crate::ball::widen(b.clone(), &m))))
}

fn mathieu<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: Which) -> R<'h> {
    let (av, q, z) = (a[0], a[1], a[2]);
    if fx.is_int(z, 0) && matches!(which, Which::S | Which::CPrime) {
        return Ok(Some(fx.int(0)));
    }
    if fx.is_int(q, 0) && !numeric(fx, a) {
        // q = 0: ce = cos(nu z), se = sin(nu z), se' = nu cos(nu z) with nu = sqrt(a) (DLMF 28.12: se_nu(z, 0) = sin(nu z))
        let r = fx.sqrt(av);
        let arg = fx.mul(&[r, z]);
        return Ok(Some(match which {
            Which::C => fx.call("cos", &[arg]),
            Which::S => fx.call("sin", &[arg]),
            Which::SPrime => fx.mul(&[r, fx.call("cos", &[arg])]),
            Which::CPrime => fx.neg(fx.mul(&[r, fx.call("sin", &[arg])])),
        }));
    }
    if let Some(nz) = fx.negated(z) {
        if fx.num(z).is_none() {
            let name = match which {
                Which::C => "mathieu_c",
                Which::S => "mathieu_s",
                Which::SPrime => "mathieu_s_prime",
                Which::CPrime => "mathieu_c_prime",
            };
            let f = fx.call(name, &[av, q, nz]);
            return Ok(Some(if matches!(which, Which::S | Which::CPrime) { fx.neg(f) } else { f }));
        }
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    let v = mathieu_numeric(fx, a, which)?;
    match v {
        Some(v) if imaginary_axis(fx, a) => on_imaginary_axis(fx, v, which).map(Some),
        v => Ok(v),
    }
}

/// Are a and q real and z = i y on the imaginary axis (y real and nonzero)?
fn imaginary_axis<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> bool {
    let real = |t: T<'h>| match fx.num(t) {
        Some(Num::Real(_)) => true,
        Some(Num::Complex(_, y)) => y.is_zero(),
        None => false,
    };
    real(a[0]) && real(a[1]) && matches!(fx.num(a[2]), Some(Num::Complex(x, y)) if x.is_zero() && !y.is_zero())
}

/// For real a and q the Mathieu functions have real Taylor coefficients in z, and ce, se' are even, se, ce' odd
/// (DLMF 28.2.(vi)): on the imaginary axis ce(i y) and se'(i y) are real and se(i y), ce'(i y) purely imaginary. The
/// other part of a computed value is rounding noise (of the order 10^-59 at machine arguments) and is dropped, so
/// that a function of real q such as se(a(q), q, i/2) stays on a line instead of drifting off it.
fn on_imaginary_axis<'h>(fx: &mut Fx<'_, '_, '_, 'h>, v: T<'h>, which: Which) -> Res<T<'h>> {
    Ok(match which {
        Which::C | Which::SPrime => fx.eval(fx.call("re", &[v]))?,
        Which::S | Which::CPrime => {
            let y = fx.eval(fx.call("im_part", &[v]))?;
            fx.eval(fx.mul(&[fx.i(), y]))?
        }
    })
}

/// The numeric value of a Mathieu function at numeric arguments.
fn mathieu_numeric<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: Which) -> R<'h> {
    let z = a[2];
    // machine arguments: the double-double kernel (round 29), the ball path when it declines
    if let Some((ms, real)) = crate::fast29::machine_args(fx, a) {
        let code = match which {
            Which::C => 0,
            Which::S => 1,
            Which::SPrime => 2,
            Which::CPrime => 3,
        };
        if let Some(v) = crate::fast29::mathieu(ms[0], ms[1], ms[2], code) {
            return Ok(crate::fast29::machine_out(fx, v, real));
        }
    }
    let Some(zf) = parts(fx, z) else { return Ok(None) };
    let extra = 40 + (zf.1.abs() * 4.0) as u64;
    let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
    let mut f = |fx: &mut Fx<'_, '_, '_, 'h>, ys: &[T<'h>], w: u64| value_at(fx, ys, which, w);
    let Some(v) = estimated(fx, &xs, wk.w, &mut f)? else { return Ok(None) };
    Ok(wk.out(fx, v, wk.real))
}

fn mathieu_c<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    mathieu(fx, a, Which::C)
}
fn mathieu_s<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    mathieu(fx, a, Which::S)
}
fn mathieu_s_prime<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    mathieu(fx, a, Which::SPrime)
}
fn mathieu_c_prime<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    mathieu(fx, a, Which::CPrime)
}

/// The eigenvalue of index `j` (0-based, ascending) of the symmetric tridiagonal matrix (diag, off), by bisection
/// with Sturm counts.
fn tridiag_eigen(diag: &[f64], off: &[f64], j: usize) -> f64 {
    let n = diag.len();
    let bound: f64 = diag.iter().map(|d| d.abs()).fold(0.0, f64::max) + 2.0 * off.iter().map(|o| o.abs()).fold(0.0, f64::max) + 1.0;
    let count = |x: f64| {
        // the number of eigenvalues below x
        let mut c = 0;
        let mut d = 1.0;
        for i in 0..n {
            let o2 = if i == 0 { 0.0 } else { off[i - 1] * off[i - 1] };
            d = diag[i] - x - if i == 0 { 0.0 } else { o2 / if d == 0.0 { 1e-300 } else { d } };
            if d < 0.0 {
                c += 1;
            }
        }
        c
    };
    let (mut lo, mut hi) = (-bound, bound);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if mid == lo || mid == hi {
            break;
        }
        if count(mid) > j {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    0.5 * (lo + hi)
}

/// [`tridiag_eigen`] for the other modules.
pub(crate) fn tridiag_eigen_pub(diag: &[f64], off: &[f64], j: usize) -> f64 {
    tridiag_eigen(diag, off, j)
}

/// The machine characteristic value a_r(q) at an integer r >= 0 and real q.
fn char_a_f64(r: usize, q: f64) -> f64 {
    let n = r / 2 + 30 + (q.abs().sqrt() * 4.0) as usize;
    if r.is_multiple_of(2) {
        let diag: Vec<f64> = (0..n).map(|m| (2.0 * m as f64).powi(2)).collect();
        let mut off = vec![q; n - 1];
        off[0] = std::f64::consts::SQRT_2 * q;
        tridiag_eigen(&diag, &off, r / 2)
    } else {
        let mut diag: Vec<f64> = (0..n).map(|m| (2.0 * m as f64 + 1.0).powi(2)).collect();
        diag[0] += q;
        let off = vec![q; n - 1];
        tridiag_eigen(&diag, &off, r / 2)
    }
}

/// The large-parameter expansion of a_r(q) at an integer r >= 0 and a real machine q of large modulus (DLMF 28.8.1,
/// h = sqrt|q|, s = 2r + 1; for q < 0 the value is a_r(|q|) at even r and b_r(|q|), with s = 2r - 1, at odd r; the
/// terms through h^-5, the next one is smaller by about s^2/(2^5 h)). `None` unless h is large against s^2.
fn char_a_asymptotic(r: f64, q: f64) -> Option<f64> {
    let h = q.abs().sqrt();
    let s = if q < 0.0 && !(r as u64).is_multiple_of(2) { 2.0 * r - 1.0 } else { 2.0 * r + 1.0 };
    if h.is_nan() || h <= 32.0 * s * s {
        return None;
    }
    let (s2, s4, s6) = (s * s, s.powi(4), s.powi(6));
    let terms = [
        -(s2 + 1.0) / 8.0,
        -s * (s2 + 3.0) / (128.0 * h),
        -(5.0 * s4 + 34.0 * s2 + 9.0) / (4096.0 * h * h),
        -s * (33.0 * s4 + 410.0 * s2 + 405.0) / (131072.0 * h.powi(3)),
        -(63.0 * s6 + 1260.0 * s4 + 2943.0 * s2 + 486.0) / (1048576.0 * h.powi(4)),
        -s * (527.0 * s6 + 15617.0 * s4 + 69001.0 * s2 + 41607.0) / (33554432.0 * h.powi(5)),
    ];
    // the smallest terms first
    Some(terms.iter().rev().sum::<f64>() + 2.0 * s * h - 2.0 * h * h)
}

fn mathieu_characteristic_a<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    char_value(fx, a, false)
}

/// The machine characteristic value b_r(q) at an even integer r >= 2 and real q: the eigenvalue of the truncated
/// tridiagonal matrix of the Fourier coefficients B_(2m+2) of se_r (DLMF 28.4.8).
fn char_b_even_f64(r: usize, q: f64) -> f64 {
    let n = r / 2 + 30 + (q.abs().sqrt() * 4.0) as usize;
    let diag: Vec<f64> = (0..n).map(|m| (2.0 * m as f64 + 2.0).powi(2)).collect();
    let off = vec![q; n - 1];
    tridiag_eigen(&diag, &off, r / 2 - 1)
}

/// `mathieu_characteristic_b(r, q)`: the characteristic value b_r(q) of the odd Mathieu functions, for which the odd
/// solution se_r is periodic (period pi for even r, 2 pi for odd r) at integers r >= 1. The relations of DLMF 28.2.26
/// and 28.2.27 reduce it: b_(2n+1)(q) = a_(2n+1)(-q) and b_(2n+2)(-q) = b_(2n+2)(q); at other real r it is the
/// value with characteristic exponent r, which is a_r(q) (DLMF 28.12(i)); b_(-r) = b_r. b_0 does not exist.
fn mathieu_characteristic_b<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (r, q) = (a[0], a[1]);
    let Some(rf) = fx.f64_of(r) else { return Ok(None) };
    let rf = rf.abs();
    let int_r = rf.fract() == 0.0 && rf < 1e4;
    if int_r && rf == 0.0 {
        return Ok(None);
    }
    if fx.is_int(q, 0) {
        return Ok(Some(fx.pow(r, fx.int(2))));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    if !int_r {
        return char_value(fx, a, false);
    }
    if !(rf as u64).is_multiple_of(2) {
        let mq = fx.eval(fx.neg(q))?;
        return char_value(fx, &[r, mq], false);
    }
    let aq = fx.eval(fx.call("abs", &[q]))?;
    char_value(fx, &[r, aq], true)
}

/// The characteristic value a_r(q), or with `odd` b_r(q) at an even integer r >= 2 and q >= 0.
fn char_value<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], odd: bool) -> R<'h> {
    let (r, q) = (a[0], a[1]);
    if fx.is_int(q, 0) {
        return Ok(Some(fx.pow(r, fx.int(2))));
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    let (Some(rf), Some(qp)) = (fx.f64_of(r), parts(fx, q)) else { return Ok(None) };
    if qp.1 != 0.0 {
        return Ok(None);
    }
    let qf = qp.0;
    let rf = rf.abs();
    let int_r = rf.fract() == 0.0 && rf < 1e4;
    // the large-parameter expansion: b_r(q) ~ a_(r-1)(q) to every order for q -> +infinity (DLMF 28.8.1, where
    // b_(m+1) shares the expansion of a_m)
    let asym = |rf: f64, qf: f64| if odd { char_a_asymptotic(rf - 1.0, qf) } else { char_a_asymptotic(rf, qf) };
    // beyond |q| = 1e5 the Taylor steps of the solver number in the tens of thousands (and the bits of the working
    // precision grow with them): machine arguments take the large-parameter expansion at an integer order, every
    // other call stays unevaluated
    if qf.abs() > 1e5 {
        let machine = crate::fast29::machine_args(fx, a).is_some();
        return Ok((machine && int_r).then(|| asym(rf, qf)).flatten().map(|v| fx.real(v)));
    }
    // the start
    let a0 = if odd {
        char_b_even_f64(rf as usize, qf)
    } else if int_r {
        char_a_f64(rf as usize, qf)
    } else {
        let n2 = rf * rf;
        n2 + if (n2 - 1.0).abs() > 0.1 { qf * qf / (2.0 * (n2 - 1.0)) } else { 0.0 }
    };
    if let Some((ms, _)) = crate::fast29::machine_args(fx, a) {
        if let Some(v) = crate::fast29::mathieu_char_a(rf, ms[1].re, a0, odd) {
            return Ok(Some(fx.real(v)));
        }
        // the secant method on a double-double solution found no root (the even solution grows like e^(c sqrt|q|)
        // and the roots drown in rounding): the expansion, or a clean refusal instead of a precision spiral
        if int_r {
            if let Some(v) = asym(rf, qf) {
                return Ok(Some(fx.real(v)));
            }
        }
    }
    if qf.abs() > 2e4 {
        return Ok(None);
    }
    let Some((wk, xs)) = work(fx, a, 40) else { return Ok(None) };
    let w = wk.w;
    let pi = crate::cover::pi_at(fx, w)?;
    let half_pi = fx.eval(fx.mul(&[fx.rat(1, 2), pi]))?;
    let rr = fx.eval(fx.call("abs", &[xs[0]]))?;
    // the condition whose root is a_r: y'(pi/2) or y(pi/2) of the even solution at integer r; y(pi) - cos(pi r) else
    let cond = |fx: &mut Fx<'_, '_, '_, 'h>, av: T<'h>, ww: u64| -> R<'h> {
        let qw = retag(fx, mid_term(fx, xs[1]), ww);
        if odd {
            let zt = retag(fx, half_pi, ww);
            let Some((y, _)) = solve(fx, av, qw, zt, 0, 1, ww)? else { return Ok(None) };
            Ok(Some(y))
        } else if int_r {
            let zt = retag(fx, half_pi, ww);
            let Some((y, d)) = solve(fx, av, qw, zt, 1, 0, ww)? else { return Ok(None) };
            Ok(Some(if (rf as u64).is_multiple_of(2) { d } else { y }))
        } else {
            let zt = retag(fx, pi, ww);
            let Some((y, _)) = solve(fx, av, qw, zt, 1, 0, ww)? else { return Ok(None) };
            let c = fx.eval(fx.call("cos", &[fx.mul(&[retag(fx, pi, ww), retag(fx, mid_term(fx, rr), ww)])]))?;
            Ok(Some(fx.eval(fx.sub(y, c))?))
        }
    };
    // secant iterations at precisions 64, 128, ..., w + 16 bits, each stage restarted next to the previous root
    let mut x1 = ball_of(fx, fx.real(a0), 64);
    let mut ww = 64u64;
    loop {
        x1 = ball_of(fx, mid_term(fx, x1), ww);
        let d = fx.pow(fx.int(2), fx.int(-((ww / 2) as i64)));
        let mut x0 = fx.eval(fx.add(&[x1, fx.mul(&[d, fx.add(&[fx.int(1), fx.call("abs", &[x1])])])]))?;
        let Some(mut f0) = cond(fx, x0, ww)? else { return Ok(None) };
        for _ in 0..40 {
            let Some(f1) = cond(fx, x1, ww)? else { return Ok(None) };
            let df = fx.eval(fx.sub(f1, f0))?;
            if fx.num(df).is_none_or(|v| v.is_zero()) {
                break;
            }
            let x2 = fx.eval(fx.sub(x1, fx.div(fx.mul(&[f1, fx.sub(x1, x0)]), df)))?;
            if fx.num(x2).is_none() {
                return Ok(None);
            }
            let x2 = ball_of(fx, mid_term(fx, x2), ww);
            let step = mag(fx, fx.sub(x2, x1));
            x0 = x1;
            f0 = f1;
            x1 = x2;
            if step <= mag(fx, x1).max(1.0) * 2f64.powi(-((ww as i32).min(1000) - 8)) {
                break;
            }
        }
        if ww >= w + 16 {
            break;
        }
        ww = (ww * 2).min(w + 16);
    }
    let m = mc_arith::num::Mag::from_float_up(&crate::ball::float_of((mag(fx, x1).max(1.0) * 2f64.powi(-((wk.p as i32).min(1000) - 4))).max(1e-300)));
    let v = crate::cover::map_balls(fx, retag(fx, x1, w), |b| crate::ball::widen(b.clone(), &m));
    Ok(wk.out(fx, v, true))
}

/// A machine number or ball as a ball at w bits.
fn ball_of<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, w: u64) -> T<'h> {
    match fx.num(t) {
        Some(Num::Real(Real::Machine(v))) => fx.ar.number(&Num::Real(Real::Ball(mc_arith::num::Ball::exact(crate::ball::float_of(v)), w))),
        _ => retag(fx, t, w),
    }
}

macro_rules! def {
    ($name:literal, $ar:expr, $e:expr, $s:literal) => {
        Def { name: $name, arities: $ar, listable: true, exact: Some($e), numeric: None, summary: $s }
    };
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    def!("mathieu_c", &[3], mathieu_c, "mathieu_c(a, q, z): the even Mathieu function, the even Floquet solution of y'' + (a - 2 q cos 2z) y = 0"),
    def!("mathieu_s", &[3], mathieu_s, "mathieu_s(a, q, z): the odd Mathieu function, the odd Floquet solution of y'' + (a - 2 q cos 2z) y = 0"),
    def!("mathieu_s_prime", &[3], mathieu_s_prime, "mathieu_s_prime(a, q, z): the derivative of the odd Mathieu function mathieu_s(a, q, z) in z"),
    def!("mathieu_c_prime", &[3], mathieu_c_prime, "mathieu_c_prime(a, q, z): the derivative of the even Mathieu function mathieu_c(a, q, z) in z"),
    def!("mathieu_characteristic_a", &[2], mathieu_characteristic_a, "mathieu_characteristic_a(r, q): the characteristic value a_r(q) of the even Mathieu functions"),
    def!("mathieu_characteristic_b", &[2], mathieu_characteristic_b, "mathieu_characteristic_b(r, q): the characteristic value b_r(q) of the odd Mathieu functions"),
];

#[cfg(test)]
mod asymptotic_tests {
    use super::*;

    /// The large-parameter expansion against the eigenvalue of the truncated Fourier matrix (an independent route).
    #[test]
    fn asymptotic_matches_the_tridiagonal_eigenvalue() {
        for (r, q) in [(0.0, 1e5), (1.0, 1e5), (0.0, -1e5), (1.0, -1e5), (2.0, 7e5), (0.0, 2e6), (1.0, 3e6)] {
            let want = char_a_f64(r as usize, q);
            let got = char_a_asymptotic(r, q).unwrap();
            assert!(((got - want) / want).abs() < 1e-12, "a_{r}({q}): {got} vs {want}");
        }
        assert!(char_a_asymptotic(10.0, 1e5).is_none());
    }
}
