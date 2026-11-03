//! Decimal views of numbers for printing (04-syntax section 4). The printer of the concrete syntax lays them out;
//! this module only produces correct digits.
//!
//! - integers by spike 3's divide-and-conquer conversion, rationals as `num/den`;
//! - machine reals as the shortest decimal that reads back to the same value;
//! - balls with only their correct digits (decision B.11): the largest d such that every point of the ball rounds
//!   to the same d significant digits. A ball with a zero radius stands for its working precision, so its radius
//!   for this purpose is half a unit in the last place. A ball with no correct digit is shown in the input form
//!   `ball(m, r)`, with the radius rounded up.
use crate::num::{stopped, Ball, Int, Mag, Rat};
use crate::number::Real;
use std::f64::consts::LOG10_2;

/// Correct significant digits of a number: ±d1.d2d3... × 10^exp10.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Digits {
    /// The sign.
    pub neg: bool,
    /// At least one digit; the first is nonzero unless the value is zero.
    pub digits: String,
    /// The decimal exponent of the first digit.
    pub exp10: i64,
}

impl Digits {
    /// Positional notation when the decimal point falls within the digits or shortly before them
    /// (`3.1415926535897932385`, `0.00125`), scientific otherwise (`1.25e-40`, `1.2e5`). There is always a decimal
    /// point, so the text never reads as an exact integer.
    pub fn to_text(&self) -> String {
        let sign = if self.neg { "-" } else { "" };
        let (d, e) = (&self.digits, self.exp10);
        if (0..d.len() as i64).contains(&e) {
            let (int, frac) = d.split_at(e as usize + 1);
            format!("{sign}{int}.{frac}")
        } else if (-6..0).contains(&e) {
            format!("{sign}0.{}{d}", "0".repeat((-e - 1) as usize))
        } else {
            let (first, rest) = d.split_at(1);
            format!("{sign}{first}.{rest}e{e}")
        }
    }
}

/// An integer in decimal.
pub fn integer(x: &Int) -> String {
    let digits = x.mag.to_decimal();
    if x.neg {
        format!("-{digits}")
    } else {
        digits
    }
}

/// A rational as `num/den`, or an integer.
pub fn rational(q: &Rat) -> String {
    if q.is_int() {
        return integer(&q.num);
    }
    format!("{}/{}", integer(&q.num), q.den.to_decimal())
}

/// A machine real: the shortest decimal that reads back to the same value (`1.5`, `1e-300`, `-0.0`), and `inf`,
/// `-inf`, `nan`.
pub fn machine(v: f64) -> String {
    if v.is_nan() {
        "nan".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "inf" } else { "-inf" }.to_string()
    } else {
        format!("{v:?}")
    }
}

/// The correct digits of a ball at precision p, or `None` when not even the first digit is certain (for example
/// when the ball contains zero).
pub fn ball_digits(b: &Ball, p: u64) -> Option<Digits> {
    if b.mid.is_zero() {
        return b.rad.is_zero().then(|| Digits { neg: false, digits: "0".to_string(), exp10: 0 });
    }
    let r = if b.rad.is_zero() { Mag::pow2(b.mid.top() - p as i64 - 1) } else { b.rad };
    let m = b.mid.abs();
    let rf = r.to_float();
    let lo = m.sub_exact(&rf);
    if lo.is_zero() || lo.neg {
        return None;
    }
    let hi = m.add_exact(&rf);
    // m / r is about 10^est (r < 2^(r.e + 32)); a rounding boundary inside the ball costs at most a few steps down,
    // since the boundaries of different digit counts are distinct points
    let est = ((m.top() - (r.e + 32)) as f64 * LOG10_2).floor() as i64 + 1;
    let mut d = est.max(1) as usize + 1;
    loop {
        if stopped() {
            return None;
        }
        let (s1, e1) = lo.to_sci(d);
        let (s2, e2) = hi.to_sci(d);
        if s1 == s2 && e1 == e2 {
            return Some(Digits { neg: b.mid.neg, digits: s1, exp10: e1 });
        }
        if d == 1 {
            return None;
        }
        d -= 1;
    }
}

