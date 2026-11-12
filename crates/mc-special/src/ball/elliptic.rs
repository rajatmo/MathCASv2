//! Elliptic integrals and Jacobi elliptic functions on balls (real arguments, parameter convention m = k^2).
//!
//! - **R_F, R_D, R_J** by Carlson's duplication on balls (each step is an identity, so the balls stay enclosures),
//!   finished by the symmetric Taylor polynomial with a proven remainder. Writing the arguments as A (1 - X_i) with A
//!   their weighted mean and eps = max |X_i|, the integrand factor prod (1 - X_i/(s + 1))^(-e_i) (exponents summing to
//!   3/2 for R_F, 5/2 for R_D and R_J) is majorised coefficientwise by (1 - eps w)^(-3/2) resp. (1 - eps w)^(-5/2), so
//!   the degree-n part of the expansion is at most c_n eps^n with c_n = (3/2)_n/(n! (2n + 1)) (decreasing, below 1
//!   from n = 1) resp. c_n = 3 (5/2)_n/(n! (2n + 3)) (c_n/n decreasing, c_6 < 6). Hence after the degree-7 polynomial
//!   R_F has remainder at most A^(-1/2) eps^8/(1 - eps), and after degree 5 R_D and R_J have at most
//!   4^-N A^(-3/2) 6 eps^6/(1 - eps)^2 (eps <= 1/2 when finishing). The steps continue until that bound is below
//!   2^-wp (about wp/16 steps for R_F, wp/12 for R_D, R_J), or until the arguments' own radii dominate the
//!   deviations. R_J's step terms R_C(alpha^2, beta^2) = R_C(1, 1 + t)/alpha come from the series
//!   sum (-t)^k/(2k + 1) (tail at most 4|t|^N for |t| <= 3/4) or from atan / log for larger |t|. The principal value
//!   of R_J for p < 0 is Carlson's transformation to a positive p (as in the machine path).
//! - **K(m), E(m)** by the AGM of 1 and sqrt(1 - m): b_n <= M <= a_n for n >= 1, so M is enclosed by
//!   [lower(b_n), upper(a_n)]; E = K (1 - m/2 - sum_(n>=1) 2^(n-1) c_n^2) with c_1 = m/(2(1 + sqrt(1 - m))) and
//!   c_(n+1) = c_n^2/(4 a_(n+1)); once c_n <= M/4 the terms decrease by more than 2 and the tail is at most twice the
//!   first omitted term, which is bounded by 2^n (c_n^2/(4 M))^2.
//! - **F, E, Pi (incomplete)**: phi = j pi + psi with |psi| < pi/2 decided rigorously (a psi ball that may reach
//!   ±pi/2 gives no value at that precision; the retries resolve it), then the Carlson forms of the machine path and
//!   the quasi-periodicity with the complete integrals. Pi(n | m) for n > 1 is the principal value
//!   -(m/(3n)) R_J(0, 1 - m, 1, 1 - m/n) (= K(m) - Pi(m/n | m)); the incomplete Pi uses the principal value of R_J
//!   where 1 - n sin^2 phi < 0.
//! - **sn, cn, dn**: the amplitude am(u | m) is the inverse of the increasing function F(. | m) (m < 1). An
//!   approximation phi (the machine value, then Newton steps at doubling precisions, non-rigorous) is proved by
//!   checking with ball F that F(phi - d) < u < F(phi + d) for every u and m in their balls; then am lies in
//!   [phi - d, phi + d] and sn = sin, cn = cos of that ball, dn = sqrt(cn^2 + (1 - m) sn^2). m = 1 by tanh and sech,
//!   m > 1 by the reciprocal-modulus transformation. inverse_jacobi_sn(v, m) = v R_F(1 - v^2, 1 - m v^2, 1).
//!
//! Every function returns `None` where the value is complex or infinite (K, E, F beyond pi/2 for m >= 1 (E: m > 1),
//! poles of Pi), for machine arguments, and when the stop flag is raised.
use super::{
    f64_of, float_of, floor_float, hull, int, is_pos, log2_up, lower, mag_up, pi, rat_ball, real, refine, upper,
};
use mc_arith::num::{stopped, Ball, Float, Mag, Nat, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::elementary as el;
use std::cmp::Ordering;

/// A cap on duplication steps and series terms (the loops also poll the stop flag).
const MAX_STEPS: usize = 1 << 20;

// ---------------------------------------------------------------- helpers

/// x 2^k exactly.
fn scale2(b: &Ball, k: i64) -> Ball {
    let mid = if b.mid.is_zero() { b.mid.clone() } else { Float::new(b.mid.neg, b.mid.man.clone(), b.mid.exp + k) };
    Ball { mid, rad: b.rad.mul_2exp(k) }
}

fn frac(n: i64, d: i64, wp: u64) -> Ball {
    Ball::from_ratio(n, d, wp)
}

fn sq(b: &Ball, wp: u64) -> Ball {
    b.mul(b, wp)
}

fn mag_le(a: &Mag, b: &Mag) -> bool {
    a.to_float().cmp_exact(&b.to_float()) != Ordering::Greater
}

fn mag_max(a: Mag, b: Mag) -> Mag {
    if mag_le(&a, &b) {
        b
    } else {
        a
    }
}

/// Is the ball provably negative?
fn is_neg(b: &Ball) -> bool {
    b.mid.neg && !b.contains_zero()
}

fn is_exact_zero(b: &Ball) -> bool {
    b.rad.is_zero() && b.mid.is_zero()
}

/// sqrt of a ball whose exact values are known to be >= 0 (a lower end below zero comes from rounding): the hull
/// of 0 and sqrt(upper end) when the ball touches zero.
fn sqrt_nn(x: &Ball, wp: u64) -> Option<Ball> {
    if is_exact_zero(x) {
        return Some(x.clone());
    }
    if is_pos(x) {
        if let Some(r) = el::sqrt(x, wp) {
            return Some(r);
        }
    }
    let hi = upper(x);
    if hi.neg || hi.is_zero() {
        return Some(int(0));
    }
    let r = el::sqrt(&Ball::exact(hi), wp)?;
    Some(hull(&Float::zero(), &upper(&r), wp))
}

/// A real argument at a working precision, with its rational value when it is exact.
struct Arg<'a> {
    b: Ball,
    q: Option<&'a Rat>,
}

