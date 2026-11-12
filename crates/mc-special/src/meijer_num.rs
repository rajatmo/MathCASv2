//! Numerical values of the Meijer G-function at any precision (round 22), on complex balls.
//!
//! With the convention of DLMF 16.17.1 (Luke, *The Special Functions and Their Approximations* I, 1969, section
//! 5.2; Prudnikov, Brychkov and Marichev, *Integrals and Series* 3, 1990, section 8.2),
//!
//! ```text
//! G^{m,n}_{p,q}(z | a; b) = 1/(2 pi i) integral over L of
//!     prod_{j<=m} Gamma(b_j - s) prod_{j<=n} Gamma(1 - a_j + s)
//!   / (prod_{j>m} Gamma(1 - b_j + s) prod_{j>n} Gamma(a_j - s)) z^s ds,
//! ```
//!
//! the values come from **Slater's theorem** (DLMF 16.17.2; Luke 5.2.(6)): closing the contour around the poles of
//! the first m gamma factors,
//!
//! ```text
//! G = sum_{k<=m} A_k z^{b_k} pF(q-1)(1 + b_k - a; 1 + b_k - b (without b_k); (-1)^(p-m-n) z),
//! A_k = prod_{l<=m, l!=k} Gamma(b_l - b_k) prod_{l<=n} Gamma(1 + b_k - a_l)
//!     / (prod_{l>m} Gamma(1 + b_k - b_l) prod_{l>n} Gamma(a_l - b_k)),
//! ```
//!
//! valid for p < q (every z), and for p = q when |z| < 1. For p > q, and p = q with |z| > 1, the same sum is taken
//! for G^{n,m}_{q,p}(1/z | 1 - b; 1 - a) (DLMF 16.19.1), with the power z^(a_k - 1) taken from the principal
//! logarithm of z itself (not of 1/z), so the identity holds on the negative real axis too. `z^b` is always
//! `exp(b log z)` with the principal logarithm; with the fourth argument r of the generalized function,
//! `G(z, r) = G(z^(1/r))` with `log z^(1/r) = (log z)/r`.
//!
//! - **Confluent poles** (parameters b_k among the first m differing by integers, where the poles of the gamma
//!   factors coincide and the true expansion has logarithms; Luke 5.2 and Adamchik and Marichev, *The algorithm for
//!   calculating integrals of hypergeometric type functions and its realization in REDUCE system*, ISSAC 1990,
//!   section 3): the parameters are perturbed, b_k -> b_k + (k + 1) h with h = 2^-H, and the Slater sum of the
//!   perturbed function is evaluated at a working precision large enough to absorb the cancellation of the 1/h
//!   terms (H bits per coincident factor). G is analytic in its parameters, so the value moves by O(h); the sum is
//!   taken at h and 2h and the radius is widened by twice their difference. The same perturbation handles a
//!   regularized series (1 + b_k - b_l a non-positive integer for l > m). This is the one value in the module that
//!   is not a rigorous enclosure: the O(h) term is estimated, not bounded (its size is about 2^-H against the
//!   2^-p asked).
//! - The pFq series and its tail bound are those of [`crate::ball::cseries::pfq_sum`]; the working precision is
//!   doubled while the result has fewer than p correct bits (cancellation between the k terms: G^{2,0}_{0,2}(z),
//!   2 K_0(2 sqrt z), is a difference of two growing series), up to 16 p bits.
//! - Not covered: p = q with |z| = 1 (the series converge too slowly or not at all), large arguments of p < q where
//!   the cancellation exceeds the precision cap (an asymptotic expansion would be needed). Where the contour cannot
//!   separate the two families of poles (a_l - b_k a positive integer, l <= n, k <= m) the value is the limit of
//!   the perturbed function when it exists (a zero 1/gamma factor cancelling the coincidence), else none.
use mc_arith::num::{stopped, Ball, Float, Mag};
use mc_numeric::cball::{self, CBall};

/// The parameters of G^{m,n}_{p,q}: `an` = a_1..a_n, `ap` = a_(n+1)..a_p, `bm` = b_1..b_m, `bq` = b_(m+1)..b_q.
#[derive(Clone, Debug)]
pub struct Params {
    /// a_1, ..., a_n
    pub an: Vec<CBall>,
    /// a_(n+1), ..., a_p
    pub ap: Vec<CBall>,
    /// b_1, ..., b_m
    pub bm: Vec<CBall>,
    /// b_(m+1), ..., b_q
    pub bq: Vec<CBall>,
}

