//! Double-double arithmetic: a value hi + lo with |lo| <= ulp(hi)/2, about 106 bits, for the machine-precision
//! paths. Every machine special function computes in this arithmetic and rounds once at the end, so its error is
//! dominated by that final rounding (decision SF.3).
//!
//! The error-free transformations are Knuth's two-sum and Dekker's product with Veltkamp splitting (no fused
//! multiply-add: the default x86-64 target has none, and a library `fma` call is slow); the split of values above
//! 2^995 works on a scaled copy. [`Scaled`] carries an extra binary exponent for results that would leave the double
//! range.

/// A double-double number hi + lo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DD {
    /// the leading part (the nearest double to the value)
    pub hi: f64,
    /// the trailing part
    pub lo: f64,
}

/// ln 2 as a double-double.
pub const LN2: DD = DD { hi: std::f64::consts::LN_2, lo: 2.319_046_813_846_299_6e-17 };
/// pi as a double-double.
pub const PI: DD = DD { hi: std::f64::consts::PI, lo: 1.224_646_799_147_353_2e-16 };
/// ln(2 pi) / 2 as a double-double.
pub const HALF_LN_2PI: DD = DD { hi: 0.918_938_533_204_672_8, lo: -3.878_294_158_067_241_4e-17 };
/// ln(2 pi) as a double-double.
pub const LN_2PI: DD = DD { hi: 1.837_877_066_409_345_6, lo: -7.756_588_316_134_483e-17 };
/// ln(pi) as a double-double.
pub const LN_PI: DD = DD { hi: 1.144_729_885_849_400_2, lo: 1.026_595_116_270_782_6e-17 };

#[inline]
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

#[inline]
fn quick_two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    (s, b - (s - a))
}

#[inline]
fn split(a: f64) -> (f64, f64) {
    if a.abs() > 6.7e299 {
        // 134217729 a would overflow: split a scaled copy (exact powers of two)
        let (h, l) = split(a * 3.725_290_298_461_914e-9);
        return (h * 268_435_456.0, l * 268_435_456.0);
    }
    let c = 134_217_729.0 * a;
    let h = c - (c - a);
    (h, a - h)
}

#[inline]
fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    (p, ((ah * bh - p) + ah * bl + al * bh) + al * bl)
}

impl DD {
    /// Zero.
    pub const ZERO: DD = DD { hi: 0.0, lo: 0.0 };
    /// One.
    pub const ONE: DD = DD { hi: 1.0, lo: 0.0 };