fn arg(r: &Real, wp: u64) -> Option<Arg<'_>> {
    match r {
        Real::Exact(q) => Some(Arg { b: rat_ball(q, wp), q: Some(q) }),
        Real::Ball(b, _) => Some(Arg { b: b.clone(), q: None }),
        Real::Machine(_) => None,
    }
}

/// 1 - x (exact for a rational x).
fn one_minus(a: &Arg, wp: u64) -> Ball {
    match a.q {
        Some(q) => rat_ball(&Rat::from_i64(1).sub(q), wp),
        None => int(1).sub(&a.b, wp),
    }
}

/// Is the argument exactly 1?
fn is_one(a: &Arg) -> bool {
    match a.q {
        Some(q) => q.cmp_value(&Rat::from_i64(1)) == Ordering::Equal,
        None => a.b.rad.is_zero() && a.b.mid.cmp_exact(&Float::from_i64(1)) == Ordering::Equal,
    }
}

/// An upper bound of eps = max |A - t|/A over the arguments, and whether the arguments' radii dominate the
/// midpoint deviations (further duplication steps would not help).
fn deviation(a: &Ball, ts: &[&Ball], wp: u64) -> Option<(Mag, bool)> {
    let lo = lower(a);
    if lo.neg || lo.is_zero() {
        return None;
    }
    let alo = Mag::from_float_down(&lo);
    let (mut e, mut mids, mut rads) = (Mag::ZERO, Mag::ZERO, Mag::ZERO);
    for t in ts {
        let d = a.sub(t, wp);
        e = mag_max(e, mag_up(&d));
        mids = mag_max(mids, Mag::from_float_up(&d.mid));
        rads = mag_max(rads, d.rad);
    }
    Some((e.div_up(&alo), mag_le(&mids, &rads)))
}

fn mag_pow(m: &Mag, n: u32) -> Mag {
    let mut r = Mag::pow2(0);
    for _ in 0..n {
        r = r.mul(m);
    }
    r
}

// ---------------------------------------------------------------- Carlson's integrals

/// The duplication step: (sqrt x sqrt y + sqrt x sqrt z + sqrt y sqrt z) and the square roots.
fn lambda(x: &Ball, y: &Ball, z: &Ball, wp: u64) -> Option<(Ball, [Ball; 3])> {
    let (sx, sy, sz) = (sqrt_nn(x, wp)?, sqrt_nn(y, wp)?, sqrt_nn(z, wp)?);
    let lam = sx.mul(&sy, wp).add(&sx.mul(&sz, wp), wp).add(&sy.mul(&sz, wp), wp);
    Some((lam, [sx, sy, sz]))
}

fn step(t: &Ball, lam: &Ball, wp: u64) -> Ball {
    scale2(&t.add(lam, wp), -2)
}

/// R_F(x, y, z) for balls whose values are >= 0 (at most one of them zero).
pub fn rf_b(x: &Ball, y: &Ball, z: &Ball, wp: u64) -> Option<Ball> {
    let (mut x, mut y, mut z) = (x.clone(), y.clone(), z.clone());
    for it in 0..MAX_STEPS {
        if it % 8 == 0 && stopped() {
            return None;
        }
        let a = x.add(&y, wp).add(&z, wp).div(&int(3), wp)?;
        let (eps, flat) = deviation(&a, &[&x, &y, &z], wp)?;
        let le = eps.log2();
        if le < -1.0 && (8.0 * le + 1.0 < -(wp as f64) - 4.0 || flat) {
            // X + Y + Z = 0 for every point: Z = -(X + Y)
            let xx = a.sub(&x, wp).div(&a, wp)?;
            let yy = a.sub(&y, wp).div(&a, wp)?;
            let zz = xx.add(&yy, wp).neg();
            let xy = xx.mul(&yy, wp);
            let e2 = xy.sub(&sq(&zz, wp), wp);
            let e3 = xy.mul(&zz, wp);
            let e22 = sq(&e2, wp);
            let terms = [
                (-1, 10, e2.clone()),
                (1, 14, e3.clone()),
                (1, 24, e22.clone()),
                (-3, 44, e2.mul(&e3, wp)),
                (-5, 208, e22.mul(&e2, wp)),
                (3, 104, sq(&e3, wp)),
                (1, 16, e22.mul(&e3, wp)),
            ];
            let mut poly = int(1);
            for (n, d, t) in terms.iter() {
                poly = poly.add(&t.mul(&frac(*n, *d, wp), wp), wp);
            }
            let inv = int(1).div(&el::sqrt(&a, wp)?, wp)?;
            let err = mag_up(&inv).mul(&mag_pow(&eps, 8)).mul_2exp(1);
            return Some(super::widen(poly.mul(&inv, wp), &err));
        }
        let (lam, _) = lambda(&x, &y, &z, wp)?;
        x = step(&x, &lam, wp);
        y = step(&y, &lam, wp);
        z = step(&z, &lam, wp);
    }
    None
}

/// The degree-5 polynomial of R_D and R_J, with the remainder bound for eps <= 1/2, times the unit 4^-k A^(-3/2).
fn finish5(e: [Ball; 4], a: &Ball, k: i64, eps: &Mag, wp: u64) -> Option<Ball> {
    let [e2, e3, e4, e5] = e;
    let terms = [
        (-3, 14, e2.clone()),
        (1, 6, e3.clone()),
        (9, 88, sq(&e2, wp)),
        (-3, 22, e4),
        (-9, 52, e2.mul(&e3, wp)),
        (3, 26, e5),
    ];
    let mut poly = int(1);
    for (n, d, t) in terms.iter() {
        poly = poly.add(&t.mul(&frac(*n, *d, wp), wp), wp);
    }
    let unit = scale2(&int(1).div(&a.mul(&el::sqrt(a, wp)?, wp), wp)?, -2 * k);
    let err = mag_up(&unit).mul(&mag_pow(eps, 6)).mul(&Mag::pow2(5)); // 6/(1 - eps)^2 <= 24 < 32
    Some(super::widen(poly.mul(&unit, wp), &err))
}

