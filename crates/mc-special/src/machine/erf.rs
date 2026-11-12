//! The error functions erf, erfc, erfi and the inverses of erf and erfc for machine reals (decision SF.3).
//!
//! Every value is formed in double-double arithmetic (with the high-order, small terms of the polynomials in plain
//! doubles) to a relative error of about 2^-60 and rounded once; the tests measure at most about one ulp over their
//! grids. Algorithms, for x >= 0 (erf is odd, erfc(-x) = 2 - erfc(x), erfi is odd):
//!
//! - **x < 1/2**: the Maclaurin series erf(x) = 2/sqrt(pi) sum (-1)^k x^(2k+1) / (k! (2k+1)) (16 terms; the same with
//!   positive terms for erfi), erfc = 1 - erf.
//! - **1/2 <= x < 6** (erfc) and **1/2 <= x < 8** (erfi): Taylor expansions at the centres i/8, |x - i/8| <= 1/16,
//!   of erfc and of Dawson's function F(x) = e^(-x^2) int_0^x e^(t^2) dt (erfi = 2/sqrt(pi) e^(x^2) F), with
//!   coefficients from the derivative recurrences (erfc' = -2/sqrt(pi) e^(-x^2), F' = 1 - 2xF), computed once at
//!   first use from 320-bit ball values of erfc(i/8) and erfi(i/8) (decision SF.5), truncated at 2^-72 relative.
//! - **x >= 6** (erfc): the even part of Laplace's continued fraction erfc(x) = 2x e^(-x^2)/sqrt(pi) /
//!   (2x^2 + 1 - 1*2/(2x^2 + 5 - 3*4/(2x^2 + 9 - ...))) to the depth 5 + 144/x^2 (measured to reach 2^-64; at most 9
//!   levels), the deep levels in doubles and the last two in double-double; x^2 exact as a double-double and
//!   e^(-x^2) in double-double carried with its own exponent, so the value keeps full relative accuracy down to the
//!   subnormal range (erfc(x) = 0 from x = 27.23).
//! - **x >= 8** (erfi): the asymptotic series of Dawson's function F(x) = 1/(2x) sum (2k-1)!!/(2x^2)^k, summed to its
//!   2^-64 term (at most 18 terms); erfi overflows to infinity from x = 26.64.
//! - **inverse_erf(y)**, |y| <= 1/2: four terms of the inverse series as the start, then Halley's iteration
//!   x <- x - u/(1 + x u), u = (erf(x) - y)/erf'(x), until the step is below 1e-7 relative (two steps; the cubic
//!   convergence leaves an error far below an ulp). |y| > 1/2: sign(y) inverse_erfc(1 - |y|) (an exact difference).
//! - **inverse_erfc(q)**, q < 1/2: the start solves x^2 + ln(sqrt(pi)/2 (x + sqrt(x^2 + 4/pi))) = -ln q (the bound
//!   erfc(x) <= 2 e^(-x^2)/(sqrt(pi)(x + sqrt(x^2 + 4/pi))) makes it an overestimate) by Newton's method in doubles,
//!   then Halley's iteration on erfc(x) - q with the residual and the derivative as scaled double-doubles (so it works
//!   down to subnormal q), Newton's step when Halley's denominator is small, until the step is below 1e-7; 1/2 <= q
//!   <= 1 as inverse_erf(1 - q), q > 1 as -inverse_erfc(2 - q) (exact differences).
//! - **erf_complex**: by symmetry in the first quadrant; |z|^2 >= 50 the asymptotic expansion of erfc, |z|^2 < 50
//!   the Maclaurin series (y > x) or Kummer's form (x >= y) when min(x, y) <= 4 (cancellation at most 47 bits), else
//!   Laplace's continued fraction, all in complex double-double ([`erf_complex`]).
use crate::ball::erf::{erfc_ball, erfi_ball};
use crate::dd::{self, Scaled, DD, LN2, PI};
use mc_arith::num::Ball;
use mc_numeric::elementary as el;
use mc_numeric::machine::C64;
use std::ops::{Add, Div, Mul, Neg, Sub};
use std::sync::OnceLock;