impl Params {
    fn p(&self) -> usize {
        self.an.len() + self.ap.len()
    }
    fn q(&self) -> usize {
        self.bm.len() + self.bq.len()
    }
    /// The parameters of G^{n,m}_{q,p}(1/z | 1 - b; 1 - a).
    fn flipped(&self, wp: u64) -> Params {
        let one = CBall::from_i64(1);
        let f = |v: &[CBall]| v.iter().map(|x| one.sub(x, wp)).collect::<Vec<_>>();
        Params { an: f(&self.bm), ap: f(&self.bq), bm: f(&self.an), bq: f(&self.ap) }
    }
}

fn f64_of(b: &Ball) -> f64 {
    crate::ball::f64_of(&b.mid)
}

/// The approximate value of a complex ball.
fn approx(z: &CBall) -> (f64, f64) {
    (f64_of(&z.re), f64_of(&z.im))
}

/// Does the complex ball contain an integer (an integer <= `max` when given)?
fn contains_int(z: &CBall, max: Option<i64>) -> bool {
    if !z.im.contains_zero() {
        return false;
    }
    let x = f64_of(&z.re);
    if !x.is_finite() || x.abs() > 1e15 {
        return false;
    }
    let n = x.round();
    if max.is_some_and(|m| n > m as f64) {
        return false;
    }
    // exact enough to see a difference far below the integer
    let bits = (z.re.mid.top() - z.re.mid.exp).max(0) as u64 + 64;
    z.re.sub(&Ball::from_i64(n as i64), bits.min(1 << 20)).contains_zero()
}

/// 1/Gamma(x): an exact zero at an exact pole.
fn rgamma(x: &CBall, wp: u64) -> Option<CBall> {
    if x.is_real() && x.re.rad.is_zero() {
        if let Some(n) = crate::ball::float_int(&x.re.mid) {
            if n.to_i64().is_some_and(|v| v <= 0) {
                return Some(CBall::from_i64(0));
            }
        }
    }
    CBall::from_i64(1).div(&gamma(x, wp)?, wp)
}

/// Gamma(x) on a complex ball without a pole.
fn gamma(x: &CBall, wp: u64) -> Option<CBall> {
    if contains_int(x, Some(0)) {
        return None;
    }
    if x.is_real() {
        return Some(CBall::real(crate::ball::gamma::gamma_ball(&x.re, wp)?));
    }
    crate::ball::complex::gamma(x, wp)
}

/// The kind of degeneracy of the Slater sum over the b's.
#[derive(PartialEq, Eq, Debug)]
enum Degenerate {
    No,
    /// coincident poles or a regularized series: perturb
    Perturb,
    /// the contour cannot separate the poles
    Undefined,
}

fn degenerate(par: &Params, wp: u64) -> Degenerate {
    let one = CBall::from_i64(1);
    for bk in &par.bm {
        for al in &par.an {
            // 1 + b_k - a_l a non-positive integer: a pole of both families
            if contains_int(&one.add(bk, wp).sub(al, wp), Some(0)) {
                return Degenerate::Undefined;
            }
        }
    }
    for (k, bk) in par.bm.iter().enumerate() {
        for (l, bl) in par.bm.iter().enumerate() {
            if l != k && contains_int(&bl.sub(bk, wp), None) {
                return Degenerate::Perturb;
            }
        }
        for bl in &par.bq {
            if contains_int(&one.add(bk, wp).sub(bl, wp), Some(0)) {
                return Degenerate::Perturb;
            }
        }
    }
    Degenerate::No
}