/// Should the duplication of R_D or R_J stop? (eps <= 1/2 and the remainder below 2^-wp, or the radii dominate)
fn done5(eps: &Mag, flat: bool, wp: u64) -> bool {
    let le = eps.log2();
    le < -1.0 && (6.0 * le + 5.0 < -(wp as f64) - 4.0 || flat)
}

/// R_D(x, y, z) for balls with values x, y >= 0 (at most one zero), z > 0.
pub fn rd_b(x: &Ball, y: &Ball, z: &Ball, wp: u64) -> Option<Ball> {
    let (mut x, mut y, mut z) = (x.clone(), y.clone(), z.clone());
    let mut sum = int(0);
    for k in 0..MAX_STEPS as i64 {
        if k % 8 == 0 && stopped() {
            return None;
        }
        let a = x.add(&y, wp).add(&z.mul(&int(3), wp), wp).div(&int(5), wp)?;
        let (eps, flat) = deviation(&a, &[&x, &y, &z], wp)?;
        if done5(&eps, flat, wp) {
            let xx = a.sub(&x, wp).div(&a, wp)?;
            let yy = a.sub(&y, wp).div(&a, wp)?;
            // X + Y + 3 Z = 0
            let zz = xx.add(&yy, wp).div(&int(-3), wp)?;
            let xy = xx.mul(&yy, wp);
            let z2 = sq(&zz, wp);
            let e2 = xy.sub(&z2.mul(&int(6), wp), wp);
            let e3 = xy.mul(&int(3), wp).sub(&z2.mul(&int(8), wp), wp).mul(&zz, wp);
            let e4 = xy.sub(&z2, wp).mul(&z2, wp).mul(&int(3), wp);
            let e5 = xy.mul(&z2, wp).mul(&zz, wp);
            let tail = finish5([e2, e3, e4, e5], &a, k, &eps, wp)?;
            return Some(sum.mul(&int(3), wp).add(&tail, wp));
        }
        let (lam, [_, _, sz]) = lambda(&x, &y, &z, wp)?;
        let t = int(1).div(&sz.mul(&z.add(&lam, wp), wp), wp)?;
        sum = sum.add(&scale2(&t, -2 * k), wp);
        x = step(&x, &lam, wp);
        y = step(&y, &lam, wp);
        z = step(&z, &lam, wp);
    }
    None
}

/// R_C(1, 1 + t) for a ball t > -1.
fn rc1_b(t: &Ball, wp: u64) -> Option<Ball> {
    let lt = log2_up(t);
    if lt <= (0.75f64).log2() {
        // sum_k (-t)^k/(2k+1), |coefficients| <= 1: the tail after the term t^N is at most |t|^N/(1 - |t|) <= 4|t|^N
        let mt = t.neg();
        let mut pw = int(1);
        let mut s = int(0);
        for k in 0..MAX_STEPS as i64 {
            if k % 32 == 0 && stopped() {
                return None;
            }
            if log2_up(&pw) < -(wp as f64) - 6.0 {
                return Some(super::widen(s, &mag_up(&pw).mul_2exp(2)));
            }
            s = s.add(&pw.div(&int(2 * k + 1), wp)?, wp);
            pw = pw.mul(&mt, wp);
        }
        return None;
    }
    if is_pos(t) {
        let st = el::sqrt(t, wp)?;
        return el::atan(&st, wp)?.div(&st, wp);
    }
    // -1 < t < 0: atanh(s)/s = log((1 + s)/(1 - s))/(2 s), s = sqrt(-t)
    let s = el::sqrt(&t.neg(), wp)?;
    let one = int(1);
    let q = one.add(&s, wp).div(&one.sub(&s, wp), wp)?;
    el::log(&q, wp)?.div(&scale2(&s, 1), wp)
}

/// R_J(x, y, z, p) for balls with values x, y, z >= 0 (at most one zero) and p > 0.
pub fn rj_b(x: &Ball, y: &Ball, z: &Ball, p: &Ball, wp: u64) -> Option<Ball> {
    if !is_pos(p) {
        return None;
    }
    let (mut x, mut y, mut z, mut p) = (x.clone(), y.clone(), z.clone(), p.clone());
    let mut sum = int(0);
    for k in 0..MAX_STEPS as i64 {
        if k % 8 == 0 && stopped() {
            return None;
        }
        let a = x.add(&y, wp).add(&z, wp).add(&p.mul(&int(2), wp), wp).div(&int(5), wp)?;
        let (eps, flat) = deviation(&a, &[&x, &y, &z, &p], wp)?;
        if done5(&eps, flat, wp) {
            let xx = a.sub(&x, wp).div(&a, wp)?;
            let yy = a.sub(&y, wp).div(&a, wp)?;
            let zz = a.sub(&z, wp).div(&a, wp)?;
            // X + Y + Z + 2 P = 0
            let pp = scale2(&xx.add(&yy, wp).add(&zz, wp), -1).neg();
            let xyz = xx.mul(&yy, wp).mul(&zz, wp);
            let p2 = sq(&pp, wp);
            let p3 = p2.mul(&pp, wp);
            let e2 = xx.mul(&yy, wp).add(&xx.mul(&zz, wp), wp).add(&yy.mul(&zz, wp), wp).sub(&p2.mul(&int(3), wp), wp);
            let e3 = xyz.add(&e2.mul(&pp, wp).mul(&int(2), wp), wp).add(&p3.mul(&int(4), wp), wp);
            let e4 = xyz.mul(&int(2), wp).add(&e2.mul(&pp, wp), wp).add(&p3.mul(&int(3), wp), wp).mul(&pp, wp);
            let e5 = xyz.mul(&p2, wp);
            let tail = finish5([e2, e3, e4, e5], &a, k, &eps, wp)?;
            return Some(sum.mul(&int(3), wp).add(&tail, wp));
        }
        let (lam, [sx, sy, sz]) = lambda(&x, &y, &z, wp)?;
        let sp = el::sqrt(&p, wp)?;
        let alpha = p.mul(&sx.add(&sy, wp).add(&sz, wp), wp).add(&sx.mul(&sy, wp).mul(&sz, wp), wp);
        let beta = sp.mul(&p.add(&lam, wp), wp);
        // R_C(alpha^2, beta^2) = R_C(1, 1 + t)/alpha
        let t = beta.sub(&alpha, wp).mul(&beta.add(&alpha, wp), wp).div(&sq(&alpha, wp), wp)?;
        let term = rc1_b(&t, wp)?.div(&alpha, wp)?;
        sum = sum.add(&scale2(&term, -2 * k), wp);
        x = step(&x, &lam, wp);
        y = step(&y, &lam, wp);
        z = step(&z, &lam, wp);
        p = step(&p, &lam, wp);
    }
    None
}