/// A decimal upper bound of a radius with three significant digits.
fn radius_up(r: &Mag) -> Digits {
    let (s, e) = r.to_float().to_sci(3);
    // to_sci rounds to nearest: one more unit in the last place is an upper bound
    let up = s.parse::<u32>().expect("three digits") + 1;
    if up == 1000 {
        return Digits { neg: false, digits: "100".to_string(), exp10: e + 1 };
    }
    Digits { neg: false, digits: up.to_string(), exp10: e }
}

/// A ball for printing: its correct digits, or the input form `ball(m, r)` when it has none.
pub fn ball(b: &Ball, p: u64) -> String {
    if let Some(d) = ball_digits(b, p) {
        return d.to_text();
    }
    let mid = if b.mid.is_zero() {
        "0".to_string()
    } else {
        let digits = ((p as f64 * LOG10_2).ceil() as usize).clamp(1, 20);
        let (s, e) = b.mid.abs().to_sci(digits);
        Digits { neg: b.mid.neg, digits: s.trim_end_matches('0').to_string(), exp10: e }.fixed_digits().to_text()
    };
    format!("ball({mid}, {})", radius_up(&b.rad).to_text())
}

impl Digits {
    /// At least one digit after trimming.
    fn fixed_digits(mut self) -> Digits {
        if self.digits.is_empty() {
            self.digits.push('0');
        }
        self
    }
}

/// A real number for printing.
pub fn real(x: &Real) -> String {
    match x {
        Real::Exact(q) => rational(q),
        Real::Machine(v) => machine(*v),
        Real::Ball(b, p) => ball(b, *p),
    }
}

/// The number of bits of working precision that holds `digits` decimal digits: ceil(digits log2(10)).
pub fn digits_to_bits(digits: u64) -> u64 {
    (digits as f64 / LOG10_2).ceil() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::Float;
    use crate::number::Kind;

    fn ball_of(n: i64, d: i64, p: u64) -> (Ball, u64) {
        match Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d))).to_kind(Kind::Ball, p) {
            Real::Ball(b, p) => (b, p),
            _ => unreachable!(),
        }
    }

    #[test]
    fn layouts() {
        let d = |s: &str, e| Digits { neg: false, digits: s.to_string(), exp10: e };
        assert_eq!(d("31415926535897932385", 0).to_text(), "3.1415926535897932385");
        assert_eq!(d("125", -3).to_text(), "0.00125");
        assert_eq!(d("125", -40).to_text(), "1.25e-40");
        assert_eq!(d("12", 5).to_text(), "1.2e5");
        assert_eq!(d("12", 1).to_text(), "12.");
        assert_eq!(d("1", 0).to_text(), "1.");
        assert_eq!(machine(1.5), "1.5");
        assert_eq!(machine(1e-300), "1e-300");
        assert_eq!(machine(f64::NEG_INFINITY), "-inf");
        assert_eq!(rational(&Rat::new(Int::from_i64(-3), Int::from_i64(6))), "-1/2");
    }

    #[test]
    fn balls_show_correct_digits() {
        let (third, p) = ball_of(1, 3, 200);
        let d = ball_digits(&third, p).unwrap();
        // 200 bits hold about 60 digits
        assert!((58..=61).contains(&d.digits.len()), "{d:?}");
        assert!(d.digits.chars().all(|c| c == '3') && d.exp10 == -1);
        // a ball accurate to about 20 digits: 3 (1/3) at 70 bits
        let (b, p) = ball_of(2, 3, 70);
        let wide = Ball { mid: b.mid.clone(), rad: Mag::pow2(b.mid.top() - 70) };
        let d = ball_digits(&wide, p).unwrap();
        assert!(d.digits.starts_with("666666") && d.digits.len() <= 22, "{d:?}");
        // a ball containing zero has no correct digit
        let z = Ball { mid: Float::from_i64(1), rad: Mag::pow2(1) };
        assert_eq!(ball_digits(&z, 64), None);
        assert!(ball(&z, 64).starts_with("ball(1."), "{}", ball(&z, 64));
        assert_eq!(digits_to_bits(20), 67);
    }
}
