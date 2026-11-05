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
#![warn(missing_docs)]
pub mod ball;
pub mod bernoulli;
pub mod dd;
pub mod exact;
pub mod functions;
pub mod machine;
mod natives;

pub use natives::failure_tags;
use functions::Special;
use mc_core::Heap;
use mc_eval::Kernel;
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
        let names: Vec<&str> = super::functions::ALL.iter().map(|f| f.name()).collect();
        let listed: Vec<&str> = super::FUNCTIONS.iter().map(|e| e.0).collect();
        assert_eq!(names, listed);
    }
}