/// R_C(x, y) for x >= 0: y > 0 directly, y < 0 the principal value sqrt(x/(x - y)) R_C(x - y, -y).
fn rc_b(x: &Ball, y: &Ball, wp: u64) -> Option<Ball> {
    if is_pos(y) {
        return rf_b(x, y, y, wp);
    }
    if !is_neg(y) {
        return None;
    }
    if is_exact_zero(x) {
        return Some(int(0));
    }
    let d = x.sub(y, wp);
    let my = y.neg();
    sqrt_nn(&x.div(&d, wp)?, wp)?.mul(&rf_b(&d, &my, &my, wp)?, wp).into()
}

/// The principal value of R_J(x, y, z, p) for p < 0 (Carlson's transformation to
/// p' = y + (z - y)(y - x)/(y - p) > 0 with x <= y <= z, sorted by midpoints).
fn rj_pv_b(x: &Ball, y: &Ball, z: &Ball, p: &Ball, wp: u64) -> Option<Ball> {
    let mut v = [x.clone(), y.clone(), z.clone()];
    v.sort_by(|a, b| a.mid.cmp_exact(&b.mid));
    let [xt, yt, zt] = v;
    if !is_pos(&yt) {
        return None;
    }
    let a = int(1).div(&yt.sub(p, wp), wp)?;
    let b = a.mul(&zt.sub(&yt, wp), wp).mul(&yt.sub(&xt, wp), wp);
    let pt = yt.add(&b, wp);
    let rho = xt.mul(&zt, wp).div(&yt, wp)?;
    let tau = p.mul(&pt, wp).div(&yt, wp)?;
    let rcx = rc_b(&rho, &tau, wp)?;
    let inner =
        b.mul(&rj_b(&xt, &yt, &zt, &pt, wp)?, wp).add(&rcx.sub(&rf_b(&xt, &yt, &zt, wp)?, wp).mul(&int(3), wp), wp);
    Some(a.mul(&inner, wp))
}

// ---------------------------------------------------------------- complete integrals

/// (K(m), E(m)) for balls m < 1, mc = 1 - m > 0, by the AGM with the enclosure b_n <= M <= a_n.
fn agm_b(m: &Ball, mc: &Ball, wp: u64) -> Option<(Ball, Ball)> {
    if !is_pos(mc) {
        return None;
    }
    let one = int(1);
    let b0 = el::sqrt(mc, wp)?;
    let opb = one.add(&b0, wp);
    let mut a = scale2(&opb, -1);
    let mut b = el::sqrt(&b0, wp)?;
    let mut c = m.div(&scale2(&opb, 1), wp)?;
    let mut s = scale2(m, -1).add(&sq(&c, wp), wp);
    for n in 1..MAX_STEPS as i64 {
        if n % 8 == 0 && stopped() {
            return None;
        }
        let m_lo = lower(&b);
        if m_lo.neg || m_lo.is_zero() {
            return None;
        }
        let m_lo = Mag::from_float_down(&m_lo);
        let cm = mag_up(&c);
        // a_n - b_n = c_n^2/(2 a_(n+1)) <= c_n^2/(2 M)
        if mag_le(&cm.mul_2exp(2), &m_lo) && 2.0 * cm.log2() - m_lo.log2() < -(wp as f64) - 8.0 {
            let mm = hull(&lower(&b), &upper(&a), wp);
            let k = pi(wp)?.div(&scale2(&mm, 1), wp)?;
            // the tail of S: at most 2 * 2^n c_(n+1)^2 <= 2^(n+1) (c_n^2/(4 M))^2
            let c1 = cm.mul(&cm).div_up(&m_lo.mul_2exp(2));
            let tail = c1.mul(&c1).mul_2exp(n + 1);
            let e = k.mul(&super::widen(one.sub(&s, wp), &tail), wp);
            return Some((k, e));
        }
        let an = scale2(&a.add(&b, wp), -1);
        b = sqrt_nn(&a.mul(&b, wp), wp)?;
        a = an;
        c = sq(&c, wp).div(&scale2(&a, 2), wp)?;
        s = s.add(&scale2(&sq(&c, wp), n), wp);
    }
    None
}

/// Pi(n | m) for m < 1 (mc = 1 - m > 0), n != 1; `omn` = 1 - n, `omq` = 1 - m/n (used for n > 1).
fn pi_complete_b(n: &Ball, m: &Ball, mc: &Ball, omn: &Ball, omq: impl Fn() -> Option<Ball>, wp: u64) -> Option<Ball> {
    let zero = int(0);
    let one = int(1);
    if is_pos(omn) {
        let (k, _) = agm_b(m, mc, wp)?;
        let r = rj_b(&zero, mc, &one, omn, wp)?;
        return Some(k.add(&r.mul(n, wp).div(&int(3), wp)?, wp));
    }
    if !is_neg(omn) {
        return None;
    }
    // the principal value K(m) - Pi(m/n | m) = -(m/(3n)) R_J(0, mc, 1, 1 - m/n)
    let q = m.div(n, wp)?;
    let r = rj_b(&zero, mc, &one, &omq()?, wp)?;
    q.mul(&r, wp).div(&int(-3), wp)
}

// ---------------------------------------------------------------- incomplete integrals

/// phi = j pi + psi with |psi| < pi/2 (proved): j, sin psi, cos psi.
struct Phase {
    j: i64,
    s: Ball,
    c: Ball,
}

