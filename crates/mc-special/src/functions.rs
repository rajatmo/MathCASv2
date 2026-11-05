//! The functions of the package as one enumeration: names, arities, descriptions, and their machine and ball
//! values. Each is a [`LibraryFn`] of `mc-numeric`, registered by [`crate::install`] so that `n` and comparisons
//! evaluate it inside any term (decision SF.9). A name may have several arities (`gamma(x)` and the incomplete
//! `gamma(a, x)`, `beta(a, b)` and `beta(z, a, b)`, `elliptic_e(m)` and `elliptic_e(phi, m)`, `elliptic_pi(n, m)` and
//! `elliptic_pi(n, phi, m)`): each arity is its own variant; one native per name dispatches on the argument count.
use crate::{ball, machine};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::fun::Mach;
use mc_numeric::library::LibraryFn;
use mc_numeric::machine::C64;

/// A special function (one name and arity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Special {
    /// Gamma(x)
    Gamma,
    /// ln Gamma(x) (principal value, complex for negative non-integers)
    LogGamma,
    /// psi(x) = Gamma'(x)/Gamma(x)
    Digamma,
    /// psi^(n)(x)
    Polygamma,
    /// B(a, b)
    Beta,
    /// zeta(s)
    Zeta,
    /// K_nu(x)
    BesselK,
    /// I_nu(x)
    BesselI,
    /// erf(x)
    Erf,
    /// erfc(x) = 1 - erf(x)
    Erfc,
    /// erfi(x) = -i erf(i x)
    Erfi,
    /// the inverse of erf
    InverseErf,
    /// the inverse of erfc
    InverseErfc,
    /// the upper incomplete gamma function Gamma(a, x)
    GammaInc,
    /// Q(a, x) = Gamma(a, x)/Gamma(a)
    GammaRegularized,
    /// the incomplete beta function B_z(a, b)
    BetaInc,
    /// I_z(a, b) = B_z(a, b)/B(a, b)
    BetaRegularized,
    /// Ei(x)
    ExpIntegralEi,
    /// E_nu(x)
    ExpIntegralE,
    /// li(x)
    LogIntegral,
    /// Si(x)
    SinIntegral,
    /// Ci(x)
    CosIntegral,
    /// Shi(x)
    SinhIntegral,
    /// Chi(x)
    CoshIntegral,
    /// Li_s(z)
    Polylog,
    /// 2F1(a, b; c; z)
    Hyp2F1,
    /// 1F1(a; b; z)
    Hyp1F1,
    /// 0F1(; b; z)
    Hyp0F1,
    /// J_nu(x)
    BesselJ,
    /// Y_nu(x)
    BesselY,
    /// Ai(x)
    AiryAi,
    /// Bi(x)
    AiryBi,
    /// Ai'(x)
    AiryAiPrime,
    /// Bi'(x)
    AiryBiPrime,
    /// K(m)
    EllipticK,
    /// E(m)
    EllipticE,
    /// E(phi | m)
    EllipticEInc,
    /// F(phi | m)
    EllipticF,
    /// Pi(n | m)
    EllipticPi,
    /// Pi(n; phi | m)
    EllipticPiInc,
    /// sn(u | m)
    JacobiSn,
    /// cn(u | m)
    JacobiCn,
    /// dn(u | m)
    JacobiDn,
    /// the inverse of sn
    InverseJacobiSn,
}