/// The Slater sum (module comment) for parameters without degeneracy, at argument z with log z = l, wp bits.
fn slater_plain(par: &Params, z: &CBall, l: &CBall, wp: u64) -> Option<CBall> {
    let one = CBall::from_i64(1);
    let sign_neg = (par.p() as i64 - par.bm.len() as i64 - par.an.len() as i64).rem_euclid(2) == 1;
    let w = if sign_neg { z.neg() } else { z.clone() };
    let mut sum = CBall::from_i64(0);
    for (k, bk) in par.bm.iter().enumerate() {
        if stopped() {
            return None;
        }
        let mut c = one.clone();
        for (j, bl) in par.bm.iter().enumerate() {
            if j != k {
                c = c.mul(&gamma(&bl.sub(bk, wp), wp)?, wp);
            }
        }
        for al in &par.an {
            c = c.mul(&gamma(&one.add(bk, wp).sub(al, wp), wp)?, wp);
        }
        for bl in &par.bq {
            c = c.mul(&rgamma(&one.add(bk, wp).sub(bl, wp), wp)?, wp);
        }
        for al in &par.ap {
            c = c.mul(&rgamma(&al.sub(bk, wp), wp)?, wp);
        }
        if c.is_exact_zero() {
            continue;
        }
        let upper: Vec<CBall> = par.an.iter().chain(&par.ap).map(|a| one.add(bk, wp).sub(a, wp)).collect();
        let lower: Vec<CBall> = par
            .bm
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != k)
            .map(|(_, b)| b)
            .chain(&par.bq)
            .map(|b| one.add(bk, wp).sub(b, wp))
            .collect();
        let f = pfq_sum(&upper, &lower, &w, wp)?;
        let pw = cball::exp(&bk.mul(l, wp), wp)?;
        sum = sum.add(&c.mul(&pw, wp).mul(&f, wp), wp);
    }
    Some(sum)
}

/// log2 of an upper bound of |z| (exponents beyond the range of doubles included).
fn log2_mag(z: &CBall) -> f64 {
    let m = |b: &Ball| Mag::from_float_up(&b.mid).add(&b.rad);
    let v = m(&z.re).add(&m(&z.im));
    if v.is_zero() {
        f64::NEG_INFINITY
    } else {
        v.log2()
    }
}