fn reduce(phi: &Ball, wp: u64) -> Option<Phase> {
    let w = wp + phi.mid.top().max(0) as u64 + 8;
    let pw = pi(w)?;
    let t = phi.div(&pw, w)?;
    let j = floor_float(&t.mid.add_exact(&Float::new(false, Nat::from_u64(1), -1)))?;
    if j.unsigned_abs() > 1 << 50 {
        return None;
    }
    let psi = phi.sub(&pw.mul(&int(j), w), w);
    let hp = scale2(&pw, -1);
    let apsi = Ball { mid: psi.mid.abs(), rad: psi.rad };
    if apsi.lt(&hp) != Some(true) {
        return None;
    }
    let (s, c) = el::sin_cos(&psi, wp)?;
    Some(Phase { j, s, c })
}

/// Delta^2 = 1 - m sin^2 psi: c^2 + mc s^2 when mc >= 0 (values >= 0 by construction), otherwise provably positive
/// and only for j = 0.
fn delta2(ph: &Phase, m: &Ball, mc: &Ball, wp: u64) -> Option<Ball> {
    let s2 = sq(&ph.s, wp);
    if !lower(mc).neg {
        return Some(sq(&ph.c, wp).add(&mc.mul(&s2, wp), wp));
    }
    if ph.j != 0 {
        return None;
    }
    let d = int(1).sub(&m.mul(&s2, wp), wp);
    is_pos(&d).then_some(d)
}

/// F(phi | m) on balls.
fn f_b(phi: &Ball, m: &Ball, mc: &Ball, wp: u64) -> Option<Ball> {
    let ph = reduce(phi, wp)?;
    let d2 = delta2(&ph, m, mc, wp)?;
    let mut v = ph.s.mul(&rf_b(&sq(&ph.c, wp), &d2, &int(1), wp)?, wp);
    if ph.j != 0 {
        let (k, _) = agm_b(m, mc, wp)?;
        v = v.add(&k.mul(&int(2 * ph.j), wp), wp);
    }
    Some(v)
}

/// E(phi | m) on balls.
fn e_inc_b(phi: &Ball, m: &Ball, mc: &Ball, wp: u64) -> Option<Ball> {
    let ph = reduce(phi, wp)?;
    let d2 = delta2(&ph, m, mc, wp)?;
    let c2 = sq(&ph.c, wp);
    let one = int(1);
    let s3 = sq(&ph.s, wp).mul(&ph.s, wp);
    let f = ph.s.mul(&rf_b(&c2, &d2, &one, wp)?, wp);
    let d = s3.mul(&rd_b(&c2, &d2, &one, wp)?, wp).mul(m, wp).div(&int(3), wp)?;
    let mut v = f.sub(&d, wp);
    if ph.j != 0 {
        let e = if is_exact_zero(mc) { one } else { agm_b(m, mc, wp)?.1 };
        v = v.add(&e.mul(&int(2 * ph.j), wp), wp);
    }
    Some(v)
}

// ---------------------------------------------------------------- entry points

/// The Carlson arguments as balls, each required to be >= 0 (provably, or an exact zero).
fn nonneg_args(rs: &[&Real], wp: u64) -> Option<Vec<Ball>> {
    let bs: Option<Vec<Ball>> = rs.iter().map(|r| super::real_ball(r, wp)).collect();
    let bs = bs?;
    bs.iter().all(|b| is_exact_zero(b) || is_pos(b)).then_some(bs)
}

/// Carlson's R_F(x, y, z), x, y, z >= 0 with at most one zero.
pub fn carlson_rf(x: &Real, y: &Real, z: &Real, p: u64) -> Option<CBall> {
    refine(p, 24, |wp| {
        let b = nonneg_args(&[x, y, z], wp)?;
        Some(real(rf_b(&b[0], &b[1], &b[2], wp)?))
    })
}

/// Carlson's R_D(x, y, z), x, y >= 0 with at most one zero, z > 0.
pub fn carlson_rd(x: &Real, y: &Real, z: &Real, p: u64) -> Option<CBall> {
    refine(p, 24, |wp| {
        let b = nonneg_args(&[x, y, z], wp)?;
        if !is_pos(&b[2]) {
            return None;
        }
        Some(real(rd_b(&b[0], &b[1], &b[2], wp)?))
    })
}

/// Carlson's R_J(x, y, z, p), x, y, z >= 0 with at most one zero, p != 0 (the Cauchy principal value for p < 0).
pub fn carlson_rj(x: &Real, y: &Real, z: &Real, q: &Real, p: u64) -> Option<CBall> {
    refine(p, 32, |wp| {
        let b = nonneg_args(&[x, y, z], wp)?;
        let pb = super::real_ball(q, wp)?;
        let r = if is_pos(&pb) { rj_b(&b[0], &b[1], &b[2], &pb, wp)? } else { rj_pv_b(&b[0], &b[1], &b[2], &pb, wp)? };
        Some(real(r))
    })
}

/// Carlson's R_C(x, y) = R_F(x, y, y), x >= 0, y != 0 (the Cauchy principal value for y < 0).
pub fn carlson_rc(x: &Real, y: &Real, p: u64) -> Option<CBall> {
    refine(p, 24, |wp| {
        let b = nonneg_args(&[x], wp)?;
        Some(real(rc_b(&b[0], &super::real_ball(y, wp)?, wp)?))
    })
}

/// The complete integral of the first kind K(m), m < 1.
pub fn elliptic_k(m: &Real, p: u64) -> Option<CBall> {
    refine(p, 24, |wp| {
        let ma = arg(m, wp)?;
        let mc = one_minus(&ma, wp);
        Some(real(agm_b(&ma.b, &mc, wp)?.0))
    })
}

/// The complete integral of the second kind E(m), m <= 1 (E(1) = 1).
pub fn elliptic_e(m: &Real, p: u64) -> Option<CBall> {
    refine(p, 24, |wp| {
        let ma = arg(m, wp)?;
        if is_one(&ma) {
            return Some(real(int(1)));
        }
        let mc = one_minus(&ma, wp);
        Some(real(agm_b(&ma.b, &mc, wp)?.1))
    })
}

