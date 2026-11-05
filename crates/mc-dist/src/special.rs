//! Machine special functions the distributions need: the error function family, the regularized incomplete gamma
//! and beta functions, and log-gamma in double-double.
//!
//! STOPGAP (decision DI.2): package 3C adds `erf`, `erfc`, `inverse_erf`, `inverse_erfc` and the incomplete gamma
//! and beta functions to `mc-special` in parallel with this package. Until both are merged these private
//! machine versions serve the numeric paths of this crate; afterwards the coordinator can switch the bodies of
//! [`erfc_dd`], [`gamma_pq`] and [`beta_i`] to `mc-special`'s functions (the signatures here are what the
//! distributions need: both tails at once, for accuracy in either tail). Symbolic results never use these: they
//! are written with `erf`, `erfc`, `gamma(a, x)` and `beta(z, a, b)` calls.
//!
//! Accuracy (measured in the crate's tests against reference values): erfc to a few ulps over its range,
//! including the far tail; the incomplete functions to about 1e-14 relative for parameters up to 10^4 (the
//! prefactors x^a e^-x / Gamma(a) and x^a (1-x)^b / B(a, b) are formed in double-double, so large parameters do
//! not cancel; the continued fractions and series are plain f64).
use mc_special::dd::{self, DD};
use mc_special::machine::gamma::lgamma_re;

/// sqrt(pi) as a double-double (computed, not copied: a few operations).
fn sqrt_pi() -> DD {
    dd::PI.sqrt()
}

/// 1/sqrt(2) as a double-double.
fn frac_1_sqrt_2() -> DD {
    DD::new(0.5).sqrt()
}

/// sqrt(2 pi) as a double-double.
fn sqrt_2pi() -> DD {
    dd::PI.mul_f(2.0).sqrt()
}

/// Iteration cap of the series and continued fractions (never reached for parameters below 10^8).
const MAX_ITER: usize = 1 << 20;

/// erf(x) for |x| < 4 by the series without cancellation (all terms positive):
/// erf(x) = 2x e^{-x^2} / sqrt(pi) * sum_n (2x^2)^n / (1 3 5 ... (2n+1)), in double-double.
fn erf_series(x: DD) -> DD {
    let x2 = x.sqr();
    let t2 = x2.mul_f(2.0);
    let mut term = DD::ONE;
    let mut sum = DD::ONE;
    let mut n = 0.0;
    while n < 400.0 {
        n += 1.0;
        term = (term * t2).div_f(2.0 * n + 1.0);
        sum = sum + term;
        if term.hi < 1e-34 * sum.hi {
            break;
        }
    }
    x.mul_f(2.0) * dd::exp(-x2) * sum / sqrt_pi()
}

/// The continued fraction of erfc for x >= 4, without the factor e^{-x^2}: sqrt(pi) e^{x^2} erfc(x) =
/// 1/(x + (1/2)/(x + 1/(x + (3/2)/(x + ...)))), evaluated by the modified Lentz method.
fn erfc_cf_scaled(x: f64) -> f64 {
    const TINY: f64 = 1e-300;
    let mut f = x;
    let mut c = x;
    let mut d = 0.0;
    for n in 1..MAX_ITER {
        let a = n as f64 / 2.0;
        d = x + a * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = x + a / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = c * d;
        f *= delta;
        if (delta - 1.0).abs() < 1e-17 {
            break;
        }
    }
    1.0 / (f * std::f64::consts::PI.sqrt())
}

/// erfc(x) for a double-double argument (so that callers can pass (mu - x)/(sqrt(2) sigma) without a rounding
/// of the division magnifying in the far tail).
pub fn erfc_dd(x: DD) -> f64 {
    if x.hi.is_nan() {
        return f64::NAN;
    }
    if x.hi < 0.0 {
        return 2.0 - erfc_dd(-x);
    }
    if x.hi < 4.0 {
        return (DD::ONE - erf_series(x)).to_f64();
    }
    if x.hi > 27.3 {
        return 0.0;
    }
    // e^{-x^2} with x^2 in double-double: the rounding of x^2 would cost x^2 ulps
    dd::exp(-x.sqr()).to_f64() * erfc_cf_scaled(x.hi)
}

/// erfc(x).
pub fn erfc(x: f64) -> f64 {
    erfc_dd(DD::new(x))
}

