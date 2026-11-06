//! Automatic ticks: about `n` positions at nice numbers (1, 2 or 5 times a power of ten) and their labels, with as
//! many decimals as the step needs, and in scientific form (`m × 10^k`) for very large or small values.
use crate::label::Label;

/// The nice step for about `n` ticks over [lo, hi].
pub fn step(lo: f64, hi: f64, n: usize) -> f64 {
    let raw = (hi - lo) / n.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let k = raw / mag;
    mag * if k < 1.5 {
        1.0
    } else if k < 3.5 {
        2.0
    } else if k < 7.5 {
        5.0
    } else {
        10.0
    }
}

/// About `n` tick positions in [lo, hi] at multiples of [`step`] (at most 50).
pub fn positions(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    if !(lo.is_finite() && hi.is_finite() && hi > lo) {
        return vec![];
    }
    let s = step(lo, hi, n);
    let first = (lo / s).ceil();
    let mut v = vec![];
    let mut k = 0.0;
    // multiples of the step rather than repeated sums: no drift
    while v.len() < 50 {
        let t = (first + k) * s;
        if t > hi + s * 1e-9 {
            break;
        }
        v.push(t);
        k += 1.0;
    }
    v
}

/// The label of tick `t` on an axis [lo, hi] with about `n` ticks: decimals as the step needs (no float noise, no
/// `-0`), a minus sign (U+2212); values of 10^6 and more, or all under 10^-4, as `m × 10^k`.
pub fn label(t: f64, lo: f64, hi: f64, n: usize) -> Label {
    let s = step(lo, hi, n);
    let top = lo.abs().max(hi.abs());
    if t.abs() < s * 1e-9 {
        return Label::plain("0");
    }
    if top >= 1e6 || top < 1e-4 {
        let k = top.log10().floor();
        let scale = 10f64.powf(k);
        let d = (-(s / scale).log10().floor()).max(0.0) as usize;
        let m = format!("{:.*}", d, t / scale).replace('-', "\u{2212}");
        return Label::scientific(&m, k as i64);
    }
    Label::plain(&decimal(t, s))
}

/// `t` with as many decimals as the step `s` needs.
pub fn decimal(t: f64, s: f64) -> String {
    let d = (-s.log10().floor()).max(0.0) as usize;
    let r = format!("{:.*}", d, t);
    if r.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') {
        format!("{:.*}", d, 0.0)
    } else {
        r.replace('-', "\u{2212}")
    }
}

/// Automatic ticks with labels.
pub fn auto(lo: f64, hi: f64, n: usize) -> Vec<(f64, Option<Label>)> {
    positions(lo, hi, n).into_iter().map(|t| (t, Some(label(t, lo, hi, n)))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_boxes::tree::notation;

    #[test]
    fn nice_positions_and_labels() {
        assert_eq!(positions(0.0, 1.0, 6), vec![0.0, 0.2, 0.4, 0.6000000000000001, 0.8, 1.0]);
        assert_eq!(decimal(0.30000000000000004, 0.1), "0.3");
        assert_eq!(decimal(-0.0000000001, 0.1), "0.0");
        assert_eq!(decimal(200.0, 100.0), "200");
        assert_eq!(decimal(-2.5, 0.5), "\u{2212}2.5");
        assert_eq!(notation(&label(2.0e6, 0.0, 5.0e6, 6).row), "2\u{d7}(10^6)");
        assert_eq!(notation(&label(0.0, -1.0, 1.0, 6).row), "0");
        assert!(positions(1.0, 1.0, 6).is_empty());
        assert!(positions(f64::NAN, 1.0, 6).is_empty());
    }
}