/// ln 2 / 64 as a double-double (an exact scaling of [`LN2`]).
const LN2_64: DD = DD { hi: LN2.hi / 64.0, lo: LN2.lo / 64.0 };

/// The terms kept in double-double at the low end of a polynomial (the others are summed in doubles).
const HEAD: usize = 5;

/// The constants and tables of this module, computed at first use.
struct Tables {
    /// 2 / sqrt(pi)
    two_sqrt_pi: DD,
    /// sqrt(pi) / 2
    sqrt_pi_2: DD,
    /// 2/sqrt(pi) (-1)^k / (k! (2k+1)): erf(x) = x sum_k c_k (x^2)^k
    erf0: Vec<DD>,
    /// 2/sqrt(pi) / (k! (2k+1)): erfi(x) = x sum_k c_k (x^2)^k
    erfi0: Vec<DD>,
    /// Taylor coefficients of erfc at i/8, i = ERFC_FIRST..=ERFC_LAST
    erfc_c: Vec<Vec<DD>>,
    /// Taylor coefficients of Dawson's function at i/8, i = DAWSON_FIRST..=DAWSON_LAST
    dawson_c: Vec<Vec<DD>>,
}

const ERFC_FIRST: usize = 4;
const ERFC_LAST: usize = 48;
const DAWSON_FIRST: usize = 4;
const DAWSON_LAST: usize = 64;
/// The working precision of the table construction.
const TABLE_BITS: u64 = 320;

/// 2^(j/64), j = 0..63, as double-doubles (computed at first use).
fn exp2_table() -> &'static [DD; 64] {
    static T: OnceLock<[DD; 64]> = OnceLock::new();
    T.get_or_init(|| std::array::from_fn(|j| dd::exp(LN2.mul_f(j as f64 / 64.0))))
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(build_tables)
}

fn build_tables() -> Tables {
    let _scope = mc_arith::num::StopScope::enter(None);
    let sqrt_pi = PI.sqrt();
    let two_sqrt_pi = DD::new(2.0).div(sqrt_pi);
    let mut erf0 = Vec::new();
    let mut erfi0 = Vec::new();
    let mut fact = DD::ONE;
    for k in 0..18 {
        if k > 0 {
            fact = fact.mul_f(k as f64);
        }
        let c = two_sqrt_pi.div(fact.mul_f((2 * k + 1) as f64));
        erf0.push(if k % 2 == 1 { c.neg() } else { c });
        erfi0.push(c);
    }
    let w = TABLE_BITS;
    // invariant: a constant table built once with the stop flag off, from fixed exact inputs at a fixed precision
    let k2 = Ball::from_i64(2).div(&el::sqrt(&crate::ball::pi(w).unwrap(), w).unwrap(), w).unwrap();
    let erfc_c = (ERFC_FIRST..=ERFC_LAST)
        .map(|i| {
            let x0 = Ball::from_ratio(i as i64, 8, w);
            // invariant: a constant table built once with the stop flag off, from fixed exact inputs at a fixed precision
            let e0 = erfc_ball(&x0, w).expect("erfc at a table centre");
            // invariant: a constant table built once with the stop flag off, from fixed exact inputs at a fixed precision
            let g0 = el::exp(&x0.mul(&x0, w).neg(), w).unwrap();
            // g_n: the Taylor coefficients of e^(-x^2); erfc's are d_n = -2/sqrt(pi) g_(n-1)/n
            let mut g = vec![g0.clone(), g0.mul(&x0, w).mul(&Ball::from_i64(-2), w)];
            let mut c = vec![e0];
            for n in 1..80usize {
                if n >= 2 {
                    let next = x0.mul(&g[n - 1], w).add(&g[n - 2], w).mul(&Ball::from_i64(-2), w);
                    // invariant: a constant table built once with the stop flag off, from fixed exact inputs at a fixed precision
                    g.push(next.div(&Ball::from_i64(n as i64), w).unwrap());
                }
                // invariant: a constant table built once with the stop flag off, from fixed exact inputs at a fixed precision
                let d = k2.mul(&g[n - 1], w).div(&Ball::from_i64(-(n as i64)), w).unwrap();
                c.push(d);
                if done(&c) {
                    break;
                }
            }
            c.iter().map(|b| super::float_dd(&b.mid)).collect()
        })
        .collect();
    // invariant: a constant table built once with the stop flag off, from fixed exact inputs at a fixed precision
    let sqrt_pi_b = el::sqrt(&crate::ball::pi(w).unwrap(), w).unwrap();
    let dawson_c = (DAWSON_FIRST..=DAWSON_LAST)
        .map(|i| {
            let x0 = Ball::from_ratio(i as i64, 8, w);
            // invariant: a constant table built once with the stop flag off, from fixed exact inputs at a fixed precision
            let e = el::exp(&x0.mul(&x0, w).neg(), w).unwrap();
            // invariant: a constant table built once with the stop flag off, from fixed exact inputs at a fixed precision
            let f0 = erfi_ball(&x0, w).expect("erfi at a table centre").mul(&e, w).mul(&sqrt_pi_b, w);
            // invariant: a constant table built once with the stop flag off, from fixed exact inputs at a fixed precision
            let f0 = f0.div(&Ball::from_i64(2), w).unwrap();
            let f1 = Ball::from_i64(1).sub(&x0.mul(&f0, w).mul(&Ball::from_i64(2), w), w);
            let mut c = vec![f0, f1];
            for n in 1..80usize {
                let next = x0.mul(&c[n], w).add(&c[n - 1], w).mul(&Ball::from_i64(-2), w);
                // invariant: a constant table built once with the stop flag off, from fixed exact inputs at a fixed precision
                c.push(next.div(&Ball::from_i64(n as i64 + 1), w).unwrap());
                if done(&c) {
                    break;
                }
            }
            c.iter().map(|b| super::float_dd(&b.mid)).collect()
        })
        .collect();
    Tables { two_sqrt_pi, sqrt_pi_2: sqrt_pi.mul_f(0.5), erf0, erfi0, erfc_c, dawson_c }
}

