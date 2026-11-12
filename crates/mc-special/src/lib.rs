//! MathCAS v2 special functions: `gamma`, `log_gamma`, `digamma`, `polygamma`, `beta`, `zeta`,
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
//! Decisions of this crate (numbered SF.k where applied):
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
//! Item 3C added, at the same four levels: `erf`, `erfc`, `erfi`, `inverse_erf`, `inverse_erfc` ([`machine::erf`],
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
mod cdd;
pub mod cover;
mod cover28;
mod weierstrass;
mod heun;
mod appell;
mod zeros;
mod qhyper;
mod mathieu;
mod fast29;
mod fast30;
mod fast33;
mod jshift;
mod round29;
pub mod dd;
pub mod exact;
mod exact_more;
pub mod functions;
mod limits;
pub mod more;
pub mod machine;
mod meijer;
pub mod meijer_num;
mod natives;
mod series30;

use functions::Special;
use mc_core::Heap;
use mc_eval::Kernel;
pub use natives::failure_tags;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The special-function library (`lib/special.mc`, round 30): series expansions the `series`
/// wrapper delegates to, and symbolic identities of the functions (rules tried when a native declines). Evaluated
/// by the engine after the other libraries.
pub const LIB_SOURCE: &str = include_str!("../../../lib/special.mc");

