//! Numbers in graphics terms: integers, rationals and machine reals read as `f64`.
use mc_core::{Store, View, T};

/// The value of a numeric literal as a finite machine real: an integer (big ones rounded), a rational or a machine
/// real. `None` for anything else, and for infinite or NaN values.
pub fn number<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<f64> {
    let v = match h.view(t) {
        View::Int(i) => i as f64,
        View::Real(x) => x,
        View::BigInt { neg, mag } => big(neg, mag),
        View::Rational { num, den } => integer(h, num)? / integer(h, den)?,
        _ => return None,
    };
    v.is_finite().then_some(v)
}

fn integer<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<f64> {
    match h.view(t) {
        View::Int(i) => Some(i as f64),
        View::BigInt { neg, mag } => Some(big(neg, mag)),
        _ => None,
    }
}

/// A big integer from its limbs (least significant first), rounded to the nearest machine real (or infinite).
fn big(neg: bool, mag: &[u64]) -> f64 {
    let m = mag.iter().rev().fold(0.0f64, |acc, &limb| acc * 18_446_744_073_709_551_616.0 + limb as f64);
    if neg {
        -m
    } else {
        m
    }
}

/// The shortest decimal form of `v` that reads back as `v`, with a minus sign (U+2212) and without a trailing
/// `.0`: tick labels given as plain numbers.
pub fn short(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    let s = format!("{v}");
    let s = s.strip_suffix(".0").map(str::to_string).unwrap_or(s);
    s.replace('-', "\u{2212}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_integers_round() {
        assert_eq!(big(false, &[0, 1]), 18_446_744_073_709_551_616.0);
        assert_eq!(big(true, &[5]), -5.0);
        assert_eq!(short(-0.0), "0");
        assert_eq!(short(-2.5), "\u{2212}2.5");
        assert_eq!(short(3.0), "3");
    }
}