    /// A double, exactly.
    #[inline]
    pub const fn new(x: f64) -> DD {
        DD { hi: x, lo: 0.0 }
    }
    /// The exact sum of two doubles.
    #[inline]
    pub fn sum(a: f64, b: f64) -> DD {
        let (s, e) = two_sum(a, b);
        DD { hi: s, lo: e }
    }
    /// The nearest double.
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.hi + self.lo
    }
    /// -x.
    #[inline]
    pub fn neg(self) -> DD {
        DD { hi: -self.hi, lo: -self.lo }
    }
    /// |x|.
    #[inline]
    pub fn abs(self) -> DD {
        if self.hi < 0.0 {
            self.neg()
        } else {
            self
        }
    }
    /// x + y.
    #[inline]
    pub fn add(self, o: DD) -> DD {
        let (s, e) = two_sum(self.hi, o.hi);
        let (t, f) = two_sum(self.lo, o.lo);
        let (s, e) = quick_two_sum(s, e + t);
        let (hi, lo) = quick_two_sum(s, e + f);
        DD { hi, lo }
    }
    /// x + y for a double y.
    #[inline]
    pub fn add_f(self, o: f64) -> DD {
        let (s, e) = two_sum(self.hi, o);
        let (hi, lo) = quick_two_sum(s, e + self.lo);
        DD { hi, lo }
    }
    /// x - y.
    #[inline]
    pub fn sub(self, o: DD) -> DD {
        self.add(o.neg())
    }
    /// x y.
    #[inline]
    pub fn mul(self, o: DD) -> DD {
        let (p, e) = two_prod(self.hi, o.hi);
        let (hi, lo) = quick_two_sum(p, e + (self.hi * o.lo + self.lo * o.hi));
        DD { hi, lo }
    }
    /// x y for a double y.
    #[inline]
    pub fn mul_f(self, o: f64) -> DD {
        let (p, e) = two_prod(self.hi, o);
        let (hi, lo) = quick_two_sum(p, e + self.lo * o);
        DD { hi, lo }
    }
    /// x 2^k, exact in the normal range.
    #[inline]
    pub fn ldexp(self, k: i32) -> DD {
        let s = pow2(k);
        DD { hi: self.hi * s, lo: self.lo * s }
    }
    /// x / y (three quotient digits).
    pub fn div(self, o: DD) -> DD {
        let q1 = self.hi / o.hi;
        let r = self.sub(o.mul_f(q1));
        let q2 = r.hi / o.hi;
        let r = r.sub(o.mul_f(q2));
        let q3 = r.hi / o.hi;
        let (hi, lo) = quick_two_sum(q1, q2);
        DD { hi, lo }.add_f(q3)
    }
    /// x / y for a double y.
    pub fn div_f(self, o: f64) -> DD {
        self.div(DD::new(o))
    }
    /// 1 / x.
    pub fn recip(self) -> DD {
        DD::ONE.div(self)
    }
    /// x^2.
    #[inline]
    pub fn sqr(self) -> DD {
        self.mul(self)
    }
    /// sqrt(x) for x >= 0 (one Newton step on the double square root).
    pub fn sqrt(self) -> DD {
        if self.hi <= 0.0 {
            return DD::new(self.hi.sqrt());
        }
        let y = self.hi.sqrt();
        let r = self.sub(DD::new(y).sqr());
        DD::new(y).add_f(r.hi / (2.0 * y))
    }
    /// x^n for an integer n >= 0 (binary powering).
    pub fn powi(self, mut n: u64) -> DD {
        let (mut b, mut r) = (self, DD::ONE);
        while n > 0 {
            if n & 1 == 1 {
                r = r.mul(b);
            }
            n >>= 1;
            if n > 0 {
                b = b.sqr();
            }
        }
        r
    }
    /// The nearest integer to hi (ties away from zero), as a double.
    pub fn round_hi(self) -> f64 {
        self.hi.round()
    }
}

/// 2^k for any integer k, exactly where representable (0 or infinity beyond).
pub fn pow2(k: i32) -> f64 {
    if (-1022..=1023).contains(&k) {
        f64::from_bits(((k + 1023) as u64) << 52)
    } else if k > 1023 {
        f64::INFINITY
    } else if k >= -1074 {
        f64::from_bits(1u64 << (k + 1074))
    } else {
        0.0
    }
}

/// x 2^k for a double x with IEEE overflow and gradual underflow (one rounding).
pub fn ldexp(x: f64, k: i64) -> f64 {
    let k = k.clamp(-3000, 3000) as i32;
    if k > 1000 {
        return ldexp(x * pow2(1000), (k - 1000) as i64);
    }
    if k < -1000 {
        // scale into the subnormal range in one final step so that the rounding happens once
        let (m, e) = frexp(x);
        let t = e as i64 + k as i64;
        if t < -1080 {
            return 0.0 * x.signum();
        }
        return if t >= -1021 { m * pow2(t as i32) } else { (m * pow2(t as i32 + 600)) * pow2(-600) };
    }
    x * pow2(k)
}

/// (m, e) with x = m 2^e and 0.5 <= |m| < 1 (m = x for zero, infinities and NaN).
pub fn frexp(x: f64) -> (f64, i32) {
    if x == 0.0 || !x.is_finite() {
        return (x, 0);
    }
    let bits = x.to_bits();
    let be = ((bits >> 52) & 0x7ff) as i32;
    if be == 0 {
        let (m, e) = frexp(x * pow2(64));
        return (m, e - 64);
    }
    let m = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52));
    (m, be - 1022)
}

