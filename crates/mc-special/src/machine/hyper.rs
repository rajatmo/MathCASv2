//! Hypergeometric functions of machine reals, in double-double with one final rounding (decision SF.3).
//!
//! - **The series** pFq(a; b; z) = sum_k prod (a_i)_k / prod (b_j)_k z^k / k! is summed in double-double with a
//!   running binary exponent (so terms beyond the double range do not overflow early); it stops when the next term
//!   bound t r / (1 - r) is below 2^-110 of the sum, r the term ratio once k exceeds twice the largest parameter
//!   (the ratio is then monotone). An exact non-positive integer a_i makes it a finite sum; a non-positive integer
//!   b_j reached before that is a pole (NaN).
//! - **2F1**, z < 1: |z| <= 1/2 the series; -1 <= z < -1/2 Pfaff's (1-z)^-a 2F1(a, c-b; c; z/(z-1)) (argument in
//!   (1/3, 1/2]); 1/2 < z < 1 the connection formula in 1 - z (DLMF 15.8.4)
//!   A 2F1(a, b; a+b-c+1; 1-z) + B (1-z)^(c-a-b) 2F1(c-a, c-b; c-a-b+1; 1-z); z < -1 Pfaff then that formula in
//!   1 - z/(z-1) = 1/(1-z). When c-a-b (b-a for z < -1) is within 2^-17 of an integer m the formula is a 0/0
//!   limit: it is evaluated at c-a-b = m + t for t = ±2^-18, ±2^-17 (b moved, exactly in double-double) and the
//!   cubic through those four values is taken at the actual offset (interpolation error about 2^-72 relative, the
//!   cancellation costs 18 of the 106 bits). z = 1: Gauss's Gamma(c)Gamma(c-a-b)/(Gamma(c-a)Gamma(c-b)) for
//!   c-a-b > 0 (a signed infinity otherwise). Terminating series are summed as polynomials (Pfaff's form for
//!   z < -1/2, so the argument is in (0, 1)).
//! - **1F1** (Kummer's M): z >= 0 the series (positive terms for a, b > 0; about z terms); z < 0 Kummer's
//!   e^z M(b-a, b, -z) (the alternating series would cancel to about e^-2|z|); z > 700 (a non-terminating) the
//!   asymptotic expansion Gamma(b)/Gamma(a) e^z z^(a-b) sum_k (b-a)_k (1-a)_k / (k! z^k) when its smallest term
//!   reaches 2^-110 (the other part is relatively below e^-700).
//! - **0F1**: the series for z >= -232 (cancellation at most e^(2 sqrt|z|) < 2^44 of the 106 bits); below, the
//!   Bessel form Gamma(b) (x/2)^(1-b) J_(b-1)(x), x = 2 sqrt(-z), with Hankel's expansion in double-double (remainder
//!   below the first omitted term), or the machine J when the expansion does not reach 2^-90.
//! - **pFq** in general: the series for p <= q (any z) and p = q + 1 (|z| < 1); 1F0 = (1-z)^-a, 0F0 = e^z; z = 1
//!   for p = q + 1 only through Gauss (2F1) or a finite sum; p > q + 1 only finite sums. Otherwise NaN.
//!
//! Accuracy: within a few ulps where the function is well conditioned. Cancellation that double-double does not
//! absorb: 1F1 and pFq with large negative parameters (alternating terms much larger than the sum: Laguerre-type
//! polynomials at large z), 2F1 with large parameters of mixed sign, values near the real zeros (these are
//! ill-conditioned); the tests measure those relative to the sum of the absolute values of the terms.
use super::gamma::gamma_scaled;
use crate::dd::{exp_scaled, log, sin_cos_pi_any, Scaled, DD, PI};

/// The relative size of a negligible term (about 2^-110).
const TINY: f64 = 7.7e-34;
/// The largest number of series terms.
const MAX_TERMS: u64 = 2_000_000;
/// The rescaling step of the running exponent.
const RESCALE: i32 = 600;

