//! Machine fast paths of round 33:the Jacobi elliptic functions sn, cn, dn at a complex
//! argument u and a real parameter m, in complex double-double arithmetic ([`crate::cdd`]). Before, a complex machine
//! value went through the ball loop (6 to 35 ms per value), so `complex_plot3d` of any Jacobi function timed out.
//!
//! Method (DLMF 22.2.4-6, 22.6.9-11, 22.17, A&S 16.10-16.11):
//! - m outside [0, 1]: the real-parameter transformations to mu in (0, 1) (m < 0: mu = -m/(1 - m), v = u sqrt(1 - m);
//!   m > 1: mu = 1/m, v = u sqrt(m)), which hold for complex u;
//! - 1/2 < m < 1: the imaginary transformation sn(u | m) = -i sc(iu | 1 - m), cn = nc(iu | 1 - m), dn = dc(iu | 1 - m),
//!   so the core evaluates only m <= 1/2, where the nome q = exp(-pi K'/K) <= e^(-pi);
//! - the core: u reduced by the periods 2K and 2iK' (signs from sn(u + 2K) = -sn, cn(u + 2K) = -cn, dn(u + 2K) = dn and
//!   sn(u + 2iK') = sn, cn(u + 2iK') = -cn, dn(u + 2iK') = -dn) into |Re u| <= K, |Im u| <= K', then the theta
//!   quotients sn = (th3(0)/th2(0)) th1(v)/th4(v), cn = (th4(0)/th2(0)) th2(v)/th4(v), dn = (th4(0)/th3(0))
//!   th3(v)/th4(v), v = pi u/(2K), with the q-series summed to 1e-34 (the powers e^(+-i k v) by multiplication).
//!
//! The kernel declines (`None`) at m = 0 or 1, for |u| > 1e6, a complex m, or a non-finite value; the call then takes
//! the ball path as before.
use crate::cdd::CD;
use crate::dd::{self, DD};
use mc_numeric::machine::C64;

