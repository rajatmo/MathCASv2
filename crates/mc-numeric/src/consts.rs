//! The exact constants `pi`, `e`, `euler_gamma` and `catalan` (semantics 15): ordinary symbols whose numerical
//! values come from ball algorithms here.
//!
//! - pi: the Chudnovsky series by binary splitting (spike 3, benchmark N1), with the right spine's P skipped;
//! - e: sum 1/k! by binary splitting;
//! - catalan: G = pi/8 log(2 + sqrt 3) + 3/8 sum_k 1/((2k + 1)^2 binomial(2k, k)) (a series with ratio below 1/4);
//! - euler_gamma: Brent and McMillan's method, gamma = A/B - log n with A = sum (n^k/k!)^2 H_k and
//!   B = sum (n^k/k!)^2, error below pi e^(-4n), with the harmonic sums inside the binary splitting.
//!
//! Every value is an enclosure. The most precise value computed so far is cached per constant (decision L.3) and
//! rounded for requests at lower precision, so a session pays for pi at 10^6 digits once.
use crate::elementary;
use crate::series::{log2_factorial_lower, partial_sum, widen, Ratio};
use mc_arith::num::{stopped, Ball, Float, Int, Mag, Nat, StopScope};
use std::sync::Mutex;

/// A constant with a numerical value.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Const {
    /// pi = 3.14159...
    Pi,
    /// e = 2.71828..., the base of the natural logarithm.
    E,
    /// Euler's constant gamma = 0.57721...
    EulerGamma,
    /// Catalan's constant G = 0.91596...
    Catalan,
}

impl Const {
    /// Every constant.
    pub const ALL: [Const; 4] = [Const::Pi, Const::E, Const::EulerGamma, Const::Catalan];

    /// The symbol name.
    pub fn name(self) -> &'static str {
        match self {
            Const::Pi => "pi",
            Const::E => "e",
            Const::EulerGamma => "euler_gamma",
            Const::Catalan => "catalan",
        }
    }

    /// The constant named `name`.
    pub fn from_name(name: &str) -> Option<Const> {
        Const::ALL.into_iter().find(|c| c.name() == name)
    }

    /// The nearest machine real.
    pub fn machine(self) -> f64 {
        match self {
            Const::Pi => std::f64::consts::PI,
            Const::E => std::f64::consts::E,
            Const::EulerGamma => std::f64::consts::EULER_GAMMA,
            Const::Catalan => 0.915_965_594_177_219,
        }
    }

    /// A ball containing the constant with its midpoint at `p` bits and a radius of about 2^-p relative; `None` if
    /// the stop flag was raised during the computation.
    pub fn ball(self, p: u64) -> Option<Ball> {
        let slot = self as usize;
        if let Some((q, b)) = &CACHE.lock().unwrap_or_else(|e| e.into_inner())[slot] {
            if *q >= p {
                return Some(elementary::round(b.clone(), p));
            }
        }
        let w = p + 16;
        let b = match self {
            Const::Pi => pi(w),
            Const::E => e(w),
            Const::EulerGamma => euler_gamma(w)?,
            Const::Catalan => catalan(w)?,
        };
        if stopped() {
            return None;
        }
        let b = elementary::round(b, p);
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if cache[slot].as_ref().is_none_or(|(q, _)| *q < p) {
            cache[slot] = Some((p, b.clone()));
        }
        Some(b)
    }

    /// pi/2 as a ball at `p` bits.
    pub(crate) fn half_pi(p: u64) -> Option<Ball> {
        let pi = Const::Pi.ball(p)?;
        Some(Ball { mid: Float::new(pi.mid.neg, pi.mid.man, pi.mid.exp - 1), rad: pi.rad.mul_2exp(-1) })
    }
}

/// The most precise ball of each constant so far, with its precision.
static CACHE: Mutex<[Option<(u64, Ball)>; 4]> = Mutex::new([None, None, None, None]);

fn nat_mul_u64(x: &Int, v: u64) -> Int {
    Int::new(x.neg, x.mag.mul_u64(v))
}

// ---------------------------------------------------------------- pi