/// A non-positive integer value of a double-double, as m >= 0.
fn nonpos_int(x: DD) -> Option<u64> {
    (x.lo == 0.0 && x.hi <= 0.0 && x.hi == x.hi.trunc() && x.hi > -1e15).then_some((-x.hi) as u64)
}

/// The series sum_k prod (a_i)_k / prod (b_j)_k x^k / k! as a scaled double-double, with log2 of the largest term
/// relative to 1 (`None` at a pole of the b's or without convergence).
fn series(a: &[DD], b: &[DD], x: DD) -> Option<(Scaled, f64)> {
    let stop = a.iter().filter_map(|&v| nonpos_int(v)).min();
    let pmax = a.iter().chain(b).map(|v| v.hi.abs()).fold(0.0, f64::max);
    let xa = x.hi.abs();
    let b_pos = b.iter().all(|v| v.hi > 0.0);
    // the terms of a non-terminating series with p <= q grow until k is about |x|^(1/(q + 1 - p)): beyond the term
    // limit there is no value (answered at once instead of after MAX_TERMS terms)
    if stop.is_none() && a.len() <= b.len() && xa.powf(1.0 / (b.len() + 1 - a.len()) as f64) > MAX_TERMS as f64 {
        return None;
    }
    let (mut t, mut s, mut e) = (DD::ONE, DD::ONE, 0i64);
    let mut big = 0.0f64;
    if x.hi == 0.0 {
        return Some((Scaled::new(DD::ONE), 0.0));
    }
    for k in 0..MAX_TERMS {
        if Some(k) == stop {
            break;
        }
        let kf = k as f64;
        let mut num = x;
        for ai in a {
            num = num * ai.add_f(kf);
        }
        let mut den = DD::new(kf + 1.0);
        for bj in b {
            let d = bj.add_f(kf);
            if d.hi == 0.0 {
                return None;
            }
            den = den * d;
        }
        t = t * num / den;
        s = s + t;
        let ta = t.hi.abs();
        big = big.max(ta.log2() + e as f64);
        if s.hi.abs() > crate::dd::pow2(RESCALE) || ta > crate::dd::pow2(RESCALE) {
            s = s.ldexp(-RESCALE);
            t = t.ldexp(-RESCALE);
            e += RESCALE as i64;
        }
        if !t.hi.is_finite() || !s.hi.is_finite() {
            return None;
        }
        if stop.is_none() && b_pos && a.len() <= b.len() + 1 {
            // every b_j > 0: |k + b_j| = k + b_j from k = 0, so the ratio bound (k + |a|)/(k + b) <= max(1, its
            // value now) holds from here on, whatever the size of the parameters (a lower parameter near 1e308
            // settles after a few terms instead of running to the term limit)
            let mut r = xa;
            for (i, bj) in b.iter().enumerate() {
                match a.get(i) {
                    Some(ai) => r *= ((kf + ai.hi.abs()) / (kf + bj.hi)).max(1.0),
                    None => r /= kf + bj.hi,
                }
            }
            if a.len() == b.len() + 1 {
                r *= ((kf + a[b.len()].hi.abs()) / (kf + 1.0)).max(1.0);
            } else {
                r /= kf + 1.0;
            }
            if (r < 1.0 && ta * r / (1.0 - r) <= TINY * s.hi.abs()) || ta == 0.0 {
                return Some((Scaled { m: s, e }.norm(), big));
            }
        } else if stop.is_none() && kf > 2.0 * pmax + 2.0 {
            // the next ratio, monotone from here on
            let k1 = kf + 1.0;
            let mut r = xa / (k1 + 1.0);
            for ai in a {
                r *= (k1 + ai.hi).abs();
            }
            for bj in b {
                r /= (k1 + bj.hi).abs();
            }
            if r < 1.0 && ta * r / (1.0 - r) <= TINY * s.hi.abs() || ta == 0.0 {
                return Some((Scaled { m: s, e }.norm(), big));
            }
        }
    }
    if stop.is_some() {
        Some((Scaled { m: s, e }.norm(), big))
    } else {
        None
    }
}