/// (sn, cn, dn)(u | m) for 0 < m <= 1/2 by the theta series.
fn core(u: CD, m: DD) -> Option<(CD, CD, CD)> {
    let k = crate::fast29::k_cd(CD::real(m))?;
    let kp = crate::fast29::k_cd(CD::real(DD::ONE - m))?;
    let (k, kp) = (k.re, kp.re);
    // reduce modulo the periods
    let two_k = k.mul_f(2.0);
    let two_kp = kp.mul_f(2.0);
    let ny = (u.im / two_kp).round_hi();
    let mut u1 = CD::new(u.re, u.im - two_kp.mul_f(ny));
    let flip_y = (ny.abs() % 2.0) == 1.0;
    let nx = (u1.re / two_k).round_hi();
    u1 = CD::new(u1.re - two_k.mul_f(nx), u1.im);
    let flip_x = (nx.abs() % 2.0) == 1.0;
    // nome and theta values
    let ratio = kp / k;
    let q_quarter = dd::exp(-(dd::PI * ratio).mul_f(0.25));
    let q = q_quarter.sqr().sqr();
    let q2 = q.sqr();
    // t_n = q^(n^2), h_n = q^((n + 1/2)^2)
    const N: usize = 9;
    let mut t = [DD::ZERO; N];
    let mut h = [DD::ZERO; N];
    t[0] = DD::ONE;
    let mut e = q; // q^(2n + 1)
    for n in 1..N {
        t[n] = t[n - 1] * e;
        e = e * q2;
    }
    h[0] = q_quarter;
    let mut g = q2; // q^(2n + 2)
    for n in 1..N {
        h[n] = h[n - 1] * g;
        g = g * q2;
    }
    let th2_0 = h.iter().fold(DD::ZERO, |s, &x| s + x).mul_f(2.0);
    let th3_0 = DD::ONE + t[1..].iter().fold(DD::ZERO, |s, &x| s + x).mul_f(2.0);
    let th4_0 = DD::ONE + t[1..].iter().enumerate().fold(DD::ZERO, |s, (i, &x)| if i % 2 == 0 { s - x } else { s + x }).mul_f(2.0);
    // v = pi u / (2K); w = e^(iv)
    let v = u1.mul_dd(dd::PI / two_k);
    let w = v.mul_i().exp();
    let winv = CD::ONE / w;
    let w2 = w * w;
    let winv2 = winv * winv;
    let (mut th1, mut th2, mut th3, mut th4) = (CD::ZERO, CD::ZERO, CD::ONE, CD::ONE);
    let mut a = w; // w^(2n + 1)
    let mut b = winv;
    let mut c = CD::ONE; // w^(2n)
    let mut cinv = CD::ONE;
    for n in 0..N {
        // sin((2n+1) v) = (a - b)/(2i), cos((2n+1) v) = (a + b)/2
        let d = a - b;
        let sin_o = CD::new(d.im.mul_f(0.5), -d.re.mul_f(0.5));
        let cos_o = (a + b).scale(0.5);
        let sgn = if n % 2 == 0 { 1.0 } else { -1.0 };
        th1 = th1 + sin_o.mul_dd(h[n]).scale(2.0 * sgn);
        th2 = th2 + cos_o.mul_dd(h[n]).scale(2.0);
        if n >= 1 {
            let cos_e = (c + cinv).scale(0.5);
            th3 = th3 + cos_e.mul_dd(t[n]).scale(2.0);
            th4 = th4 + cos_e.mul_dd(t[n]).scale(2.0 * sgn);
        }
        a = a * w2;
        b = b * winv2;
        c = c * w2;
        cinv = cinv * winv2;
    }
    let sn = th1.mul_dd(th3_0 / th2_0) / th4;
    let cn = th2.mul_dd(th4_0 / th2_0) / th4;
    let dn = th3.mul_dd(th4_0 / th3_0) / th4;
    let (mut sn, mut cn, mut dn) = (sn, cn, dn);
    if flip_x {
        sn = -sn;
        cn = -cn;
    }
    if flip_y {
        cn = -cn;
        dn = -dn;
    }
    (sn.finite() && cn.finite() && dn.finite()).then_some((sn, cn, dn))
}

/// (sn, cn, dn)(u | m) for 0 < m < 1.
fn unit(u: CD, m: DD) -> Option<(CD, CD, CD)> {
    if m.hi <= 0.5 {
        return core(u, m);
    }
    // sn(u | m) = -i sc(iu | m'), cn(u | m) = nc(iu | m'), dn(u | m) = dc(iu | m'), m' = 1 - m
    let (s, c, d) = core(u.mul_i(), DD::ONE - m)?;
    let sn = (s / c).mul_i();
    let sn = -sn;
    Some((sn, CD::ONE / c, d / c))
}