/// The series pFq(a; b; w) at wp bits with a bound of its tail (the ratio bound of
/// [`crate::ball::cseries::pfq_sum`], with the magnitudes of the terms taken from their exponents, so precisions
/// beyond the range of doubles work): `None` for a divergent series, a pole of the b's or too many terms.
fn pfq_sum(a: &[CBall], b: &[CBall], w: &CBall, wp: u64) -> Option<CBall> {
    if w.is_exact_zero() {
        return Some(CBall::from_i64(1));
    }
    let nonpos = |z: &CBall| z.is_real() && z.re.rad.is_zero() && crate::ball::float_int(&z.re.mid).and_then(|n| n.to_i64()).is_some_and(|n| n <= 0);
    let stop = a.iter().filter(|z| nonpos(z)).map(|z| crate::ball::float_int(&z.re.mid).and_then(|n| n.to_i64()).unwrap_or(0).unsigned_abs()).min();
    if a.len() > b.len() + 1 && stop.is_none() {
        return None;
    }
    let up = |z: &CBall| {
        let (x, y) = approx(z);
        x.hypot(y) * (1.0 + 1e-9) + 1e-300 + crate::ball::f64_of(&z.re.rad.to_float()) + crate::ball::f64_of(&z.im.rad.to_float())
    };
    let aa: Vec<f64> = a.iter().map(up).collect();
    let bb: Vec<f64> = b.iter().map(up).collect();
    let za = up(w);
    if stop.is_none() && a.len() == b.len() + 1 && za >= 1.0 {
        return None;
    }
    // the terms of a non-terminating series with p <= q grow until k is about |w|^(1/(q + 1 - p)): beyond the term
    // limit there is no value, answered at once (|z| = 10^30 would otherwise run the limit at every precision)
    if stop.is_none() && a.len() <= b.len() && (!za.is_finite() || za.powf(1.0 / (b.len() + 1 - a.len()) as f64) > (1u64 << 20) as f64) {
        return None;
    }
    let bmax = bb.iter().copied().fold(0.0, f64::max);
    let ratio = |k: f64| -> f64 {
        let mut r = za / (k + 1.0);
        for &x in &aa {
            r *= k + x;
        }
        for &y in &bb {
            r /= k - y;
        }
        r * (1.0 + 1e-9)
    };
    let mut t = CBall::from_i64(1);
    let mut s = CBall::from_i64(1);
    let mut rho = f64::NEG_INFINITY;
    for k in 0..(1u64 << 20) {
        if k % 32 == 0 && stopped() {
            return None;
        }
        if Some(k) == stop {
            return Some(s);
        }
        let kf = k as f64;
        if stop.is_none() && kf > bmax + 1.0 {
            // rho bounds every later ratio once k is past the parameters (the ratio decreases from there)
            let lim = if a.len() == b.len() + 1 { za } else { 0.0 };
            let rho = ratio(kf).max(ratio(kf + 1.0)).max(lim);
            let extra = if rho < 1.0 { (rho / (1.0 - rho)).log2().max(-60.0).ceil() as i64 + 1 } else { 0 };
            if rho < 1.0 && log2_mag(&t) + (extra as f64) < log2_mag(&s) - wp as f64 - 4.0 {
                let e = Mag::from_float_up(&t.re.mid).add(&t.re.rad).add(&Mag::from_float_up(&t.im.mid)).add(&t.im.rad).mul_2exp(extra);
                let wid = |x: Ball| Ball { mid: x.mid, rad: x.rad.add(&e) };
                return Some(CBall::new(wid(s.re), wid(s.im)));
            }
        }
        let kb = CBall::from_i64(k as i64);
        let mut num = w.clone();
        for ai in a {
            num = num.mul(&ai.add(&kb, wp), wp);
        }
        let mut den = CBall::from_i64(k as i64 + 1);
        for bj in b {
            let d = bj.add(&kb, wp);
            if d.contains_zero() {
                return None;
            }
            den = den.mul(&d, wp);
        }
        // the term as a product of the previous one and the factors num/den: the radius of a complex ball multiplied
        // k times by a number of modulus 0.84 grows like 1.19^k (the rectangle bound of a complex product is up to
        // sqrt 2 too wide), so the relative radii add instead: rho_k <= rho_(k-1) + rel(num) + rel(den) + 2^(2 - wp),
        // and the term gets the radius |t| rho (rho below 1/16; else the ball product itself is kept). Rigorous to
        // first order; the second-order terms are covered by 1 bit of margin
        let raw = t.mul(&num, wp).div(&den, wp)?;
        rho = lg_add(lg_add(rho, rel_log2(&num)), lg_add(rel_log2(&den), 2.0 - wp as f64));
        t = if rho + 0.5 < -4.0 {
            let m = Mag::from_float_up(&raw.re.mid).add(&Mag::from_float_up(&raw.im.mid));
            let r = Mag::pow2((rho + 0.5).ceil() as i64).mul(&m);
            CBall::new(Ball { mid: raw.re.mid, rad: r }, Ball { mid: raw.im.mid, rad: r })
        } else {
            raw
        };
        s = s.add(&t, wp);
    }
    None
}

/// log2(2^a + 2^b).
fn lg_add(a: f64, b: f64) -> f64 {
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    if lo == f64::NEG_INFINITY {
        return hi;
    }
    hi + (1.0 + (lo - hi).exp2()).log2()
}

/// log2 of the relative radius of a complex ball (the sum of the radii of its parts over the larger part of the
/// midpoint, a lower bound of the modulus up to sqrt 2): -inf for an exact ball, +inf when the midpoint is 0.
fn rel_log2(z: &CBall) -> f64 {
    let r = z.re.rad.add(&z.im.rad);
    if r.is_zero() {
        return f64::NEG_INFINITY;
    }
    let l = |b: &Ball| {
        let m = Mag::from_float_up(&b.mid);
        if m.is_zero() { f64::NEG_INFINITY } else { m.log2() }
    };
    let m = l(&z.re).max(l(&z.im));
    if m == f64::NEG_INFINITY {
        return f64::INFINITY;
    }
    r.log2() - m + 0.5
}

/// The ball 2^-h c.
fn tiny(c: i64, h: i64) -> CBall {
    let f = Float::from_i64(c);
    CBall::real(Ball::exact(Float::new(f.neg, f.man, f.exp - h)))
}