/// Chudnovsky: 1/pi = 12 sum_k (-1)^k (6k)! (13591409 + 545140134 k) / ((3k)! (k!)^3 640320^(3k + 3/2)), so
/// pi = 426880 sqrt(10005) Q / T with, over a range of terms, P = prod p(k), Q = prod q(k) and
/// T(a, b) = T(a, m) Q(m, b) + P(a, m) T(m, b).
fn chud_split(a: u64, b: u64, need_p: bool) -> (Int, Int, Int) {
    if b == a + 1 {
        if a == 0 {
            return (Int::from_i64(1), Int::from_i64(1), Int::from_i64(13_591_409));
        }
        // p(k) = -(6k-5)(2k-1)(6k-1); the first two factors fit a u64 for k < 10^8
        let p = nat_mul_u64(&Int::from_u64((6 * a - 5) * (2 * a - 1)), 6 * a - 1).neg();
        // q(k) = k^3 640320^3 / 24
        let q = nat_mul_u64(&nat_mul_u64(&Int::from_u64(a * a), a), 10_939_058_860_032_000);
        let t = nat_mul_u64(&p, 13_591_409 + 545_140_134 * a);
        return (p, q, t);
    }
    let m = (a + b) / 2;
    let (p1, q1, t1) = chud_split(a, m, true);
    if stopped() {
        return (p1, q1, t1);
    }
    let (p2, q2, t2) = chud_split(m, b, need_p);
    let t = t1.mul(&q2).add(&p1.mul(&t2));
    // the right spine never needs P: skipping it saves one large product per level
    let p = if need_p { p1.mul(&p2) } else { Int::default() };
    (p, q1.mul(&q2), t)
}

/// pi at `w` bits. The series is cut where its tail is below 2^-(bits + 8) relative (each term contributes
/// log2(151931373056000) = 47.11 bits); sqrt(10005) 2^bits is floored (error below one unit, scaled by
/// 426880 Q / T ~ 0.03) and so is the final division (below one unit): floor(pi 2^bits) within two units.
fn pi(w: u64) -> Ball {
    let bits = w + 8;
    let terms = (bits + 8) / 47 + 2;
    let (_, q, t) = chud_split(0, terms, false);
    if stopped() || t.is_zero() {
        return Ball::from_i64(0); // discarded by the caller
    }
    // the integer square root cannot be interrupted (see `elementary::sqrt`)
    let s = {
        let _uninterruptible = StopScope::enter(None);
        Nat::from_u64(10005).shl(2 * bits).isqrt()
    };
    let x = s.mul(&q.mag).mul_u64(426_880).div(&t.mag);
    let (mid, err) = Float::round(false, x, -(bits as i64), w);
    Ball { mid, rad: Mag::pow2(1 - bits as i64).add(&err) }
}

// ---------------------------------------------------------------- e

/// The ratio 1/k of sum 1/k!.
struct InvFactorial;

impl Ratio for InvFactorial {
    fn p(&self, _: u64) -> Int {
        Int::from_i64(1)
    }
    fn q(&self, k: u64) -> Nat {
        Nat::from_u64(k)
    }
}

/// e = sum_{k<N} 1/k! + tail, with tail < 2/N! <= 2^(1 - log2 N!).
fn e(w: u64) -> Ball {
    let mut n = 2;
    while log2_factorial_lower(n) < w + 4 {
        n += 1;
    }
    let tail = Mag::pow2(1 - log2_factorial_lower(n) as i64);
    widen(partial_sum(&InvFactorial, n, w), &tail)
}

// ---------------------------------------------------------------- catalan

/// The ratio k (2k - 1) / (2 (2k + 1)^2) of the terms 1/((2k + 1)^2 binomial(2k, k)).
struct CatalanRatio;

impl Ratio for CatalanRatio {
    fn p(&self, k: u64) -> Int {
        Int::from_u64(k * (2 * k - 1))
    }
    fn q(&self, k: u64) -> Nat {
        Nat::from_u64(2 * (2 * k + 1) * (2 * k + 1))
    }
}

/// G = pi/8 log(2 + sqrt 3) + 3/8 S. The ratio is below 1/4, so the terms are below 4^-k and the tail after N
/// terms below 4^-N 4/3 < 2^(1 - 2N).
fn catalan(w: u64) -> Option<Ball> {
    let n = (w + 6) / 2 + 1;
    let s = widen(partial_sum(&CatalanRatio, n, w), &Mag::pow2(1 - 2 * n as i64));
    let root3 = elementary::sqrt(&Ball::from_i64(3), w)?;
    let l = elementary::log(&root3.add(&Ball::from_i64(2), w), w)?;
    let pi = Const::Pi.ball(w)?;
    let a = pi.mul(&l, w).mul(&Ball::from_ratio(1, 8, w), w);
    Some(a.add(&s.mul(&Ball::from_ratio(3, 8, w), w), w))
}

// ---------------------------------------------------------------- euler_gamma

