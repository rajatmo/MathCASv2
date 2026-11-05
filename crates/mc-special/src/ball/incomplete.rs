//! The incomplete gamma and beta functions on balls (decision SF.4).
//!
//! - **gamma(a, x)**, a > 0: x^a e^-x / a 1F1(1; a+1; x) = x^a e^-x sum_k x^k / (a (a+1) ... (a+k)) (positive terms;
//!   [`pfq_sum`] bounds the tail).
//! - **Gamma(a, x)**, any real a, x > 0: the expansion Gamma(a, x) = x^(a-1) e^-x (sum_{k<n} u_k / x^k + R_n),
//!   u_k = (a-1)(a-2)...(a-k), whose remainder is exactly R_n = u_n Gamma(a-n, x) / (x^(a-1) e^-x) (integration by
//!   parts). With Gamma(c, x) <= x^(c-1) e^-x for c <= 1 and Gamma(c, x) <= x^(c-1) e^-x / (1 - (c-1)/x) for
//!   1 < c < x + 1 (bound t^(c-1) by x^(c-1) e^((c-1)(t-x)/x)), |R_n| is at most the first omitted term |u_n| / x^n
//!   times 1 or 1 / (1 - (a-n-1)/x) (for n >= a - 1 this is the enveloping bound of DLMF 8.11(ii)). The terms are
//!   summed until that bound falls below 2^-(wp+4) of the sum; when the terms grow first (x small against a or
//!   wp) the expansion declines and:
//! - a > 0: Gamma(a, x) = Gamma(a) - gamma(a, x), with -log2 Q(a, x) + 16 extra bits (Q estimated in machine
//!   arithmetic up front; [`refine`] doubles further when the estimate falls short);
//! - a <= 0, not an integer: Gamma(a) - x^a/a 1F1(a; a+1; -x) (the alternating series costs about 2.9 x bits, the
//!   poles of Gamma(a) and of the term k = -a about log2(1/dist(a, Z)) bits, both added up front); a = -n an exact
//!   integer: Gamma(-n, x) = (-1)^n/n! (E1(x) - e^-x sum_{k<n} (-1)^k k!/x^(k+1)) (DLMF 8.4.15) with
//!   E1(x) = -gamma - ln x + x 2F2(1, 1; 2, 2; -x).
//! - **P, Q**: gamma(a, x) / Gamma(a) and Gamma(a, x) / Gamma(a), each computed directly where it is the smaller
//!   one (Q by the expansion for x > a when it converges), the other as the complement.
//! - **B_x(a, b)**, a > 0: for b < 1 x^a/a 2F1(a, 1-b; a+1; x) (positive terms, any b); for b >= 1 the positive form
//!   x^a (1-x)^b / a 2F1(a+b, 1; a+1; x) (DLMF 8.17.8); for x > 1/2 and b > 0 B(a, b) - B_(1-x)(b, a) when that
//!   loses at most 64 bits (estimated from the machine value, added up front). **I_x(a, b)** = B_x(a, b) / B(a, b).
use super::gamma::{beta_ball, gamma_ball};
use super::hyper::pfq_sum;
use super::{f64_of, float_of, int, is_pos, log2_up, lower, mag_up, real, refine, upper, widen};
use mc_arith::num::{stopped, Ball, Float, Mag, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::elementary as el;

/// The largest number of terms of the expansion.
const MAX_TERMS: u64 = 1 << 20;

/// The midpoint of a real as a double.
fn approx(r: &Real) -> f64 {
    match r {
        Real::Exact(q) => crate::bernoulli::rat_to_f64(q),
        Real::Ball(b, _) => f64_of(&b.mid),
        Real::Machine(v) => *v,
    }
}

/// x^a e^-x for x > 0.
fn pow_exp(a: &Ball, x: &Ball, wp: u64) -> Option<Ball> {
    el::exp(&a.mul(&el::log(x, wp)?, wp).sub(x, wp), wp)
}

/// Is the ball an exact zero?
fn is_zero(b: &Ball) -> bool {
    b.mid.is_zero() && b.rad.is_zero()
}

/// gamma(a, x) for a > 0 and x >= 0 by the positive series.
fn lower_series(a: &Ball, x: &Ball, wp: u64) -> Option<Ball> {
    if is_zero(x) {
        return Some(int(0));
    }
    let s = pfq_sum(&[int(1)], &[a.add(&int(1), wp)], x, wp)?;
    pow_exp(a, x, wp)?.mul(&s, wp).div(a, wp)
}

/// An upper bound of a ball as a double (rounded up).
fn hi_f(b: &Ball) -> f64 {
    let v = f64_of(&upper(b));
    v + v.abs() * 1e-12 + 1e-300
}

/// A lower bound of a ball as a double (rounded down).
fn lo_f(b: &Ball) -> f64 {
    let v = f64_of(&lower(b));
    v - v.abs() * 1e-12 - 1e-300
}

/// Gamma(a, x) for x > 0 by the expansion with its rigorous remainder (`None` when it does not reach 2^-wp).
fn upper_expansion(a: &Ball, x: &Ball, wp: u64) -> Option<Ball> {
    if !is_pos(x) {
        return None;
    }
    let (a_hi, a_lo, x_lo) = (hi_f(a), lo_f(a), lo_f(x));
    // the terms decrease while |a - k| < x and grow after k = a + x
    if x_lo <= 0.0 {
        return None;
    }
    let inv = int(1).div(x, wp)?;
    let mut t = int(1);
    let mut s = int(1);
    let target = -(wp as f64) - 4.0;
    for k in 1..MAX_TERMS {
        if k % 64 == 0 && stopped() {
            return None;
        }
        // t = u_k / x^k
        t = t.mul(&a.sub(&int(k as i64), wp), wp).mul(&inv, wp);
        let kf = k as f64;
        let lt = log2_up(&t);
        if lt - log2_up(&s).max(-1e9) < target {
            // the remainder after k terms: |t| times 1, or 1 / (1 - (a-k-1)/x) for a - k - 1 > 0 and x > a - k - 1
            let c = a_hi - kf - 1.0;
            let factor = if c <= 0.0 {
                Some(1.0)
            } else if x_lo > c * (1.0 + 1e-12) {
                Some(1.0 / (1.0 - c / x_lo) * (1.0 + 1e-12))
            } else {
                None
            };
            if let Some(f) = factor {
                let bound = mag_up(&t).mul(&Mag::from_float_up(&float_of(f)));
                s = widen(s, &bound);
                break;
            }
        }
        if lt.is_infinite() && lt < 0.0 {
            // an exact zero term: a is an integer and the sum is finite
            break;
        }
        // past the smallest term the terms grow: decline
        if (kf - a_lo).abs() > x_lo * 1.01 && kf > a_hi + x_lo {
            return None;
        }
        if lt > log2_up(&s) + 200.0 {
            return None;
        }
        s = s.add(&t, wp);
    }
    let am1 = a.sub(&int(1), wp);
    Some(pow_exp(&am1, x, wp)?.mul(&s, wp))
}

/// -log2 of a machine estimate of a value in (0, 1], clamped to [0, 10^5] (0 when unknown).
fn bits_of(v: f64) -> u64 {
    if v.is_finite() && v > 0.0 {
        (-v.log2()).clamp(0.0, 1e5) as u64
    } else {
        0
    }
}

/// Gamma(a, x) for a provably positive ball a and x > 0.
fn upper_pos(a: &Ball, x: &Ball, wp: u64, q_est: f64) -> Option<Ball> {
    if let Some(v) = upper_expansion(a, x, wp) {
        return Some(v);
    }
    let w = wp + bits_of(q_est) + 16;
    let g = gamma_ball(a, w)?;
    Some(el::round(g.sub(&lower_series(a, x, w)?, w), wp))
}

/// Gamma(a, x) for a non-positive a (or a ball reaching it) and x > 0.
fn upper_nonpos(a: &Ball, a_int: Option<i64>, x: &Ball, wp: u64) -> Option<Ball> {
    if let Some(v) = upper_expansion(a, x, wp) {
        return Some(v);
    }
    let xf = hi_f(x).max(0.0);
    if let Some(n) = a_int {
        // (-1)^n / n! (E1(x) - e^-x sum_{k<n} (-1)^k k! / x^(k+1))
        let n = (-n) as u64;
        let w = wp + (2.9 * xf) as u64 + 2 * (n as f64 * (n as f64).max(2.0).log2()) as u64 + 32;
        let e1 = e1_ball(x, w)?;
        let inv = int(1).div(x, w)?;
        let mut term = inv.clone();
        let mut sum = int(0);
        for k in 0..n {
            if k > 0 {
                term = term.mul(&int(-(k as i64)), w).mul(&inv, w);
            }
            sum = sum.add(&term, w);
        }
        let ex = el::exp(&x.neg(), w)?;
        let mut fact = int(1);
        for k in 2..=n {
            fact = fact.mul(&int(k as i64), w);
        }
        let v = e1.sub(&ex.mul(&sum, w), w).div(&fact, w)?;
        let v = if n % 2 == 1 { v.neg() } else { v };
        return Some(el::round(v, wp));
    }
    // Gamma(a) - x^a/a 1F1(a; a+1; -x)
    let near = {
        let m = f64_of(&a.mid);
        (m - m.round()).abs().max(1e-300)
    };
    let w = wp + (2.9 * xf) as u64 + (-near.log2()).max(0.0) as u64 + 32;
    let g = gamma_ball(a, w)?;
    let s = pfq_sum(std::slice::from_ref(a), &[a.add(&int(1), w)], &x.neg(), w)?;
    let xa = el::exp(&a.mul(&el::log(x, w)?, w), w)?;
    let v = g.sub(&xa.mul(&s, w).div(a, w)?, w);
    Some(el::round(v, wp))
}

/// E1(x) = -gamma - ln x + x 2F2(1, 1; 2, 2; -x) for x > 0 (the caller adds the bits the cancellation costs).
fn e1_ball(x: &Ball, wp: u64) -> Option<Ball> {
    let f = pfq_sum(&[int(1), int(1)], &[int(2), int(2)], &x.neg(), wp)?;
    let g = super::euler_gamma(wp)?;
    Some(x.mul(&f, wp).sub(&g, wp).sub(&el::log(x, wp)?, wp))
}

/// A real as a ball and its value as an exact integer, if it is one.
fn int_value(r: &Real) -> Option<i64> {
    super::exact_int(r).and_then(|n| n.to_i64())
}

/// Guard bits for arguments of size about v.
fn guard(vals: &[f64]) -> u64 {
    24 + vals.iter().map(|v| v.abs().max(1.0).log2() as u64).max().unwrap_or(0)
}

/// The arguments (a, x) of P, Q, gamma: a > 0 and x >= 0 provably.
fn pos_args(a: &Real, x: &Real, wp: u64) -> Option<(Ball, Ball)> {
    let ab = super::real_ball(a, wp)?;
    let xb = super::real_ball(x, wp)?;
    if !is_pos(&ab) || !(is_zero(&xb) || is_pos(&xb)) {
        return None;
    }
    Some((ab, xb))
}

/// P(a, x) on balls.
fn p_ball(a: &Ball, x: &Ball, wp: u64, af: f64, xf: f64) -> Option<Ball> {
    if is_zero(x) {
        return Some(int(0));
    }
    if xf > af {
        if let Some(u) = upper_expansion(a, x, wp + 8) {
            let g = gamma_ball(a, wp + 8)?;
            return Some(int(1).sub(&u.div(&g, wp + 8)?, wp));
        }
    }
    lower_series(a, x, wp)?.div(&gamma_ball(a, wp)?, wp)
}

/// Q(a, x) on balls.
fn q_ball(a: &Ball, x: &Ball, wp: u64, af: f64, xf: f64) -> Option<Ball> {
    if is_zero(x) {
        return Some(int(1));
    }
    if xf > af {
        if let Some(u) = upper_expansion(a, x, wp) {
            return u.div(&gamma_ball(a, wp)?, wp);
        }
    }
    let w = wp + bits_of(crate::machine::incomplete::gamma_q(af, xf)) + 16;
    let p = lower_series(a, x, w)?.div(&gamma_ball(a, w)?, w)?;
    Some(el::round(int(1).sub(&p, w), wp))
}

/// The regularized lower incomplete gamma function P(a, x), a > 0, x >= 0.
pub fn gamma_p(a: &Real, x: &Real, p: u64) -> Option<CBall> {
    let (af, xf) = (approx(a), approx(x));
    refine(p, guard(&[af, xf]), |wp| {
        let (ab, xb) = pos_args(a, x, wp)?;
        Some(real(p_ball(&ab, &xb, wp, af, xf)?))
    })
}

/// The regularized upper incomplete gamma function Q(a, x), a > 0, x >= 0.
pub fn gamma_q(a: &Real, x: &Real, p: u64) -> Option<CBall> {
    let (af, xf) = (approx(a), approx(x));
    refine(p, guard(&[af, xf]), |wp| {
        let (ab, xb) = pos_args(a, x, wp)?;
        Some(real(q_ball(&ab, &xb, wp, af, xf)?))
    })
}

/// The lower incomplete gamma function gamma(a, x), a > 0, x >= 0.
pub fn gamma_lower(a: &Real, x: &Real, p: u64) -> Option<CBall> {
    let (af, xf) = (approx(a), approx(x));
    refine(p, guard(&[af, xf]), |wp| {
        let (ab, xb) = pos_args(a, x, wp)?;
        if xf > af + 1.0 {
            // Gamma(a) - Gamma(a, x): Q < 1/2, no cancellation to speak of
            if let Some(u) = upper_expansion(&ab, &xb, wp + 4) {
                return Some(real(gamma_ball(&ab, wp + 4)?.sub(&u, wp)));
            }
        }
        Some(real(lower_series(&ab, &xb, wp)?))
    })
}

/// The upper incomplete gamma function Gamma(a, x) for real a and x > 0 (and x = 0 for a > 0: Gamma(a)).
pub fn gamma_upper(a: &Real, x: &Real, p: u64) -> Option<CBall> {
    let (af, xf) = (approx(a), approx(x));
    let a_int = int_value(a).filter(|&n| n <= 0);
    refine(p, guard(&[af, xf]), |wp| {
        let ab = super::real_ball(a, wp)?;
        let xb = super::real_ball(x, wp)?;
        if is_zero(&xb) {
            return if is_pos(&ab) { Some(real(gamma_ball(&ab, wp)?)) } else { None };
        }
        if !is_pos(&xb) {
            return None;
        }
        if is_pos(&ab) {
            let q = crate::machine::incomplete::gamma_q(af, xf);
            return Some(real(upper_pos(&ab, &xb, wp, q)?));
        }
        Some(real(upper_nonpos(&ab, a_int, &xb, wp)?))
    })
}

// ---------------------------------------------------------------- beta

/// 1 - r exactly (a ball keeps its radius).
fn one_minus(r: &Real, wp: u64) -> Option<Ball> {
    match r {
        Real::Exact(q) => Some(super::rat_ball(&Rat::from_i64(1).sub(q), wp)),
        Real::Ball(b, _) => Some(Ball { mid: Float::from_i64(1).sub_exact(&b.mid), rad: b.rad }),
        Real::Machine(_) => None,
    }
}

/// B_x(a, b) for a > 0, 0 <= x < 1 by a positive series: x^a/a 2F1(a, 1-b; a+1; x) for b < 1, x^a (1-x)^b / a
/// 2F1(a+b, 1; a+1; x) for b >= 1 (y = 1 - x).
fn beta_series(x: &Ball, y: &Ball, a: &Ball, b: &Ball, wp: u64) -> Option<Ball> {
    if is_zero(x) {
        return Some(int(0));
    }
    let a1 = a.add(&int(1), wp);
    let xa = el::exp(&a.mul(&el::log(x, wp)?, wp), wp)?;
    if f64_of(&b.mid) < 1.0 {
        let f = pfq_sum(&[a.clone(), int(1).sub(b, wp)], &[a1], x, wp)?;
        return xa.mul(&f, wp).div(a, wp);
    }
    let f = pfq_sum(&[a.add(b, wp), int(1)], &[a1], x, wp)?;
    let yb = el::exp(&b.mul(&el::log(y, wp)?, wp), wp)?;
    xa.mul(&yb, wp).mul(&f, wp).div(a, wp)
}

/// B_x(a, b) for a > 0, b > 0, 0 < x < 1: the series at x, or B(a, b) - B_(1-x)(b, a) for x > 1/2 when I_x is not
/// tiny (`i_est` the machine I_x).
fn beta_inc_pos(x: &Ball, y: &Ball, a: &Ball, b: &Ball, wp: u64, i_est: f64) -> Option<Ball> {
    let xf = f64_of(&x.mid);
    let bits = bits_of(i_est);
    if xf > 0.5 && bits <= 64 {
        let w = wp + bits + 16;
        let c = beta_series(y, x, b, a, w)?;
        return Some(el::round(beta_ball(a, b, w)?.sub(&c, w), wp));
    }
    beta_series(x, y, a, b, wp)
}

/// The arguments x in [0, 1], y = 1 - x, a > 0 (and b > 0 when `b_pos`).
fn beta_args(x: &Real, a: &Real, b: &Real, wp: u64, b_pos: bool) -> Option<(Ball, Ball, Ball, Ball)> {
    let xb = super::real_ball(x, wp)?;
    let yb = one_minus(x, wp)?;
    let ab = super::real_ball(a, wp)?;
    let bb = super::real_ball(b, wp)?;
    if !is_pos(&ab) || (b_pos && !is_pos(&bb)) {
        return None;
    }
    if !(is_zero(&xb) || is_pos(&xb)) || !(is_zero(&yb) || is_pos(&yb)) {
        return None;
    }
    Some((xb, yb, ab, bb))
}

/// The regularized incomplete beta function I_x(a, b), 0 <= x <= 1, a, b > 0.
pub fn beta_reg(x: &Real, a: &Real, b: &Real, p: u64) -> Option<CBall> {
    let (xf, af, bf) = (approx(x), approx(a), approx(b));
    let i_est = crate::machine::incomplete::beta_reg(xf, af, bf);
    refine(p, guard(&[af, bf]), |wp| {
        let (xb, yb, ab, bb) = beta_args(x, a, b, wp, true)?;
        if is_zero(&yb) {
            return Some(real(int(1)));
        }
        if xf > 0.5 && bits_of(i_est) <= 64 {
            // 1 - I_(1-x)(b, a)
            let w = wp + bits_of(i_est) + 16;
            let c = beta_series(&yb, &xb, &bb, &ab, w)?.div(&beta_ball(&ab, &bb, w)?, w)?;
            return Some(real(el::round(int(1).sub(&c, w), wp)));
        }
        Some(real(beta_series(&xb, &yb, &ab, &bb, wp)?.div(&beta_ball(&ab, &bb, wp)?, wp)?))
    })
}

/// The incomplete beta function B_x(a, b), 0 <= x <= 1, a > 0 (any real b for x < 1; b > 0 at x = 1).
pub fn beta_inc(x: &Real, a: &Real, b: &Real, p: u64) -> Option<CBall> {
    let (xf, af, bf) = (approx(x), approx(a), approx(b));
    let i_est = crate::machine::incomplete::beta_reg(xf, af, bf);
    refine(p, guard(&[af, bf]), |wp| {
        let (xb, yb, ab, bb) = beta_args(x, a, b, wp, false)?;
        if is_zero(&yb) {
            return if is_pos(&bb) { Some(real(beta_ball(&ab, &bb, wp)?)) } else { None };
        }
        if !is_pos(&yb) {
            return None;
        }
        if is_pos(&bb) && f64_of(&lower(&bb)) >= 1.0 {
            return Some(real(beta_inc_pos(&xb, &yb, &ab, &bb, wp, i_est)?));
        }
        if is_pos(&bb) && xf > 0.5 && bits_of(i_est) <= 64 {
            return Some(real(beta_inc_pos(&xb, &yb, &ab, &bb, wp, i_est)?));
        }
        // b < 1 (any sign): the positive series x^a/a 2F1(a, 1-b; a+1; x)
        if f64_of(&upper(&bb)) >= 1.0 {
            return None;
        }
        Some(real(beta_series(&xb, &yb, &ab, &bb, wp)?))
    })
}