/// The incomplete integral of the first kind F(phi | m): any real phi for m < 1, |phi| < pi/2 with m sin^2 phi < 1
/// otherwise.
pub fn elliptic_f(phi: &Real, m: &Real, p: u64) -> Option<CBall> {
    refine(p, 24, |wp| {
        let ph = super::real_ball(phi, wp)?;
        let ma = arg(m, wp)?;
        let mc = one_minus(&ma, wp);
        Some(real(f_b(&ph, &ma.b, &mc, wp)?))
    })
}

/// The incomplete integral of the second kind E(phi | m), with the conventions of [`elliptic_f`] (any phi for
/// m = 1).
pub fn elliptic_e_inc(phi: &Real, m: &Real, p: u64) -> Option<CBall> {
    refine(p, 32, |wp| {
        let ph = super::real_ball(phi, wp)?;
        let ma = arg(m, wp)?;
        let mc = one_minus(&ma, wp);
        Some(real(e_inc_b(&ph, &ma.b, &mc, wp)?))
    })
}

/// 1 - m/n, exact for rationals.
fn one_minus_ratio(m: &Arg, n: &Arg, wp: u64) -> Option<Ball> {
    match (m.q, n.q) {
        (Some(mq), Some(nq)) => Some(rat_ball(&Rat::from_i64(1).sub(&mq.mul(&nq.recip()?)), wp)),
        _ => Some(int(1).sub(&m.b.div(&n.b, wp)?, wp)),
    }
}

/// The complete integral of the third kind Pi(n | m) for m < 1, n != 1 (the Cauchy principal value for n > 1).
pub fn elliptic_pi(n: &Real, m: &Real, p: u64) -> Option<CBall> {
    refine(p, 32, |wp| {
        let na = arg(n, wp)?;
        let ma = arg(m, wp)?;
        let mc = one_minus(&ma, wp);
        let omn = one_minus(&na, wp);
        Some(real(pi_complete_b(&na.b, &ma.b, &mc, &omn, || one_minus_ratio(&ma, &na, wp), wp)?))
    })
}

/// The incomplete integral of the third kind Pi(n; phi | m), with the conventions of [`elliptic_f`]; the Cauchy
/// principal value where n sin^2 t passes 1.
pub fn elliptic_pi_inc(n: &Real, phi: &Real, m: &Real, p: u64) -> Option<CBall> {
    refine(p, 32, |wp| {
        let na = arg(n, wp)?;
        let ph = super::real_ball(phi, wp)?;
        let ma = arg(m, wp)?;
        let mc = one_minus(&ma, wp);
        let red = reduce(&ph, wp)?;
        let d2 = delta2(&red, &ma.b, &mc, wp)?;
        let (c2, s2) = (sq(&red.c, wp), sq(&red.s, wp));
        let one = int(1);
        let pn = one.sub(&na.b.mul(&s2, wp), wp);
        let r = if is_pos(&pn) {
            rj_b(&c2, &d2, &one, &pn, wp)?
        } else if is_neg(&pn) {
            rj_pv_b(&c2, &d2, &one, &pn, wp)?
        } else {
            return None;
        };
        let f = red.s.mul(&rf_b(&c2, &d2, &one, wp)?, wp);
        let mut v = f.add(&s2.mul(&red.s, wp).mul(&r, wp).mul(&na.b, wp).div(&int(3), wp)?, wp);
        if red.j != 0 {
            let omn = one_minus(&na, wp);
            let pc = pi_complete_b(&na.b, &ma.b, &mc, &omn, || one_minus_ratio(&ma, &na, wp), wp)?;
            v = v.add(&pc.mul(&int(2 * red.j), wp), wp);
        }
        Some(real(v))
    })
}

/// The amplitude am(u | m) for m < 1 (mc = 1 - m > 0) as a proved enclosure.
fn am_b(u: &Ball, m: &Ball, mc: &Ball, wp: u64) -> Option<Ball> {
    if is_exact_zero(u) {
        return Some(int(0));
    }
    let (uf, mf, mcf) = (f64_of(&u.mid), f64_of(&m.mid), f64_of(&mc.mid));
    if !uf.is_finite() || !mf.is_finite() || mcf.is_nan() || mcf <= 0.0 {
        return None;
    }
    let start = crate::machine::elliptic::amplitude(uf, mf, mcf);
    if !start.is_finite() {
        return None;
    }
    // Newton on F(phi) = u (non-rigorous: only the midpoints are kept), at doubling precisions
    let mut phi = float_of(start);
    let top = wp + 16;
    let mut q = 64u64;
    let mut finals = 0;
    while finals < 2 {
        if stopped() {
            return None;
        }
        let qq = q.min(top);
        let pb = Ball::exact(phi.clone());
        let fv = f_b(&pb, m, mc, qq)?;
        let s = el::sin(&pb, qq)?;
        let dn = sqrt_nn(&int(1).sub(&m.mul(&sq(&s, qq), qq), qq), qq)?;
        let corr = fv.sub(u, qq).mul(&dn, qq);
        phi = el::round(Ball::exact(phi.sub_exact(&corr.mid)), qq).mid;
        if qq == top {
            finals += 1;
        }
        q *= 2;
    }
    // the proof: F(phi - d) < u < F(phi + d) for all u and m in their balls (F increasing for m < 1)
    let mut d = Mag::pow2(phi.top().max(0) - wp as i64 + 8);
    let dn_max = if mcf > 1.0 { mcf.sqrt().log2().ceil() as i64 + 1 } else { 1 };
    d = d.add(&u.rad.mul_2exp(dn_max));
    let (u_lo, u_hi) = (lower(u), upper(u));
    for _ in 0..10 {
        let df = d.to_float();
        let (lo, hi) = (phi.sub_exact(&df), phi.add_exact(&df));
        let below = f_b(&Ball::exact(lo.clone()), m, mc, wp).map(|f| upper(&f).cmp_exact(&u_lo) == Ordering::Less);
        let above = f_b(&Ball::exact(hi.clone()), m, mc, wp).map(|f| lower(&f).cmp_exact(&u_hi) == Ordering::Greater);
        if below == Some(true) && above == Some(true) {
            return Some(hull(&lo, &hi, wp));
        }
        d = d.mul_2exp(4);
    }
    None
}