/// Every function (name and arity), in the order of [`crate::FUNCTIONS`] (a second arity follows its first).
pub static ALL: [Special; 44] = [
    Special::Gamma,
    Special::GammaInc,
    Special::LogGamma,
    Special::Digamma,
    Special::Polygamma,
    Special::Beta,
    Special::BetaInc,
    Special::Zeta,
    Special::BesselK,
    Special::BesselI,
    Special::Erf,
    Special::Erfc,
    Special::Erfi,
    Special::InverseErf,
    Special::InverseErfc,
    Special::GammaRegularized,
    Special::BetaRegularized,
    Special::ExpIntegralEi,
    Special::ExpIntegralE,
    Special::LogIntegral,
    Special::SinIntegral,
    Special::CosIntegral,
    Special::SinhIntegral,
    Special::CoshIntegral,
    Special::Polylog,
    Special::Hyp2F1,
    Special::Hyp1F1,
    Special::Hyp0F1,
    Special::BesselJ,
    Special::BesselY,
    Special::AiryAi,
    Special::AiryBi,
    Special::AiryAiPrime,
    Special::AiryBiPrime,
    Special::EllipticK,
    Special::EllipticE,
    Special::EllipticEInc,
    Special::EllipticF,
    Special::EllipticPi,
    Special::EllipticPiInc,
    Special::JacobiSn,
    Special::JacobiCn,
    Special::JacobiDn,
    Special::InverseJacobiSn,
];

use Special as S;