/// A double-double with a separate binary exponent: m 2^e, |m.hi| in [0.5, 1) (or m zero). Products of many
/// factors and values beyond the double range (gamma of large or negative arguments, Bessel recurrences) are
/// carried in this form and rounded once by [`Scaled::to_f64`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scaled {
    /// the mantissa
    pub m: DD,
    /// the binary exponent
    pub e: i64,
}

impl Scaled {
    /// A double-double value.
    pub fn new(x: DD) -> Scaled {
        Scaled { m: x, e: 0 }.norm()
    }
    /// m 2^e normalised.
    pub fn norm(self) -> Scaled {
        if self.m.hi == 0.0 || !self.m.hi.is_finite() {
            return self;
        }
        let (_, k) = frexp(self.m.hi);
        Scaled { m: self.m.ldexp(-k), e: self.e + k as i64 }
    }
    /// x y.
    pub fn mul(self, o: Scaled) -> Scaled {
        Scaled { m: self.m.mul(o.m), e: self.e + o.e }.norm()
    }
    /// x y for a double-double y.
    pub fn mul_dd(self, o: DD) -> Scaled {
        self.mul(Scaled::new(o))
    }
    /// x / y.
    pub fn div(self, o: Scaled) -> Scaled {
        Scaled { m: self.m.div(o.m), e: self.e - o.e }.norm()
    }
    /// x + y.
    pub fn add(self, o: Scaled) -> Scaled {
        if self.m.hi == 0.0 {
            return o;
        }
        if o.m.hi == 0.0 {
            return self;
        }
        let (big, small) = if self.e >= o.e { (self, o) } else { (o, self) };
        let d = big.e - small.e;
        if d > 200 {
            return big;
        }
        Scaled { m: big.m.add(small.m.ldexp(-(d as i32))), e: big.e }.norm()
    }
    /// -x.
    pub fn neg(self) -> Scaled {
        Scaled { m: self.m.neg(), e: self.e }
    }
    /// The nearest double, with IEEE overflow to infinity and underflow to zero.
    pub fn to_f64(self) -> f64 {
        if self.m.hi == 0.0 || !self.m.hi.is_finite() {
            return self.m.hi;
        }
        if self.e > 1100 {
            return f64::INFINITY * self.m.hi.signum();
        }
        if self.e < -1200 {
            return 0.0 * self.m.hi.signum();
        }
        ldexp(self.m.to_f64(), self.e)
    }
    /// ln |x| as a double-double.
    pub fn ln_abs(self) -> DD {
        log(self.m.abs()).add(LN2.mul_f(self.e as f64))
    }
}

/// e^x as a scaled double-double (accurate to about 2^-100 relative for |x| < 2^20).
pub fn exp_scaled(x: DD) -> Scaled {
    if x.hi.is_nan() {
        return Scaled { m: DD::new(f64::NAN), e: 0 };
    }
    if x.hi > 1e15 {
        return Scaled { m: DD::new(0.5), e: i64::MAX / 4 };
    }
    if x.hi < -1e15 {
        return Scaled { m: DD::ZERO, e: 0 };
    }
    let k = (x.hi / LN2.hi).round();
    let r = x.sub(LN2.mul_f(k));
    // e^r = (e^(r/512))^512, e^s - 1 by its Taylor series for |s| < 7e-4 (ten terms reach 2^-110)
    let s = r.ldexp(-9);
    let mut term = s;
    let mut em1 = s;
    for n in 2..=11 {
        term = term.mul(s).div_f(n as f64);
        em1 = em1.add(term);
    }
    for _ in 0..9 {
        // (1 + u)^2 - 1 = u (2 + u), without losing u's low digits
        em1 = em1.mul(em1.add_f(2.0));
    }
    Scaled { m: em1.add_f(1.0), e: k as i64 }.norm()
}

/// e^x as a double-double (finite while e^x is in the double range).
pub fn exp(x: DD) -> DD {
    let s = exp_scaled(x);
    s.m.ldexp(s.e.clamp(-2000, 2000) as i32)
}

