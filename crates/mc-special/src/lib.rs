//! MathCAS v2 special functions (package SF): `gamma`, `log_gamma`, `digamma`, `polygamma`, `beta`, `zeta`,
//! `bessel_k` and `bessel_i` of real arguments, at four levels:
//!
//! 1. **exact values** ([`exact`]): `gamma(5) = 24`, `gamma(1/2) = sqrt(pi)`, `zeta(2) = pi^2/6`,
//!    `zeta(-1) = -1/12`, `digamma(1) = -euler_gamma`, `bessel_k(1/2, x)` in closed form; other exact arguments
//!    stay unevaluated (`gamma(1/3)`, `zeta(3)`, `bessel_k(1, 2)`) and get values through `n`;
//! 2. **machine reals** ([`machine`]): double-double algorithms rounded once (measured within half an ulp of the
//!    references over the test grids), IEEE overflow and underflow;
//! 3. **balls at any precision** ([`ball`]): rigorous enclosures, truncation errors bounded and added to the radius,
//!    the working precision doubled while the result is too wide;
//! 4. **derivatives** (rows in `mc-calculus`) and **numerical evaluation**: the functions are registered with
//!    `mc-numeric`'s [`library`](mc_numeric::library), so `n(gamma(1/3) zeta(3), 30)` and `bessel_k(1, 2) < 1/2`
//!    evaluate them inside any term.
//!
//! Decisions of this package (numbered SF.k where applied):
//!
//! - **SF.1** Exact poles (gamma, log_gamma, digamma, polygamma at 0, -1, ...; zeta(1); bessel_k(nu, 0);
//!   bessel_i(nu, 0) for negative non-integer nu; beta where gamma(a) or gamma(b) has one) give
//!   `failure(pole, call)`, as division by zero gives a failure; balls touching a pole stay unevaluated.
//! - **SF.2** Exact closed forms as in [`exact`], with size caps; all other exact arguments stay unevaluated.
//! - **SF.3** Machine values in double-double arithmetic, one final rounding; poles as the C library (gamma(±0) =
//!   ±inf, NaN at negative integers and for zeta(1)), complex-valued arguments (K_nu or non-integer I_nu at x < 0,
//!   non-integer polygamma orders) unevaluated.
//! - **SF.4** Balls: the algorithms and bounds of [`ball`]'s modules, retries by doubling ([`ball::refine`]).
//! - **SF.5** The machine tables (Taylor coefficients of 1/gamma, Bernoulli quotients) are computed once from the
//!   exact and ball paths at first use, not copied by hand.
//! - **SF.6** log_gamma(x) for negative non-integers is the principal value ln|gamma(x)| - i pi ceil(-x) (the limit
//!   from the upper half plane), exactly, as a ball and as a machine complex number.
//! - **SF.7** Complex arguments stay unevaluated this round.
//! - **SF.8** K_nu of ball arguments with nonzero radii by monotonicity (decreasing in x, increasing in |nu|): the
//!   hull of the values at two exact corners.
//! - **SF.9** `n` reaches the functions through a registry in `mc-numeric` (the smallest correct change: one node
//!   kind in its numerical evaluation), so they are refined with the rest of their island.
//!
//! Package 3C added, at the same four levels: `erf`, `erfc`, `erfi`, `inverse_erf`, `inverse_erfc` ([`machine::erf`],
//! [`ball::erf`]); the incomplete `gamma(a, x)`, `gamma_regularized`, `beta(z, a, b)`, `beta_regularized`
//! ([`machine::incomplete`], [`ball::incomplete`]); `exp_integral_ei`, `exp_integral_e`, `log_integral`,
//! `sin_integral`, `cos_integral`, `sinh_integral`, `cosh_integral` (`expint`); `polylog` (`polylog`);
//! `hypergeometric_2f1`, `hypergeometric_1f1`, `hypergeometric_0f1`, `hypergeometric_pfq` (`hyper`); `bessel_j`,
//! `bessel_y`, `airy_ai`, `airy_bi`, `airy_ai_prime`, `airy_bi_prime` (`bessel_j`); `elliptic_k`, `elliptic_e` (one
//! and two arguments), `elliptic_f`, `elliptic_pi` (two and three arguments), `jacobi_sn`, `jacobi_cn`, `jacobi_dn`,
//! `inverse_jacobi_sn` (`elliptic`). Decisions:
//!
//! - **3C.1** A name may have several arities (`gamma(x)` / `gamma(a, x)`, `beta(a, b)` / `beta(z, a, b)`,
//!   `elliptic_e(m)` / `elliptic_e(phi, m)`, `elliptic_pi(n, m)` / `elliptic_pi(n, phi, m)`): one variant of
//!   [`functions::Special`] per arity, one native per name, one library function of `n` per name and arity.
//! - **3C.2** Exact closed forms only at special points (`exact_more`); poles give `failure(pole, call)` as SF.1
//!   (also `inverse_erf(±1)`, `elliptic_k(1)`, `log_integral(1)`).
//! - **3C.3** A machine point where the value is complex or undefined (NaN from the machine path) leaves the call
//!   unevaluated, except for the SF functions, which keep SF.3.
//! - **3C.4** Complex machine arguments are evaluated for `erf`, `erfc`, `erfi` and for `polylog` of integer order
//!   (also on the cut z > 1, the limit from below); other complex arguments stay unevaluated.
//! - **3C.5** `hypergeometric_pfq` is its own native (list parameters are not library functions of `n`): lengths
//!   (0,1), (1,1), (2,1) become the named functions, other machine or ball parameters evaluate directly, and inside
//!   `n` the parameters are numericized first and the native evaluates the result.
#![warn(missing_docs)]
pub mod ball;
pub mod bernoulli;
pub mod dd;
pub mod exact;
mod exact_more;
pub mod functions;
pub mod machine;
mod natives;