impl Special {
    /// The symbol name.
    pub const fn name(self) -> &'static str {
        match self {
            S::Gamma | S::GammaInc => "gamma",
            S::LogGamma => "log_gamma",
            S::Digamma => "digamma",
            S::Polygamma => "polygamma",
            S::Beta | S::BetaInc => "beta",
            S::Zeta => "zeta",
            S::BesselK => "bessel_k",
            S::BesselI => "bessel_i",
            S::Erf => "erf",
            S::Erfc => "erfc",
            S::Erfi => "erfi",
            S::InverseErf => "inverse_erf",
            S::InverseErfc => "inverse_erfc",
            S::GammaRegularized => "gamma_regularized",
            S::BetaRegularized => "beta_regularized",
            S::ExpIntegralEi => "exp_integral_ei",
            S::ExpIntegralE => "exp_integral_e",
            S::LogIntegral => "log_integral",
            S::SinIntegral => "sin_integral",
            S::CosIntegral => "cos_integral",
            S::SinhIntegral => "sinh_integral",
            S::CoshIntegral => "cosh_integral",
            S::Polylog => "polylog",
            S::Hyp2F1 => "hypergeometric_2f1",
            S::Hyp1F1 => "hypergeometric_1f1",
            S::Hyp0F1 => "hypergeometric_0f1",
            S::BesselJ => "bessel_j",
            S::BesselY => "bessel_y",
            S::AiryAi => "airy_ai",
            S::AiryBi => "airy_bi",
            S::AiryAiPrime => "airy_ai_prime",
            S::AiryBiPrime => "airy_bi_prime",
            S::EllipticK => "elliptic_k",
            S::EllipticE | S::EllipticEInc => "elliptic_e",
            S::EllipticF => "elliptic_f",
            S::EllipticPi | S::EllipticPiInc => "elliptic_pi",
            S::JacobiSn => "jacobi_sn",
            S::JacobiCn => "jacobi_cn",
            S::JacobiDn => "jacobi_dn",
            S::InverseJacobiSn => "inverse_jacobi_sn",
        }
    }

    /// The number of arguments.
    pub const fn arity(self) -> usize {
        match self {
            S::Gamma
            | S::LogGamma
            | S::Digamma
            | S::Zeta
            | S::Erf
            | S::Erfc
            | S::Erfi
            | S::InverseErf
            | S::InverseErfc
            | S::ExpIntegralEi
            | S::LogIntegral
            | S::SinIntegral
            | S::CosIntegral
            | S::SinhIntegral
            | S::CoshIntegral
            | S::AiryAi
            | S::AiryBi
            | S::AiryAiPrime
            | S::AiryBiPrime
            | S::EllipticK
            | S::EllipticE => 1,
            S::BetaInc | S::BetaRegularized | S::Hyp1F1 | S::EllipticPiInc => 3,
            S::Hyp2F1 => 4,
            _ => 2,
        }
    }

    /// Is this the first (listed) arity of its name? [`crate::FUNCTIONS`] has one entry per name.
    pub const fn primary(self) -> bool {
        !matches!(self, S::GammaInc | S::BetaInc | S::EllipticEInc | S::EllipticPiInc)
    }

    /// Is this one of the functions of the first special-functions package (with their own exact rules)?
    pub const fn is_original(self) -> bool {
        matches!(
            self,
            S::Gamma | S::LogGamma | S::Digamma | S::Polygamma | S::Beta | S::Zeta | S::BesselK | S::BesselI
        )
    }

    /// The one-line description (for the first arity of a name it covers the other arities too).
    pub const fn description(self) -> &'static str {
        match self {
            S::Gamma => "gamma(x): the gamma function, (x-1)! at positive integers, poles at 0, -1, ...; gamma(a, x): the upper incomplete gamma function",
            S::GammaInc => "gamma(a, x): the upper incomplete gamma function, the integral of t^(a-1) e^-t from x to infinity",
            S::LogGamma => "log_gamma(x): log of the gamma function (complex principal value for x < 0)",
            S::Digamma => "digamma(x): the logarithmic derivative gamma'(x)/gamma(x)",
            S::Polygamma => "polygamma(n, x): the n-th derivative of digamma(x), integers n >= 0",
            S::Beta => "beta(a, b): the beta function gamma(a) gamma(b) / gamma(a + b); beta(z, a, b): the incomplete beta function",
            S::BetaInc => "beta(z, a, b): the incomplete beta function, the integral of t^(a-1) (1-t)^(b-1) from 0 to z",
            S::Zeta => "zeta(s): the Riemann zeta function (pole at s = 1)",
            S::BesselK => "bessel_k(nu, x): the modified Bessel function of the second kind K_nu(x), x > 0",
            S::BesselI => "bessel_i(nu, x): the modified Bessel function of the first kind I_nu(x)",
            S::Erf => "erf(x): the error function 2/sqrt(pi) times the integral of e^(-t^2) from 0 to x (complex x too)",
            S::Erfc => "erfc(x): the complementary error function 1 - erf(x), accurate deep in the tail",
            S::Erfi => "erfi(x): the imaginary error function -i erf(i x)",
            S::InverseErf => "inverse_erf(y): the x in (-inf, inf) with erf(x) = y, -1 < y < 1",
            S::InverseErfc => "inverse_erfc(q): the x with erfc(x) = q, 0 < q < 2",
            S::GammaRegularized => "gamma_regularized(a, x): the regularized upper incomplete gamma function gamma(a, x)/gamma(a)",
            S::BetaRegularized => "beta_regularized(z, a, b): the regularized incomplete beta function beta(z, a, b)/beta(a, b)",
            S::ExpIntegralEi => "exp_integral_ei(x): the exponential integral Ei(x), the principal value of the integral of e^t/t up to x",
            S::ExpIntegralE => "exp_integral_e(nu, x): the exponential integral E_nu(x), the integral of e^(-x t)/t^nu from 1 to infinity",
            S::LogIntegral => "log_integral(x): the logarithmic integral li(x) = Ei(log x), x >= 0",
            S::SinIntegral => "sin_integral(x): the sine integral Si(x), the integral of sin(t)/t from 0 to x",
            S::CosIntegral => "cos_integral(x): the cosine integral Ci(x) = euler_gamma + log x + the integral of (cos t - 1)/t, x > 0",
            S::SinhIntegral => "sinh_integral(x): the hyperbolic sine integral Shi(x), the integral of sinh(t)/t from 0 to x",
            S::CoshIntegral => "cosh_integral(x): the hyperbolic cosine integral Chi(x), x > 0",
            S::Polylog => "polylog(s, z): the polylogarithm Li_s(z), the sum of z^k/k^s (continued analytically)",
            S::Hyp2F1 => "hypergeometric_2f1(a, b, c, z): the Gauss hypergeometric function 2F1(a, b; c; z)",
            S::Hyp1F1 => "hypergeometric_1f1(a, b, z): Kummer's confluent hypergeometric function 1F1(a; b; z)",
            S::Hyp0F1 => "hypergeometric_0f1(b, z): the confluent hypergeometric limit function 0F1(; b; z)",
            S::BesselJ => "bessel_j(nu, x): the Bessel function of the first kind J_nu(x)",
            S::BesselY => "bessel_y(nu, x): the Bessel function of the second kind Y_nu(x), x > 0",
            S::AiryAi => "airy_ai(x): the Airy function Ai(x)",
            S::AiryBi => "airy_bi(x): the Airy function Bi(x)",
            S::AiryAiPrime => "airy_ai_prime(x): the derivative Ai'(x) of the Airy function",
            S::AiryBiPrime => "airy_bi_prime(x): the derivative Bi'(x) of the Airy function",
            S::EllipticK => "elliptic_k(m): the complete elliptic integral of the first kind K(m), parameter m = k^2 < 1",
            S::EllipticE => "elliptic_e(m): the complete elliptic integral of the second kind E(m); elliptic_e(phi, m): the incomplete one",
            S::EllipticEInc => "elliptic_e(phi, m): the incomplete elliptic integral of the second kind E(phi | m)",
            S::EllipticF => "elliptic_f(phi, m): the incomplete elliptic integral of the first kind F(phi | m)",
            S::EllipticPi => "elliptic_pi(n, m): the complete elliptic integral of the third kind; elliptic_pi(n, phi, m): the incomplete one",
            S::EllipticPiInc => "elliptic_pi(n, phi, m): the incomplete elliptic integral of the third kind Pi(n; phi | m)",
            S::JacobiSn => "jacobi_sn(u, m): the Jacobi elliptic function sn(u | m) = sin(am(u | m))",
            S::JacobiCn => "jacobi_cn(u, m): the Jacobi elliptic function cn(u | m) = cos(am(u | m))",
            S::JacobiDn => "jacobi_dn(u, m): the Jacobi elliptic function dn(u | m)",
            S::InverseJacobiSn => "inverse_jacobi_sn(v, m): the inverse of sn, F(asin v | m)",
        }
    }

    /// The variant of `name` with `arity` arguments.
    pub fn find(name: &str, arity: usize) -> Option<Special> {
        ALL.iter().copied().find(|f| f.name() == name && f.arity() == arity)
    }

    /// Do machine arguments lie in the real domain (else the call stays unevaluated)? For the functions of this
    /// package the machine paths return NaN outside it, which the natives treat the same way.
    pub fn real_domain(self, a: &[f64]) -> bool {
        match self {
            S::Polygamma => a[0] >= 0.0 && a[0] == a[0].floor(),
            S::BesselK => a[1] >= 0.0 || a[1].is_nan(),
            S::BesselI => a[1] >= 0.0 || a[1].is_nan() || a[0] == a[0].floor(),
            _ => true,
        }
    }

    /// The machine value (NaN where the value is not real or not defined).
    pub fn machine_value(self, a: &[f64]) -> Mach {
        use machine::{bessel_j as bj, elliptic as el, erf, expint as ei, incomplete as inc};
        Mach::Real(match self {
            S::Gamma => machine::gamma::gamma(a[0]),
            S::LogGamma => {
                let (re, im) = machine::gamma::log_gamma(a[0]);
                if im != 0.0 {
                    return Mach::Complex(C64::new(re, im));
                }
                re
            }
            S::Digamma => machine::gamma::digamma(a[0]),
            S::Polygamma => machine::gamma::polygamma(a[0], a[1]),
            S::Beta => machine::gamma::beta(a[0], a[1]),
            S::Zeta => machine::zeta::zeta(a[0]),
            S::BesselK => machine::bessel::bessel_k(a[0], a[1]),
            S::BesselI => machine::bessel::bessel_i(a[0], a[1]),
            S::Erf => erf::erf(a[0]),
            S::Erfc => erf::erfc(a[0]),
            S::Erfi => erf::erfi(a[0]),
            S::InverseErf => erf::inverse_erf(a[0]),
            S::InverseErfc => erf::inverse_erfc(a[0]),
            S::GammaInc => inc::gamma_upper(a[0], a[1]),
            S::GammaRegularized => inc::gamma_q(a[0], a[1]),
            S::BetaInc => inc::beta_inc(a[0], a[1], a[2]),
            S::BetaRegularized => inc::beta_reg(a[0], a[1], a[2]),
            S::ExpIntegralEi => ei::exp_integral_ei(a[0]),
            S::ExpIntegralE => ei::exp_integral_e(a[0], a[1]),
            S::LogIntegral => ei::log_integral(a[0]),
            S::SinIntegral => ei::sin_integral(a[0]),
            S::CosIntegral => ei::cos_integral(a[0]),
            S::SinhIntegral => ei::sinh_integral(a[0]),
            S::CoshIntegral => ei::cosh_integral(a[0]),
            S::Polylog => {
                let v = machine::polylog::polylog(a[0], a[1]);
                if v.is_nan() && a[1] > 1.0 && a[0] == a[0].floor() {
                    let z = machine::polylog::polylog_complex(a[0], C64::new(a[1], 0.0));
                    if !z.re.is_nan() {
                        return Mach::Complex(z);
                    }
                }
                v
            }
            S::Hyp2F1 => machine::hyper::hyp_2f1(a[0], a[1], a[2], a[3]),
            S::Hyp1F1 => machine::hyper::hyp_1f1(a[0], a[1], a[2]),
            S::Hyp0F1 => machine::hyper::hyp_0f1(a[0], a[1]),
            S::BesselJ => bj::bessel_j(a[0], a[1]),
            S::BesselY => bj::bessel_y(a[0], a[1]),
            S::AiryAi => bj::airy_ai(a[0]),
            S::AiryBi => bj::airy_bi(a[0]),
            S::AiryAiPrime => bj::airy_ai_prime(a[0]),
            S::AiryBiPrime => bj::airy_bi_prime(a[0]),
            S::EllipticK => el::elliptic_k(a[0]),
            S::EllipticE => el::elliptic_e(a[0]),
            S::EllipticEInc => el::elliptic_e_inc(a[0], a[1]),
            S::EllipticF => el::elliptic_f(a[0], a[1]),
            S::EllipticPi => el::elliptic_pi(a[0], a[1]),
            S::EllipticPiInc => el::elliptic_pi_inc(a[0], a[1], a[2]),
            S::JacobiSn => el::jacobi_sn(a[0], a[1]),
            S::JacobiCn => el::jacobi_cn(a[0], a[1]),
            S::JacobiDn => el::jacobi_dn(a[0], a[1]),
            S::InverseJacobiSn => el::inverse_jacobi_sn(a[0], a[1]),
        })
    }

    /// The machine value at complex arguments, where the package has a complex machine algorithm (erf, erfc,
    /// erfi; polylog of integer order); `None` otherwise (the call stays unevaluated).
    pub fn machine_complex(self, a: &[C64]) -> Option<C64> {
        let one = C64::real(1.0);
        let r = match self {
            S::Erf => machine::erf::erf_complex(a[0]),
            S::Erfc => one - machine::erf::erf_complex(a[0]),
            S::Erfi => {
                // erfi(z) = -i erf(i z)
                let w = machine::erf::erf_complex(C64::new(-a[0].im, a[0].re));
                C64::new(w.im, -w.re)
            }
            S::Polylog if a[0].im == 0.0 => machine::polylog::polylog_complex(a[0].re, a[1]),
            _ => return None,
        };
        (!r.re.is_nan() && !r.im.is_nan()).then_some(r)
    }

    /// An enclosure at precision p.
    pub fn ball_value(self, a: &[Real], p: u64) -> Option<CBall> {
        use ball::{bessel_j as bj, elliptic as el, erf, expint as ei, incomplete as inc};
        match self {
            S::Gamma => ball::gamma::gamma(&a[0], p),
            S::LogGamma => ball::gamma::log_gamma(&a[0], p),
            S::Digamma => ball::gamma::digamma(&a[0], p),
            S::Polygamma => ball::gamma::polygamma(&a[0], &a[1], p),
            S::Beta => ball::gamma::beta(&a[0], &a[1], p),
            S::Zeta => ball::zeta::zeta(&a[0], p),
            S::BesselK => ball::bessel::bessel_k(&a[0], &a[1], p),
            S::BesselI => ball::bessel::bessel_i(&a[0], &a[1], p),
            S::Erf => erf::erf(&a[0], p),
            S::Erfc => erf::erfc(&a[0], p),
            S::Erfi => erf::erfi(&a[0], p),
            S::InverseErf => erf::inverse_erf(&a[0], p),
            S::InverseErfc => erf::inverse_erfc(&a[0], p),
            S::GammaInc => inc::gamma_upper(&a[0], &a[1], p),
            S::GammaRegularized => inc::gamma_q(&a[0], &a[1], p),
            S::BetaInc => inc::beta_inc(&a[0], &a[1], &a[2], p),
            S::BetaRegularized => inc::beta_reg(&a[0], &a[1], &a[2], p),
            S::ExpIntegralEi => ei::exp_integral_ei(&a[0], p),
            S::ExpIntegralE => ei::exp_integral_e(&a[0], &a[1], p),
            S::LogIntegral => ei::log_integral(&a[0], p),
            S::SinIntegral => ei::sin_integral(&a[0], p),
            S::CosIntegral => ei::cos_integral(&a[0], p),
            S::SinhIntegral => ei::sinh_integral(&a[0], p),
            S::CoshIntegral => ei::cosh_integral(&a[0], p),
            S::Polylog => ball::polylog::polylog(&a[0], &a[1], p),
            S::Hyp2F1 => ball::hyper::hyp_2f1(&a[0], &a[1], &a[2], &a[3], p),
            S::Hyp1F1 => ball::hyper::hyp_1f1(&a[0], &a[1], &a[2], p),
            S::Hyp0F1 => ball::hyper::hyp_0f1(&a[0], &a[1], p),
            S::BesselJ => bj::bessel_j(&a[0], &a[1], p),
            S::BesselY => bj::bessel_y(&a[0], &a[1], p),
            S::AiryAi => bj::airy_ai(&a[0], p),
            S::AiryBi => bj::airy_bi(&a[0], p),
            S::AiryAiPrime => bj::airy_ai_prime(&a[0], p),
            S::AiryBiPrime => bj::airy_bi_prime(&a[0], p),
            S::EllipticK => el::elliptic_k(&a[0], p),
            S::EllipticE => el::elliptic_e(&a[0], p),
            S::EllipticEInc => el::elliptic_e_inc(&a[0], &a[1], p),
            S::EllipticF => el::elliptic_f(&a[0], &a[1], p),
            S::EllipticPi => el::elliptic_pi(&a[0], &a[1], p),
            S::EllipticPiInc => el::elliptic_pi_inc(&a[0], &a[1], &a[2], p),
            S::JacobiSn => el::jacobi_sn(&a[0], &a[1], p),
            S::JacobiCn => el::jacobi_cn(&a[0], &a[1], p),
            S::JacobiDn => el::jacobi_dn(&a[0], &a[1], p),
            S::InverseJacobiSn => el::inverse_jacobi_sn(&a[0], &a[1], p),
        }
    }
}

impl LibraryFn for Special {
    fn name(&self) -> &'static str {
        Special::name(*self)
    }
    fn arity(&self) -> usize {
        Special::arity(*self)
    }
    fn ball(&self, args: &[Real], p: u64) -> Option<CBall> {
        if args.len() != self.arity() || args.iter().any(|a| matches!(a, Real::Machine(_))) {
            return None;
        }
        self.ball_value(args, p)
    }
    fn machine(&self, args: &[f64]) -> Mach {
        if args.len() != self.arity() || !self.real_domain(args) {
            return Mach::Real(f64::NAN);
        }
        self.machine_value(args)
    }
}

/// `hypergeometric_pfq(as, bs, z)`: the generalized hypergeometric function, whose parameter lists make it a
/// native of its own (not a library function of `n`: `n` maps into the lists and the native evaluates the
/// resulting numbers).
pub const PFQ: (&str, &str) = (
    "hypergeometric_pfq",
    "hypergeometric_pfq([a1, ..., ap], [b1, ..., bq], z): the generalized hypergeometric function pFq",
);
