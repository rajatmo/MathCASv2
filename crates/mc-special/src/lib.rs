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
pub mod cover;
pub mod dd;
pub mod exact;
mod exact_more;
pub mod functions;
mod limits;
pub mod more;
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
    ("q_factorial", "q_factorial(q, n): the q-factorial (q; q)_n/(1 - q)^n"),
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
        names.extend(super::more::DEFS.iter().map(|d| d.name));
        names.extend(super::cover::DEFS.iter().map(|d| d.name));
        let listed: Vec<&str> = super::FUNCTIONS.iter().map(|e| e.0).collect();
        assert_eq!(names, listed);
    }
}