/// The name of [`LIB_SOURCE`] in messages.
pub const LIB_ORIGIN: &str = "lib/special.mc";

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
    ("fibonacci", "fibonacci(n): the Fibonacci number F_n (any real n by Binet's formula); fibonacci(n, x): the Fibonacci polynomial"),
    ("lucas_l", "lucas_l(n): the Lucas number L_n; lucas_l(n, x): the Lucas polynomial"),
    ("pochhammer", "pochhammer(a, n): the rising factorial a (a + 1) ... (a + n - 1) = gamma(a + n)/gamma(a)"),
    ("factorial_power", "factorial_power(x, n): the falling factorial x (x - 1) ... (x - n + 1); factorial_power(x, n, h) with step h"),
    ("factorial2", "factorial2(n): the double factorial n (n - 2) (n - 4) ..."),
    ("subfactorial", "subfactorial(n): the number of derangements of n objects"),
    ("multinomial", "multinomial(n1, n2, ...): the multinomial coefficient (n1 + n2 + ...)!/(n1! n2! ...)"),
    ("harmonic_number", "harmonic_number(n): the harmonic number H_n = 1 + 1/2 + ... + 1/n; harmonic_number(n, r): the sum of 1/k^r"),
    ("hyperfactorial", "hyperfactorial(n): the product 1^1 2^2 ... n^n"),
    ("chebyshev_t", "chebyshev_t(n, x): the Chebyshev polynomial of the first kind T_n(x) = cos(n acos(x))"),
    ("chebyshev_u", "chebyshev_u(n, x): the Chebyshev polynomial of the second kind U_n(x)"),
    ("legendre_p", "legendre_p(n, x): the Legendre polynomial P_n(x); legendre_p(n, m, x): the associated Legendre function P_n^m(x)"),
    ("legendre_q", "legendre_q(n, x): the Legendre function of the second kind Q_n(x) at integer n"),
    ("hermite_h", "hermite_h(n, x): the Hermite polynomial H_n(x) (physicists' normalization)"),
    ("laguerre_l", "laguerre_l(n, x): the Laguerre polynomial L_n(x); laguerre_l(n, a, x): the generalized Laguerre polynomial"),
    ("gegenbauer_c", "gegenbauer_c(n, a, x): the Gegenbauer polynomial C_n^(a)(x); gegenbauer_c(n, x) = (2/n) T_n(x)"),
    ("jacobi_p", "jacobi_p(n, a, b, x): the Jacobi polynomial P_n^(a, b)(x)"),
    ("zernike_r", "zernike_r(n, m, r): the radial Zernike polynomial R_n^m(r)"),
    ("spherical_harmonic_y", "spherical_harmonic_y(l, m, theta, phi): the spherical harmonic Y_l^m(theta, phi)"),
    ("product_log", "product_log(z): the principal branch of the Lambert W function (w e^w = z); product_log(k, z): branch k"),
    ("dawson_f", "dawson_f(x): Dawson's integral e^(-x^2) times the integral of e^(t^2) from 0 to x"),
    ("fresnel_s", "fresnel_s(z): the Fresnel integral S(z), the integral of sin(pi t^2/2) from 0 to z"),
    ("fresnel_c", "fresnel_c(z): the Fresnel integral C(z), the integral of cos(pi t^2/2) from 0 to z"),
    ("spherical_bessel_j", "spherical_bessel_j(n, z): the spherical Bessel function j_n(z) = sqrt(pi/(2 z)) J_(n+1/2)(z)"),
    ("spherical_bessel_y", "spherical_bessel_y(n, z): the spherical Bessel function y_n(z) = sqrt(pi/(2 z)) Y_(n+1/2)(z)"),
    ("hankel_h1", "hankel_h1(nu, z): the Hankel function of the first kind J_nu(z) + i Y_nu(z)"),
    ("hankel_h2", "hankel_h2(nu, z): the Hankel function of the second kind J_nu(z) - i Y_nu(z)"),
    ("jacobi_cd", "jacobi_cd(u, m): the Jacobi elliptic function cn/dn"),
    ("jacobi_cs", "jacobi_cs(u, m): the Jacobi elliptic function cn/sn"),
    ("jacobi_dc", "jacobi_dc(u, m): the Jacobi elliptic function dn/cn"),
    ("jacobi_ds", "jacobi_ds(u, m): the Jacobi elliptic function dn/sn"),
    ("jacobi_nc", "jacobi_nc(u, m): the Jacobi elliptic function 1/cn"),
    ("jacobi_nd", "jacobi_nd(u, m): the Jacobi elliptic function 1/dn"),
    ("jacobi_ns", "jacobi_ns(u, m): the Jacobi elliptic function 1/sn"),
    ("jacobi_sc", "jacobi_sc(u, m): the Jacobi elliptic function sn/cn"),
    ("jacobi_sd", "jacobi_sd(u, m): the Jacobi elliptic function sn/dn"),
    ("hypergeometric_0f1_regularized", "hypergeometric_0f1_regularized(b, z): 0F1(; b; z)/gamma(b)"),
    ("hypergeometric_1f1_regularized", "hypergeometric_1f1_regularized(a, b, z): 1F1(a; b; z)/gamma(b)"),
    ("hypergeometric_2f1_regularized", "hypergeometric_2f1_regularized(a, b, c, z): 2F1(a, b; c; z)/gamma(c)"),
    ("elliptic_nome_q", "elliptic_nome_q(m): the nome q = exp(-pi K(1 - m)/K(m))"),
    ("bessel_k_scaled", "bessel_k_scaled(nu, x): e^x K_nu(x), without underflow at large x"),
    ("bessel_i_scaled", "bessel_i_scaled(nu, x): e^-x I_nu(x), without overflow at large x"),
    ("elliptic_theta", "elliptic_theta(a, u, q): the Jacobi theta function theta_a(u, q), a = 1, 2, 3, 4, nome |q| < 1"),
    ("elliptic_theta_prime", "elliptic_theta_prime(a, u, q): the derivative of theta_a(u, q) in u"),
    ("neville_theta_s", "neville_theta_s(z, m): the Neville theta function theta_s(z | m)"),
    ("neville_theta_c", "neville_theta_c(z, m): the Neville theta function theta_c(z | m)"),
    ("neville_theta_d", "neville_theta_d(z, m): the Neville theta function theta_d(z | m)"),
    ("neville_theta_n", "neville_theta_n(z, m): the Neville theta function theta_n(z | m)"),
    ("modular_lambda", "modular_lambda(tau): the modular lambda function theta_2^4/theta_3^4 at q = e^(i pi tau), Im tau > 0"),
    ("klein_invariant_j", "klein_invariant_j(tau): Klein's absolute invariant J(tau), Im tau > 0"),
    ("inverse_elliptic_nome_q", "inverse_elliptic_nome_q(q): the parameter m whose nome is q"),
    ("dedekind_eta", "dedekind_eta(tau): the Dedekind eta function, Im tau > 0"),
    ("q_pochhammer", "q_pochhammer(a, q, n): the q-Pochhammer symbol (a; q)_n; q_pochhammer(a, q) for n = infinity; q_pochhammer(q) = (q; q)_inf"),
    ("q_factorial", "q_factorial(n, q): the q-factorial (q; q)_n/(1 - q)^n"),
    ("q_binomial", "q_binomial(n, k, q): the q-binomial coefficient"),
    ("q_gamma", "q_gamma(z, q): the q-gamma function (q; q)_inf/(q^z; q)_inf (1 - q)^(1 - z)"),
    ("inverse_jacobi_cn", "inverse_jacobi_cn(v, m): the inverse of the Jacobi elliptic function cn"),
    ("inverse_jacobi_dn", "inverse_jacobi_dn(v, m): the inverse of the Jacobi elliptic function dn"),
    ("inverse_jacobi_cd", "inverse_jacobi_cd(v, m): the inverse of the Jacobi elliptic function cd"),
    ("inverse_jacobi_sd", "inverse_jacobi_sd(v, m): the inverse of the Jacobi elliptic function sd"),
    ("inverse_jacobi_nd", "inverse_jacobi_nd(v, m): the inverse of the Jacobi elliptic function nd"),
    ("inverse_jacobi_dc", "inverse_jacobi_dc(v, m): the inverse of the Jacobi elliptic function dc"),
    ("inverse_jacobi_nc", "inverse_jacobi_nc(v, m): the inverse of the Jacobi elliptic function nc"),
    ("inverse_jacobi_sc", "inverse_jacobi_sc(v, m): the inverse of the Jacobi elliptic function sc"),
    ("inverse_jacobi_ns", "inverse_jacobi_ns(v, m): the inverse of the Jacobi elliptic function ns"),
    ("inverse_jacobi_ds", "inverse_jacobi_ds(v, m): the inverse of the Jacobi elliptic function ds"),
    ("inverse_jacobi_cs", "inverse_jacobi_cs(v, m): the inverse of the Jacobi elliptic function cs"),
    ("jacobi_amplitude", "jacobi_amplitude(u, m): the amplitude am(u | m), the angle whose sine is sn(u | m)"),
    ("jacobi_zn", "jacobi_zn(u, m): the Jacobi zeta function Z(u | m) = E(am(u) | m) - E(m) u/K(m)"),
    ("jacobi_epsilon", "jacobi_epsilon(u, m): the Jacobi epsilon function E(am(u | m) | m)"),
    ("spherical_hankel_h1", "spherical_hankel_h1(n, z): the spherical Hankel function of the first kind j_n(z) + i y_n(z)"),
    ("spherical_hankel_h2", "spherical_hankel_h2(n, z): the spherical Hankel function of the second kind j_n(z) - i y_n(z)"),
    ("kelvin_ber", "kelvin_ber(nu, x): the Kelvin function ber_nu(x), the real part of J_nu(x e^(3 pi i/4)); kelvin_ber(x) for nu = 0"),
    ("kelvin_bei", "kelvin_bei(nu, x): the Kelvin function bei_nu(x), the imaginary part of J_nu(x e^(3 pi i/4)); kelvin_bei(x) for nu = 0"),
    ("kelvin_ker", "kelvin_ker(nu, x): the Kelvin function ker_nu(x), the real part of e^(-nu pi i/2) K_nu(x e^(pi i/4)); kelvin_ker(x) for nu = 0"),
    ("kelvin_kei", "kelvin_kei(nu, x): the Kelvin function kei_nu(x), the imaginary part of e^(-nu pi i/2) K_nu(x e^(pi i/4)); kelvin_kei(x) for nu = 0"),
    ("struve_h", "struve_h(nu, z): the Struve function H_nu(z)"),
    ("struve_l", "struve_l(nu, z): the modified Struve function L_nu(z)"),
    ("anger_j", "anger_j(nu, z): the Anger function J_nu(z), the integral of cos(nu t - z sin t) over [0, pi], over pi"),
    ("weber_e", "weber_e(nu, z): the Weber function E_nu(z), the integral of sin(nu t - z sin t) over [0, pi], over pi"),
    ("lommel_s1", "lommel_s1(mu, nu, z): the Lommel function s_(mu, nu)(z)"),
    ("lommel_s2", "lommel_s2(mu, nu, z): the Lommel function S_(mu, nu)(z)"),
    ("hypergeometric_u", "hypergeometric_u(a, b, z): Tricomi's confluent hypergeometric function U(a, b, z)"),
    ("whittaker_m", "whittaker_m(k, m, z): the Whittaker function M_(k, m)(z) = e^(-z/2) z^(m + 1/2) 1F1(m - k + 1/2; 1 + 2 m; z)"),
    ("whittaker_w", "whittaker_w(k, m, z): the Whittaker function W_(k, m)(z) = e^(-z/2) z^(m + 1/2) U(m - k + 1/2, 1 + 2 m, z)"),
    ("parabolic_cylinder_d", "parabolic_cylinder_d(nu, z): the parabolic cylinder function D_nu(z)"),
    ("hypergeometric_pfq_regularized", "hypergeometric_pfq_regularized(as, bs, z): pFq(as; bs; z) divided by the gamma functions of the bs"),
    ("fresnel_f", "fresnel_f(z): the Fresnel auxiliary function f(z)"),
    ("fresnel_g", "fresnel_g(z): the Fresnel auxiliary function g(z)"),
    ("logistic_sigmoid", "logistic_sigmoid(z): the logistic function 1/(1 + e^-z)"),
    ("riemann_siegel_theta", "riemann_siegel_theta(t): the Riemann-Siegel theta function, the phase of zeta on the critical line"),
    ("riemann_siegel_z", "riemann_siegel_z(t): the Riemann-Siegel Z function e^(i theta(t)) zeta(1/2 + i t), real for real t"),
    ("lerch_phi", "lerch_phi(z, s, a): the Lerch transcendent, the sum of z^k/(k + a)^s over k >= 0"),
    ("mittag_leffler_e", "mittag_leffler_e(a, z): the Mittag-Leffler function, the sum of z^k/gamma(a k + 1); mittag_leffler_e(a, b, z) with gamma(a k + b)"),
    ("barnes_g", "barnes_g(z): the Barnes G-function, G(z + 1) = gamma(z) G(z), G(1) = 1"),
    ("log_barnes_g", "log_barnes_g(z): the logarithm of the Barnes G-function, analytic off the negative axis"),
    ("inverse_gamma_regularized", "inverse_gamma_regularized(a, s): the x with gamma_regularized(a, x) = s"),
    ("inverse_beta_regularized", "inverse_beta_regularized(s, a, b): the x with beta_regularized(x, a, b) = s"),
    ("meijer_g", "meijer_g([[a1, ..., an], [a(n+1), ..., ap]], [[b1, ..., bm], [b(m+1), ..., bq]], z): the Meijer G-function G^(m,n)_(p,q)(z); meijer_g(as, bs, z, r) is the generalized function G(z^(1/r))"),
    ("scorer_gi", "scorer_gi(z): the Scorer function Gi(z) = Bi(z) - Hi(z), a solution of w'' - z w = -1/pi"),
    ("scorer_hi", "scorer_hi(z): the Scorer function Hi(z), the integral of exp(-t^3/3 + z t) over t > 0, over pi"),
    ("scorer_gi_prime", "scorer_gi_prime(z): the derivative of the Scorer function Gi(z)"),
    ("scorer_hi_prime", "scorer_hi_prime(z): the derivative of the Scorer function Hi(z)"),
    ("lommel_t1", "lommel_t1(mu, nu, z): the modified Lommel function t_(mu, nu)(z)"),
    ("lommel_t2", "lommel_t2(mu, nu, z): the modified Lommel function T_(mu, nu)(z)"),
    ("carlson_rf", "carlson_rf(x, y, z): Carlson's symmetric elliptic integral of the first kind R_F(x, y, z)"),
    ("carlson_rk", "carlson_rk(x, y): the complete symmetric integral R_K(x, y) = (2/pi) R_F(0, x, y)"),
    ("coulomb_f", "coulomb_f(l, eta, rho): the regular Coulomb wave function F_l(eta, rho)"),
    ("coulomb_g", "coulomb_g(l, eta, rho): the irregular Coulomb wave function G_l(eta, rho)"),
    ("coulomb_h1", "coulomb_h1(l, eta, rho): the outgoing Coulomb wave function H+_l(eta, rho) = G + i F"),
    ("coulomb_h2", "coulomb_h2(l, eta, rho): the incoming Coulomb wave function H-_l(eta, rho) = G - i F"),
    ("weierstrass_half_periods", "weierstrass_half_periods([g2, g3]): the reduced half-periods [omega1, omega3] of the Weierstrass functions of invariants g2, g3"),
    ("weierstrass_half_period_w1", "weierstrass_half_period_w1([g2, g3]): the half-period omega1 of the invariants g2, g3"),
    ("weierstrass_half_period_w2", "weierstrass_half_period_w2([g2, g3]): the half-period omega2 = -omega1 - omega3 of the invariants g2, g3"),
    ("weierstrass_half_period_w3", "weierstrass_half_period_w3([g2, g3]): the half-period omega3 of the invariants g2, g3"),
    ("weierstrass_e1", "weierstrass_e1([g2, g3]): the root e1 = weierstrass_p(omega1, [g2, g3]) of 4 t^3 - g2 t - g3"),
    ("weierstrass_e2", "weierstrass_e2([g2, g3]): the root e2 = weierstrass_p(omega2, [g2, g3]) of 4 t^3 - g2 t - g3"),
    ("weierstrass_e3", "weierstrass_e3([g2, g3]): the root e3 = weierstrass_p(omega3, [g2, g3]) of 4 t^3 - g2 t - g3"),
    ("weierstrass_eta1", "weierstrass_eta1([g2, g3]): the quasi-period eta1 = weierstrass_zeta(omega1, [g2, g3])"),
    ("weierstrass_eta2", "weierstrass_eta2([g2, g3]): the quasi-period eta2 = weierstrass_zeta(omega2, [g2, g3])"),
    ("weierstrass_eta3", "weierstrass_eta3([g2, g3]): the quasi-period eta3 = weierstrass_zeta(omega3, [g2, g3])"),
    ("weierstrass_invariants", "weierstrass_invariants([omega1, omega3]): the invariants [g2, g3] of the lattice of half-periods omega1, omega3"),
    ("weierstrass_invariant_g2", "weierstrass_invariant_g2([omega1, omega3]): the invariant g2 of the lattice of half-periods omega1, omega3"),
    ("weierstrass_invariant_g3", "weierstrass_invariant_g3([omega1, omega3]): the invariant g3 of the lattice of half-periods omega1, omega3"),
    ("elliptic_exp_prime", "elliptic_exp_prime(u, [a, b]): the derivative in u of elliptic_exp(u, [a, b])"),
    ("weierstrass_p", "weierstrass_p(u, [g2, g3]): the Weierstrass elliptic function p(u; g2, g3)"),
    ("weierstrass_p_prime", "weierstrass_p_prime(u, [g2, g3]): the derivative of the Weierstrass elliptic function p(u; g2, g3)"),
    ("weierstrass_sigma", "weierstrass_sigma(u, [g2, g3]): the Weierstrass sigma function, sigma'/sigma = zeta"),
    ("weierstrass_zeta", "weierstrass_zeta(u, [g2, g3]): the Weierstrass zeta function, zeta' = -p"),
    ("elliptic_log", "elliptic_log([x, y], [a, b]): the elliptic logarithm, the integral of dt/(2 y) from infinity to the point (x, y) of y^2 = x^3 + a x^2 + b x"),
    ("elliptic_exp", "elliptic_exp(u, [a, b]): the point [x, y] of y^2 = x^3 + a x^2 + b x whose elliptic logarithm is u"),
    ("inverse_weierstrass_p", "inverse_weierstrass_p(p, [g2, g3]): a u with weierstrass_p(u, [g2, g3]) = p; inverse_weierstrass_p([p, q], [g2, g3]) also with weierstrass_p_prime(u, [g2, g3]) = q"),
    ("heun_g", "heun_g(a, q, alpha, beta, gamma, delta, z): the local Heun function, the solution of the general Heun equation with y(0) = 1"),
    ("heun_c", "heun_c(q, alpha, gamma, delta, epsilon, z): the confluent Heun function, the solution of the confluent Heun equation with y(0) = 1"),
    ("heun_b", "heun_b(q, alpha, gamma, delta, epsilon, z): the biconfluent Heun function, the solution of the biconfluent Heun equation with y(0) = 1"),
    ("heun_d", "heun_d(q, alpha, gamma, delta, epsilon, z): the doubly-confluent Heun function, the solution with y(1) = 1, y'(1) = 0"),
    ("heun_t", "heun_t(q, alpha, gamma, delta, epsilon, z): the triconfluent Heun function, the solution with y(0) = 1, y'(0) = 0"),
    ("appell_f1", "appell_f1(a, b1, b2, c, x, y): the Appell function F1, the sum of (a)_(m+n) (b1)_m (b2)_n/((c)_(m+n) m! n!) x^m y^n"),
    ("appell_f2", "appell_f2(a, b1, b2, c1, c2, x, y): the Appell function F2, the sum of (a)_(m+n) (b1)_m (b2)_n/((c1)_m (c2)_n m! n!) x^m y^n"),
    ("appell_f3", "appell_f3(a1, a2, b1, b2, c, x, y): the Appell function F3, the sum of (a1)_m (a2)_n (b1)_m (b2)_n/((c)_(m+n) m! n!) x^m y^n"),
    ("appell_f4", "appell_f4(a, b, c1, c2, x, y): the Appell function F4, the sum of (a)_(m+n) (b)_(m+n)/((c1)_m (c2)_n m! n!) x^m y^n"),
    ("bessel_j_zero", "bessel_j_zero(nu, k): the k-th positive zero of the Bessel function J_nu; bessel_j_zero(nu, k, x0): the k-th zero above x0"),
    ("bessel_y_zero", "bessel_y_zero(nu, k): the k-th positive zero of the Bessel function Y_nu; bessel_y_zero(nu, k, x0): the k-th zero above x0"),
    ("airy_ai_zero", "airy_ai_zero(k): the k-th zero of the Airy function Ai (negative); airy_ai_zero(k, x0): the k-th zero below x0"),
    ("airy_bi_zero", "airy_bi_zero(k): the k-th zero of the Airy function Bi (negative); airy_bi_zero(k, x0): the k-th zero below x0"),
    ("zeta_zero", "zeta_zero(k): the k-th nontrivial zero 1/2 + i t of the zeta function above the real axis (k < 0: below); zeta_zero(k, t0): the k-th zero above height t0"),
    ("q_hypergeometric_pfq", "q_hypergeometric_pfq([a1, ..., ar], [b1, ..., bs], q, z): the basic hypergeometric series r phi s"),
    ("bilateral_hypergeometric_pfq", "bilateral_hypergeometric_pfq([a1, ..., ap], [b1, ..., bq], z): the bilateral hypergeometric series, the sum over all integers n of prod (a_i)_n/prod (b_j)_n z^n"),
    ("mathieu_c", "mathieu_c(a, q, z): the even Mathieu function, the even Floquet solution of y'' + (a - 2 q cos 2z) y = 0"),
    ("mathieu_s", "mathieu_s(a, q, z): the odd Mathieu function, the odd Floquet solution of y'' + (a - 2 q cos 2z) y = 0"),
    ("mathieu_s_prime", "mathieu_s_prime(a, q, z): the derivative of the odd Mathieu function mathieu_s(a, q, z) in z"),
    ("mathieu_c_prime", "mathieu_c_prime(a, q, z): the derivative of the even Mathieu function mathieu_c(a, q, z) in z"),
    ("mathieu_characteristic_a", "mathieu_characteristic_a(r, q): the characteristic value a_r(q) of the even Mathieu functions"),
    ("mathieu_characteristic_b", "mathieu_characteristic_b(r, q): the characteristic value b_r(q) of the odd Mathieu functions"),
    ("spheroidal_eigenvalue", "spheroidal_eigenvalue(n, m, gamma): the spheroidal eigenvalue lambda of degree n and order m, the separation constant of the spheroidal wave equation"),
    ("spheroidal_ps", "spheroidal_ps(n, m, gamma, x): the angular spheroidal function of the first kind Ps^m_n(x, gamma^2)"),
    ("stieltjes_gamma", "stieltjes_gamma(n): the Stieltjes constant gamma_n, the coefficients of the Laurent series of zeta at 1; stieltjes_gamma(n, a): the generalized constant of the Hurwitz zeta function"),
    ("siegel_theta", "siegel_theta(omega, s): the Siegel (Riemann) theta function, the sum over integer vectors n of exp(pi i n.omega.n + 2 pi i n.s); siegel_theta([nu1, nu2], omega, s) with characteristics"),
];