/// Have the last two coefficients c_n (1/16)^n fallen below 2^-72 |c_0|?
fn done(c: &[Ball]) -> bool {
    let n = c.len() - 1;
    if n < 6 {
        return false;
    }
    let l0 = crate::ball::log2_up(&c[0]);
    let small = |k: usize| crate::ball::log2_up(&c[k]) - 4.0 * (k as f64) < l0 - 72.0;
    small(n) && small(n - 1)
}

/// sum_k c_k h^k: the terms from [`HEAD`] on in doubles, the first ones in double-double.
#[inline]
fn poly(c: &[DD], h: f64) -> DD {
    let mut t = 0.0;
    for k in (HEAD.min(c.len())..c.len()).rev() {
        t = t * h + c[k].hi;
    }
    let mut s = DD::new(t);
    for k in (0..HEAD.min(c.len())).rev() {
        s = s.mul_f(h).add(c[k]);
    }
    s
}

/// sum_k c_k t^k for a double-double t (the first four terms in double-double).
#[inline]
fn poly_dd(c: &[DD], t: DD) -> DD {
    const H: usize = 4;
    let mut v = 0.0;
    for k in (H..c.len()).rev() {
        v = v * t.hi + c[k].hi;
    }
    let mut s = DD::new(v);
    for k in (0..H).rev() {
        s = s.mul(t).add(c[k]);
    }
    s
}

/// The Taylor expansion at the centre nearest to x (centres i/8, i in first..).
#[inline]
fn centred(table: &[Vec<DD>], first: usize, x: f64) -> DD {
    let i = (x * 8.0).round();
    let c = &table[i as usize - first];
    poly(c, x - i * 0.125)
}

/// e^x as a scaled double-double (relative error about 2^-100 for |x| < 10^9): x = (64k + j) ln2/64 + r,
/// |r| <= ln2/128, e^r by its Taylor series (the terms from r^3 on in doubles), 2^(j/64) from a table.
pub(crate) fn exp_dd(x: DD) -> Scaled {
    if x.hi.is_nan() || x.hi.abs() >= 1e9 {
        return dd::exp_scaled(x);
    }
    let n = (x.hi * (64.0 / LN2.hi)).round();
    let r = x.sub(LN2_64.mul_f(n));
    let rh = r.hi;
    let q = 0.5
        + rh * (1.0 / 6.0
            + rh * (1.0 / 24.0 + rh * (1.0 / 120.0 + rh * (1.0 / 720.0 + rh * (1.0 / 5040.0 + rh / 40320.0)))));
    let em1 = r.add(r.sqr().mul_f(q));
    let j = n.rem_euclid(64.0);
    let k = ((n - j) / 64.0) as i64;
    let tj = exp2_table()[j as usize];
    Scaled { m: tj.add(tj.mul(em1)), e: k }.norm()
}