/// erf(x).
pub fn erf(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x.abs() < 4.0 {
        return erf_series(DD::new(x)).to_f64();
    }
    1.0_f64.copysign(x) - erfc(x.abs()).copysign(x)
}

/// e^{x^2} erfc(x) for x >= 0 (the scaled complementary error function, which stays finite in the tail).
pub fn erfcx(x: f64) -> f64 {
    if x < 4.0 {
        let xd = DD::new(x);
        return (dd::exp(xd.sqr()) * (DD::ONE - erf_series(xd))).to_f64();
    }
    erfc_cf_scaled(x)
}

/// The standard normal cdf Phi(z) = erfc(-z/sqrt(2))/2, the division by sqrt(2) in double-double.
pub fn norm_cdf(z: f64) -> f64 {
    erfc_dd(DD::new(-z) * frac_1_sqrt_2()) / 2.0
}

/// The standard normal density.
pub fn norm_pdf(z: f64) -> f64 {
    (dd::exp(-DD::new(z).sqr().mul_f(0.5)) / sqrt_2pi()).to_f64()
}

/// Phi(z)/phi(z) for z <= 0 (Mills' ratio of the lower tail), finite in the far tail.
fn mills_lower(z: f64) -> f64 {
    (2.0 * std::f64::consts::PI).sqrt() / 2.0 * erfcx(-z * std::f64::consts::FRAC_1_SQRT_2)
}

/// The standard normal quantile Phi^-1(q), 0 < q < 1: Acklam's rational start (relative error about 1e-9),
/// then Halley steps on the lower tail of min(q, 1 - q) with the residual taken relative to q, so the far tails
/// keep full relative accuracy.
pub fn norm_quantile(q: f64) -> f64 {
    if q.is_nan() || !(0.0..=1.0).contains(&q) {
        return f64::NAN;
    }
    if q == 0.0 {
        return f64::NEG_INFINITY;
    }
    if q == 1.0 {
        return f64::INFINITY;
    }
    if q > 0.5 {
        return -norm_quantile(1.0 - q);
    }
    // the lower half: z <= 0
    let mut z = acklam(q);
    for _ in 0..6 {
        let p = norm_cdf(z);
        // u = (Phi(z) - q)/phi(z) = (p/q - 1)/(1 + (p/q - 1)) ... written through Mills' ratio to stay finite
        let r = (p - q) / p;
        let u = r * mills_lower(z.min(0.0));
        let step = u / (1.0 + z * u / 2.0);
        z -= step;
        // Halley steps converge cubically: after a step of relative size 1e-7 the error is far below an ulp
        if step.abs() <= 1e-7 * z.abs().max(1e-300) {
            break;
        }
    }
    z
}

/// Acklam's rational approximation of the normal quantile for 0 < q <= 1/2.
fn acklam(q: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] =
        [-5.447_609_879_822_406e1, 1.615_858_368_580_409e2, -1.556_989_798_598_866e2, 6.680_131_188_771_972e1, -1.328_068_155_288_572e1];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [7.784_695_709_041_462e-3, 3.224_671_290_700_398e-1, 2.445_134_137_142_996, 3.754_408_661_907_416];
    if q < 0.024_25 {
        let s = (-2.0 * q.ln()).sqrt();
        (((((C[0] * s + C[1]) * s + C[2]) * s + C[3]) * s + C[4]) * s + C[5])
            / ((((D[0] * s + D[1]) * s + D[2]) * s + D[3]) * s + 1.0)
    } else {
        let s = q - 0.5;
        let r = s * s;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * s
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    }
}

/// ln Gamma(a) for a > 0 in double-double.
pub fn lgamma(a: f64) -> DD {
    lgamma_re(DD::new(a))
}

/// e^v for a double-double exponent, as a machine real (overflow to inf, underflow to 0).
fn exp_dd(v: DD) -> f64 {
    if v.hi > 709.8 {
        return f64::INFINITY;
    }
    if v.hi < -745.2 {
        return 0.0;
    }
    dd::exp(v).to_f64()
}

/// x^a e^{-x} / Gamma(a), the prefactor of the incomplete gamma function, for a > 0 and x > 0, with the exponent
/// formed in double-double (no cancellation between a ln x, x and ln Gamma(a) for large a).
pub fn gamma_prefactor(a: f64, x: f64) -> f64 {
    gamma_prefactor_l(a, x, lgamma(a))
}