/// The Slater sum with the perturbation of the module comment where the poles coincide.
fn slater(par: &Params, z: &CBall, l: &CBall, wp: u64) -> Option<CBall> {
    if par.bm.is_empty() {
        return Some(CBall::from_i64(0));
    }
    match degenerate(par, wp) {
        Degenerate::No => slater_plain(par, z, l, wp),
        // the families meet (a_l - b_k a positive integer): the limit of the perturbed function, when it exists
        // (a zero 1/gamma factor of the coefficient cancels the pole); a divergent limit leaves the two sums at h
        // and 2h far apart, and the widened result has no correct bits
        Degenerate::Undefined | Degenerate::Perturb => {
            // the arguments' radii would be amplified by the 1/h terms: the sums are taken at the midpoints, and
            // the effect of the radii estimated by moving every argument by its radius
            if has_radius(par) || has_radius_c(z) || has_radius_c(l) {
                let (pm, zm, lm) = (mid_params(par), mid(z), mid(l));
                let v = perturbed(&pm, &zm, &lm, wp)?;
                let (ps, zs, ls) = (mid_params(&shift_params(par, wp)), mid(&shift(z, wp)), mid(&shift(l, wp)));
                let vs = perturbed(&ps, &zs, &ls, wp)?;
                return Some(widen_by(v, &vs, wp));
            }
            perturbed(par, z, l, wp)
        }
    }
}

/// The Slater sum of perturbed parameters, at h and 2h (module comment).
fn perturbed(par: &Params, z: &CBall, l: &CBall, wp: u64) -> Option<CBall> {
    {
        {
            let m = par.bm.len() as u64;
            let h = (wp.saturating_sub(32) / (m + 1)).max(16) as i64;
            let shifted = |scale: i64| {
                let mut q = par.clone();
                for (k, b) in q.bm.iter_mut().enumerate() {
                    *b = b.add(&tiny(scale * (k as i64 + 1), h), wp);
                }
                q
            };
            let v1 = slater_plain(&shifted(1), z, l, wp)?;
            let v2 = slater_plain(&shifted(2), z, l, wp)?;
            Some(widen_by(v1, &v2, wp))
        }
    }
}

/// v with both radii widened by twice |v - w| (an estimated error).
fn widen_by(v: CBall, w: &CBall, wp: u64) -> CBall {
    let d = v.sub(w, wp);
    let e = Mag::from_float_up(&d.re.mid).add(&d.re.rad).add(&Mag::from_float_up(&d.im.mid)).add(&d.im.rad).mul_2exp(1);
    let widen = |b: Ball| Ball { mid: b.mid, rad: b.rad.add(&e) };
    let real = v.is_real() && w.is_real();
    CBall::new(widen(v.re), if real { v.im } else { widen(v.im) })
}

fn has_radius_c(z: &CBall) -> bool {
    !z.re.rad.is_zero() || !z.im.rad.is_zero()
}

fn has_radius(par: &Params) -> bool {
    par.an.iter().chain(&par.ap).chain(&par.bm).chain(&par.bq).any(has_radius_c)
}

/// The midpoint of a ball, exactly.
fn mid(z: &CBall) -> CBall {
    CBall::new(Ball::exact(z.re.mid.clone()), Ball::exact(z.im.mid.clone()))
}

/// The midpoint moved by the radius (both parts).
fn shift(z: &CBall, wp: u64) -> CBall {
    let s = |b: &Ball| Ball::exact(b.mid.clone()).add(&Ball::exact(b.rad.to_float()), wp);
    CBall::new(s(&z.re), if z.im.mid.is_zero() && z.im.rad.is_zero() { z.im.clone() } else { s(&z.im) })
}

fn map_params(par: &Params, f: impl Fn(&CBall) -> CBall) -> Params {
    let g = |v: &[CBall]| v.iter().map(&f).collect();
    Params { an: g(&par.an), ap: g(&par.ap), bm: g(&par.bm), bq: g(&par.bq) }
}

fn mid_params(par: &Params) -> Params {
    map_params(par, mid)
}

fn shift_params(par: &Params, wp: u64) -> Params {
    map_params(par, |z| shift(z, wp))
}

/// log2 of an upper bound of |z|.
fn log2_abs(z: &CBall) -> f64 {
    let (x, y) = approx(z);
    x.hypot(y).log2()
}

/// G at the point z (log z = l) at working precision wp.
fn eval_at(par: &Params, l: &CBall, wp: u64) -> Option<CBall> {
    let z = cball::exp(l, wp)?;
    let (p, q) = (par.p(), par.q());
    let lz = log2_abs(&z);
    if p < q || (p == q && lz < -1e-9) {
        return slater(par, &z, l, wp);
    }
    if p > q || (p == q && lz > 1e-9) {
        let f = par.flipped(wp);
        let zi = z.recip(wp)?;
        return slater(&f, &zi, &l.neg(), wp);
    }
    None
}

