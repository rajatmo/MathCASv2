//! Numbers in graphics terms: integers, rationals and machine reals read as `f64`.
use mc_api::{Store, View, T};

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

/// The value of an exact constant expression as a machine real: a number, `pi`, `e`, or sums, products and powers
/// of these (tick positions such as `pi/2` or `2pi`). `None` for anything else.
pub fn constant<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<f64> {
    if let Some(v) = number(h, t) {
        return Some(v);
    }
    let v = match h.view(t) {
        View::Sym(s) => match h.sym_name(s).as_str() {
            "pi" => std::f64::consts::PI,
            "e" => std::f64::consts::E,
            _ => return None,
        },
        View::App { head, args } => {
            let View::Sym(f) = h.view(head) else { return None };
            let vals: Option<Vec<f64>> = args.iter().map(|&a| constant(h, a)).collect();
            let vals = vals?;
            match (h.sym_name(f).as_str(), vals.as_slice()) {
                ("add", _) => vals.iter().sum(),
                ("mul", _) => vals.iter().product(),
                ("pow", &[a, b]) => a.powf(b),
                _ => return None,
            }
        }
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
    use mc_api::Store;

    #[test]
    fn big_integers_round() {
        assert_eq!(big(false, &[0, 1]), 18_446_744_073_709_551_616.0);
        assert_eq!(big(true, &[5]), -5.0);
        assert_eq!(short(-0.0), "0");
        assert_eq!(short(-2.5), "\u{2212}2.5");
        assert_eq!(short(3.0), "3");
    }

    #[test]
    fn exact_constants_have_values() {
        let h = mc_core::Heap::new(mc_core::Config::default());
        let pi = h.sym_term(h.sym("pi"));
        let half_pi = h.call(h.sym("mul"), &[h.rational(h.int(1), h.int(2)), pi]);
        let two_pi_sq = h.call(h.sym("pow"), &[h.call(h.sym("mul"), &[h.int(2), pi]), h.int(2)]);
        assert_eq!(constant(&h, half_pi), Some(std::f64::consts::FRAC_PI_2));
        assert!((constant(&h, two_pi_sq).unwrap() - 4.0 * std::f64::consts::PI * std::f64::consts::PI).abs() < 1e-12);
        assert_eq!(constant(&h, h.sym_term(h.sym("x"))), None);
    }
}
