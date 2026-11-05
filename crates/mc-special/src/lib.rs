//! MathCAS v2 special functions: `gamma`, `log_gamma`, `digamma`, `polygamma`, `beta`, `zeta`, `bessel_k` and
//! `bessel_i` of real arguments, at four levels:
//!
//! 1. exact values ([`exact`]): `gamma(5) = 24`, `gamma(1/2) = sqrt(pi)`, `zeta(2) = pi^2/6`, `bessel_k(1/2, x)` in
//!    closed form; other exact arguments stay unevaluated;
//! 2. machine reals ([`machine`]): double-double algorithms rounded once (a few ulps at most);
//! 3. balls at any precision ([`ball`]): rigorous enclosures with bounded truncation errors and precision retries;
//! 4. derivatives (rows in `mc-calculus`) and numerical evaluation (`n`, through `mc-numeric`'s library hook).
#![warn(missing_docs)]
pub mod ball;
pub mod bernoulli;
pub mod dd;
pub mod machine;