/// ln x for a double-double x > 0: y + x e^-y - 1 for y the double logarithm.
pub(crate) fn log_dd(x: DD) -> DD {
    if x.hi > 0.0 && x.hi < 1e-290 {
        // scale the subnormals (and their neighbours) into the normal range first
        return log_dd(x.ldexp(600)).sub(LN2.mul_f(600.0));
    }
    let y = x.hi.ln();
    if !y.is_finite() {
        return DD::new(y);
    }
    let e = exp_dd(DD::new(-y));
    let t = x.mul(e.m).ldexp(e.e as i32);
    DD::new(y).add(t.add_f(-1.0))
}

/// A scaled double-double as a double-double (zero below the double range).
fn to_dd(s: Scaled) -> DD {
    s.m.ldexp(s.e.clamp(-2000, 2000) as i32)
}

/// erf(x) for |x| < 1/2.
#[inline]
fn erf_small(x: f64) -> DD {
    let t = DD::new(x).sqr();
    poly_dd(&tables().erf0, t).mul_f(x)
}

/// erfi(x) for |x| < 1/2.
#[inline]
fn erfi_small(x: f64) -> DD {
    let t = DD::new(x).sqr();
    poly_dd(&tables().erfi0, t).mul_f(x)
}

/// erfc(x) for x >= 6 (any x >= 3) by the continued fraction.
fn erfc_cf(x: f64) -> Scaled {
    let tb = tables();
    let x2 = DD::new(x).sqr();
    let b = 2.0 * x2.hi;
    let n = 5 + (144.0 / x2.hi).ceil() as usize;
    let mut t = b + (4 * n + 1) as f64;
    for k in (3..=n).rev() {
        let k = k as f64;
        t = (b + 4.0 * k - 3.0) - (2.0 * k - 1.0) * (2.0 * k) / t;
    }
    let b2 = x2.ldexp(1);
    let t1 = b2.add_f(5.0).sub(DD::new(12.0 / t));
    let f = b2.add_f(1.0).sub(DD::new(2.0).div(t1));
    exp_dd(x2.neg()).mul_dd(tb.two_sqrt_pi.mul_f(x).div(f))
}

/// erfc(x) for x >= 0 as a scaled double-double (relative error about 2^-60).
fn erfc_pos(x: f64) -> Scaled {
    if x < 0.5 {
        Scaled::new(DD::ONE.sub(erf_small(x)))
    } else if x < 6.0 {
        Scaled::new(centred(&tables().erfc_c, ERFC_FIRST, x))
    } else if x < 1e5 {
        erfc_cf(x)
    } else {
        Scaled::new(DD::ZERO)
    }
}

/// erf(x) as a double-double.
fn erf_dd(x: f64) -> DD {
    let a = x.abs();
    if a < 0.5 {
        return erf_small(x);
    }
    let v = DD::ONE.sub(to_dd(erfc_pos(a)));
    if x < 0.0 {
        v.neg()
    } else {
        v
    }
}

/// The error function erf(x) = 2/sqrt(pi) int_0^x e^(-t^2) dt.
pub fn erf(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x.abs() > 6.5 {
        // 1 - erfc(6.5) rounds to 1
        return 1f64.copysign(x);
    }
    erf_dd(x).to_f64()
}

/// The complementary error function erfc(x) = 1 - erf(x), with full relative accuracy in the tail (zero from
/// x = 27.23 on, gradual underflow before).
pub fn erfc(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x >= 0.0 {
        return erfc_pos(x).to_f64();
    }
    if x > -0.5 {
        return DD::ONE.sub(erf_small(x)).to_f64();
    }
    if x < -6.5 {
        return 2.0;
    }
    DD::new(2.0).sub(to_dd(erfc_pos(-x))).to_f64()
}

