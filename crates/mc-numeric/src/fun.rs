//! The elementary functions as one enumeration: names, and evaluation on balls and on machine numbers.
use crate::cball::{self, CBall};
use crate::elementary as el;
use crate::machine::{self, C64};
use mc_arith::num::Ball;

/// An elementary function of one argument.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Fun {
    /// e^x
    Exp,
    /// the natural logarithm (principal)
    Log,
    /// the sine
    Sin,
    /// the cosine
    Cos,
    /// the tangent
    Tan,
    /// the inverse tangent
    Atan,
    /// the inverse sine
    Asin,
    /// the inverse cosine
    Acos,
    /// the hyperbolic sine
    Sinh,
    /// the hyperbolic cosine
    Cosh,
    /// the hyperbolic tangent
    Tanh,
    /// the square root (principal)
    Sqrt,
}

/// A machine number: real, or complex when a function leaves the real line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mach {
    /// A machine real.
    Real(f64),
    /// A complex machine number.
    Complex(C64),
}

impl Mach {
    /// As a complex number.
    pub fn c64(self) -> C64 {
        match self {
            Mach::Real(x) => C64::real(x),
            Mach::Complex(z) => z,
        }
    }
}

impl Fun {
    /// Every function.
    pub const ALL: [Fun; 12] = [
        Fun::Exp,
        Fun::Log,
        Fun::Sin,
        Fun::Cos,
        Fun::Tan,
        Fun::Atan,
        Fun::Asin,
        Fun::Acos,
        Fun::Sinh,
        Fun::Cosh,
        Fun::Tanh,
        Fun::Sqrt,
    ];

    /// The symbol name.
    pub fn name(self) -> &'static str {
        match self {
            Fun::Exp => "exp",
            Fun::Log => "log",
            Fun::Sin => "sin",
            Fun::Cos => "cos",
            Fun::Tan => "tan",
            Fun::Atan => "atan",
            Fun::Asin => "asin",
            Fun::Acos => "acos",
            Fun::Sinh => "sinh",
            Fun::Cosh => "cosh",
            Fun::Tanh => "tanh",
            Fun::Sqrt => "sqrt",
        }
    }

    /// A one-line description (for the function list).
    pub fn description(self) -> &'static str {
        match self {
            Fun::Exp => "exp(x): e^x; exact at 0, 1 and log(y), balls and machine numbers numerically",
            Fun::Log => "log(x): the principal natural logarithm; exact at 1, e, e^q and -1",
            Fun::Sin => "sin(x): the sine; exact at rational multiples of pi with denominator 1, 2, 3, 4 or 6",
            Fun::Cos => "cos(x): the cosine; exact at rational multiples of pi with denominator 1, 2, 3, 4 or 6",
            Fun::Tan => "tan(x): the tangent; exact at rational multiples of pi with denominator 1, 3, 4 or 6",
            Fun::Atan => "atan(x): the inverse tangent; exact at 0, ±1, ±3^(1/2), ±3^(-1/2)",
            Fun::Asin => "asin(x): the principal inverse sine; exact at the sines of multiples of pi/6 and pi/4",
            Fun::Acos => "acos(x): the principal inverse cosine; exact at the cosines of multiples of pi/6 and pi/4",
            Fun::Sinh => "sinh(x): the hyperbolic sine",
            Fun::Cosh => "cosh(x): the hyperbolic cosine",
            Fun::Tanh => "tanh(x): the hyperbolic tangent",
            Fun::Sqrt => "sqrt(x): the principal square root, the power x^(1/2) (exact radicals in canonical form)",
        }
    }

    /// The function named `name`.
    pub fn from_name(name: &str) -> Option<Fun> {
        Fun::ALL.into_iter().find(|f| f.name() == name)
    }

    /// Odd functions satisfy f(-x) = -f(x); even ones f(-x) = f(x) (used for exact arguments with a negative
    /// coefficient).
    pub fn parity(self) -> Option<bool> {
        match self {
            Fun::Sin | Fun::Tan | Fun::Atan | Fun::Asin | Fun::Sinh | Fun::Tanh => Some(true),
            Fun::Cos | Fun::Cosh => Some(false),
            _ => None,
        }
    }

    /// An enclosure of f on the ball at `p` bits, or `None` (outside the real domain, or not decided).
    pub fn ball(self, x: &Ball, p: u64) -> Option<Ball> {
        match self {
            Fun::Exp => el::exp(x, p),
            Fun::Log => el::log(x, p),
            Fun::Sin => el::sin(x, p),
            Fun::Cos => el::cos(x, p),
            Fun::Tan => el::tan(x, p),
            Fun::Atan => el::atan(x, p),
            Fun::Asin => el::asin(x, p),
            Fun::Acos => el::acos(x, p),
            Fun::Sinh => el::sinh(x, p),
            Fun::Cosh => el::cosh(x, p),
            Fun::Tanh => el::tanh(x, p),
            Fun::Sqrt => el::sqrt(x, p),
        }
    }

    /// An enclosure of f on the complex ball at `p` bits (principal branches, [`crate::cball`]), or `None`.
    pub fn cball(self, z: &CBall, p: u64) -> Option<CBall> {
        match self {
            Fun::Exp => cball::exp(z, p),
            Fun::Log => cball::log(z, p),
            Fun::Sin => cball::sin(z, p),
            Fun::Cos => cball::cos(z, p),
            Fun::Tan => cball::tan(z, p),
            Fun::Atan => cball::atan(z, p),
            Fun::Asin => cball::asin(z, p),
            Fun::Acos => cball::acos(z, p),
            Fun::Sinh => cball::sinh(z, p),
            Fun::Cosh => cball::cosh(z, p),
            Fun::Tanh => cball::tanh(z, p),
            Fun::Sqrt => cball::sqrt(z, p),
        }
    }

    /// The shared machine function (decision E.4).
    pub fn real(self) -> machine::RealFn {
        match self {
            Fun::Exp => machine::exp,
            Fun::Log => machine::log,
            Fun::Sin => machine::sin,
            Fun::Cos => machine::cos,
            Fun::Tan => machine::tan,
            Fun::Atan => machine::atan,
            Fun::Asin => machine::asin,
            Fun::Acos => machine::acos,
            Fun::Sinh => machine::sinh,
            Fun::Cosh => machine::cosh,
            Fun::Tanh => machine::tanh,
            Fun::Sqrt => machine::sqrt,
        }
    }

    /// f in machine arithmetic: the shared real function inside its real domain (and for NaN), the principal
    /// complex value outside it or for a complex argument (decision L.9).
    pub fn machine(self, x: Mach) -> Mach {
        if let Mach::Real(v) = x {
            let real = match self {
                Fun::Log | Fun::Sqrt => v >= 0.0 || v.is_nan(),
                Fun::Asin | Fun::Acos => (-1.0..=1.0).contains(&v) || v.is_nan(),
                _ => true,
            };
            if real {
                return Mach::Real(self.real()(v));
            }
        }
        let z = x.c64();
        Mach::Complex(match self {
            Fun::Exp => z.exp(),
            Fun::Log => z.log(),
            Fun::Sin => z.sin(),
            Fun::Cos => z.cos(),
            Fun::Tan => z.tan(),
            Fun::Atan => z.atan(),
            Fun::Asin => z.asin(),
            Fun::Acos => z.acos(),
            Fun::Sinh => z.sinh(),
            Fun::Cosh => z.cosh(),
            Fun::Tanh => z.tanh(),
            Fun::Sqrt => z.sqrt(),
        })
    }
}