/// 1/Gamma(x) as a scaled double-double (zero at the poles).
fn rgamma(x: DD) -> Scaled {
    if nonpos_int(x).is_some() {
        return Scaled::new(DD::ZERO);
    }
    Scaled::new(DD::ONE) / gamma_scaled(x)
}

/// Gamma(x), `None` at a pole.
fn gamma(x: DD) -> Option<Scaled> {
    nonpos_int(x).is_none().then(|| gamma_scaled(x))
}

/// y^p for y > 0 as a scaled double-double.
fn pow_pos(y: DD, p: DD) -> Scaled {
    exp_scaled(log(y) * p)
}

fn fin(s: Option<Scaled>) -> f64 {
    s.map_or(f64::NAN, |v| v.to_f64())
}

// ---------------------------------------------------------------- 2F1

/// The connection formula in 1 - z for 1/2 < z < 1 (`omz` = 1 - z), c - a - b not an integer.
fn f21_connection(a: DD, b: DD, c: DD, omz: DD) -> Option<Scaled> {
    let s = c - a - b;
    let gc = gamma(c)?;
    let ca = rgamma(c - a);
    let cb = rgamma(c - b);
    let ra = rgamma(a);
    let rb = rgamma(b);
    let mut out = Scaled::new(DD::ZERO);
    if ca.m.hi != 0.0 && cb.m.hi != 0.0 {
        let f1 = series(&[a, b], &[DD::ONE - s], omz)?.0;
        out = out + gc * gamma(s)? * ca * cb * f1;
    }
    if ra.m.hi != 0.0 && rb.m.hi != 0.0 {
        let f2 = series(&[c - a, c - b], &[s.add_f(1.0)], omz)?.0;
        out = out + gc * gamma(-s)? * ra * rb * pow_pos(omz, s) * f2;
    }
    Some(out)
}

/// The connection formula with the degenerate case: c-a-b within 2^-17 of an integer m by cubic interpolation in
/// t = c-a-b-m through t = ±2^-18, ±2^-17 (b moved).
fn f21_near_one(a: DD, b: DD, c: DD, omz: DD) -> Option<Scaled> {
    let s = c - a - b;
    let m = s.hi.round();
    let delta = s.add_f(-m);
    let eps = crate::dd::pow2(-18);
    if delta.hi.abs() > 2.0 * eps {
        return f21_connection(a, b, c, omz);
    }
    // b_t = c - a - m - t, so c - a - b_t = m + t
    let b0 = (c - a).add_f(-m);
    let nodes = [-2.0 * eps, -eps, eps, 2.0 * eps];
    let mut vals = [Scaled::new(DD::ZERO); 4];
    for (v, &t) in vals.iter_mut().zip(&nodes) {
        *v = f21_connection(a, b0.add_f(-t), c, omz)?;
    }
    // Lagrange at delta
    let mut out = Scaled::new(DD::ZERO);
    for i in 0..4 {
        let mut w = DD::ONE;
        for j in 0..4 {
            if i != j {
                w = w * delta.add_f(-nodes[j]) / DD::new(nodes[i] - nodes[j]);
            }
        }
        out = out + vals[i].mul_dd(w);
    }
    Some(out)
}

/// 2F1(a, b; c; z) for z < 1 (`omz` = 1 - z exactly), not terminating.
fn f21_general(a: DD, b: DD, c: DD, z: DD, omz: DD) -> Option<Scaled> {
    if z.hi.abs() <= 0.5 {
        return Some(series(&[a, b], &[c], z)?.0);
    }
    if z.hi > 0.5 {
        return f21_near_one(a, b, c, omz);
    }
    // Pfaff: (1-z)^-a 2F1(a, c-b; c; w), w = z/(z-1) = 1 - 1/(1-z)
    let pre = pow_pos(omz, -a);
    let w_omz = omz.recip();
    let w = DD::ONE - w_omz;
    let f = if w.hi <= 0.5 { series(&[a, c - b], &[c], w)?.0 } else { f21_near_one(a, c - b, c, w_omz)? };
    Some(pre * f)
}