/// Dawson's function F(x) for x >= 8 by its asymptotic series (the first two terms in double-double).
fn dawson_asym(x: f64) -> DD {
    let x2 = DD::new(x).sqr();
    let inv = DD::ONE.div(x2.ldexp(1));
    let v = inv.hi;
    let mut term = v;
    let mut rest = 0.0;
    let mut k = 2.0;
    loop {
        term *= (2.0 * k - 1.0) * v;
        rest += term;
        if term < 1e-21 || k > 40.0 {
            break;
        }
        k += 1.0;
    }
    DD::ONE.add(inv).add_f(rest).div(DD::new(x).ldexp(1))
}

/// The imaginary error function erfi(x) = -i erf(ix) = 2/sqrt(pi) int_0^x e^(t^2) dt (infinite beyond x = 26.64).
pub fn erfi(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let a = x.abs();
    if a < 0.5 {
        return erfi_small(x).to_f64();
    }
    if a > 27.0 {
        return f64::INFINITY.copysign(x);
    }
    let t = tables();
    let f = if a < 8.0 { centred(&t.dawson_c, DAWSON_FIRST, a) } else { dawson_asym(a) };
    let v = exp_dd(DD::new(a).sqr()).mul_dd(t.two_sqrt_pi.mul(f)).to_f64();
    v.copysign(x)
}

/// inverse_erf(y) for 0 < y <= 1/2.
fn inv_erf_small(y: f64) -> f64 {
    let t = tables();
    if y < 1e-300 {
        return t.sqrt_pi_2.hi * y;
    }
    let c = t.sqrt_pi_2.hi * y;
    if y < 1e-9 {
        // the next term is c^3/3, below 2^-58 relative
        return t.sqrt_pi_2.mul_f(y).add_f(c * c * c / 3.0).to_f64();
    }
    let c2 = c * c;
    let mut x = c * (1.0 + c2 * (1.0 / 3.0 + c2 * (7.0 / 30.0 + c2 * (127.0 / 630.0 + c2 * (4369.0 / 22680.0)))));
    for _ in 0..6 {
        let f = erf_dd(x).sub(DD::new(y)).to_f64();
        let fp = t.two_sqrt_pi.hi * (-x * x).exp();
        let u = f / fp;
        // Halley: f''/f' = -2x
        let dx = u / (1.0 + x * u);
        x -= dx;
        if dx.abs() <= 1e-7 * x {
            break;
        }
    }
    x
}

/// inverse_erfc(q) for 0 < q < 1/2.
fn inv_erfc_tail(q: f64) -> f64 {
    let t = tables();
    let l = -q.ln();
    // the start: x^2 + ln(sqrt(pi)/2 (x + sqrt(x^2 + 4/pi))) = l (an overestimate; Newton from above, convex)
    let four_pi = 4.0 / std::f64::consts::PI;
    let mut x = l.sqrt();
    for _ in 0..20 {
        let s = (x * x + four_pi).sqrt();
        let g = x * x + (t.sqrt_pi_2.hi * (x + s)).ln() - l;
        let dx = g / (2.0 * x + 1.0 / s);
        x -= dx;
        if dx.abs() < 1e-4 * x {
            break;
        }
    }
    // -q scaled first (normalising a subnormal directly would need 2^1074)
    let mq = Scaled { m: DD::new(-q * dd::pow2(600)), e: -600 }.norm();
    let k = Scaled::new(t.two_sqrt_pi.neg());
    for _ in 0..12 {
        let f = erfc_pos(x).add(mq);
        let fp = k.mul(exp_dd(DD::new(x).sqr().neg()));
        let u = f.div(fp).to_f64();
        let den = 1.0 + x * u;
        let dx = if den > 0.5 { u / den } else { u };
        x -= dx;
        if dx.abs() <= 1e-7 {
            break;
        }
    }
    x
}

/// The inverse error function: erf(x) = y for -1 <= y <= 1 (±1 give ±inf, NaN outside).
pub fn inverse_erf(y: f64) -> f64 {
    if y.is_nan() || y.abs() > 1.0 {
        return f64::NAN;
    }
    if y.abs() == 1.0 {
        return f64::INFINITY.copysign(y);
    }
    if y == 0.0 {
        return y;
    }
    let a = y.abs();
    let x = if a <= 0.5 { inv_erf_small(a) } else { inv_erfc_tail(1.0 - a) };
    x.copysign(y)
}

