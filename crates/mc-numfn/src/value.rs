//! The number types a [`crate::NumFn`] runs on.
//!
//! - [`Value`]: what the interpreted backend needs (conversion from and to the arithmetic's [`Num`], finiteness):
//!   machine reals (`f64`), complex machine numbers ([`C64`]) and any number ([`Num`]: exact, balls at any
//!   precision, complex).
//! - [`Scalar`]: a machine field (`f64`, [`C64`]) with a magnitude, for iterations written once for both (moved
//!   unchanged from `mc-roots`, decision N0.6). Every `Scalar` is a `Value`.
use mc_arith::{Num, Real};
use mc_numeric::machine::C64;
use std::ops::{Add, Div, Mul, Sub};

/// A number type a function can be evaluated on.
pub trait Value: Clone + Send + Sync {
    /// The value of a number, if it is of this type: any real for `f64` (a complex number only with an exactly
    /// zero imaginary part); any number for [`C64`] and [`Num`].
    fn from_num(n: &Num) -> Option<Self>;
    /// The number.
    fn to_number(&self) -> Num;
    /// Finite (both parts; a ball with a finite midpoint and radius).
    fn finite(&self) -> bool;
}

/// A machine scalar: a field with a magnitude and conversions from and to numbers.
pub trait Scalar:
    Copy + Send + Sync + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> + Div<Output = Self>
{
    /// The real number `x`.
    fn real(x: f64) -> Self;
    /// The magnitude |x| (overflow-safe for complex numbers).
    fn abs(self) -> f64;
    /// The complex conjugate (the identity on reals).
    fn conj(self) -> Self;
    /// Both parts finite.
    fn is_finite(self) -> bool;
    /// The scalar of a number: any real for `f64` (a complex number only with an exactly zero imaginary part); any
    /// real or complex number for [`C64`].
    fn of_num(n: &Num) -> Option<Self>;
    /// The machine number.
    fn to_num(self) -> Num;
    /// A real scalar clamped into [lo, hi] (bounds apply to real variables only; complex scalars are unchanged).
    fn clamp_real(self, lo: f64, hi: f64) -> Self;
}

impl<S: Scalar> Value for S {
    fn from_num(n: &Num) -> Option<S> {
        S::of_num(n)
    }
    fn to_number(&self) -> Num {
        self.to_num()
    }
    fn finite(&self) -> bool {
        self.is_finite()
    }
}

fn real_finite(r: &Real) -> bool {
    match r {
        Real::Exact(_) => true,
        Real::Machine(x) => x.is_finite(),
        // midpoints and radii are finite binary floats
        Real::Ball(..) => true,
    }
}

impl Value for Num {
    fn from_num(n: &Num) -> Option<Num> {
        Some(n.clone())
    }
    fn to_number(&self) -> Num {
        self.clone()
    }
    fn finite(&self) -> bool {
        match self {
            Num::Real(r) => real_finite(r),
            Num::Complex(a, b) => real_finite(a) && real_finite(b),
        }
    }
}

impl Scalar for f64 {
    fn real(x: f64) -> f64 {
        x
    }
    fn abs(self) -> f64 {
        f64::abs(self)
    }
    fn conj(self) -> f64 {
        self
    }
    fn is_finite(self) -> bool {
        f64::is_finite(self)
    }
    fn of_num(n: &Num) -> Option<f64> {
        match n {
            Num::Real(r) => Some(r.to_f64()),
            Num::Complex(re, im) if im.is_zero() => Some(re.to_f64()),
            Num::Complex(..) => None,
        }
    }
    fn to_num(self) -> Num {
        Num::Real(Real::Machine(self))
    }
    fn clamp_real(self, lo: f64, hi: f64) -> f64 {
        self.clamp(lo, hi)
    }
}

impl Scalar for C64 {
    fn real(x: f64) -> C64 {
        C64::real(x)
    }
    fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
    fn conj(self) -> C64 {
        C64::new(self.re, -self.im)
    }
    fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
    fn of_num(n: &Num) -> Option<C64> {
        Some(match n {
            Num::Real(r) => C64::real(r.to_f64()),
            Num::Complex(re, im) => C64::new(re.to_f64(), im.to_f64()),
        })
    }
    fn to_num(self) -> Num {
        Num::Complex(Real::Machine(self.re), Real::Machine(self.im))
    }
    fn clamp_real(self, _lo: f64, _hi: f64) -> C64 {
        self
    }
}

/// The largest magnitude of the entries (the maximum norm); 0 for an empty slice.
pub fn norm<S: Scalar>(v: &[S]) -> f64 {
    v.iter().fold(0.0, |m, x| m.max(x.abs()))
}

/// The squared Euclidean norm, the merit function of the line searches.
pub fn norm2<S: Scalar>(v: &[S]) -> f64 {
    // scaled by the largest entry, so that values near the overflow threshold still compare correctly
    let m = norm(v);
    if m == 0.0 || !m.is_finite() {
        return m * m;
    }
    let s: f64 = v.iter().map(|x| (x.abs() / m).powi(2)).sum();
    s * m * m
}

/// All entries finite.
pub fn all_finite<S: Scalar>(v: &[S]) -> bool {
    v.iter().all(|x| x.is_finite())
}
