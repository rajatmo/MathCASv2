//! The functions of the package as one enumeration: names, arities, descriptions, and their machine and ball
//! values. Each is a [`LibraryFn`] of `mc-numeric`, registered by [`crate::install`] so that `n` and comparisons
//! evaluate it inside any term (decision SF.9).
use crate::{ball, machine};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::fun::Mach;
use mc_numeric::library::LibraryFn;
use mc_numeric::machine::C64;

/// A special function.
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
}

/// Every function, in the order of [`crate::FUNCTIONS`].
pub static ALL: [Special; 8] = [
    Special::Gamma,
    Special::LogGamma,
    Special::Digamma,
    Special::Polygamma,
    Special::Beta,
    Special::Zeta,
    Special::BesselK,
    Special::BesselI,
];

impl Special {
    /// The symbol name.
    pub const fn name(self) -> &'static str {
        match self {
            Special::Gamma => "gamma",
            Special::LogGamma => "log_gamma",
            Special::Digamma => "digamma",
            Special::Polygamma => "polygamma",
            Special::Beta => "beta",
            Special::Zeta => "zeta",
            Special::BesselK => "bessel_k",
            Special::BesselI => "bessel_i",
        }
    }

    /// The number of arguments.
    pub const fn arity(self) -> usize {
        match self {
            Special::Gamma | Special::LogGamma | Special::Digamma | Special::Zeta => 1,
            _ => 2,
        }
    }

    /// The one-line description.
    pub const fn description(self) -> &'static str {
        match self {
            Special::Gamma => "gamma(x): the gamma function, (x-1)! at positive integers; poles at 0, -1, -2, ...",
            Special::LogGamma => "log_gamma(x): log of the gamma function (complex principal value for x < 0)",
            Special::Digamma => "digamma(x): the logarithmic derivative gamma'(x)/gamma(x)",
            Special::Polygamma => "polygamma(n, x): the n-th derivative of digamma(x), integers n >= 0",
            Special::Beta => "beta(a, b): the beta function gamma(a) gamma(b) / gamma(a + b)",
            Special::Zeta => "zeta(s): the Riemann zeta function (pole at s = 1)",
            Special::BesselK => "bessel_k(nu, x): the modified Bessel function of the second kind K_nu(x), x > 0",
            Special::BesselI => "bessel_i(nu, x): the modified Bessel function of the first kind I_nu(x)",
        }
    }

    /// Do machine arguments lie in the real domain (else the call stays unevaluated)?
    pub fn real_domain(self, a: &[f64]) -> bool {
        match self {
            Special::Polygamma => a[0] >= 0.0 && a[0] == a[0].floor(),
            Special::BesselK => !(a[1] < 0.0),
            Special::BesselI => !(a[1] < 0.0) || a[0] == a[0].floor(),
            _ => true,
        }
    }

    /// The machine value.
    pub fn machine_value(self, a: &[f64]) -> Mach {
        Mach::Real(match self {
            Special::Gamma => machine::gamma::gamma(a[0]),
            Special::LogGamma => {
                let (re, im) = machine::gamma::log_gamma(a[0]);
                if im != 0.0 {
                    return Mach::Complex(C64::new(re, im));
                }
                re
            }
            Special::Digamma => machine::gamma::digamma(a[0]),
            Special::Polygamma => machine::gamma::polygamma(a[0], a[1]),
            Special::Beta => machine::gamma::beta(a[0], a[1]),
            Special::Zeta => machine::zeta::zeta(a[0]),
            Special::BesselK => machine::bessel::bessel_k(a[0], a[1]),
            Special::BesselI => machine::bessel::bessel_i(a[0], a[1]),
        })
    }

    /// An enclosure at precision p.
    pub fn ball_value(self, a: &[Real], p: u64) -> Option<CBall> {
        match self {
            Special::Gamma => ball::gamma::gamma(&a[0], p),
            Special::LogGamma => ball::gamma::log_gamma(&a[0], p),
            Special::Digamma => ball::gamma::digamma(&a[0], p),
            Special::Polygamma => ball::gamma::polygamma(&a[0], &a[1], p),
            Special::Beta => ball::gamma::beta(&a[0], &a[1], p),
            Special::Zeta => ball::zeta::zeta(&a[0], p),
            Special::BesselK => ball::bessel::bessel_k(&a[0], &a[1], p),
            Special::BesselI => ball::bessel::bessel_i(&a[0], &a[1], p),
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