/// inverse_erfc(q) for 0 < q <= 1.
fn inverse_erfc_le1(q: f64) -> f64 {
    if q == 1.0 {
        return 0.0;
    }
    if q >= 0.5 {
        inv_erf_small(1.0 - q)
    } else {
        inv_erfc_tail(q)
    }
}

/// The inverse complementary error function: erfc(x) = q for 0 <= q <= 2 (0 gives inf, 2 gives -inf, NaN outside),
/// with full relative accuracy for tiny q (down to the subnormals) and near q = 1.
pub fn inverse_erfc(q: f64) -> f64 {
    if q.is_nan() || !(0.0..=2.0).contains(&q) {
        return f64::NAN;
    }
    if q == 0.0 {
        return f64::INFINITY;
    }
    if q == 2.0 {
        return f64::NEG_INFINITY;
    }
    if q > 1.0 {
        return -inverse_erfc_le1(2.0 - q);
    }
    inverse_erfc_le1(q)
}

// ---------------------------------------------------------------- complex

/// A complex double-double.
#[derive(Clone, Copy, Debug)]
struct Cdd {
    re: DD,
    im: DD,
}

impl Cdd {
    fn new(re: DD, im: DD) -> Cdd {
        Cdd { re, im }
    }
    fn add(self, o: Cdd) -> Cdd {
        Cdd::new(self.re.add(o.re), self.im.add(o.im))
    }
    fn mul(self, o: Cdd) -> Cdd {
        Cdd::new(self.re.mul(o.re).sub(self.im.mul(o.im)), self.re.mul(o.im).add(self.im.mul(o.re)))
    }
    fn mul_f(self, v: f64) -> Cdd {
        Cdd::new(self.re.mul_f(v), self.im.mul_f(v))
    }
    fn mul_dd(self, v: DD) -> Cdd {
        Cdd::new(self.re.mul(v), self.im.mul(v))
    }
    fn div_f(self, v: f64) -> Cdd {
        Cdd::new(self.re.div_f(v), self.im.div_f(v))
    }
    /// 1/self (no scaling: the moduli here are between 2^-500 and 2^500).
    fn recip(self) -> Cdd {
        let d = self.re.sqr().add(self.im.sqr());
        Cdd::new(self.re.div(d), self.im.neg().div(d))
    }
    /// |self| as a double.
    fn abs(self) -> f64 {
        self.re.hi.hypot(self.im.hi)
    }
}

/// e^(-z^2) for z = x + iy as (mantissa, binary exponent): e^(y^2 - x^2) (cos 2xy - i sin 2xy), the squares exact
/// in double-double and the phase reduced by an exact multiple of pi.
fn exp_neg_sq(x: f64, y: f64) -> (Cdd, i64) {
    let m = exp_dd(DD::new(y).sqr().sub(DD::new(x).sqr()));
    let phase = DD::new(x).mul_f(y).ldexp(1).neg();
    let (sn, cs) = dd::sin_cos_pi_any(phase.div(PI));
    (Cdd::new(m.m.mul(cs), m.m.mul(sn)), m.e)
}

/// erf(z) by the Maclaurin series (for |z|^2 < 700; it cancels to about 2x^2 bits when y > x).
fn erf_maclaurin(z: Cdd, r2: f64) -> Cdd {
    let w = z.mul(z);
    let w = Cdd::new(w.re.neg(), w.im.neg());
    let mut t = z;
    let mut s = z;
    let mut k = 1.0;
    while k < 2000.0 {
        t = t.mul(w).div_f(k);
        let v = t.div_f(2.0 * k + 1.0);
        s = s.add(v);
        if k > r2 && v.abs() < 1e-33 * s.abs() {
            break;
        }
        k += 1.0;
    }
    s.mul_dd(tables().two_sqrt_pi)
}