/// [`gamma_prefactor`] with ln Gamma(a) given (callers that evaluate many x at one a compute it once).
pub fn gamma_prefactor_l(a: f64, x: f64, lga: DD) -> f64 {
    let v = dd::log(DD::new(x)).mul_f(a) - DD::new(x) - lga;
    exp_dd(v)
}

/// The regularized incomplete gamma functions (P(a, x), Q(a, x)) = (gamma(a, x), Gamma(a, x))/Gamma(a) for a > 0,
/// x >= 0: the series for P when x < a + 1, else the continued fraction for Q (both from the same prefactor); the
/// other one is the complement.
pub fn gamma_pq(a: f64, x: f64) -> (f64, f64) {
    gamma_pq_l(a, x, lgamma(a))
}

/// [`gamma_pq`] with ln Gamma(a) given.
pub fn gamma_pq_l(a: f64, x: f64, lga: DD) -> (f64, f64) {
    if x.is_nan() || a.is_nan() {
        return (f64::NAN, f64::NAN);
    }
    if x <= 0.0 {
        return (0.0, 1.0);
    }
    if x == f64::INFINITY {
        return (1.0, 0.0);
    }
    let pre = gamma_prefactor_l(a, x, lga);
    if x < a + 1.0 {
        // P = pre/a * sum_n x^n / ((a+1)...(a+n))
        let mut term = 1.0 / a;
        let mut sum = term;
        let mut ap = a;
        for _ in 0..MAX_ITER {
            ap += 1.0;
            term *= x / ap;
            sum += term;
            if term.abs() < sum.abs() * 1e-17 {
                break;
            }
        }
        let p = (pre * sum).min(1.0);
        (p, 1.0 - p)
    } else {
        // Q = pre * 1/(x + 1 - a - 1(1-a)/(x + 3 - a - 2(2-a)/(x + 5 - a - ...))) (modified Lentz)
        const TINY: f64 = 1e-300;
        let mut b = x + 1.0 - a;
        let mut c = 1.0 / TINY;
        let mut d = 1.0 / b;
        let mut h = d;
        for i in 1..MAX_ITER {
            let an = -(i as f64) * (i as f64 - a);
            b += 2.0;
            d = an * d + b;
            if d.abs() < TINY {
                d = TINY;
            }
            c = b + an / c;
            if c.abs() < TINY {
                c = TINY;
            }
            d = 1.0 / d;
            let delta = d * c;
            h *= delta;
            if (delta - 1.0).abs() < 1e-17 {
                break;
            }
        }
        let q = (pre * h).min(1.0);
        (1.0 - q, q)
    }
}

/// ln B(a, b) in double-double.
pub fn lbeta(a: f64, b: f64) -> DD {
    lgamma(a) + lgamma(b) - lgamma(a + b)
}

/// The regularized incomplete beta function and its complement (I_x(a, b), 1 - I_x(a, b)) for a, b > 0 and
/// 0 <= x <= 1, with y = 1 - x passed separately (callers often know it more accurately than 1 - x): the
/// continued fraction on the side where it converges fast, the other value as the complement.
pub fn beta_i(a: f64, b: f64, x: f64, y: f64) -> (f64, f64) {
    beta_i_l(a, b, x, y, lbeta(a, b))
}

/// [`beta_i`] with ln B(a, b) given (symmetric in a and b, so it serves both sides).
pub fn beta_i_l(a: f64, b: f64, x: f64, y: f64, lb: DD) -> (f64, f64) {
    if x.is_nan() || y.is_nan() || a.is_nan() || b.is_nan() {
        return (f64::NAN, f64::NAN);
    }
    if x <= 0.0 {
        return (0.0, 1.0);
    }
    if y <= 0.0 {
        return (1.0, 0.0);
    }
    if x > (a + 1.0) / (a + b + 2.0) {
        let (q, p) = beta_i_l(b, a, y, x, lb);
        return (p, q);
    }
    // front = x^a y^b / (a B(a, b)), the exponent in double-double
    let v = dd::log(DD::new(x)).mul_f(a) + dd::log(DD::new(y)).mul_f(b) - lb;
    let front = exp_dd(v) / a;
    let i = (front * beta_cf(a, b, x)).min(1.0);
    (i, 1.0 - i)
}