use functions::Special;
use mc_core::Heap;
use mc_eval::Kernel;
pub use natives::failure_tags;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The functions [`install`] registers, with a one-line description each.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (Special::Gamma.name(), Special::Gamma.description()),
    (Special::LogGamma.name(), Special::LogGamma.description()),
    (Special::Digamma.name(), Special::Digamma.description()),
    (Special::Polygamma.name(), Special::Polygamma.description()),
    (Special::Beta.name(), Special::Beta.description()),
    (Special::Zeta.name(), Special::Zeta.description()),
    (Special::BesselK.name(), Special::BesselK.description()),
    (Special::BesselI.name(), Special::BesselI.description()),
    (Special::Erf.name(), Special::Erf.description()),
    (Special::Erfc.name(), Special::Erfc.description()),
    (Special::Erfi.name(), Special::Erfi.description()),
    (Special::InverseErf.name(), Special::InverseErf.description()),
    (Special::InverseErfc.name(), Special::InverseErfc.description()),
    (Special::GammaRegularized.name(), Special::GammaRegularized.description()),
    (Special::BetaRegularized.name(), Special::BetaRegularized.description()),
    (Special::ExpIntegralEi.name(), Special::ExpIntegralEi.description()),
    (Special::ExpIntegralE.name(), Special::ExpIntegralE.description()),
    (Special::LogIntegral.name(), Special::LogIntegral.description()),
    (Special::SinIntegral.name(), Special::SinIntegral.description()),
    (Special::CosIntegral.name(), Special::CosIntegral.description()),
    (Special::SinhIntegral.name(), Special::SinhIntegral.description()),
    (Special::CoshIntegral.name(), Special::CoshIntegral.description()),
    (Special::Polylog.name(), Special::Polylog.description()),
    (Special::Hyp2F1.name(), Special::Hyp2F1.description()),
    (Special::Hyp1F1.name(), Special::Hyp1F1.description()),
    (Special::Hyp0F1.name(), Special::Hyp0F1.description()),
    (Special::BesselJ.name(), Special::BesselJ.description()),
    (Special::BesselY.name(), Special::BesselY.description()),
    (Special::AiryAi.name(), Special::AiryAi.description()),
    (Special::AiryBi.name(), Special::AiryBi.description()),
    (Special::AiryAiPrime.name(), Special::AiryAiPrime.description()),
    (Special::AiryBiPrime.name(), Special::AiryBiPrime.description()),
    (Special::EllipticK.name(), Special::EllipticK.description()),
    (Special::EllipticE.name(), Special::EllipticE.description()),
    (Special::EllipticF.name(), Special::EllipticF.description()),
    (Special::EllipticPi.name(), Special::EllipticPi.description()),
    (Special::JacobiSn.name(), Special::JacobiSn.description()),
    (Special::JacobiCn.name(), Special::JacobiCn.description()),
    (Special::JacobiDn.name(), Special::JacobiDn.description()),
    (Special::InverseJacobiSn.name(), Special::InverseJacobiSn.description()),
    functions::PFQ,
];

/// Register the natives of [`FUNCTIONS`] with `k` and the functions with `mc-numeric`'s numerical evaluation;
/// `stop` is the flag the long numeric loops poll (the engine's arithmetic stop flag).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    natives::install(k, h, stop);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_table_lists_every_function() {
        let mut names: Vec<&str> =
            super::functions::ALL.iter().filter(|f| f.primary()).map(|f| f.name()).collect();
        names.push(super::functions::PFQ.0);
        let listed: Vec<&str> = super::FUNCTIONS.iter().map(|e| e.0).collect();
        assert_eq!(names, listed);
    }
}