/// (sn, cn, dn)(u | m) at a complex double-double u and a real double-double m; `None` when out of range.
pub(crate) fn jacobi(u: CD, m: DD) -> Option<(CD, CD, CD)> {
    if u.finite() && u.abs() <= 1e6 && m.hi == 0.0 && m.lo == 0.0 {
        // DLMF 22.5.3: sn(u | 0) = sin u, cn(u | 0) = cos u, dn(u | 0) = 1 (the other paths divide by K' = inf; a
        // machine m = 0 with a complex u never returned)
        return Some((u.sin(), u.cos(), CD::ONE));
    }
    if u.finite() && u.abs() <= 700.0 && m.hi == 1.0 && m.lo == 0.0 {
        // DLMF 22.5.4: sn(u | 1) = tanh u, cn(u | 1) = dn(u | 1) = sech u
        let e = u.exp();
        let e2 = e * e;
        let den = e2 + CD::ONE;
        let sech = e.scale(2.0) / den;
        return Some(((e2 - CD::ONE) / den, sech, sech));
    }
    if !u.finite() || !m.hi.is_finite() || u.abs() > 1e6 || m.hi == 0.0 || m.hi == 1.0 || m.hi.abs() > 1e15 {
        return None;
    }
    if m.hi > 0.0 && m.hi < 1.0 {
        return unit(u, m);
    }
    if m.hi > 1.0 {
        // A&S 16.11: mu = 1/m, v = u sqrt(m): sn = sn(v | mu)/sqrt(m), cn = dn(v | mu), dn = cn(v | mu)
        let rt = m.sqrt();
        let (s, c, d) = unit(u.mul_dd(rt), DD::ONE / m)?;
        return Some((s.mul_dd(DD::ONE / rt), d, c));
    }
    // A&S 16.10: mu = -m/(1 - m), v = u sqrt(1 - m): sn = sd(v | mu)/sqrt(1 - m), cn = cd(v | mu), dn = nd(v | mu)
    let mc = DD::ONE - m;
    let rt = mc.sqrt();
    let mu = -m / mc;
    let (s, c, d) = unit(u.mul_dd(rt), mu)?;
    Some((s.mul_dd(DD::ONE / rt) / d, c / d, CD::ONE / d))
}

/// A Jacobi quotient p/q of {s, c, d, n} at complex machine-precision arguments (`n` = 1).
pub(crate) fn ratio(p: char, q: char, u: CD, m: CD) -> Option<C64> {
    if m.im.hi != 0.0 || m.im.lo != 0.0 {
        return None;
    }
    let (s, c, d) = jacobi(u, m.re)?;
    let pick = |ch: char| match ch {
        's' => s,
        'c' => c,
        'd' => d,
        _ => CD::ONE,
    };
    let den = pick(q);
    if den.re.hi == 0.0 && den.im.hi == 0.0 {
        return None;
    }
    let r = (pick(p) / den).to_c64();
    (r.re.is_finite() && r.im.is_finite()).then_some(r)
}

/// sn, cn or dn (`which` 0, 1, 2) at complex machine arguments.
pub(crate) fn single(which: u8, u: C64, m: C64) -> Option<C64> {
    if m.im != 0.0 {
        return None;
    }
    let (s, c, d) = jacobi(CD::c(u), DD::new(m.re))?;
    let r = match which {
        0 => s,
        1 => c,
        _ => d,
    }
    .to_c64();
    (r.re.is_finite() && r.im.is_finite()).then_some(r)
}

// ---------------------------------------------------------------- Carlson RC, RJ and the elliptic integrals of the third kind

/// R_C(x, y) by Carlson's duplication (Carlson, Numer. Algorithms 10 (1995) 13-26, section 2; DLMF 19.36.1, 19.2.17) in
/// double-double numbers, principal roots: the series in s = (y - mu)/mu to the seventh order when |s| < 1e-5
/// (the omitted terms are of order 1e-40).
fn rc_cd(x: CD, y: CD) -> Option<CD> {
    let (mut x, mut y) = (x, y);
    for _ in 0..200 {
        let mu = (x + y.scale(2.0)) / CD::f(3.0);
        if mu.abs() == 0.0 || !mu.finite() {
            return None;
        }
        let s = (y - mu) / mu;
        if s.abs() < 1e-5 {
            let c = |n: f64, d: f64| CD::f(n) / CD::f(d);
            let poly = s * (c(9.0, 22.0) + s * (c(159.0, 208.0) + s * c(9.0, 8.0)));
            let poly = c(3.0, 8.0) + poly;
            let poly = c(1.0, 7.0) + s * poly;
            let poly = c(3.0, 10.0) + s * poly;
            let tot = CD::ONE + s * s * poly;
            return Some(tot / mu.sqrt());
        }
        let lam = x.sqrt() * y.sqrt() * CD::f(2.0) + y;
        x = (x + lam).scale(0.25);
        y = (y + lam).scale(0.25);
    }
    None
}