/// log2 of the larger radius of the two parts (-inf for an exact ball).
fn log2_rad(z: &CBall) -> f64 {
    let lr = |b: &Ball| if b.rad.is_zero() { f64::NEG_INFINITY } else { b.rad.log2() };
    lr(&z.re).max(lr(&z.im))
}

/// Correct bits of a complex ball relative to its magnitude (from the exponents, so values beyond the range of
/// doubles, e.g. the partial sums of a series of terms near e^1000 that cancel, are measured correctly: the
/// squares of such a value overflow doubles and used to give an infinite accuracy).
fn accuracy(z: &CBall) -> f64 {
    let lm = |b: &Ball| {
        let m = Mag::from_float_up(&b.mid);
        if m.is_zero() {
            f64::NEG_INFINITY
        } else {
            m.log2()
        }
    };
    let lr = |b: &Ball| if b.rad.is_zero() { f64::NEG_INFINITY } else { b.rad.log2() };
    let rad = lr(&z.re).max(lr(&z.im));
    if rad == f64::NEG_INFINITY {
        return f64::INFINITY;
    }
    let a = lm(&z.re).max(lm(&z.im));
    if a == f64::NEG_INFINITY {
        return 0.0;
    }
    a - rad
}

/// log2 of the largest term of the Slater series that [`eval_at`] sums (in machine arithmetic): the cancellation
/// of a sum of terms near e^|z| costs that many bits, so the working precision starts there and a value that needs
/// more than the cap is declined at once (instead of repeating gamma functions at every doubled precision).
pub(crate) fn cancellation_bits(par: &Params, z: &CBall) -> f64 {
    let (p, q) = (par.p(), par.q());
    let lz = log2_abs(z);
    let (pe, ze) = if p < q || (p == q && lz < -1e-9) {
        (par.clone(), z.clone())
    } else {
        match z.recip(64) {
            Some(zi) => (par.flipped(64), zi),
            None => return 0.0,
        }
    };
    let one = CBall::from_i64(1);
    let za = {
        let (x, y) = approx(&ze);
        x.hypot(y)
    };
    let mut worst = 0.0f64;
    for (k, bk) in pe.bm.iter().enumerate() {
        let upper: Vec<(f64, f64)> = pe.an.iter().chain(&pe.ap).map(|a| approx(&one.add(bk, 64).sub(a, 64))).collect();
        let lower: Vec<(f64, f64)> = pe
            .bm
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != k)
            .map(|(_, b)| b)
            .chain(&pe.bq)
            .map(|b| approx(&one.add(bk, 64).sub(b, 64)))
            .collect();
        let g = crate::ball::cseries::max_term_log2(&upper, &lower, za);
        if g.is_finite() {
            worst = worst.max(g);
        }
    }
    worst
}

