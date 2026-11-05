//! zeta(s) for machine reals: Euler-Maclaurin in double-double for s >= 0 (M = 16 correction terms, N the least
//! with the remainder bound below 2^-80), the functional equation zeta(s) = 2 (2 pi)^(s-1) sin(pi s/2)
//! Gamma(1 - s) zeta(1 - s) for s < 0 (exact zeros at the negative even integers, scaled so that it neither
//! overflows nor underflows early).
use super::gamma::{em_coefficients, gamma_scaled, lgamma_stirling};
use crate::dd::{exp, exp_scaled, log, sin_cos_pi_any, DD, LN_2PI};

const M: u64 = 16;

/// log2 of the Euler-Maclaurin remainder bound (see [`crate::ball::zeta`]).
fn bound_log2(s: f64, n: f64) -> f64 {
    let mut l = 2.0;
    for i in 0..2 * M {
        l += (s + i as f64).abs().max(1e-300).log2();
    }
    l -= 2.0 * M as f64 * (2.0 * std::f64::consts::PI).log2();
    l += (1.0 - s - 2.0 * M as f64) * n.log2();
    l - (s + 2.0 * M as f64 - 1.0).log2()
}

/// zeta(s) for a double-double s >= 0, s != 1.
pub fn zeta_dd(s: DD) -> DD {
    let mut n = 6.0;
    while bound_log2(s.hi, n) > -80.0 && n < 1e4 {
        n += 2.0;
    }
    let mut sum = DD::ONE;
    let mut k = 2.0;
    while k < n {
        sum = sum.add(exp(log(DD::new(k)).mul(s).neg()));
        k += 1.0;
    }
    let nb = DD::new(n);
    let n_s = exp(log(nb).mul(s).neg());
    sum = sum.add(nb.mul(n_s).div(s.add_f(-1.0)));
    sum = sum.add(n_s.mul_f(0.5));
    let r = nb.recip();
    let r2 = r.sqr();
    let mut poch = s;
    let mut pw = n_s.mul(r);
    for (j, c) in em_coefficients().iter().take(M as usize).enumerate() {
        let j = j as f64 + 1.0;
        sum = sum.add(c.mul(poch).mul(pw));
        poch = poch.mul(s.add_f(2.0 * j - 1.0)).mul(s.add_f(2.0 * j));
        pw = pw.mul(r2);
    }
    sum
}

/// zeta(s) for a machine real (zeta(1) is NaN: the pole is two-sided).
pub fn zeta(s: f64) -> f64 {
    if s.is_nan() || s == 1.0 || s == f64::NEG_INFINITY {
        return f64::NAN;
    }
    if s >= 64.0 {
        // 1 + 2^-s + 3^-s ... rounds to 1 + 2^-s
        return 1.0 + (-s).exp2();
    }
    if s >= 0.0 {
        return zeta_dd(DD::new(s)).to_f64();
    }
    let (sin, _) = sin_cos_pi_any(DD::new(s * 0.5));
    if sin.hi == 0.0 {
        return 0.0;
    }
    let one_minus = DD::sum(1.0, -s);
    let sm1 = DD::sum(s, -1.0);
    // Gamma(1 - s) (2 pi)^(s - 1)
    let g = if one_minus.hi >= 20.0 {
        exp_scaled(lgamma_stirling(one_minus).add(sm1.mul(LN_2PI)))
    } else {
        gamma_scaled(one_minus).mul(exp_scaled(sm1.mul(LN_2PI)))
    };
    g.mul_dd(sin.mul_f(2.0)).mul_dd(zeta_dd(one_minus)).to_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ulps(got: f64, want: f64) -> f64 {
        ((got - want) / want).abs() / f64::EPSILON
    }

    #[test]
    fn known_values() {
        assert!(ulps(zeta(2.0), std::f64::consts::PI.powi(2) / 6.0) <= 1.0);
        assert!(ulps(zeta(3.0), 1.202_056_903_159_594_2) <= 1.0);
        assert!(ulps(zeta(0.5), -1.460_354_508_809_586_8) <= 1.0);
        assert_eq!(zeta(0.0), -0.5);
        assert!(ulps(zeta(-1.0), -1.0 / 12.0) <= 1.0);
        assert_eq!(zeta(-2.0), 0.0);
        assert!(ulps(zeta(-3.0), 1.0 / 120.0) <= 1.0);
        assert!(zeta(1.0).is_nan());
        assert_eq!(zeta(100.0), 1.0);
        assert!(zeta(-400.5).is_infinite());
    }
}
