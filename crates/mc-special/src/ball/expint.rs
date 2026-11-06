//! Exponential, logarithmic, sine and cosine integrals on balls, real arguments (decision SF.4).
//!
//! - **Ei(x), x != 0:** the power series Ei(x) = gamma + ln|x| + x 2F2(1, 1; 2, 2; x) ([`pfq_sum`], whose tail
//!   bound comes from the term ratio). For x < 0 (E1(-x) = -Ei(x)) the alternating terms reach e^|x| while the value
//!   is about e^-|x|: the working precision gets 2.886 |x| + 16 extra bits up front. The zero of Ei near 0.3725 is
//!   absorbed by [`refine`] (the precision doubles while the relative accuracy is short).
//! - **E1(y), large y > 0:** e^y y E1(y) = sum_{k<n} (-1)^k k!/y^k + R_n with |R_n| <= n!/y^n (the expansion of
//!   1/(1 + t/y) under int_0^inf e^-t dt / (1 + t/y) is enveloping), used when its smallest term reaches 2^-wp.
//! - **Ei(x), large x > 0:** e^-x Ei(x) = PV int_0^inf e^-t/(x - t) dt; expanding 1/(x - t) leaves
//!   x e^-x Ei(x) = sum_{k<n} k!/x^k + R_n (n >= 3) with R_n = x^-n PV int_0^inf h(t)/(x - t) dt, h(t) = e^-t t^n.
//!   Splitting at 2x, the principal part is int_0^2x (h(t) - h(x))/(x - t) dt (the odd part integrates to zero), at
//!   most 2x sup |h'| <= 4x n!/sqrt(2 pi (n-1)) (from sup e^-t t^j = (j/e)^j <= j!/sqrt(2 pi j)), and the rest is
//!   at most n!/x. So |R_n| <= (n!/x^n) (4 x^2/sqrt(2 pi (n-1)) + 1), with the least n <= x where that reaches
//!   2^-wp (x >= about 0.69 wp + 2 log2 x); otherwise the positive series, for x <= 10^5 (about x terms; larger
//!   arguments at precisions where the expansion is not yet accurate have no value).
//! - **E_nu(x), x > 0:** the asymptotic expansion E_nu(x) = (e^-x/x) (sum_{k<K} (-1)^k (nu)_k / x^k + R_K), whose
//!   remainder for real nu, x > 0 and K >= -nu is bounded by the first omitted term (DLMF 8.11(ii) with
//!   a = 1 - nu), when its smallest such term reaches 2^-wp. Otherwise, for nu not a positive integer,
//!   E_nu(x) = x^(nu-1) Gamma(1-nu) - sum_k (-x)^k/(k! (k+1-nu)) (DLMF 8.19.10), with extra bits for the
//!   cancellation (e^2x, and 1/|nu - round(nu)| near positive integers); for nu = n >= 1 the limit form
//!   E_n(x) = (-x)^(n-1)/(n-1)! (psi(n) - ln x) - sum_{k != n-1} (-x)^k/((k-n+1) k!) (DLMF 8.19.8). The sums stop
//!   at k + 1 >= 2x once 2 |(-x)^k/k!| / d is below 2^-wp of the partial sum, d a lower bound of |j + 1 - nu| over
//!   the remaining j (the ratios of (-x)^k/k! are at most 1/2 from there), so their cost does not grow with nu; for nu = -j <= 0 the finite sum E_-j(x) = (e^-x/x) sum_{l<=j} j!/((j-l)! x^l), real for every
//!   x != 0. E_nu(0) = 1/(nu-1) for nu > 1. Ball arguments with nonzero radii: E_nu(x) decreases in nu and in
//!   x > 0, so its range is the hull of the values at two exact corners (as decision SF.8).
//! - **li(x) = Ei(ln x)**, x >= 0, ln x = log1p(x - 1) near 1 (x - 1 exact for rationals), li(0) = 0.
//! - **Si, Ci:** the series Si(x) = x 1F2(1/2; 3/2, 3/2; -x^2/4), Ci(x) = gamma + ln x - (x^2/4)
//!   2F3(1, 1; 2, 2, 3/2; -x^2/4) with 1.443 x + 16 extra bits (the terms reach e^x); for large x
//!   Si = pi/2 - f cos x - g sin x, Ci = f sin x - g cos x with f ~ (1/x) sum (-1)^k (2k)!/x^2k and
//!   g ~ (1/x^2) sum (-1)^k (2k+1)!/x^2k, whose remainders for x > 0 are bounded by the first omitted terms
//!   (DLMF 6.12(ii)), when their smallest terms reach 2^-wp. Si is odd; Ci(x < 0) is complex (no value).
//! - **Shi, Chi:** the series with x^2/4 (positive terms), or (Ei(x) +- E1(x))/2 where both asymptotic expansions
//!   apply. Shi is odd; Chi(x < 0) is complex.
//!
//! Every long loop polls the stop flag (through [`pfq_sum`] or directly).
use super::gamma::gamma_ball;
use super::hyper::pfq_sum;
use super::{euler_gamma, f64_of, hull, int, is_pos, log2_up, lower, mag_up, pi, real, refine, upper, widen};
use mc_arith::num::{stopped, Ball, Float, Mag, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::elementary as el;

/// The largest argument of the positive Ei series and of the alternating series (about |x| terms each).
const MAX_SERIES_ARG: f64 = 1e5;
/// The largest number of terms of the finite and asymptotic sums.
const MAX_TERMS: u64 = 1 << 22;
/// log2(e).
const LOG2_E: f64 = std::f64::consts::LOG2_E;

/// An upper bound of |x| over the ball, as a double.
fn up_f(b: &Ball) -> f64 {
    f64_of(&mag_up(b).to_float()) * (1.0 + 1e-12)
}

/// The lower end of a ball as a double (rounded to nearest: only for estimates and comparisons with margins).
fn lo_f(b: &Ball) -> f64 {
    f64_of(&lower(b))
}

/// A double approximation of an exact or ball real (`None` for machine reals).
fn approx(r: &Real) -> Option<f64> {
    match r {
        Real::Exact(q) => Some(crate::bernoulli::rat_to_f64(q)),
        Real::Ball(b, _) => Some(f64_of(&b.mid)),
        Real::Machine(_) => None,
    }
}

fn is_wide(r: &Real) -> bool {
    matches!(r, Real::Ball(b, _) if !b.rad.is_zero())
}

fn half(wp: u64) -> Ball {
    Ball::from_ratio(1, 2, wp)
}

// ---------------------------------------------------------------- Ei and E1

/// gamma + ln|x| + x 2F2(1, 1; 2, 2; x): Ei(x) for a ball x without zero.
fn ei_series(x: &Ball, wp: u64) -> Option<Ball> {
    let ax = if x.mid.neg { x.neg() } else { x.clone() };
    let l = el::log(&ax, wp)?;
    let s = pfq_sum(&[int(1), int(1)], &[int(2), int(2)], x, wp)?.mul(x, wp);
    Some(euler_gamma(wp)?.add(&l, wp).add(&s, wp))
}

/// log2 of the smallest term of the E1 expansion at y (about sqrt(2 pi y) e^-y).
fn e1_asym_log2(y: f64) -> f64 {
    0.5 * ((2.0 * std::f64::consts::PI).log2() + y.log2()) - y * LOG2_E
}

/// Does the E1 expansion reach 2^-wp at y?
fn e1_asym_ok(y: f64, wp: u64) -> bool {
    y > 4.0 && e1_asym_log2(y) < -(wp as f64) - 8.0
}

/// E1(y) by the enveloping asymptotic expansion, if its terms reach 2^-wp (y > 0).
fn e1_asymptotic(y: &Ball, wp: u64) -> Option<Ball> {
    let ylo = lo_f(y);
    if !e1_asym_ok(ylo, wp) {
        return None;
    }
    let r = int(1).div(y, wp)?;
    let (mut t, mut s) = (int(1), int(0));
    let mut k = 0i64;
    loop {
        if k % 32 == 0 && stopped() {
            return None;
        }
        if log2_up(&t) < -(wp as f64) - 4.0 {
            break;
        }
        if k as f64 > ylo + 2.0 {
            return None;
        }
        s = s.add(&t, wp);
        k += 1;
        t = t.mul(&r, wp).mul(&int(-k), wp);
    }
    let s = widen(s, &mag_up(&t));
    el::exp(&y.neg(), wp)?.mul(&s, wp).div(y, wp)
}

/// log2 of an upper bound of 4 x^2/sqrt(2 pi (n-1)) + 1 (no overflow for huge x).
fn ei_rem_factor_log2(x: f64, n: f64) -> f64 {
    (2.0 + 2.0 * x.log2() - 0.5 * (2.0 * std::f64::consts::PI * (n - 1.0) * 0.998).log2()).max(0.0) + 1.0
}

/// The number of terms n (3 <= n <= x) of the Ei expansion at x whose remainder bound (relative to e^x/x) is
/// below 2^-(wp+8), if there is one: log2 of (n!/x^n) (4 x^2/sqrt(2 pi (n-1)) + 1) along n.
fn ei_asym_terms(x: f64, wp: u64) -> Option<u64> {
    if x.is_nan() || x < 8.0 {
        return None;
    }
    let lx = x.log2();
    // log2(n!/x^n), from n = 1
    let mut l = -lx;
    let mut n = 1.0f64;
    while n < x.floor() {
        n += 1.0;
        l += n.log2() - lx;
        if n >= 3.0 && l + ei_rem_factor_log2(x, n) < -(wp as f64) - 8.0 {
            return Some(n as u64);
        }
        if n > 4.0 * wp as f64 + 64.0 {
            return None;
        }
    }
    None
}

/// Does the Ei expansion reach 2^-wp at x?
fn ei_asym_ok(x: f64, wp: u64) -> bool {
    ei_asym_terms(x, wp).is_some()
}

/// Ei(x) for x > 0 by the asymptotic expansion with the remainder bound of the module docs.
fn ei_asymptotic(x: &Ball, wp: u64) -> Option<Ball> {
    let (xlo, xhi) = (lo_f(x), up_f(x));
    let n = ei_asym_terms(xlo, wp)?;
    let r = int(1).div(x, wp)?;
    let (mut t, mut s) = (int(1), int(0));
    for k in 0..n {
        if k % 32 == 0 && stopped() {
            return None;
        }
        s = s.add(&t, wp);
        t = t.mul(&r, wp).mul(&int(k as i64 + 1), wp);
    }
    // t = n!/x^n now (on the ball); the factor 4 x^2/sqrt(2 pi (n-1)) + 1 is bounded by a power of two
    let c = Mag::pow2(ei_rem_factor_log2(xhi, n as f64).ceil() as i64);
    let s = widen(s, &mag_up(&t).mul(&c));
    el::exp(x, wp)?.mul(&s, wp).div(x, wp)
}

/// E1(y) for a ball y > 0 (the caller adds the series' cancellation bits to wp; see [`e1_extra`]).
fn e1_ball(y: &Ball, wp: u64) -> Option<Ball> {
    if !is_pos(y) {
        return None;
    }
    if let Some(v) = e1_asymptotic(y, wp) {
        return Some(v);
    }
    if up_f(y) > MAX_SERIES_ARG {
        return None;
    }
    Some(ei_series(&y.neg(), wp)?.neg())
}

/// The extra bits E1(y) needs at target precision wp (none where the expansion applies).
fn e1_extra(y: f64, wp: u64) -> u64 {
    if e1_asym_ok(y, wp) {
        0
    } else {
        (2.886 * y.abs()).ceil().min(1e7) as u64 + 16
    }
}

/// Ei(x) for a ball x without zero (wp including [`ei_extra`]).
pub(crate) fn ei_point(x: &Ball, wp: u64) -> Option<Ball> {
    if x.contains_zero() {
        return None;
    }
    if x.mid.neg {
        return Some(e1_ball(&x.neg(), wp)?.neg());
    }
    if let Some(v) = ei_asymptotic(x, wp) {
        return Some(v);
    }
    if up_f(x) > MAX_SERIES_ARG {
        return None;
    }
    ei_series(x, wp)
}

/// The extra bits of Ei at x.
fn ei_extra(x: f64, wp: u64) -> u64 {
    if x < 0.0 {
        e1_extra(-x, wp)
    } else {
        0
    }
}

/// Ei(x) for real x != 0 (Ei(x) = -E1(-x) for x < 0); `None` at 0 (-inf) and for x > 10^5 where the asymptotic
/// expansion does not reach the precision.
pub fn exp_integral_ei(x: &Real, p: u64) -> Option<CBall> {
    let xa = approx(x)?;
    refine(p, 24, |wp| {
        let w = wp + ei_extra(xa, wp);
        Some(real(ei_point(&super::real_ball(x, w)?, w)?))
    })
}

// ---------------------------------------------------------------- E_nu

/// E_-j(x) = (e^-x/x) sum_{l<=j} j!/((j-l)! x^l), for any x != 0.
fn en_negint(j: u64, x: &Ball, wp: u64) -> Option<Ball> {
    if x.contains_zero() || j > MAX_TERMS {
        return None;
    }
    let r = int(1).div(x, wp)?;
    let (mut t, mut s) = (int(1), int(1));
    for l in 1..=j {
        if l % 32 == 0 && stopped() {
            return None;
        }
        t = t.mul(&r, wp).mul(&int((j - l + 1) as i64), wp);
        s = s.add(&t, wp);
    }
    Some(el::exp(&x.neg(), wp)?.mul(&s, wp).mul(&r, wp))
}

/// The smallest term index K >= -nu for every nu in the ball.
fn asym_kmin(nu: &Ball) -> f64 {
    (-lo_f(nu)).ceil().max(0.0) + 1.0
}

/// E_nu(x) by the asymptotic expansion, if its terms reach 2^-wp after K >= -nu (x > 0).
fn en_asymptotic(nu: &Ball, x: &Ball, wp: u64) -> Option<Ball> {
    let xlo = lo_f(x);
    if xlo.is_nan() || xlo <= 2.0 {
        return None;
    }
    let kmin = asym_kmin(nu);
    if kmin.is_nan() || kmin >= MAX_TERMS as f64 {
        return None;
    }
    let limit = 4.0 * xlo + kmin + up_f(nu) + 64.0;
    let r = int(1).div(x, wp)?;
    let (mut t, mut s) = (int(1), int(0));
    let mut prev = f64::INFINITY;
    let mut k = 0u64;
    loop {
        if k.is_multiple_of(32) && stopped() {
            return None;
        }
        let lt = log2_up(&t);
        if k as f64 >= kmin && lt < -(wp as f64) - 4.0 {
            break;
        }
        if (k as f64 > kmin && lt > prev) || k as f64 > limit {
            return None;
        }
        prev = lt;
        s = s.add(&t, wp);
        t = t.mul(&nu.add(&int(k as i64), wp), wp).mul(&r, wp).neg();
        k += 1;
    }
    let s = widen(s, &mag_up(&t));
    el::exp(&x.neg(), wp)?.mul(&s, wp).div(x, wp)
}

/// Would the E_nu expansion apply at (nu, x)? (An estimate: the sum itself decides.)
fn en_asym_likely(nu: f64, x: f64, wp: u64) -> bool {
    // beyond MAX_TERMS terms the expansion is not summed; and at |nu| ~ 2^53 the counter `k += 1.0` below stopped
    // advancing, which hung the session (it does not poll the stop flag)
    if x.is_nan() || x <= 2.0 || nu.is_nan() || nu.abs() >= MAX_TERMS as f64 {
        return false;
    }
    // log2 |(nu)_k / x^k| along k, from k = 0 to the first k >= -nu where it falls below -wp
    let kmin = (-nu).ceil().max(0.0) + 1.0;
    let mut l = 0.0f64;
    let mut prev = f64::INFINITY;
    let mut k = 0.0;
    while k < 4.0 * x + kmin + nu.abs() + 64.0 {
        if k >= kmin && l < -(wp as f64) - 8.0 {
            return true;
        }
        if k > kmin && l > prev {
            return false;
        }
        prev = l;
        l += ((nu + k).abs().max(1e-300) / x).log2();
        k += 1.0;
    }
    false
}

/// sum_{k >= 0, k != skip} (-x)^k / (k! (k + a)) for x > 0. With u_k = (-x)^k/k!, once k + 1 >= 2x the ratios
/// |u_(k+1)/u_k| are at most 1/2, so the tail from k is at most 2 |u_k| / d with d a lower bound of |j + a| over
/// j >= k, j != skip: k + a when that is positive, else `dmin` (1 for integer a with the singular term skipped, the
/// distance of a to the integers otherwise).
fn alt_sum(a: &Ball, x: &Ball, skip: Option<u64>, dmin: &Mag, wp: u64) -> Option<Ball> {
    let xhi = up_f(x);
    let mx = x.neg();
    let (mut u, mut s) = (int(1), int(0));
    for k in 0..MAX_TERMS {
        if k % 32 == 0 && stopped() {
            return None;
        }
        if (k + 1) as f64 >= 2.0 * xhi {
            let ka = a.add(&int(k as i64), wp);
            let d = if is_pos(&ka) { Mag::from_float_down(&ka.mid).sub_down(&ka.rad) } else { *dmin };
            if !d.is_zero() {
                let tail = mag_up(&u).mul_2exp(1).div_up(&d);
                if tail.is_zero() || tail.log2() < log2_up(&s) - wp as f64 - 4.0 {
                    return Some(widen(s, &tail));
                }
            }
        }
        if Some(k) != skip {
            s = s.add(&u.div(&a.add(&int(k as i64), wp), wp)?, wp);
        }
        u = u.mul(&mx, wp).div(&int(k as i64 + 1), wp)?;
    }
    None
}

/// E_n(x) for an integer n >= 1 by the limit form of the series (x > 0):
/// (-x)^m/m! (psi(n) - ln x) - sum_{k != m} (-x)^k/((k - m) k!), m = n - 1.
fn en_int_series(n: u64, x: &Ball, wp: u64) -> Option<Ball> {
    let m = n - 1;
    let um = el::pow_int(x, &mc_arith::num::Int::from_u64(m), wp)?.div(&gamma_ball(&int(n as i64), wp)?, wp)?;
    let um = if m % 2 == 1 { um.neg() } else { um };
    let psi = super::gamma::digamma_ball(&int(n as i64), wp)?;
    let lead = um.mul(&psi.sub(&el::log(x, wp)?, wp), wp);
    let a = int(1 - n as i64);
    Some(lead.sub(&alt_sum(&a, x, Some(m), &Mag::pow2(0), wp)?, wp))
}

/// E_nu(x) = x^(nu-1) Gamma(1-nu) - sum_k (-x)^k/(k! (k + 1 - nu)) for nu not a positive integer (x > 0).
fn en_frac_series(nu: &Ball, x: &Ball, wp: u64) -> Option<Ball> {
    let one = int(1);
    let a = one.sub(nu, wp);
    let g = gamma_ball(&a, wp)?;
    let pw = el::exp(&nu.sub(&one, wp).mul(&el::log(x, wp)?, wp), wp)?;
    // the distance of nu (and of a) to the integers, a lower bound
    let (_, f) = super::split_int(nu, wp)?;
    let dmin = Mag::from_float_down(&f.mid).sub_down(&f.rad);
    Some(pw.mul(&g, wp).sub(&alt_sum(&a, x, None, &dmin, wp)?, wp))
}

/// E_nu(x) at a narrow point (`n` the exact integer value of nu, if it has one).
fn en_point(nu: &Ball, n: Option<i64>, x: &Ball, wp: u64) -> Option<Ball> {
    if let Some(n) = n {
        if n <= 0 {
            return en_negint(n.unsigned_abs(), x, wp);
        }
    }
    if !is_pos(x) {
        return None;
    }
    if let Some(v) = en_asymptotic(nu, x, wp) {
        return Some(v);
    }
    if up_f(x) > MAX_SERIES_ARG {
        return None;
    }
    match n {
        Some(n) => en_int_series(n as u64, x, wp),
        None => en_frac_series(nu, x, wp),
    }
}

/// The extra bits of E_nu(x): the series' cancellation, and 1/|nu - round(nu)| near positive integers.
fn en_extra(nu: f64, x: f64, wp: u64) -> u64 {
    if x <= 0.0 || en_asym_likely(nu, x, wp) {
        return 0;
    }
    let mut e = 4.33 * x + (x + nu.abs() + 2.0).log2() + 16.0;
    if nu >= 0.5 {
        let d = (nu - nu.round()).abs();
        if d > 0.0 {
            e += (-d.log2()).max(0.0);
        }
    }
    e.ceil().min(1e7) as u64
}

/// The integer value of an exact real (or exact ball).
fn int_of(r: &Real) -> Option<i64> {
    super::exact_int(r)?.to_i64()
}

/// E_nu(x) at an exact point.
fn en_exact(nu: &Real, x: &Real, p: u64) -> Option<Ball> {
    let (na, xa) = (approx(nu)?, approx(x)?);
    let n = int_of(nu);
    if xa == 0.0 {
        // E_nu(0) = 1/(nu - 1) for nu > 1, infinite otherwise
        let r = refine(p, 24, |wp| {
            let v = super::real_ball(nu, wp)?.sub(&int(1), wp);
            is_pos(&v).then(|| int(1).div(&v, wp)).flatten().map(real)
        })?;
        return Some(r.re);
    }
    let r = refine(p, 24, |wp| {
        let w = wp + en_extra(na, xa, wp);
        Some(real(en_point(&super::real_ball(nu, w)?, n, &super::real_ball(x, w)?, w)?))
    })?;
    Some(r.re)
}

/// E_nu(x) = int_1^inf e^(-x t) t^-nu dt for real nu and x > 0 (x = 0: 1/(nu - 1) for nu > 1; x < 0 only for
/// integers nu <= 0, where it is real). Ball arguments with radii: the hull of two exact corners (E_nu(x)
/// decreases in nu and in x > 0).
pub fn exp_integral_e(nu: &Real, x: &Real, p: u64) -> Option<CBall> {
    if !is_wide(nu) && !is_wide(x) {
        return Some(real(en_exact(nu, x, p)?));
    }
    let nb = super::real_ball(nu, p)?;
    let xb = super::real_ball(x, p)?;
    let (x_lo, x_hi) = (lower(&xb), upper(&xb));
    if x_lo.neg || x_lo.is_zero() {
        return None;
    }
    let pt = |f: Float| Real::Ball(Ball::exact(f), p);
    let lo = en_exact(&pt(upper(&nb)), &pt(x_hi), p)?;
    let hi = en_exact(&pt(lower(&nb)), &pt(x_lo), p)?;
    Some(real(hull(&lower(&lo), &upper(&hi), p)))
}

// ---------------------------------------------------------------- li

/// li(x) = Ei(ln x) for x > 0, x != 1, with ln x = log1p(x - 1) near 1 (wp including the extra bits).
fn li_point(x: &Real, wp: u64) -> Option<Ball> {
    let xa = approx(x)?;
    let t = if (0.5..1.5).contains(&xa) {
        let d = match x {
            Real::Exact(q) => super::rat_ball(&q.sub(&Rat::from_i64(1)), wp),
            _ => super::real_ball(x, wp)?.sub(&int(1), wp),
        };
        el::log1p(&d, wp)?
    } else {
        el::log(&super::real_ball(x, wp)?, wp)?
    };
    ei_point(&t, wp)
}

/// li(x) = Ei(ln x) for x >= 0 (li(0) = 0; `None` at the pole x = 1 and for x < 0, where it is complex).
pub fn log_integral(x: &Real, p: u64) -> Option<CBall> {
    let xa = approx(x)?;
    if let Real::Exact(q) = x {
        if q.is_zero() {
            return Some(real(int(0)));
        }
    }
    if xa <= 0.0 && !is_wide(x) {
        return None;
    }
    refine(p, 24, |wp| {
        let w = wp + ei_extra(xa.max(1e-300).ln(), wp);
        Some(real(li_point(x, w)?))
    })
}

// ---------------------------------------------------------------- Si, Ci

/// Do the f, g expansions reach 2^-wp at x (smallest terms about sqrt(2 pi x) e^-x)?
fn fg_asym_ok(x: f64, wp: u64) -> bool {
    x > 4.0 && e1_asym_log2(x) < -(wp as f64) - 8.0
}

/// The auxiliary functions f(x), g(x) by their asymptotic expansions (x > 0), if they reach 2^-wp.
fn fg_asymptotic(x: &Ball, wp: u64) -> Option<(Ball, Ball)> {
    let xlo = lo_f(x);
    if !fg_asym_ok(xlo, wp) {
        return None;
    }
    let r2 = int(1).div(&x.mul(x, wp), wp)?;
    let (mut a, mut b) = (int(1), int(1));
    let (mut sa, mut sb) = (int(0), int(0));
    let mut k = 0i64;
    loop {
        if k % 32 == 0 && stopped() {
            return None;
        }
        let small = -(wp as f64) - 4.0;
        if log2_up(&a) < small && log2_up(&b) < small {
            break;
        }
        if (2 * k) as f64 > xlo + 4.0 {
            return None;
        }
        sa = sa.add(&a, wp);
        sb = sb.add(&b, wp);
        a = a.mul(&r2, wp).mul(&int(-(2 * k + 1) * (2 * k + 2)), wp);
        b = b.mul(&r2, wp).mul(&int(-(2 * k + 2) * (2 * k + 3)), wp);
        k += 1;
    }
    let f = widen(sa, &mag_up(&a)).div(x, wp)?;
    let g = widen(sb, &mag_up(&b)).mul(&r2, wp);
    Some((f, g))
}

/// The extra bits of the Si, Ci series at x.
fn sici_extra(x: f64, wp: u64) -> u64 {
    if fg_asym_ok(x.abs(), wp) {
        0
    } else {
        (1.443 * x.abs()).ceil().min(1e7) as u64 + 16
    }
}

/// Si(x) for a ball x (odd; the series for any x, the expansions for large |x|).
pub(crate) fn si_point(x: &Ball, wp: u64) -> Option<Ball> {
    if !x.contains_zero() {
        let ax = if x.mid.neg { x.neg() } else { x.clone() };
        if let Some((f, g)) = fg_asymptotic(&ax, wp) {
            let (s, c) = el::sin_cos(&ax, wp)?;
            let v = pi(wp)?.mul(&half(wp), wp).sub(&f.mul(&c, wp), wp).sub(&g.mul(&s, wp), wp);
            return Some(if x.mid.neg { v.neg() } else { v });
        }
    }
    if up_f(x) > MAX_SERIES_ARG {
        return None;
    }
    let z = x.mul(x, wp).mul(&Ball::from_ratio(-1, 4, wp), wp);
    let h = half(wp);
    let th = Ball::from_ratio(3, 2, wp);
    Some(x.mul(&pfq_sum(&[h], &[th.clone(), th], &z, wp)?, wp))
}

/// Ci(x) for a ball x > 0.
pub(crate) fn ci_point(x: &Ball, wp: u64) -> Option<Ball> {
    if !is_pos(x) {
        return None;
    }
    if let Some((f, g)) = fg_asymptotic(x, wp) {
        let (s, c) = el::sin_cos(x, wp)?;
        return Some(f.mul(&s, wp).sub(&g.mul(&c, wp), wp));
    }
    if up_f(x) > MAX_SERIES_ARG {
        return None;
    }
    let z = x.mul(x, wp).mul(&Ball::from_ratio(-1, 4, wp), wp);
    let th = Ball::from_ratio(3, 2, wp);
    let s = pfq_sum(&[int(1), int(1)], &[int(2), int(2), th], &z, wp)?.mul(&z, wp);
    Some(euler_gamma(wp)?.add(&el::log(x, wp)?, wp).add(&s, wp))
}

/// Si(x) = int_0^x sin(t)/t dt for real x.
pub fn sin_integral(x: &Real, p: u64) -> Option<CBall> {
    let xa = approx(x)?;
    if let Real::Exact(q) = x {
        if q.is_zero() {
            return Some(real(int(0)));
        }
    }
    refine(p, 24, |wp| {
        let w = wp + sici_extra(xa, wp);
        Some(real(si_point(&super::real_ball(x, w)?, w)?))
    })
}

/// Ci(x) = gamma + ln x + int_0^x (cos(t) - 1)/t dt for x > 0 (`None` at 0, -inf, and for x < 0, complex).
pub fn cos_integral(x: &Real, p: u64) -> Option<CBall> {
    let xa = approx(x)?;
    refine(p, 24, |wp| {
        let w = wp + sici_extra(xa, wp);
        Some(real(ci_point(&super::real_ball(x, w)?, w)?))
    })
}

// ---------------------------------------------------------------- Shi, Chi

/// Ei(x) and E1(x) for x > 0 when both expansions apply.
fn ei_e1_asymptotic(x: &Ball, wp: u64) -> Option<(Ball, Ball)> {
    if !ei_asym_ok(lo_f(x), wp) {
        return None;
    }
    Some((ei_asymptotic(x, wp)?, e1_asymptotic(x, wp)?))
}

/// Shi(x) for a ball x (odd).
fn shi_point(x: &Ball, wp: u64) -> Option<Ball> {
    if !x.contains_zero() {
        let ax = if x.mid.neg { x.neg() } else { x.clone() };
        if let Some((ei, e1)) = ei_e1_asymptotic(&ax, wp) {
            let v = ei.add(&e1, wp).mul(&half(wp), wp);
            return Some(if x.mid.neg { v.neg() } else { v });
        }
    }
    if up_f(x) > MAX_SERIES_ARG {
        return None;
    }
    let z = x.mul(x, wp).mul(&Ball::from_ratio(1, 4, wp), wp);
    let h = half(wp);
    let th = Ball::from_ratio(3, 2, wp);
    Some(x.mul(&pfq_sum(&[h], &[th.clone(), th], &z, wp)?, wp))
}

/// Chi(x) for a ball x > 0.
pub(crate) fn chi_point(x: &Ball, wp: u64) -> Option<Ball> {
    if !is_pos(x) {
        return None;
    }
    if let Some((ei, e1)) = ei_e1_asymptotic(x, wp) {
        return Some(ei.sub(&e1, wp).mul(&half(wp), wp));
    }
    if up_f(x) > MAX_SERIES_ARG {
        return None;
    }
    let z = x.mul(x, wp).mul(&Ball::from_ratio(1, 4, wp), wp);
    let th = Ball::from_ratio(3, 2, wp);
    let s = pfq_sum(&[int(1), int(1)], &[int(2), int(2), th], &z, wp)?.mul(&z, wp);
    Some(euler_gamma(wp)?.add(&el::log(x, wp)?, wp).add(&s, wp))
}

/// Shi(x) = int_0^x sinh(t)/t dt for real x.
pub fn sinh_integral(x: &Real, p: u64) -> Option<CBall> {
    approx(x)?;
    if let Real::Exact(q) = x {
        if q.is_zero() {
            return Some(real(int(0)));
        }
    }
    refine(p, 24, |wp| Some(real(shi_point(&super::real_ball(x, wp)?, wp)?)))
}

/// Chi(x) = gamma + ln x + int_0^x (cosh(t) - 1)/t dt for x > 0 (`None` at 0 and for x < 0, complex).
pub fn cosh_integral(x: &Real, p: u64) -> Option<CBall> {
    approx(x)?;
    refine(p, 24, |wp| Some(real(chi_point(&super::real_ball(x, wp)?, wp)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_arith::num::Int;

    fn digits(b: &Ball, d: usize) -> String {
        let (s, e) = b.mid.to_sci(d);
        format!("{}{s}e{e}", if b.mid.neg { "-" } else { "" })
    }

    fn q(n: i64, d: i64) -> Real {
        Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)))
    }

    #[test]
    fn published_values() {
        let v = exp_integral_ei(&q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 45), "189511781635593675546652093433163426901706058e0");
        let v = exp_integral_e(&q(1, 1), &q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 45), "219383934395520273677163775460121649031047293e-1");
        let v = log_integral(&q(2, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 45), "104516378011749278484458888919461313652261558e0");
        let v = sin_integral(&q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 45), "946083070367183014941353313823179657812337955e-1");
        let v = cos_integral(&q(1, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 45), "337403922900968134662646203889150769997578033e-1");
    }
}