/// R_J(x, y, z, p) by Carlson's duplication with R_C (the algorithm of the 1995 paper, section 2; DLMF 19.36.1,
/// 19.36.4), principal roots, with the series of order 6 in the quantities delta = (a - x)/a, ... below 1e-5.
fn rj_cd(x: CD, y: CD, z: CD, p: CD) -> Option<CD> {
    let (mut x, mut y, mut z, mut p) = (x, y, z, p);
    let mut sum = CD::ZERO;
    let mut fac = 1.0f64;
    for _ in 0..200 {
        let ave = (x + y + z + p.scale(2.0)) / CD::f(5.0);
        if ave.abs() == 0.0 || !ave.finite() {
            return None;
        }
        let dx = (ave - x) / ave;
        let dy = (ave - y) / ave;
        let dz = (ave - z) / ave;
        let dp = (ave - p) / ave;
        if dx.abs().max(dy.abs()).max(dz.abs()).max(dp.abs()) < 1e-5 {
            let ea = dx * (dy + dz) + dy * dz;
            let eb = dx * dy * dz;
            let ec = dp * dp;
            let ed = ea - ec.scale(3.0);
            let ee = eb + (dp * (ea - ec)).scale(2.0);
            // the series (DLMF 19.36.1 with the symmetric functions E2..E5 of the five arguments)
            let c1 = CD::f(3.0) / CD::f(14.0);
            let c2 = CD::f(1.0) / CD::f(3.0);
            let c3 = CD::f(3.0) / CD::f(22.0);
            let c4 = CD::f(3.0) / CD::f(26.0);
            let c5 = c3.scale(0.75);
            let c6 = c4.scale(1.5);
            let c7 = c2.scale(0.5);
            let c8 = c3.scale(2.0);
            let poly = CD::ONE + ed * (-c1 + c5 * ed - c6 * ee) + eb * (c7 + dp * (-c8 + dp * c4)) + dp * ea * (c2 - dp * c3) - c2 * dp * ec;
            let tail = (poly / (ave * ave.sqrt())).scale(fac);
            return Some(sum.scale(3.0) + tail);
        }
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let lam = sx * (sy + sz) + sy * sz;
        let t = p * (sx + sy + sz) + sx * sy * sz;
        let alpha = t * t;
        let u = p + lam;
        let beta = p * u * u;
        sum = sum + rc_cd(alpha, beta)?.scale(fac);
        fac *= 0.25;
        x = (x + lam).scale(0.25);
        y = (y + lam).scale(0.25);
        z = (z + lam).scale(0.25);
        p = (p + lam).scale(0.25);
    }
    None
}

/// Is the value a real number on a cut of the integrals (so that the side matters)?
fn on_axis_beyond_one(z: CD) -> bool {
    z.im.hi == 0.0 && z.im.lo == 0.0 && z.re.hi >= 1.0
}

/// Pi(n | m) = R_F(0, 1 - m, 1) + (n/3) R_J(0, 1 - m, 1, 1 - n) (DLMF 19.25.14 at phi = pi/2) for complex machine
/// n, m off the cuts n >= 1, m >= 1 (`None` there and where the algorithm fails).
pub(crate) fn elliptic_pi_complete(n: CD, m: CD) -> Option<C64> {
    if on_axis_beyond_one(n) || on_axis_beyond_one(m) {
        return None;
    }
    let one = CD::ONE;
    let y = one - m;
    let rf = crate::fast29::carlson_rf_cd(CD::ZERO, y, one)?;
    let rj = rj_cd(CD::ZERO, y, one, one - n)?;
    let v = (rf + (n * rj) / CD::f(3.0)).to_c64();
    (v.re.is_finite() && v.im.is_finite()).then_some(v)
}