/// The continued fraction of the incomplete beta function (modified Lentz), converging fast for
/// x < (a + 1)/(a + b + 2).
fn beta_cf(a: f64, b: f64, x: f64) -> f64 {
    const TINY: f64 = 1e-300;
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < TINY {
        d = TINY;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..MAX_ITER {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < 1e-17 {
            break;
        }
    }
    h
}

/// e^v of a double-double, as a machine real; used by the densities.
pub fn exp_of(v: DD) -> f64 {
    exp_dd(v)
}

/// ln(x) in double-double.
pub fn ln(x: f64) -> DD {
    dd::log(DD::new(x))
}

/// ln(1 + x) in double-double, accurate for small x.
pub fn ln_1p(x: f64) -> DD {
    // 1 + x is exact as a double-double (two-sum), so no low bits of x are lost
    let s = DD::sum(1.0, x);
    dd::log(s)
}

/// The digamma function (a stable entry of `mc-special`).
pub fn digamma(x: f64) -> f64 {
    mc_special::machine::gamma::digamma(x)
}

/// The trigamma function psi'(x).
pub fn trigamma(x: f64) -> f64 {
    mc_special::machine::gamma::polygamma(1.0, x)
}

#[cfg(test)]
#[allow(clippy::excessive_precision)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rel: f64) -> bool {
        a == b || (a - b).abs() <= rel * b.abs()
    }

    #[test]
    fn error_function_values() {
        // reference values to 17 digits
        let cases = [
            (0.5, 0.520_499_877_813_046_54, 0.479_500_122_186_953_46),
            (1.0, 0.842_700_792_949_714_87, 0.157_299_207_050_285_13),
            (2.0, 0.995_322_265_018_952_73, 0.004_677_734_981_047_265_8),
            (3.5, 0.999_999_256_901_627_66, 7.430_983_723_414_128e-7),
            (5.0, 0.999_999_999_998_462_5, 1.537_459_794_428_034_8e-12),
            (10.0, 1.0, 2.088_487_583_762_544_8e-45),
            (26.0, 1.0, 5.663_192_408_856_143e-296),
        ];
        for (x, e, c) in cases {
            assert!(close(erf(x), e, 4e-16), "erf({x}) = {}", erf(x));
            assert!(close(erfc(x), c, 2e-15), "erfc({x}) = {} vs {c}", erfc(x));
            assert!(close(erf(-x), -e, 4e-16));
            assert!(close(erfc(-x), 2.0 - c, 4e-16));
        }
        assert!(close(erf(1e-10), 1.128_379_167_095_512_6e-10, 2e-16));
    }

    #[test]
    fn normal_quantile_round_trips() {
        for &q in &[1e-300, 1e-100, 1e-20, 1e-5, 0.01, 0.024, 0.025, 0.1, 0.3, 0.5, 0.7, 0.975, 0.999_999] {
            let z = norm_quantile(q);
            let back = if q < 0.5 { norm_cdf(z) } else { 1.0 - norm_cdf(-z) };
            // the rounding of z alone moves Phi(z) by about z^2 ulps relatively
            let tol = 4e-16 * (1.0 + z * z) + if q > 0.5 { 2e-16 / (1.0 - q) } else { 0.0 };
            assert!(close(back, q, tol), "q = {q}: z = {z}, back {back}");
        }
        assert!(close(norm_quantile(0.975), 1.959_963_984_540_054, 1e-15));
        assert!(close(norm_quantile(1e-10), -6.361_340_902_404_056, 1e-15));
    }

    #[test]
    fn incomplete_gamma_and_beta() {
        // P(3, 2) = 1 - 5 e^-2
        let (p, q) = gamma_pq(3.0, 2.0);
        assert!(close(p, 1.0 - 5.0 * (-2.0f64).exp(), 1e-15) && close(q, 5.0 * (-2.0f64).exp(), 1e-15));
        // Q(1/2, x) = erfc(sqrt(x))
        for x in [0.1, 1.0, 5.0, 30.0] {
            assert!(close(gamma_pq(0.5, x).1, erfc(x.sqrt()), 1e-14), "{x}");
        }
        // I_x(2, 3) = 6x^2 - 8x^3 + 3x^4
        for x in [0.1, 0.4, 0.9] {
            let want = 6.0 * x * x - 8.0 * x * x * x + 3.0 * x * x * x * x;
            assert!(close(beta_i(2.0, 3.0, x, 1.0 - x).0, want, 1e-14), "{x}");
        }
        // a large parameter: P(1000, 1000) = 0.50840...
        assert!(close(gamma_pq(1000.0, 1000.0).0, 0.504_205_244_180_215_5, 1e-13));
    }
}