/// Register the natives of [`FUNCTIONS`] with `k` and the functions with `mc-numeric`'s numerical evaluation;
/// `stop` is the flag the long numeric loops poll (the engine's arithmetic stop flag).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    natives::install(k, h, stop);
    series30::install(k, h);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_table_lists_every_function() {
        let mut names: Vec<&str> =
            super::functions::ALL.iter().filter(|f| f.primary()).map(|f| f.name()).collect();
        names.push(super::functions::PFQ.0);
        names.extend(super::more::DEFS.iter().map(|d| d.name));
        names.extend(super::cover::DEFS.iter().map(|d| d.name));
        names.extend(super::meijer::DEFS.iter().map(|d| d.name));
        names.extend(super::cover28::DEFS.iter().map(|d| d.name));
        names.extend(super::weierstrass::DEFS.iter().map(|d| d.name));
        names.extend(super::heun::DEFS.iter().map(|d| d.name));
        names.extend(super::appell::DEFS.iter().map(|d| d.name));
        names.extend(super::zeros::DEFS.iter().map(|d| d.name));
        names.extend(super::qhyper::DEFS.iter().map(|d| d.name));
        names.extend(super::mathieu::DEFS.iter().map(|d| d.name));
        names.extend(super::round29::DEFS.iter().map(|d| d.name));
        let listed: Vec<&str> = super::FUNCTIONS.iter().map(|e| e.0).collect();
        assert_eq!(names, listed);
    }
}