/// Pi(n; phi | m) = sin(phi) (R_F(cos^2, 1 - m sin^2, 1) + (n/3) sin^2 R_J(cos^2, 1 - m sin^2, 1, 1 - n sin^2)) (DLMF
/// 19.25.14) for -pi/2 < Re phi < pi/2, extended by Pi(n; phi + k pi | m) = Pi(n; phi | m) + 2 k Pi(n | m).
pub(crate) fn elliptic_pi_inc(n: CD, phi: CD, m: CD) -> Option<C64> {
    if on_axis_beyond_one(n) || on_axis_beyond_one(m) || phi.abs() > 1e6 {
        return None;
    }
    let k = (phi.re / dd::PI).round_hi();
    let ph = CD::new(phi.re - dd::PI.mul_f(k), phi.im);
    let part = if ph.abs() == 0.0 {
        CD::ZERO
    } else {
        let sn = ph.sin();
        let cs = ph.cos();
        let (s2, c2, one) = (sn * sn, cs * cs, CD::ONE);
        let rf = crate::fast29::carlson_rf_cd(c2, one - m * s2, one)?;
        let rj = rj_cd(c2, one - m * s2, one, one - n * s2)?;
        sn * (rf + (n * s2 * rj) / CD::f(3.0))
    };
    let tot = if k == 0.0 {
        part
    } else {
        let full = elliptic_pi_complete(n, m)?;
        part + CD::c(full).scale(2.0 * k)
    };
    let v = tot.to_c64();
    (v.re.is_finite() && v.im.is_finite()).then_some(v)
}

/// F(phi | m) for real machine phi and m > 1 where m sin^2(phi) > 1 (a complex value on the cut of the integrand, taken
/// from below as for K(m): m - i0, so 1 - m sin^2 is approached from above): F(phi + k pi) = F(phi) + 2 k K(m)
/// (DLMF 19.2.10) and F(phi) = sin(phi) R_F(cos^2, 1 - m sin^2, 1) (DLMF 19.25.5) for |phi| <= pi/2.
pub(crate) fn elliptic_f_beyond(phi: f64, m: f64) -> Option<C64> {
    if !(m > 1.0 && m < 1e300) || !phi.is_finite() || phi.abs() > 1e6 {
        return None;
    }
    let pd = DD::new(phi);
    let k = (pd / dd::PI).round_hi();
    let ph = pd - dd::PI.mul_f(k);
    let (sn, cs) = CD::sin_cos_real(ph);
    let (s2, c2) = (sn * sn, cs * cs);
    let y = CD::new(DD::ONE - DD::new(m) * s2, DD::new(1e-290));
    let part = CD::real(sn) * crate::fast29::carlson_rf_cd(CD::real(c2), y, CD::ONE)?;
    let tot = if k == 0.0 {
        part
    } else {
        let kc = elliptic_k_above_one(m)?;
        part + CD::c(kc).scale(2.0 * k)
    };
    let v = tot.to_c64();
    (v.re.is_finite() && v.im.is_finite()).then_some(v)
}

// ---------------------------------------------------------------- modular functions

/// tau reduced to the fundamental domain (|Re| <= 1/2, |tau| >= 1) by tau -> tau - n and tau -> -1/tau in
/// double-double arithmetic (the input is an exact dyadic number, so the reduction keeps 30 digits where the nome
/// series at a point near the real axis lost them all); the operations in order (`true` for -1/tau).
fn reduce_tau(tau: CD) -> Option<(CD, Vec<bool>)> {
    // below Im tau = 1e-8 the double-double reduction loses the point (modular_lambda(2.1 + 1e-300 i) gave a finite
    // value of no meaning; at a dyadic point that close to the axis the true one is exponentially close to 0, 1 or a pole)
    if !tau.finite() || tau.im.hi < 1e-8 || tau.abs() > 1e12 {
        return None;
    }
    let mut t = tau;
    let mut ops = vec![];
    for _ in 0..2000 {
        let n = t.re.round_hi();
        if n != 0.0 {
            t = CD::new(t.re - DD::new(n), t.im);
            if (n.abs() % 2.0) == 1.0 {
                ops.push(false);
            }
        }
        let n2 = t.re.hi * t.re.hi + t.im.hi * t.im.hi;
        if n2 < 1.0 - 1e-30 {
            t = -(CD::ONE / t);
            ops.push(true);
        } else {
            return Some((t, ops));
        }
    }
    None
}

