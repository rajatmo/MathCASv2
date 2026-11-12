//! Complex double-double numbers (round 29): re + i im with double-double parts, about 32
//! digits, for the machine fast paths of the round 28 functions ([`crate::fast29`]). A series or a Taylor
//! continuation that cancels ten digits still rounds to the correct double, so the machine values keep the
//! precision contract (agreement with the 40-digit value of the exact argument) without the ball loop.
//!
//! The elementary functions take one Newton step from the double result (sqrt, log) or reduce exactly (sin and cos
//! of the real part by [`crate::dd::sin_cos_pi_any`] of x/pi, exp of the real part by [`crate::dd::exp`]).
use crate::dd::{self, DD};
use mc_numeric::machine::C64;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// A complex double-double number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CD {
    /// the real part
    pub re: DD,
    /// the imaginary part
    pub im: DD,
}

impl CD {
    /// Zero.
    pub const ZERO: CD = CD { re: DD::ZERO, im: DD::ZERO };
    /// One.
    pub const ONE: CD = CD { re: DD::ONE, im: DD::ZERO };

    /// re + i im.
    pub const fn new(re: DD, im: DD) -> CD {
        CD { re, im }
    }
    /// A real double-double number.
    pub const fn real(re: DD) -> CD {
        CD { re, im: DD::ZERO }
    }
    /// A double, exactly.
    pub const fn f(x: f64) -> CD {
        CD { re: DD::new(x), im: DD::ZERO }
    }
    /// A complex machine number, exactly.
    pub const fn c(z: C64) -> CD {
        CD { re: DD::new(z.re), im: DD::new(z.im) }
    }
    /// The nearest complex machine number.
    pub fn to_c64(self) -> C64 {
        C64::new(self.re.to_f64(), self.im.to_f64())
    }
    /// |z| as a double.
    pub fn abs(self) -> f64 {
        self.re.hi.hypot(self.im.hi)
    }
    /// Is every part finite?
    pub fn finite(self) -> bool {
        self.re.hi.is_finite() && self.im.hi.is_finite() && self.re.lo.is_finite() && self.im.lo.is_finite()
    }
    /// z times a double.
    pub fn scale(self, s: f64) -> CD {
        CD { re: self.re.mul_f(s), im: self.im.mul_f(s) }
    }
    /// z times a real double-double.
    pub fn mul_dd(self, s: DD) -> CD {
        CD { re: self.re * s, im: self.im * s }
    }
    /// i z.
    pub fn mul_i(self) -> CD {
        CD { re: -self.im, im: self.re }
    }
    /// z^2.
    pub fn sqr(self) -> CD {
        self * self
    }
    /// e^z (finite while |e^z| is in the double range).
    pub fn exp(self) -> CD {
        let m = dd::exp(self.re);
        let (s, c) = dd::sin_cos_pi_any(self.im / dd::PI);
        CD { re: m * c, im: m * s }
    }
    /// The principal square root (one Newton step on the machine root).
    pub fn sqrt(self) -> CD {
        let s0 = CD::c(self.to_c64().sqrt());
        if s0.re.hi == 0.0 && s0.im.hi == 0.0 {
            return s0;
        }
        // s = s0 + (z - s0^2)/(2 s0)
        s0 + (self - s0.sqr()) / s0.scale(2.0)
    }
    /// The principal logarithm (one Newton step on the machine logarithm: w + z e^-w - 1).
    pub fn log(self) -> CD {
        let w0 = CD::c(self.to_c64().log());
        if !w0.finite() {
            return w0;
        }
        w0 + (self * (-w0).exp() - CD::ONE)
    }
    /// z^a = exp(a log z) (0 at z = 0).
    pub fn pow(self, a: CD) -> CD {
        if self.re.hi == 0.0 && self.im.hi == 0.0 {
            return CD::ZERO;
        }
        (a * self.log()).exp()
    }
    /// sin z.
    pub fn sin(self) -> CD {
        // (e^(iz) - e^(-iz))/(2i)
        let a = self.mul_i().exp();
        let b = (-self.mul_i()).exp();
        let d = a - b;
        CD { re: d.im.mul_f(0.5), im: -d.re.mul_f(0.5) }
    }
    /// cos z.
    pub fn cos(self) -> CD {
        let a = self.mul_i().exp();
        let b = (-self.mul_i()).exp();
        (a + b).scale(0.5)
    }
    /// sin z and cos z of a real z exactly reduced (the imaginary part ignored).
    pub fn sin_cos_real(x: DD) -> (DD, DD) {
        dd::sin_cos_pi_any(x / dd::PI)
    }
}

impl Add for CD {
    type Output = CD;
    #[inline]
    fn add(self, o: CD) -> CD {
        CD { re: self.re + o.re, im: self.im + o.im }
    }
}

impl Sub for CD {
    type Output = CD;
    #[inline]
    fn sub(self, o: CD) -> CD {
        CD { re: self.re - o.re, im: self.im - o.im }
    }
}

impl Neg for CD {
    type Output = CD;
    #[inline]
    fn neg(self) -> CD {
        CD { re: -self.re, im: -self.im }
    }
}

impl Mul for CD {
    type Output = CD;
    #[inline]
    fn mul(self, o: CD) -> CD {
        if self.im.hi == 0.0 && self.im.lo == 0.0 && o.im.hi == 0.0 && o.im.lo == 0.0 {
            return CD::real(self.re * o.re);
        }
        CD { re: self.re * o.re - self.im * o.im, im: self.re * o.im + self.im * o.re }
    }
}

/// The quotient with the divisor scaled by a power of two (no premature overflow).
impl Div for CD {
    type Output = CD;
    fn div(self, o: CD) -> CD {
        if o.im.hi == 0.0 && o.im.lo == 0.0 {
            return CD { re: self.re / o.re, im: self.im / o.re };
        }
        let s = o.re.hi.abs().max(o.im.hi.abs());
        let k = if s > 0.0 && s.is_finite() { -(dd::frexp(s).1) } else { 0 };
        let (a, b) = (o.re.ldexp(k), o.im.ldexp(k));
        let d = a * a + b * b;
        let re = (self.re * a + self.im * b) / d;
        let im = (self.im * a - self.re * b) / d;
        CD { re: re.ldexp(k), im: im.ldexp(k) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: CD, b: CD, tol: f64) -> bool {
        (a - b).abs() <= tol * b.abs().max(1e-300)
    }

    #[test]
    fn elementary() {
        let z = CD::new(DD::new(0.7), DD::new(-1.3));
        assert!(close(z.sqrt().sqr(), z, 1e-30));
        assert!(close(z.log().exp(), z, 1e-30));
        let s = z.sin();
        let c = z.cos();
        assert!(close(s.sqr() + c.sqr(), CD::ONE, 1e-30));
        assert!(close((z / CD::new(DD::new(3.0), DD::new(4.0))) * CD::new(DD::new(3.0), DD::new(4.0)), z, 1e-30));
        let third = DD::ONE / DD::new(3.0);
        assert!(close(CD::f(8.0).pow(CD::real(third)), CD::f(2.0), 1e-30));
    }
}
