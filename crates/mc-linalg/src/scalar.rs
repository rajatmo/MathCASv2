//! Machine scalars: `f64` and the complex [`Cx`], behind the [`Scalar`] trait the dense kernels are generic over.
use std::fmt::Debug;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// A complex machine number `re + im i`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cx {
    /// the real part
    pub re: f64,
    /// the imaginary part
    pub im: f64,
}

impl Cx {
    /// `re + im i`.
    pub const fn new(re: f64, im: f64) -> Cx {
        Cx { re, im }
    }
}

impl Add for Cx {
    type Output = Cx;
    #[inline]
    fn add(self, o: Cx) -> Cx {
        Cx::new(self.re + o.re, self.im + o.im)
    }
}

impl Sub for Cx {
    type Output = Cx;
    #[inline]
    fn sub(self, o: Cx) -> Cx {
        Cx::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for Cx {
    type Output = Cx;
    #[inline]
    fn mul(self, o: Cx) -> Cx {
        Cx::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
}

impl Div for Cx {
    type Output = Cx;
    /// Smith's algorithm (no overflow for large but representable operands).
    #[inline]
    fn div(self, o: Cx) -> Cx {
        if o.re.abs() >= o.im.abs() {
            let r = o.im / o.re;
            let d = o.re + o.im * r;
            Cx::new((self.re + self.im * r) / d, (self.im - self.re * r) / d)
        } else {
            let r = o.re / o.im;
            let d = o.re * r + o.im;
            Cx::new((self.re * r + self.im) / d, (self.im * r - self.re) / d)
        }
    }
}

impl Neg for Cx {
    type Output = Cx;
    #[inline]
    fn neg(self) -> Cx {
        Cx::new(-self.re, -self.im)
    }
}

impl AddAssign for Cx {
    #[inline]
    fn add_assign(&mut self, o: Cx) {
        *self = *self + o;
    }
}

impl SubAssign for Cx {
    #[inline]
    fn sub_assign(&mut self, o: Cx) {
        *self = *self - o;
    }
}

/// A machine scalar: real (`f64`) or complex ([`Cx`]).
pub trait Scalar:
    Copy
    + Debug
    + Default
    + PartialEq
    + Send
    + Sync
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
    + AddAssign
    + SubAssign
{
    /// Is this the complex type?
    const COMPLEX: bool;
    /// zero
    fn zero() -> Self {
        Self::default()
    }
    /// one
    fn one() -> Self {
        Self::real(1.0)
    }
    /// A real value.
    fn real(v: f64) -> Self;
    /// The modulus.
    fn abs(self) -> f64;
    /// The squared modulus.
    fn abs2(self) -> f64;
    /// The complex conjugate.
    fn conj(self) -> Self;
    /// The real part.
    fn re(self) -> f64;
    /// The imaginary part.
    fn im(self) -> f64;
    /// Multiplication by a real.
    fn scale(self, s: f64) -> Self;
    /// As a complex number.
    fn to_cx(self) -> Cx;
    /// Is it finite (no NaN or infinite part)?
    fn is_finite(self) -> bool;
}

impl Scalar for f64 {
    const COMPLEX: bool = false;
    #[inline]
    fn real(v: f64) -> f64 {
        v
    }
    #[inline]
    fn abs(self) -> f64 {
        f64::abs(self)
    }
    #[inline]
    fn abs2(self) -> f64 {
        self * self
    }
    #[inline]
    fn conj(self) -> f64 {
        self
    }
    #[inline]
    fn re(self) -> f64 {
        self
    }
    #[inline]
    fn im(self) -> f64 {
        0.0
    }
    #[inline]
    fn scale(self, s: f64) -> f64 {
        self * s
    }
    #[inline]
    fn to_cx(self) -> Cx {
        Cx::new(self, 0.0)
    }
    #[inline]
    fn is_finite(self) -> bool {
        f64::is_finite(self)
    }
}

impl Scalar for Cx {
    const COMPLEX: bool = true;
    #[inline]
    fn real(v: f64) -> Cx {
        Cx::new(v, 0.0)
    }
    #[inline]
    fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
    #[inline]
    fn abs2(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
    #[inline]
    fn conj(self) -> Cx {
        Cx::new(self.re, -self.im)
    }
    #[inline]
    fn re(self) -> f64 {
        self.re
    }
    #[inline]
    fn im(self) -> f64 {
        self.im
    }
    #[inline]
    fn scale(self, s: f64) -> Cx {
        Cx::new(self.re * s, self.im * s)
    }
    #[inline]
    fn to_cx(self) -> Cx {
        self
    }
    #[inline]
    fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
}

/// The principal square root of a complex number.
pub fn csqrt(z: Cx) -> Cx {
    if z.im == 0.0 {
        return if z.re >= 0.0 { Cx::new(z.re.sqrt(), 0.0) } else { Cx::new(0.0, (-z.re).sqrt()) };
    }
    let r = z.abs();
    let a = ((r + z.re.abs()) / 2.0).sqrt();
    let b = z.im.abs() / (2.0 * a);
    if z.re >= 0.0 {
        Cx::new(a, b.copysign(z.im))
    } else {
        Cx::new(b, a.copysign(z.im))
    }
}

/// The unit-modulus sign `z/|z|` (one for zero), used by complex Householder reflections.
pub fn phase<S: Scalar>(z: S) -> S {
    let a = z.abs();
    if a == 0.0 {
        S::one()
    } else {
        z.scale(1.0 / a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complex_arithmetic() {
        let a = Cx::new(1.0, 2.0);
        let b = Cx::new(3.0, -1.0);
        assert_eq!(a * b, Cx::new(5.0, 5.0));
        let q = (a * b) / b;
        assert!((q - a).abs() < 1e-15);
        let q = a / Cx::new(0.0, 2.0);
        assert!((q - Cx::new(1.0, -0.5)).abs() < 1e-15);
        assert_eq!(csqrt(Cx::new(-4.0, 0.0)), Cx::new(0.0, 2.0));
        let s = csqrt(Cx::new(3.0, 4.0));
        assert!((s - Cx::new(2.0, 1.0)).abs() < 1e-15);
        let s = csqrt(Cx::new(-3.0, -4.0));
        assert!((s - Cx::new(1.0, -2.0)).abs() < 1e-15);
        assert_eq!(phase(Cx::new(0.0, 0.0)), Cx::new(1.0, 0.0));
    }
}