/// The theta constants (th2, th3, th4) at the nome q = exp(i pi tau) for tau in the fundamental domain (|q| <= e^(-pi sqrt3/2)).
fn theta_constants(tau: CD) -> (CD, CD, CD) {
    let q = (tau.mul_i().mul_dd(dd::PI)).exp();
    let q2 = q * q;
    // q^(n^2) and q^((n + 1/2)^2) by recurrences: q^((n+1)^2) = q^(n^2) q^(2n+1)
    let q4 = (tau.mul_i().mul_dd(dd::PI.mul_f(0.25))).exp(); // q^(1/4)
    let (mut th3, mut th4, mut th2) = (CD::ONE, CD::ONE, CD::ZERO);
    let (mut t, mut e) = (CD::ONE, q); // q^(n^2), q^(2n+1)
    let (mut h, mut g) = (q4, q2); // q^((n+1/2)^2), q^(2n+2)
    th2 = th2 + h;
    for n in 1..14 {
        t = t * e;
        e = e * q2;
        let term = t.scale(2.0);
        th3 = th3 + term;
        th4 = if n % 2 == 0 { th4 + term } else { th4 - term };
        h = h * g;
        g = g * q2;
        th2 = th2 + h;
    }
    (th2.scale(2.0), th3, th4)
}

/// Klein's J(tau) = (th2^8 + th3^8 + th4^8)^3/(54 (th2 th3 th4)^8) at a point of the upper half-plane, by the reduction
/// to the fundamental domain (J(tau + 1) = J(tau) = J(-1/tau)).
pub(crate) fn klein_j(tau: CD) -> Option<C64> {
    let (t, _) = reduce_tau(tau)?;
    let (t2, t3, t4) = theta_constants(t);
    let p8 = |x: CD| {
        let x2 = x * x;
        let x4 = x2 * x2;
        x4 * x4
    };
    let s = p8(t2) + p8(t3) + p8(t4);
    let num = s * s * s;
    let den = (p8(t2 * t3 * t4)).scale(54.0);
    let v = (num / den).to_c64();
    (v.re.is_finite() && v.im.is_finite()).then_some(v)
}

/// The modular lambda function lambda(tau) = (th2/th3)^4: lambda(tau - 1) = lambda/(lambda - 1) and lambda(-1/tau) =
/// 1 - lambda, both involutions, so lambda(tau) is the reduced value mapped back through the recorded operations.
pub(crate) fn modular_lambda(tau: CD) -> Option<C64> {
    let (t, ops) = reduce_tau(tau)?;
    let (t2, t3, _) = theta_constants(t);
    let r = t2 / t3;
    let r2 = r * r;
    let mut x = r2 * r2;
    for &s_op in ops.iter().rev() {
        x = if s_op { CD::ONE - x } else { x / (x - CD::ONE) };
    }
    let v = x.to_c64();
    (v.re.is_finite() && v.im.is_finite()).then_some(v)
}

