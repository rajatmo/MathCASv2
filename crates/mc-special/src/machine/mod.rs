//! Machine-precision special functions (decision SF.3): each computes in double-double arithmetic ([`crate::dd`])
//! and rounds once, so the result is within about an ulp of the true value (the tests measure at most 4 ulps over
//! their grids). Values beyond the double range overflow to infinity and underflow (gradually) to zero as IEEE
//! does. Poles follow the C library: gamma(±0) = ±inf, gamma at negative integers NaN, two-sided poles NaN
//! (zeta(1), digamma at negative integers), one-sided ones a signed infinity. Arguments where the value is complex
//! (K_nu or non-integer I_nu at x < 0) give NaN here and stay unevaluated in the natives.
pub mod bessel;
pub mod bessel_j;
pub mod elliptic;
pub mod erf;
pub mod expint;
pub mod gamma;
pub mod hyper;
pub mod incomplete;
pub mod polylog;
pub mod zeta;

use crate::dd::DD;
use std::sync::OnceLock;

/// The Taylor coefficients g_k of 1/Gamma(1 + z) (|g_k| 2^-k below 2^-110 beyond the table), computed once from
/// the ball path at 256 bits (decision SF.5: no hand-copied tables).
pub fn rgamma_coefficients() -> &'static [DD] {
    static TABLE: OnceLock<Vec<DD>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let _scope = mc_arith::num::StopScope::enter(None);
        let balls = crate::ball::gamma::rgamma_taylor(48, 256).expect("the coefficients of 1/gamma");
        balls.iter().map(|b| float_dd(&b.mid)).collect()
    })
}

/// The nearest double-double of a float.
pub fn float_dd(f: &mc_arith::num::Float) -> DD {
    let hi = crate::ball::f64_of(f);
    let rest = f.sub_exact(&crate::ball::float_of(hi));
    DD { hi, lo: crate::ball::f64_of(&rest) }
}