/// 2F1 with double-double parameters, z < 1.
fn f21(a: DD, b: DD, c: DD, z: DD, omz: DD) -> Option<Scaled> {
    let ta = nonpos_int(a);
    let tb = nonpos_int(b);
    let term = match (ta, tb) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    };
    if let Some(m) = nonpos_int(c) {
        if term.is_none_or(|t| t > m) {
            return None;
        }
    }
    if term.is_some() {
        if z.hi >= -0.5 {
            return Some(series(&[a, b], &[c], z)?.0);
        }
        // Pfaff with the terminating parameter first: a finite sum in w = z/(z-1) in (1/3, 1)
        let (p, q) = if ta.is_some() && (tb.is_none() || ta <= tb) { (a, b) } else { (b, a) };
        let w = DD::ONE - omz.recip();
        return Some(pow_pos(omz, -p) * series(&[p, c - q], &[c], w)?.0);
    }
    f21_general(a, b, c, z, omz)
}

/// The Gauss hypergeometric function 2F1(a, b; c; z) for real z <= 1 (NaN for z > 1, at poles c = 0, -1, ...
/// that the series reaches, and where the value is complex).
pub fn hyp_2f1(a: f64, b: f64, c: f64, z: f64) -> f64 {
    if [a, b, c, z].iter().any(|v| !v.is_finite()) || z > 1.0 {
        return f64::NAN;
    }
    let (ad, bd, cd) = (DD::new(a), DD::new(b), DD::new(c));
    // 2F1(a, b; b; z) = (1 - z)^-a (DLMF 15.4.6)
    if z < 1.0 && nonpos_int(cd).is_none() && (b == c || a == c) {
        let e = if b == c { ad } else { bd };
        return pow_pos(DD::sum(1.0, -z), -e).to_f64();
    }
    if z == 1.0 {
        let term = nonpos_int(ad).or(nonpos_int(bd));
        if term.is_some() {
            return fin(f21(ad, bd, cd, DD::ONE, DD::ZERO));
        }
        let s = cd - ad - bd;
        if s.hi > 0.0 {
            let Some(gc) = gamma(cd) else { return f64::NAN };
            let Some(gs) = gamma(s) else { return f64::NAN };
            return (gc * gs * rgamma(cd - ad) * rgamma(cd - bd)).to_f64();
        }
        // divergent: the sign of the late terms, Gamma(c)/(Gamma(a)Gamma(b))
        let Some(gc) = gamma(cd) else { return f64::NAN };
        let sg = (gc * rgamma(ad) * rgamma(bd)).m.hi;
        return if sg == 0.0 { f64::NAN } else { f64::INFINITY.copysign(sg) };
    }
    fin(f21(ad, bd, cd, DD::new(z), DD::sum(1.0, -z)))
}

// ---------------------------------------------------------------- 1F1

/// Kummer's M(a, b, z) by its asymptotic expansion for large z > 0 (`None` if its terms do not reach 2^-110).
fn m11_asymptotic(a: DD, b: DD, z: DD) -> Option<Scaled> {
    let ba = b - a;
    let oa = DD::ONE - a;
    let r = z.recip();
    let (mut t, mut s) = (DD::ONE, DD::ONE);
    let mut prev = f64::INFINITY;
    for k in 0..400 {
        let kf = k as f64;
        t = t * ba.add_f(kf) * oa.add_f(kf) * r / DD::new(kf + 1.0);
        let ta = t.hi.abs();
        if ta > prev {
            return None;
        }
        s = s + t;
        if ta <= TINY * s.hi.abs() {
            let pre = gamma(b)? * rgamma(a) * exp_scaled(z + log(z) * (a - b));
            return Some(pre.mul_dd(s));
        }
        prev = ta;
    }
    None
}