/// erf(z) by Kummer's form 2z/sqrt(pi) e^(-z^2) sum (2z^2)^k/(1 3 ... (2k+1)) (for |z|^2 < 50; it cancels to about
/// 2y^2 bits when x > y).
fn erf_kummer(z: Cdd, x: f64, y: f64, r2: f64) -> Cdd {
    let w = z.mul(z).mul_f(2.0);
    let mut t = Cdd::new(DD::ONE, DD::ZERO);
    let mut s = t;
    let mut k = 1.0;
    while k < 2000.0 {
        t = t.mul(w).div_f(2.0 * k + 1.0);
        s = s.add(t);
        if k > 2.0 * r2 && t.abs() < 1e-33 * s.abs() {
            break;
        }
        k += 1.0;
    }
    let (e, ex) = exp_neg_sq(x, y);
    let e = Cdd::new(e.re.ldexp(ex as i32), e.im.ldexp(ex as i32));
    z.mul(e).mul(s).mul_dd(tables().two_sqrt_pi)
}

/// erfc(z) / e^(-z^2) for Re z > 0 by the asymptotic expansion 1/(z sqrt(pi)) sum (-1)^k (2k-1)!!/(2z^2)^k, summed
/// until a term is below `target` of the sum (`None` when the terms turn upward first). For |ph z| <= pi/4 the
/// remainder is at most the first omitted term; for pi/4 < |ph z| < pi/2 at most that times csc(2 |ph z|)
/// (DLMF 7.12(i)), which the caller's target accounts for.
fn erfc_asym_scaled(z: Cdd, target: f64) -> Option<Cdd> {
    let u = z.mul(z).mul_f(2.0).recip();
    let mut t = Cdd::new(DD::ONE, DD::ZERO);
    let mut s = t;
    let mut last = 1.0;
    for k in 1..4000 {
        t = t.mul(u).mul_f(-(2.0 * k as f64 - 1.0));
        let a = t.abs();
        if a < target * s.abs() {
            return Some(s.mul(z.mul_dd(PI.sqrt()).recip()));
        }
        if a > last {
            return None;
        }
        last = a;
        s = s.add(t);
    }
    None
}

/// erfc(z) / e^(-z^2) for Re z > 0 by Laplace's continued fraction 1/sqrt(pi) / (z + (1/2)/(z + 1/(z + (3/2)/(z +
/// ...)))) (modified Lentz; `None` without convergence in 5000 levels).
fn erfc_cf_scaled(z: Cdd) -> Option<Cdd> {
    let tiny = 1e-300;
    let mut f = z;
    let mut c = z;
    let mut d = Cdd::new(DD::ZERO, DD::ZERO);
    for k in 1..5000 {
        let a = 0.5 * k as f64;
        d = z.add(d.mul_f(a));
        if d.abs() < tiny {
            d = Cdd::new(DD::new(tiny), DD::ZERO);
        }
        d = d.recip();
        c = z.add(c.recip().mul_f(a));
        if c.abs() < tiny {
            c = Cdd::new(DD::new(tiny), DD::ZERO);
        }
        let del = c.mul(d);
        f = f.mul(del);
        if (del.re.hi - 1.0).abs() + del.im.hi.abs() < 1e-32 {
            return Some(f.mul_dd(PI.sqrt()).recip());
        }
    }
    None
}

/// erf(z) for x > 0, y > 0 from erfc(z) = e^(-z^2) g: 1 - e^(-z^2) g, the real part formed in double-double, the
/// imaginary part -Im(e^(-z^2) g) rounded directly (so a tiny imaginary part keeps its relative accuracy).
fn erf_from_erfc(x: f64, y: f64, g: Cdd) -> C64 {
    let (e, ex) = exp_neg_sq(x, y);
    let v = e.mul(g);
    let re_s = Scaled { m: v.re, e: ex }.norm();
    let im = -Scaled { m: v.im, e: ex }.norm().to_f64();
    let re = if re_s.m.hi == 0.0 || re_s.e < -1100 {
        1.0
    } else if re_s.e > 1100 {
        -re_s.to_f64()
    } else {
        DD::ONE.sub(to_dd(re_s)).to_f64()
    };
    C64::new(re, im)
}