/// The integers of a range [a, b) of Brent and McMillan's sums, with term ratio p/q = n^2/k^2 and harmonic
/// increments c/d = 1/k: P, Q, D = prod d, C / D = sum c/d, T / Q = sum of the term products and
/// V / (Q D) = sum of the term products times the partial harmonic sums.
struct GammaSplit {
    p: Nat,
    q: Nat,
    d: Nat,
    c: Nat,
    t: Nat,
    v: Nat,
}

fn gamma_split(n2: &Nat, a: u64, b: u64, need_p: bool) -> GammaSplit {
    if b == a + 1 {
        let k = Nat::from_u64(a);
        return GammaSplit { p: n2.clone(), q: k.mul(&k), d: k, c: Nat::from_u64(1), t: n2.clone(), v: n2.clone() };
    }
    let m = (a + b) / 2;
    let l = gamma_split(n2, a, m, true);
    if stopped() {
        return l;
    }
    let r = gamma_split(n2, m, b, need_p);
    // V = V1 Q2 D2 + P1 (C1 D2 T2 + V2 D1), derived from the right range's terms being the left's last term
    // product times its own, with the left's whole harmonic sum added to its partial sums
    let v = l.v.mul(&r.q).mul(&r.d).add(&l.p.mul(&l.c.mul(&r.d).mul(&r.t).add(&r.v.mul(&l.d))));
    let t = l.t.mul(&r.q).add(&l.p.mul(&r.t));
    let c = l.c.mul(&r.d).add(&r.c.mul(&l.d));
    let p = if need_p { l.p.mul(&r.p) } else { Nat::zero() };
    GammaSplit { p, q: l.q.mul(&r.q), d: l.d.mul(&r.d), c, t, v }
}

/// gamma = A/B - log n = V / (D (Q + T)) - log n, with n chosen so that pi e^(-4n) < 2^-(w+2) and K terms so
/// that the truncated tails of A and B (both below 2 b_K (log K + 2) with b_K = (n^K/K!)^2, the ratio of
/// consecutive terms being below 1/12 from K > 3.59 n on) change A/B by less than 2^-(w+3).
fn euler_gamma(w: u64) -> Option<Ball> {
    let n = (w + 4).div_ceil(5) + 1;
    let ln2_n = (n as f64).log2();
    let mut k = (3.6 * n as f64) as u64 + 2;
    let mut log2_fact: f64 = (2..=k).map(|j| (j as f64).log2()).sum();
    loop {
        let log2_bk = 2.0 * (k as f64 * ln2_n - log2_fact);
        let factor = ((k as f64).ln() + (n as f64).ln() + 3.0).log2() + 2.0;
        // four bits of margin for the rounding of these estimates
        if log2_bk + factor + 4.0 <= -((w + 3) as f64) {
            break;
        }
        k += 1;
        log2_fact += (k as f64).log2();
    }
    let n2 = Nat::from_u64(n * n);
    let s = gamma_split(&n2, 1, k + 1, false);
    if stopped() {
        return None;
    }
    let num = Float::from_nat(s.v);
    let den = Float::from_nat(s.d.mul(&s.q.add(&s.t)));
    if stopped() || den.is_zero() {
        return None;
    }
    let (m, err) = num.div(&den, w);
    let ratio = Ball { mid: m, rad: err.add(&Mag::pow2(-(w as i64) - 1)) };
    let log_n = elementary::log(&Ball::from_i64(n as i64), w)?;
    Some(ratio.sub(&log_n, w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digits(c: Const, d: usize) -> String {
        let b = c.ball(mc_arith::text::digits_to_bits(d as u64 + 5)).unwrap();
        assert!(b.has_digits(d as u32 + 2), "{c:?}");
        b.mid.to_sci(d + 4).0[..d].to_string()
    }

    #[test]
    fn known_digits() {
        assert_eq!(digits(Const::Pi, 50), "31415926535897932384626433832795028841971693993751");
        assert_eq!(digits(Const::E, 50), "27182818284590452353602874713526624977572470936999");
        assert_eq!(digits(Const::EulerGamma, 50), "57721566490153286060651209008240243104215933593992");
        assert_eq!(digits(Const::Catalan, 50), "91596559417721901505460351493238411077414937428167");
    }

    #[test]
    fn machine_values_are_the_nearest() {
        for c in Const::ALL {
            let b = c.ball(200).unwrap();
            let v = mc_arith::num::float::dyadic_to_f64(b.mid.neg, &b.mid.man, b.mid.exp);
            assert_eq!(v, c.machine(), "{c:?}");
            assert_eq!(Const::from_name(c.name()), Some(c));
        }
    }
}