/// M(a, b, z) with double-double parameters.
fn m11(a: DD, b: DD, z: DD) -> Option<Scaled> {
    let term = nonpos_int(a);
    if let Some(m) = nonpos_int(b) {
        if term.is_none_or(|t| t > m) {
            return None;
        }
    }
    if z.hi == 0.0 || a.hi == 0.0 {
        return Some(Scaled::new(DD::ONE));
    }
    if z.hi < 0.0 && term.is_none() {
        // Kummer's transformation
        return Some(exp_scaled(z) * m11(b - a, b, -z)?);
    }
    if z.hi > 700.0 && term.is_none() {
        if let Some(v) = m11_asymptotic(a, b, z) {
            return Some(v);
        }
    }
    Some(series(&[a], &[b], z)?.0)
}

/// Kummer's confluent hypergeometric function M(a, b, z) = 1F1(a; b; z) (NaN at poles b = 0, -1, ... that the
/// series reaches; IEEE overflow).
pub fn hyp_1f1(a: f64, b: f64, z: f64) -> f64 {
    if [a, b, z].iter().any(|v| !v.is_finite()) {
        return f64::NAN;
    }
    fin(m11(DD::new(a), DD::new(b), DD::new(z)))
}

// ---------------------------------------------------------------- 0F1

/// J_nu(x) by Hankel's expansion in double-double for large x (`None` when the terms do not reach 2^-90).
fn j_hankel(nu: DD, x: DD) -> Option<DD> {
    let mu = nu.sqr().mul_f(4.0);
    let r = x.recip();
    let (mut p, mut q) = (DD::ONE, DD::ZERO);
    let mut t = DD::ONE;
    let mut prev = f64::INFINITY;
    let mut ok = false;
    for k in 1..200 {
        let j = (2 * k - 1) as f64;
        // a_k / x^k = a_(k-1)/x^(k-1) (mu - (2k-1)^2) / (8 k x)
        t = t * mu.add_f(-j * j) * r / DD::new(8.0 * k as f64);
        let ta = t.hi.abs();
        if ta > prev && k as f64 > nu.hi.abs() {
            break;
        }
        // P = sum (-1)^i a_2i / x^2i, Q = sum (-1)^i a_(2i+1) / x^(2i+1)
        match k % 4 {
            0 => p = p + t,
            1 => q = q + t,
            2 => p = p - t,
            _ => q = q - t,
        }
        if ta < 1e-28 {
            ok = true;
            break;
        }
        prev = ta;
    }
    if !ok {
        return None;
    }
    // chi = x - (nu/2 + 1/4) pi
    let chi_pi = x / PI - nu.mul_f(0.5).add_f(0.25);
    let (sn, cs) = sin_cos_pi_any(chi_pi);
    let amp = (DD::new(2.0) / (PI * x)).sqrt();
    Some(amp * (p * cs - q * sn))
}

/// 0F1(; b; z) with a double-double b.
fn f01(b: DD, z: DD) -> Option<Scaled> {
    if nonpos_int(b).is_some() {
        return None;
    }
    if z.hi >= -232.0 {
        return Some(series(&[], &[b], z)?.0);
    }
    // Gamma(b) (x/2)^(1-b) J_(b-1)(x), x = 2 sqrt(-z)
    let x = (-z).sqrt().mul_f(2.0);
    let nu = b.add_f(-1.0);
    let pre = gamma(b)? * pow_pos(x.mul_f(0.5), -nu);
    match j_hankel(nu, x) {
        Some(j) => Some(pre.mul_dd(j)),
        None => {
            let j = super::bessel_j::bessel_j(nu.hi, x.hi);
            Some(pre.mul_dd(DD::new(j)))
        }
    }
}