/// ln x for x > 0 (one Newton step from the double logarithm: y + x e^-y - 1).
pub fn log(x: DD) -> DD {
    let y = x.hi.ln();
    if !y.is_finite() {
        return DD::new(y);
    }
    let e = exp_scaled(DD::new(-y));
    let t = x.mul(e.m).ldexp(e.e as i32);
    DD::new(y).add(t.add_f(-1.0))
}

/// sin(pi r) and cos(pi r) for |r| <= 1/2 by their Taylor series (about 2^-104 absolute).
pub fn sin_cos_pi(r: DD) -> (DD, DD) {
    let x = PI.mul(r);
    let x2 = x.sqr();
    let (mut s, mut ts) = (x, x);
    let (mut c, mut tc) = (DD::ONE, DD::ONE);
    let mut k = 1.0;
    loop {
        ts = ts.mul(x2).div_f(-(2.0 * k) * (2.0 * k + 1.0));
        tc = tc.mul(x2).div_f(-(2.0 * k - 1.0) * (2.0 * k));
        s = s.add(ts);
        c = c.add(tc);
        if ts.hi.abs() < 1e-34 && tc.hi.abs() < 1e-34 {
            break;
        }
        k += 1.0;
    }
    (s, c)
}

/// sin(pi x) and cos(pi x) for any double-double x, with the exact reduction x = n + r, |r| <= 1/2 (so the zeros
/// at integers and half-integers are exact).
pub fn sin_cos_pi_any(x: DD) -> (DD, DD) {
    let n = x.hi.round();
    let r = DD::sum(x.hi - n, x.lo);
    let (n, r) = if r.hi > 0.5 {
        (n + 1.0, r.add_f(-1.0))
    } else if r.hi < -0.5 {
        (n - 1.0, r.add_f(1.0))
    } else {
        (n, r)
    };
    let (s, c) = sin_cos_pi(r);
    if n.rem_euclid(2.0) == 1.0 {
        (s.neg(), c.neg())
    } else {
        (s, c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_and_functions() {
        // ln 2, pi from series identities and the functions
        assert!((exp(LN2).sub(DD::new(2.0))).hi.abs() < 1e-30);
        assert!((log(DD::new(2.0)).sub(LN2)).hi.abs() < 1e-31);
        assert!(exp(HALF_LN_2PI.mul_f(2.0)).sub(PI.mul_f(2.0)).hi.abs() < 1e-30);
        assert!(exp(LN_PI).sub(PI).hi.abs() < 1e-30);
        assert!(log(DD::new(2.0).mul(PI)).sub(LN_2PI).hi.abs() < 1e-30);
        let (s, c) = sin_cos_pi(DD::new(0.25));
        let h = DD::new(0.5).sqrt();
        assert!(s.sub(h).hi.abs() < 1e-31 && c.sub(h).hi.abs() < 1e-31);
        let (s, _) = sin_cos_pi_any(DD::new(7.0));
        assert_eq!(s.hi, 0.0);
        let x = DD::new(3.0).div(DD::new(7.0));
        assert!(x.mul_f(7.0).sub(DD::new(3.0)).hi.abs() < 1e-31);
        let e = exp_scaled(DD::new(-745.5));
        assert!(e.e < -1000 && e.m.hi > 0.5);
        assert_eq!(Scaled::new(DD::new(3.0)).mul(Scaled { m: DD::new(0.5), e: -1074 }).to_f64(), f64::from_bits(2));
    }

    #[test]
    fn scaling() {
        assert_eq!(ldexp(1.5, -1074), f64::from_bits(2));
        assert_eq!(ldexp(1.0, 1024), f64::INFINITY);
        assert_eq!(ldexp(-1.0, -1200), -0.0);
        assert_eq!(frexp(8.0), (0.5, 4));
        assert_eq!(Scaled { m: DD::new(0.75), e: 3 }.to_f64(), 6.0);
        assert_eq!(Scaled { m: DD::new(0.5), e: 1025 }.to_f64(), f64::INFINITY);
    }
}