/// erf(z) for x > 0, y > 0.
fn erf_quadrant(x: f64, y: f64) -> C64 {
    let z = Cdd::new(DD::new(x), DD::new(y));
    let r2 = x * x + y * y;
    if r2 >= 50.0 {
        // beyond ph z = pi/4 the remainder bound grows by csc(2 ph z) = r^2 / (2xy)
        let target = 1e-20 * if y > x { (2.0 * x * y / r2).min(1.0) } else { 1.0 };
        if let Some(g) = erfc_asym_scaled(z, target) {
            return erf_from_erfc(x, y, g);
        }
        if x * y < 1e-18 {
            // x tiny against the imaginary axis: erf(iy) + x erf'(iy) (the next term is x^2 y relative)
            let re = exp_dd(DD::new(y).sqr()).mul_dd(tables().two_sqrt_pi.mul_f(x)).to_f64();
            return C64::new(re, erfi(y));
        }
    }
    let m = x.min(y);
    if m * m <= 16.0 && (r2 < 50.0 || (x < y && r2 < 700.0)) {
        // (Kummer's form only below 50: beyond, the expansion always converges for x >= y)
        let w = if x < y { erf_maclaurin(z, r2) } else { erf_kummer(z, x, y, r2) };
        return C64::new(w.re.to_f64(), w.im.to_f64());
    }
    match erfc_cf_scaled(z) {
        Some(g) => erf_from_erfc(x, y, g),
        None => C64::new(f64::NAN, f64::NAN),
    }
}

/// The error function of a complex machine number, erf(-z) = -erf(z) and erf(conj z) = conj erf(z) reducing it to
/// the first quadrant: real and imaginary arguments by [`erf`] and [`erfi`]; |z|^2 >= 50 the asymptotic expansion of
/// erfc (the remainder bound grows by csc(2 ph z) near the imaginary axis, which the stopping rule includes);
/// min(|x|, |y|) <= 4 and |z|^2 < 50 (or |x| < |y|, |z|^2 < 700 where the expansion does not reach its target) the
/// Maclaurin series (y > x) or Kummer's form (x >= y), each cancelling to at most 2 min(x, y)^2 / ln 2 <= 47 of the
/// 106 bits; the rest (4 < x, y, |z|^2 < 50) Laplace's continued fraction.
/// Near the complex zeros of erf the error relative to |erf(z)| grows with the cancellation there.
pub fn erf_complex(z: C64) -> C64 {
    let (x, y) = (z.re, z.im);
    if x.is_nan() || y.is_nan() || y.is_infinite() {
        return C64::new(f64::NAN, f64::NAN);
    }
    if y == 0.0 {
        return C64::new(erf(x), y);
    }
    if x == 0.0 {
        return C64::new(x, erfi(y));
    }
    if x.is_infinite() {
        return C64::new(1f64.copysign(x), 0.0 * y);
    }
    // z = ±q or ±conj(q) for q = |x| + i|y|
    let w = erf_quadrant(x.abs(), y.abs());
    let w = if (x < 0.0) != (y < 0.0) { C64::new(w.re, -w.im) } else { w };
    if x < 0.0 {
        C64::new(-w.re, -w.im)
    } else {
        w
    }
}

#[cfg(test)]
#[allow(clippy::excessive_precision)] // published values to more digits than a double holds
mod tests {
    use super::*;

    fn ulps(got: f64, want: f64) -> f64 {
        ((got - want) / want).abs() / f64::EPSILON
    }

    #[test]
    fn published_values() {
        assert!(ulps(erf(1.0), 0.842_700_792_949_714_869_341_220_635_082_609_259_296) <= 1.0);
        assert!(ulps(erfc(10.0), 2.088_487_583_762_544_757_000_786_294_957_788_611_560_818_119_321e-45) <= 1.0);
        assert!(ulps(inverse_erf(0.5), 0.476_936_276_204_469_873_381_418_353_643_130_559_808_969_749_059_47) <= 1.0);
        assert_eq!(erf(0.0), 0.0);
        assert_eq!(erfc(0.0), 1.0);
        assert_eq!(erfc(-40.0), 2.0);
        assert_eq!(erfc(30.0), 0.0);
        assert_eq!(erfi(30.0), f64::INFINITY);
        assert_eq!(inverse_erf(1.0), f64::INFINITY);
        assert!(inverse_erf(1.5).is_nan());
        assert_eq!(inverse_erfc(1.0), 0.0);
    }
}