/// The confluent hypergeometric limit function 0F1(; b; z) (NaN at b = 0, -1, ...).
pub fn hyp_0f1(b: f64, z: f64) -> f64 {
    if !b.is_finite() || !z.is_finite() {
        return f64::NAN;
    }
    fin(f01(DD::new(b), DD::new(z)))
}

// ---------------------------------------------------------------- pFq

/// The generalized hypergeometric function pFq(a; b; z) for real parameters and z: p <= q any z; p = q + 1 for
/// |z| < 1 (2F1 for z <= 1 by its transformations); finite sums otherwise; NaN where not supported.
pub fn hyp_pfq(a: &[f64], b: &[f64], z: f64) -> f64 {
    if a.iter().chain(b).any(|v| !v.is_finite()) || !z.is_finite() {
        return f64::NAN;
    }
    // equal parameters in a and b cancel
    let mut av: Vec<f64> = a.to_vec();
    let mut bv: Vec<f64> = Vec::new();
    for &x in b {
        if let Some(i) = av.iter().position(|&y| y == x) {
            av.remove(i);
        } else {
            bv.push(x);
        }
    }
    match (av.len(), bv.len()) {
        (2, 1) => return hyp_2f1(av[0], av[1], bv[0], z),
        (1, 1) => return hyp_1f1(av[0], bv[0], z),
        (0, 1) => return hyp_0f1(bv[0], z),
        (0, 0) => return z.exp(),
        (1, 0) if z < 1.0 => return pow_pos(DD::sum(1.0, -z), DD::new(-av[0])).to_f64(),
        _ => {}
    }
    let ad: Vec<DD> = av.iter().map(|&v| DD::new(v)).collect();
    let bd: Vec<DD> = bv.iter().map(|&v| DD::new(v)).collect();
    let terminating = ad.iter().any(|&v| nonpos_int(v).is_some());
    let (p, q) = (ad.len(), bd.len());
    if terminating || p <= q || (p == q + 1 && z.abs() < 1.0) {
        return fin(series(&ad, &bd, DD::new(z)).map(|v| v.0));
    }
    f64::NAN
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ulps(got: f64, want: f64) -> f64 {
        ((got - want) / want).abs() / f64::EPSILON
    }

    #[test]
    fn known_values() {
        // 2F1(1, 1; 2; z) = -log(1-z)/z
        for z in [0.3f64, -0.7, 0.9, -5.0, -1e6] {
            let want = -(-z).ln_1p() / z;
            assert!(ulps(hyp_2f1(1.0, 1.0, 2.0, z), want) <= 4.0, "{z}");
        }
        // 1F1(1; 2; z) = (e^z - 1)/z
        for z in [0.5f64, -3.0, 20.0, -40.0] {
            let want = z.exp_m1() / z;
            assert!(ulps(hyp_1f1(1.0, 2.0, z), want) <= 4.0, "{z}");
        }
        // 0F1(; 1/2; -x^2/4) = cos x
        for x in [1.0f64, 10.0, 40.0] {
            assert!((hyp_0f1(0.5, -x * x / 4.0) - x.cos()).abs() <= 4.0 * f64::EPSILON, "{x}");
        }
        // Gauss: 2F1(1/2, 1/3; 2; 1) = Gamma(2)Gamma(7/6)/(Gamma(3/2)Gamma(5/3)) = 1.15959526696392836577...
        let want = 1.159_595_266_963_928_4;
        assert!(ulps(hyp_2f1(0.5, 1.0 / 3.0, 2.0, 1.0), want) <= 8.0);
        assert!(hyp_2f1(1.0, 1.0, 2.0, 1.5).is_nan());
        assert!(hyp_1f1(1.0, -2.0, 1.0).is_nan());
        // a terminating series before the pole: 1 + 4/3 + 2/3
        assert!(ulps(hyp_1f1(-2.0, -3.0, 2.0), 3.0) <= 1.0);
    }
}