/// (sn, cn, dn) for m < 1 from the amplitude.
fn sncndn_core(u: &Ball, m: &Ball, mc: &Ball, wp: u64) -> Option<(Ball, Ball, Ball)> {
    let am = am_b(u, m, mc, wp)?;
    let (s, c) = el::sin_cos(&am, wp)?;
    let dn = sqrt_nn(&sq(&c, wp).add(&mc.mul(&sq(&s, wp), wp), wp), wp)?;
    Some((s, c, dn))
}

/// (sn, cn, dn)(u | m) on balls.
fn sncndn_b(u: &Real, m: &Real, wp: u64) -> Option<(Ball, Ball, Ball)> {
    let ub = super::real_ball(u, wp)?;
    let ma = arg(m, wp)?;
    if is_one(&ma) {
        let t = el::tanh(&ub, wp)?;
        let s = int(1).div(&el::cosh(&ub, wp)?, wp)?;
        return Some((t, s.clone(), s));
    }
    let mc = one_minus(&ma, wp);
    if is_pos(&mc) {
        // sn(u + 2K) = -sn(u), cn(u + 2K) = -cn(u), dn(u + 2K) = dn(u) (DLMF 22.4.1-3): beyond a few periods the
        // amplitude's Newton iteration cannot start from a double (D23-22, n(jacobi_sn(10^20, 1/2), 20) did not
        // return), so u is reduced modulo 2K with K computed at the bits the reduction consumes
        let uf = f64_of(&ub.mid).abs();
        if uf > 8.0 && uf.is_finite() {
            let k = rf_b(&int(0), &mc, &int(1), wp + 16)?;
            let n = super::cspecial::nearest_int(&ub.div(&k.mul(&int(2), wp + 16), wp + 16)?.mid);
            if !n.is_zero() {
                let wp2 = wp + n.mag.bits() + 16;
                let ma2 = arg(m, wp2)?;
                let mc2 = one_minus(&ma2, wp2);
                let ub2 = super::real_ball(u, wp2)?;
                let kb = rf_b(&int(0), &mc2, &int(1), wp2)?;
                let nb = if n.neg { Ball::from_nat(n.mag.clone()).neg() } else { Ball::from_nat(n.mag.clone()) };
                let ured = el::round(ub2.sub(&kb.mul(&nb.mul(&int(2), wp2), wp2), wp2), wp + 16);
                let (s, c, d) = sncndn_core(&ured, &ma.b, &mc, wp)?;
                return Some(if n.mag.low_bits_nonzero(1) { (s.neg(), c.neg(), d) } else { (s, c, d) });
            }
        }
        return sncndn_core(&ub, &ma.b, &mc, wp);
    }
    if !is_neg(&mc) {
        return None;
    }
    // m > 1: mu = 1/m, v = u sqrt(m): sn = sn(v|mu)/sqrt(m), cn = dn(v|mu), dn = cn(v|mu)
    let rt = el::sqrt(&ma.b, wp)?;
    let (mu, muc) = match ma.q {
        Some(q) => {
            let r = q.recip()?;
            (rat_ball(&r, wp), rat_ball(&Rat::from_i64(1).sub(&r), wp))
        }
        None => {
            let r = int(1).div(&ma.b, wp)?;
            (r.clone(), int(1).sub(&r, wp))
        }
    };
    let (s, c, d) = sncndn_core(&ub.mul(&rt, wp), &mu, &muc, wp)?;
    Some((s.div(&rt, wp)?, d, c))
}

/// The extra working bits of the Jacobi functions: the amplitude's absolute error is relative to |u|.
fn jacobi_guard(u: &Real) -> u64 {
    let b = match u {
        Real::Exact(q) => rat_ball(q, 64),
        Real::Ball(b, _) => b.clone(),
        Real::Machine(_) => return 32,
    };
    32 + log2_up(&b).clamp(0.0, 1e6) as u64
}

/// The Jacobi elliptic function sn(u | m), any real u and m.
pub fn jacobi_sn(u: &Real, m: &Real, p: u64) -> Option<CBall> {
    refine(p, jacobi_guard(u), |wp| Some(real(sncndn_b(u, m, wp)?.0)))
}

/// The Jacobi elliptic function cn(u | m), any real u and m.
pub fn jacobi_cn(u: &Real, m: &Real, p: u64) -> Option<CBall> {
    refine(p, jacobi_guard(u), |wp| Some(real(sncndn_b(u, m, wp)?.1)))
}

/// The Jacobi elliptic function dn(u | m), any real u and m.
pub fn jacobi_dn(u: &Real, m: &Real, p: u64) -> Option<CBall> {
    refine(p, jacobi_guard(u), |wp| Some(real(sncndn_b(u, m, wp)?.2)))
}