/// G^{m,n}_{p,q}(z | a; b), or G(z, r) when `r` is given, to p bits (module comment). `None` when there is no
/// value by these methods.
pub fn meijer_g(par: &Params, z: &CBall, r: Option<&CBall>, p: u64) -> Option<CBall> {
    if z.is_exact_zero() {
        return at_zero(par, p);
    }
    if z.contains_zero() {
        return None;
    }
    // cancellation of terms near e^|z| costs 1.44 |z| bits: the cap covers |z| up to about 11000 (1F1-like terms) at p = 53
    let cap = (16 * p).max(p + 16384);
    // with coincident poles the perturbation needs about (m + 1) (p + 40) bits (H = p + 40 per factor 1/h)
    let m = par.bm.len().max(par.an.len()) as u64;
    let degen = degenerate(par, 64) != Degenerate::No || degenerate(&par.flipped(64), 64) != Degenerate::No;
    let mut wp = if degen { (m + 1) * (p + 40) + 32 } else { p + 32 };
    let need = if r.is_none() { cancellation_bits(par, z) } else { 0.0 };
    if need + p as f64 > cap as f64 {
        return None;
    }
    wp = wp.max(need as u64 + p + 32);
    let mut best: Option<(CBall, f64)> = None;
    let mut last_rad: Option<f64> = None;
    let mut misses = 0;
    loop {
        if stopped() {
            return None;
        }
        let mut l = cball::log(z, wp)?;
        if let Some(r) = r {
            l = l.div(r, wp)?;
        }
        let ev = eval_at(par, &l, wp);
        let ev_acc = ev.as_ref().map(accuracy);
        // no value (a pole, a divergent series, too many terms) twice in a row at growing precision: doubling
        // again repeats the gamma functions at ever larger precision for nothing
        misses = if ev.is_none() { misses + 1 } else { 0 };
        if misses >= 2 {
            break;
        }
        if let Some(v) = ev {
            let acc = accuracy(&v);
            if acc >= p as f64 + 2.0 {
                return Some(v.round(p));
            }
            let rad = log2_rad(&v);
            let shrunk = last_rad.is_some_and(|r0: f64| rad < r0 - wp as f64 / 8.0);
            last_rad = Some(rad);
            if let Some((_, a0)) = &best {
                // no progress from doubling: the arguments' radii limit the accuracy. A radius that shrinks with
                // the precision is progress even when the relative accuracy does not move: the midpoint of a
                // ball still dominated by cancellation noise shrinks with it (terms near e^1000 at p = 53)
                if acc < *a0 + 2.0 && !shrunk {
                    if acc > *a0 {
                        best = Some((v, acc));
                    }
                    break;
                }
            }
            if best.as_ref().is_none_or(|(_, a0)| acc > *a0) {
                best = Some((v, acc));
            }
        }
        // the next precision: a ball with some correct bits is short of about p + 8 - acc of them, so grow by that
        // (at most doubling); a ball dominated by cancellation noise doubles (the noise level need not fall with the
        // precision when the perturbation of coincident poles grows with it)
        let step = match ev_acc {
            Some(acc) => {
                if acc.is_finite() && acc >= 0.0 { ((p as f64 + 8.0 - acc).max(64.0) as u64).min(wp) } else { wp }
            }
            None => wp,
        };
        if wp + step > cap {
            break;
        }
        wp += step;
    }
    let (v, acc) = best?;
    (acc > 0.0).then(|| v.round(p))
}

/// G at z = 0: 0 when p <= q and every b_k (k <= m) has a positive real part.
fn at_zero(par: &Params, _p: u64) -> Option<CBall> {
    if par.p() > par.q() {
        return None;
    }
    if par.bm.iter().all(|b| f64_of(&b.re) - crate::ball::f64_of(&b.re.rad.to_float()) > 0.0) {
        return Some(CBall::from_i64(0));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(v: f64) -> CBall {
        CBall::real(Ball::exact(crate::ball::float_of(v)))
    }

    fn val(z: &CBall) -> f64 {
        f64_of(&z.re)
    }

    #[test]
    fn bessel_and_exponential_cases() {
        // G^{1,0}_{0,1}(z | ; b) = z^b e^-z
        let par = Params { an: vec![], ap: vec![], bm: vec![c(0.5)], bq: vec![] };
        let v = meijer_g(&par, &c(2.0), None, 80).unwrap();
        assert!((val(&v) - 2f64.sqrt() * (-2f64).exp()).abs() < 1e-15);
        // G^{2,0}_{0,2}(z | ; 0, 0) = 2 K_0(2 sqrt z): confluent poles
        let par = Params { an: vec![], ap: vec![], bm: vec![c(0.0), c(0.0)], bq: vec![] };
        let v = meijer_g(&par, &c(1.0), None, 80).unwrap();
        assert!((val(&v) - 2.0 * 0.11389387274953344).abs() < 1e-15, "{}", val(&v));
        // G^{1,1}_{1,1}(z | a; b) = Gamma(1 - a + b) z^b (1 + z)^(a - b - 1), both sides of |z| = 1
        let par = Params { an: vec![c(0.5)], ap: vec![], bm: vec![c(0.25)], bq: vec![] };
        for z in [0.3, 3.0] {
            let v = meijer_g(&par, &c(z), None, 80).unwrap();
            let e = 1.2254167024651776 * z.powf(0.25) * (1.0 + z).powf(-0.75);
            assert!((val(&v) - e).abs() < 1e-15, "{z}: {} {e}", val(&v));
        }
    }
}