/// K(m) for a real machine m > 1, on the cut from below (Im m = -0): K(m) = (K(1/m) - i K(1 - 1/m))/sqrt(m)
/// (DLMF 19.7.3 with the sign of the lower side), the real K by the double-double AGM; `None` for other m.
pub(crate) fn elliptic_k_above_one(m: f64) -> Option<C64> {
    if !(m > 1.0 && m < 1e300) {
        return None;
    }
    let md = DD::new(m);
    let inv = DD::ONE / md;
    let a = crate::fast29::k_cd(CD::real(inv))?.re;
    let b = crate::fast29::k_cd(CD::real(DD::ONE - inv))?.re;
    let rt = md.sqrt();
    let v = CD::new(a / rt, -(b / rt)).to_c64();
    (v.re.is_finite() && v.im.is_finite()).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn klein_j_and_lambda_at_known_points() {
        // J(i) = 1, J(rho) = 0, J(2 i) = 66^3/1728, lambda(i) = 1/2, and the invariance under tau -> tau + 1, -1/tau
        let j = klein_j(CD::c(C64::new(0.0, 1.0))).unwrap();
        assert!((j.re - 1.0).abs() < 1e-14 && j.im.abs() < 1e-14, "{j:?}");
        let j = klein_j(CD::c(C64::new(0.0, 2.0))).unwrap();
        assert!((j.re - 66.0f64.powi(3) / 1728.0).abs() < 1e-11 * j.re.abs(), "{j:?}");
        let a = klein_j(CD::c(C64::new(0.3, 0.7))).unwrap();
        let b = klein_j(CD::c(C64::new(1.3, 0.7))).unwrap();
        assert!((a.re - b.re).hypot(a.im - b.im) < 1e-13 * a.re.hypot(a.im));
        let l = modular_lambda(CD::c(C64::new(0.0, 1.0))).unwrap();
        assert!((l.re - 0.5).abs() < 1e-15 && l.im.abs() < 1e-15, "{l:?}");
        // lambda(tau) + lambda(-1/tau) = 1
        let (x, y) = (modular_lambda(CD::c(C64::new(0.2, 0.9))).unwrap(), modular_lambda(CD::c(C64::new(-0.2 / 0.85, 0.9 / 0.85))).unwrap());
        assert!((x.re + y.re - 1.0).abs() < 1e-14 && (x.im + y.im).abs() < 1e-14, "{x:?} {y:?}");
    }

    fn close(a: C64, b: C64, tol: f64) -> bool {
        (a.re - b.re).hypot(a.im - b.im) <= tol * (1.0 + b.re.hypot(b.im))
    }

    #[test]
    fn k_above_one_matches_the_ball_value() {
        let v = elliptic_k_above_one(3.0).unwrap();
        assert!((v.re - 1.0010773804561062).abs() < 3e-16 && (v.im + 1.17142008414677).abs() < 3e-16, "{v:?}");
    }

    #[test]
    fn agrees_with_the_defining_identities() {
        // sn^2 + cn^2 = 1, dn^2 + m sn^2 = 1, and addition at the real axis against the real kernel
        for &m in &[0.1, 0.3, 0.5, 0.7, 0.95, -2.0, 3.0] {
            for &(x, y) in &[(0.3, 0.2), (1.7, -0.9), (-2.5, 3.1), (0.1, 5.0), (7.0, 0.4)] {
                let (s, c, d) = jacobi(CD::c(C64::new(x, y)), DD::new(m)).expect("value");
                let one = (s * s + c * c).to_c64();
                assert!(close(one, C64::real(1.0), 1e-14), "m {m} u {x} {y}: {one:?}");
                let one = (d * d + s * s.scale(m)).to_c64();
                assert!(close(one, C64::real(1.0), 1e-14), "m {m} u {x} {y}: {one:?}");
            }
        }
        for &m in &[0.2, 0.8, -1.0, 4.0] {
            let u = 0.7;
            let (s, c, d) = jacobi(CD::f(u), DD::new(m)).expect("value");
            assert!((s.re.to_f64() - crate::machine::elliptic::jacobi_sn(u, m)).abs() < 1e-15);
            assert!((c.re.to_f64() - crate::machine::elliptic::jacobi_cn(u, m)).abs() < 1e-15);
            assert!((d.re.to_f64() - crate::machine::elliptic::jacobi_dn(u, m)).abs() < 1e-15);
        }
    }
}