/// The inverse of sn: F(asin v | m) = v R_F(1 - v^2, 1 - m v^2, 1) for |v| <= 1 and m v^2 <= 1 (not both zero).
pub fn inverse_jacobi_sn(v: &Real, m: &Real, p: u64) -> Option<CBall> {
    refine(p, 24, |wp| {
        let va = arg(v, wp)?;
        let ma = arg(m, wp)?;
        let (x, y) = match (va.q, ma.q) {
            (Some(vq), Some(mq)) => {
                let v2 = vq.mul(vq);
                let one = Rat::from_i64(1);
                let (x, y) = (one.sub(&v2), one.sub(&mq.mul(&v2)));
                if x.num.neg || y.num.neg || (x.is_zero() && y.is_zero()) {
                    return None;
                }
                (rat_ball(&x, wp), rat_ball(&y, wp))
            }
            _ => {
                let v2 = sq(&va.b, wp);
                // |v| <= 1 for every point, and m v^2 <= 1: provably, or by construction when m <= 1
                let av = Ball { mid: va.b.mid.abs(), rad: va.b.rad };
                if upper(&av).cmp_exact(&Float::from_i64(1)) == Ordering::Greater {
                    return None;
                }
                let x = int(1).sub(&v2, wp);
                let y = int(1).sub(&ma.b.mul(&v2, wp), wp);
                if upper(&ma.b).cmp_exact(&Float::from_i64(1)) == Ordering::Greater && !is_pos(&y) {
                    return None;
                }
                (x, y)
            }
        };
        Some(real(va.b.mul(&rf_b(&x, &y, &int(1), wp)?, wp)))
    })
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
        // K(1/2) to 60 digits, E(1/2) = 1.35064388104767550252017473533872584134952236692... to 45
        let k = elliptic_k(&q(1, 2), 220).unwrap().re;
        assert_eq!(digits(&k, 60), "185407467730137191843385034719526004621759882352176690558593e0");
        let e = elliptic_e(&q(1, 2), 220).unwrap().re;
        assert_eq!(digits(&e, 45), "135064388104767550252017473533872584134952237e0");
        // Carlson's test values and principal values
        let b = |n: i64| int(n);
        let w = 200;
        let rf = rf_b(&b(1), &b(2), &b(0), w).unwrap();
        assert_eq!(digits(&rf, 30), "131102877714605990523241979495e0");
        let rd = rd_b(&b(0), &b(2), &b(1), w).unwrap();
        assert_eq!(digits(&rd, 30), "179721035210338831115988373842e0");
        let rj = rj_b(&b(0), &b(1), &b(2), &b(3), w).unwrap();
        assert_eq!(digits(&rj, 30), "776886237785823320141902826405e-1");
        let pv = rj_pv_b(&b(1), &b(2), &b(3), &b(-1), w).unwrap();
        assert_eq!(digits(&pv, 30), "-932404524386764120148090381153e-2");
        let rc = rc_b(&b(1), &b(-2), w).unwrap();
        assert_eq!(digits(&rc, 30), "380172998150473173765547127440e-1");
        // Pi(3 | 1/2) (principal value), Pi(-2 | 1/3), F(7/2 | 1/3), sn(3 | 7/10), Pi(3; 2 | 1/2)
        let v = elliptic_pi(&q(3, 1), &q(1, 2), 200).unwrap().re;
        assert_eq!(digits(&v, 30), "-192988208490139329156209148562e-1");
        let v = elliptic_pi(&q(-2, 1), &q(1, 3), 200).unwrap().re;
        assert_eq!(digits(&v, 25), "9744840179267495832003095e-1");
        let v = elliptic_f(&q(7, 2), &q(1, 3), 200).unwrap().re;
        assert_eq!(digits(&v, 30), "382878148127304978767573055181e0");
        let v = jacobi_sn(&q(3, 1), &q(7, 10), 200).unwrap().re;
        assert_eq!(digits(&v, 30), "849605564618232140574057894050e-1");
        let v = elliptic_pi_inc(&q(3, 1), &q(2, 1), &q(1, 2), 200).unwrap().re;
        assert_eq!(digits(&v, 25), "-5187284939303218479137386e-1");
        let v = jacobi_dn(&q(1, 1), &q(3, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 25), "1523645758099074179791477e-3");
        let v = jacobi_cn(&q(1, 1), &q(-3, 1), 200).unwrap().re;
        assert_eq!(digits(&v, 25), "1554067037071299762977459e-1");
        // domains
        assert!(elliptic_k(&q(1, 1), 64).is_none() && elliptic_k(&q(3, 2), 64).is_none());
        assert!(elliptic_f(&q(2, 1), &q(2, 1), 64).is_none());
        assert_eq!(digits(&elliptic_e(&q(1, 1), 64).unwrap().re, 5), "10000e0");
        assert!(elliptic_pi(&q(1, 1), &q(1, 2), 64).is_none());
        assert!(inverse_jacobi_sn(&q(3, 2), &q(1, 2), 64).is_none());
    }

    #[test]
    fn identities() {
        let p = 300;
        // Legendre's relation E K' + E' K - K K' = pi/2 at m = 1/3
        let (k, e) = (elliptic_k(&q(1, 3), p).unwrap().re, elliptic_e(&q(1, 3), p).unwrap().re);
        let (k1, e1) = (elliptic_k(&q(2, 3), p).unwrap().re, elliptic_e(&q(2, 3), p).unwrap().re);
        let l = e.mul(&k1, p).add(&e1.mul(&k, p), p).sub(&k.mul(&k1, p), p);
        let d = l.sub(&scale2(&pi(p).unwrap(), -1), p);
        assert!(d.contains_zero() && d.rad.log2() < -(p as f64) + 8.0);
        // sn^2 + cn^2 = 1, dn^2 + m sn^2 = 1, F(asin sn(u)) = u at u = 5/2, m = 9/10
        let (u, m) = (q(5, 2), q(9, 10));
        let (s, c, dn) = sncndn_b(&u, &m, p).unwrap();
        let one = int(1);
        assert!(sq(&s, p).add(&sq(&c, p), p).sub(&one, p).contains_zero());
        let mb = rat_ball(&Rat::new(Int::from_i64(9), Int::from_i64(10)), p);
        assert!(sq(&dn, p).add(&mb.mul(&sq(&s, p), p), p).sub(&one, p).contains_zero());
        let sv = Real::Ball(s, p);
        let inv = inverse_jacobi_sn(&sv, &m, 200).unwrap().re;
        // u = 5/2 is below K(9/10) = 2.5780..., so F(asin sn u | m) = u
        assert!(inv.sub(&rat_ball(&Rat::new(Int::from_i64(5), Int::from_i64(2)), p), 200).contains_zero());
        // E(phi + pi | m) = E(phi | m) + 2 E(m) and F likewise with 2 K(m), at phi = 1/3
        let w = 200;
        let ep = elliptic_e_inc(&q(1, 3), &q(1, 3), w).unwrap().re;
        let pi_plus =
            Real::Ball(rat_ball(&Rat::new(Int::from_i64(1), Int::from_i64(3)), 400).add(&pi(400).unwrap(), 400), 400);
        let e2 = elliptic_e_inc(&pi_plus, &q(1, 3), w).unwrap().re;
        let em = elliptic_e(&q(1, 3), w).unwrap().re;
        assert!(e2.sub(&ep, w).sub(&scale2(&em, 1), w).contains_zero());
        let fp = elliptic_f(&q(1, 3), &q(1, 3), w).unwrap().re;
        let f2 = elliptic_f(&pi_plus, &q(1, 3), w).unwrap().re;
        let km = elliptic_k(&q(1, 3), w).unwrap().re;
        assert!(f2.sub(&fp, w).sub(&scale2(&km, 1), w).contains_zero());
    }
}
