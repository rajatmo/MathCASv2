//! Embeddings: which complex root of a square-free polynomial over Q is meant, as a certified isolating disk.
//!
//! The roots are approximated together by the Ehrlich–Aberth iteration in ball midpoint arithmetic at a working
//! precision that doubles until the result is certified; each approximation `z` then gets the inclusion disk of
//! radius `d |f(z)| / |f'(z)|` (from `f'/f = sum 1/(z - z_i)`, the disk contains a root; Henrici, *Applied and
//! Computational Complex Analysis* I, section 6.4), evaluated with balls. When the `d` disks are pairwise disjoint,
//! each holds exactly one root. A disk whose mirror image meets no other disk holds a real root (the conjugate of
//! its root lies in the mirror image, hence in the same disk); its real interval is then the enclosure.
//!
//! The order is the order of root objects `root(x => f(x), k)`: real roots ascending, then the others by real part
//! and imaginary part, conjugate pairs with the negative imaginary part first.
//!
//! An [`Embedding`] keeps the disk; [`enclose`] refines the root inside it on demand by Newton's method at a higher
//! precision, certifying that the new disk lies in the old one, so the root meant never changes.
use crate::qpoly::{self, q, QPoly};
use mc_arith::num::{Ball, Float, Int, Mag, Nat, Rat};
use mc_numeric::cball::CBall;
use mc_poly::work::{Meter, WorkResult};
use std::cmp::Ordering;

/// One root of a square-free polynomial: its place in the order of root objects and an isolating disk.
#[derive(Clone, Debug, PartialEq)]
pub struct Embedding {
    /// the 1-based index in the order of root objects
    pub index: usize,
    /// is the root real?
    pub real: bool,
    /// the real part of the disk's centre (a dyadic rational)
    pub re: Rat,
    /// the imaginary part of the centre (0 for a real root)
    pub im: Rat,
    /// the radius: the disk holds exactly this root of the polynomial
    pub radius: Rat,
}

/// The highest working precision tried (bits).
const MAX_BITS: u64 = 1 << 15;

// ------------------------------------------------------------------------------------------- conversions

/// A rational as a ball at precision `w`.
pub fn rat_ball(c: &Rat, w: u64) -> Ball {
    let num = Ball::exact(Float::new(c.num.neg, c.num.mag.clone(), 0));
    if c.den == Nat::from_u64(1) {
        return num;
    }
    // invariant: a canonical denominator is positive; after a stop its ball is junk and the quotient may fail
    // (the value is discarded: the caller sees the stop)
    match num.div(&Ball::from_nat(c.den.clone()), w) {
        Some(q) => q,
        None => {
            assert!(mc_arith::num::stopped(), "a positive denominator");
            num
        }
    }
}

/// A dyadic float as a rational.
pub fn float_rat(x: &Float) -> Rat {
    let m = Int::new(x.neg, x.man.clone());
    if x.exp >= 0 {
        Rat::from_int(m.shl(x.exp as u64))
    } else {
        Rat::new(m, Int::from_nat(Nat::pow2((-x.exp) as u64)))
    }
}

/// A radius as a rational.
fn mag_rat(r: &Mag) -> Rat {
    float_rat(&r.to_float())
}

/// The point `re + i im` (exact).
fn point(re: &Float, im: &Float) -> CBall {
    CBall::new(Ball::exact(re.clone()), Ball::exact(im.clone()))
}

/// A rational as a float at precision `w` (rounded).
fn rat_float(c: &Rat, w: u64) -> Float {
    rat_ball(c, w).mid
}

/// The midpoint of a ball rounded to `w` bits.
fn round_mid(b: &Ball, w: u64) -> Float {
    Float::round(b.mid.neg, b.mid.man.clone(), b.mid.exp, w).0
}

/// An upper bound of |b| (a rational).
fn abs_up(b: &Ball) -> Rat {
    float_rat(&b.mid.abs()).add(&mag_rat(&b.rad))
}

/// A lower bound of |b| (0 when the ball contains 0).
fn abs_down(b: &Ball) -> Rat {
    let v = float_rat(&b.mid.abs()).sub(&mag_rat(&b.rad));
    if v.num.neg {
        q(0)
    } else {
        v
    }
}

/// A rational upper bound of `sqrt(x)` for `x >= 0`, a little above it.
fn sqrt_up(x: &Rat) -> Rat {
    if x.is_zero() {
        return q(0);
    }
    // through machine floats on a scaled value, then checked exactly
    let f = rat_f64(x);
    let mut r = if f.is_finite() && f > 0.0 { f64_rat(f.sqrt() * (1.0 + 1e-12)) } else { q(1) };
    if r.is_zero() {
        r = q(1);
    }
    // bisection-free repair: double until r^2 >= x
    let mut guard = 0;
    while r.mul(&r).cmp_value(x) == Ordering::Less && guard < 4096 {
        r = r.mul(&q(2));
        guard += 1;
    }
    r
}

/// A rational as a machine float (approximately; for starting values and ordering only).
pub fn rat_f64(x: &Rat) -> f64 {
    mc_arith::num::float::ratio_to_f64(x.num.neg, &x.num.mag, &x.den)
}

/// A machine float as an exact rational (0 for non-finite values).
fn f64_rat(v: f64) -> Rat {
    match mc_arith::num::float::f64_to_dyadic(v) {
        Some((neg, man, exp)) => float_rat(&Float::new(neg, man, exp)),
        None => q(0),
    }
}

// ------------------------------------------------------------------------------------------- evaluation

/// `(f(z), f'(z))` with balls at precision `w` (coefficients given as balls).
fn eval2(cs: &[Ball], z: &CBall, w: u64) -> (CBall, CBall) {
    let zero = CBall::from_i64(0);
    let mut v = zero.clone();
    let mut dv = zero;
    for c in cs.iter().rev() {
        dv = dv.mul(z, w).add(&v, w);
        v = v.mul(z, w).add(&CBall::real(c.clone()), w);
    }
    (v, dv)
}

/// `a(z)` for a rational polynomial and a complex ball.
pub fn eval_ball(a: &[Rat], z: &CBall, w: u64) -> CBall {
    let mut v = CBall::from_i64(0);
    for c in a.iter().rev() {
        v = v.mul(z, w).add(&CBall::real(rat_ball(c, w)), w);
    }
    v
}

/// The coefficient balls of `f` at precision `w`.
fn coef_balls(f: &[Rat], w: u64) -> Vec<Ball> {
    f.iter().map(|c| rat_ball(c, w)).collect()
}

/// An upper bound of the radius `d |f(z)| / |f'(z)|` of the inclusion disk at the exact point `z`; `None` when
/// `f'(z)` cannot be bounded away from 0.
fn inclusion_radius(cs: &[Ball], z: &CBall, w: u64) -> Option<Rat> {
    let d = cs.len() as i64 - 1;
    let (v, dv) = eval2(cs, z, w);
    let num2 = sq(&abs_up(&v.re)).add(&sq(&abs_up(&v.im)));
    let den2 = sq(&abs_down(&dv.re)).add(&sq(&abs_down(&dv.im)));
    if den2.is_zero() {
        return None;
    }
    let r2 = num2.mul(&q(d * d)).mul(&den2.recip()?);
    Some(sqrt_up(&r2))
}

fn sq(x: &Rat) -> Rat {
    x.mul(x)
}

// ------------------------------------------------------------------------------------------- isolation

/// An approximation with its certified disk.
#[derive(Clone)]
struct Disk {
    re: Rat,
    im: Rat,
    r: Rat,
}

impl Disk {
    /// Do the closed disks `self` and `o` (mirrored when `mirror`) meet?
    fn meets(&self, o: &Disk, mirror: bool) -> bool {
        let oim = if mirror { o.im.neg() } else { o.im.clone() };
        let dx = self.re.sub(&o.re);
        let dy = self.im.sub(&oim);
        let s = self.r.add(&o.r);
        sq(&dx).add(&sq(&dy)).cmp_value(&sq(&s)) != Ordering::Greater
    }
}

/// The roots of a square-free `f` of positive degree, in the order of root objects; `None` when they could not be
/// certified within the precision limit.
pub fn roots(f: &[Rat], m: &mut Meter) -> WorkResult<Option<Vec<Embedding>>> {
    let f = qpoly::monic(&qpoly::trim(f.to_vec()));
    let d = f.len() - 1;
    if d == 0 {
        return Ok(Some(vec![]));
    }
    if d == 1 {
        let r = f[0].neg();
        return Ok(Some(vec![Embedding { index: 1, real: true, re: r, im: q(0), radius: q(0) }]));
    }
    let mut w = 64u64;
    let mut zs = start_points(&f, w);
    while w <= MAX_BITS {
        m.step((d * d) as u64 * (w / 64 + 1))?;
        aberth(&f, &mut zs, w, m)?;
        if let Some(out) = certify(&f, &zs, w, m)? {
            return Ok(Some(out));
        }
        w *= 2;
        zs = zs.iter().map(|z| point(&round_mid(&z.re, w), &round_mid(&z.im, w))).collect();
    }
    Ok(None)
}

/// Starting points on a circle of the Cauchy bound's radius, rotated off the axes.
fn start_points(f: &[Rat], w: u64) -> Vec<CBall> {
    let d = f.len() - 1;
    let mut bound = 0.0f64;
    for c in &f[..d] {
        let v = rat_f64(c).abs();
        bound = bound.max(v.powf(1.0 / d as f64));
    }
    let r = (1.0 + bound).min(1e300) * 0.9;
    (0..d)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / d as f64 + 0.4;
            let re = rat_float(&f64_rat(r * a.cos()), w);
            let im = rat_float(&f64_rat(r * a.sin()), w);
            point(&re, &im)
        })
        .collect()
}

/// Ehrlich–Aberth iterations at precision `w` until the corrections stop shrinking or are tiny.
fn aberth(f: &[Rat], zs: &mut [CBall], w: u64, m: &mut Meter) -> WorkResult<()> {
    let d = zs.len();
    let cs = coef_balls(f, w);
    let one = CBall::from_i64(1);
    let max_iter = 60 + 4 * d;
    for _ in 0..max_iter {
        m.step((d * d) as u64)?;
        let mut moved = false;
        for i in 0..d {
            let (v, dv) = eval2(&cs, &zs[i], w);
            let Some(ratio) = v.div(&dv, w) else {
                // a critical point: nudge
                let eps = point(&rat_float(&f64_rat(1e-3), w), &rat_float(&f64_rat(1e-3), w));
                zs[i] = zs[i].add(&eps, w);
                moved = true;
                continue;
            };
            let mut s = CBall::from_i64(0);
            for j in 0..d {
                if j != i {
                    if let Some(inv) = one.div(&zs[i].sub(&zs[j], w), w) {
                        s = s.add(&inv, w);
                    }
                }
            }
            let denom = one.sub(&ratio.mul(&s, w), w);
            let corr = ratio.div(&denom, w).unwrap_or(ratio);
            let z = zs[i].sub(&corr, w);
            let new = point(&round_mid(&z.re, w), &round_mid(&z.im, w));
            // a correction above 2^-(w - 8) of the size of z counts as movement
            if !small(&corr, &new, w) {
                moved = true;
            }
            zs[i] = new;
        }
        if !moved {
            break;
        }
    }
    Ok(())
}

/// Is the correction `c` below `2^-(w - 8) (|z| + 2^-w)`?
fn small(c: &CBall, z: &CBall, w: u64) -> bool {
    let mag = |b: &Ball| if b.mid.is_zero() { i64::MIN / 2 } else { b.mid.top() };
    let cm = mag(&c.re).max(mag(&c.im));
    let zm = mag(&z.re).max(mag(&z.im)).max(-(w as i64));
    cm < zm - (w as i64 - 8)
}

/// Certified disks for the approximations, ordered; `None` when they do not separate.
fn certify(f: &[Rat], zs: &[CBall], w: u64, m: &mut Meter) -> WorkResult<Option<Vec<Embedding>>> {
    let d = zs.len();
    let cs = coef_balls(f, 2 * w);
    let mut disks = Vec::with_capacity(d);
    for z in zs {
        m.step(d as u64)?;
        let Some(r) = inclusion_radius(&cs, z, 2 * w) else { return Ok(None) };
        disks.push(Disk { re: float_rat(&z.re.mid), im: float_rat(&z.im.mid), r });
    }
    for i in 0..d {
        for j in i + 1..d {
            if disks[i].meets(&disks[j], false) {
                return Ok(None);
            }
        }
    }
    // real roots: the mirror image meets no other disk
    let mut real = vec![false; d];
    for i in 0..d {
        let near_axis = disks[i].im.num.mag.bits() == 0 || sq(&disks[i].im).cmp_value(&sq(&disks[i].r)) != Ordering::Greater;
        if !near_axis {
            continue;
        }
        if (0..d).any(|j| j != i && disks[i].meets(&disks[j], true)) {
            return Ok(None);
        }
        real[i] = true;
    }
    // the real intervals must be disjoint for the order
    let mut reals: Vec<Disk> = (0..d).filter(|&i| real[i]).map(|i| disks[i].clone()).collect();
    reals.sort_by(|a, b| a.re.cmp_value(&b.re));
    for p in reals.windows(2) {
        if p[0].re.add(&p[0].r).cmp_value(&p[1].re.sub(&p[1].r)) != Ordering::Less {
            return Ok(None);
        }
    }
    // conjugate pairs: a non-real disk whose mirror meets exactly one other disk; its partner shares the real part
    let mut others: Vec<Disk> = vec![];
    for i in 0..d {
        if real[i] {
            continue;
        }
        let mut dk = disks[i].clone();
        let partners: Vec<usize> = (0..d).filter(|&j| j != i && disks[i].meets(&disks[j], true)).collect();
        if let [j] = partners.as_slice() {
            // one real part for both (the mean), the radius widened to keep each root inside
            let mean = disks[i].re.add(&disks[*j].re).mul(&qpoly::qq(1, 2));
            let shift = disks[i].re.sub(&mean);
            let shift = if shift.num.neg { shift.neg() } else { shift };
            dk.re = mean;
            dk.r = dk.r.add(&shift);
        }
        others.push(dk);
    }
    others.sort_by(|a, b| {
        let c = rat_f64(&a.re).total_cmp(&rat_f64(&b.re));
        let c = if a.re == b.re { Ordering::Equal } else { c };
        c.then(a.im.cmp_value(&b.im))
    });
    let mut out = Vec::with_capacity(d);
    for (k, r) in reals.into_iter().enumerate() {
        out.push(Embedding { index: k + 1, real: true, re: r.re, im: q(0), radius: r.r });
    }
    let nr = out.len();
    for (k, r) in others.into_iter().enumerate() {
        out.push(Embedding { index: nr + k + 1, real: false, re: r.re, im: r.im, radius: r.r });
    }
    Ok(Some(out))
}

/// An enclosure of the root `e` of `f` with about `bits` correct bits (a complex ball; a real ball for a real
/// root), refined by Newton's method inside the isolating disk; `None` when the refinement failed.
pub fn enclose(f: &[Rat], e: &Embedding, bits: u64, m: &mut Meter) -> WorkResult<Option<CBall>> {
    let f = qpoly::monic(&qpoly::trim(f.to_vec()));
    let d = f.len() - 1;
    if d == 1 {
        let w = bits.max(64);
        return Ok(Some(CBall::real(rat_ball(&f[0].neg(), w))));
    }
    let w = bits.max(64) + 32;
    let cs = coef_balls(&f, w);
    let mut z = point(&rat_float(&e.re, w), &rat_float(&e.im, w));
    if e.real {
        z.im = Ball::from_i64(0);
    }
    // Newton converges quadratically from inside an isolating disk once close; a bounded number of steps
    let steps = 8 + (64 - (w.leading_zeros() as u64)) * 2;
    for _ in 0..steps {
        m.step(d as u64 * (w / 64 + 1))?;
        let (v, dv) = eval2(&cs, &z, w);
        let Some(c) = v.div(&dv, w) else { break };
        let n = z.sub(&c, w);
        let n = point(&round_mid(&n.re, w), &round_mid(&n.im, w));
        let n = if e.real { CBall::new(n.re, Ball::from_i64(0)) } else { n };
        let done = small(&c, &n, w);
        z = n;
        if done {
            break;
        }
    }
    let Some(r) = inclusion_radius(&cs, &z, w) else { return Ok(None) };
    // the new disk must lie in the isolating disk: |z - c| + r <= R
    let dx = float_rat(&z.re.mid).sub(&e.re);
    let dy = float_rat(&z.im.mid).sub(&e.im);
    let dist = sqrt_up(&sq(&dx).add(&sq(&dy)));
    if e.radius.cmp_value(&dist.add(&r)) == Ordering::Less {
        return Ok(None);
    }
    let rb = rat_ball(&r, 64);
    let rad = Mag::from_float_up(&rb.mid).add(&rb.rad);
    let re = Ball { mid: z.re.mid.clone(), rad };
    let im = if e.real { Ball::from_i64(0) } else { Ball { mid: z.im.mid.clone(), rad } };
    Ok(Some(CBall::new(re, im)))
}

/// The root of `f` that the enclosures `z(bits)` (for growing `bits`) contain: the only isolating disk the
/// enclosure meets. `None` when no disk is singled out up to the precision limit.
pub fn locate(
    f: &[Rat],
    roots: &[Embedding],
    z: &mut dyn FnMut(u64, &mut Meter) -> WorkResult<Option<CBall>>,
    m: &mut Meter,
) -> WorkResult<Option<Embedding>> {
    let mut bits = 64;
    while bits <= MAX_BITS {
        let Some(zb) = z(bits, m)? else { return Ok(None) };
        let zd = Disk {
            re: float_rat(&zb.re.mid),
            im: float_rat(&zb.im.mid),
            r: mag_rat(&zb.re.rad).add(&mag_rat(&zb.im.rad)),
        };
        let hits: Vec<&Embedding> = roots
            .iter()
            .filter(|e| zd.meets(&Disk { re: e.re.clone(), im: e.im.clone(), r: e.radius.clone() }, false))
            .collect();
        if let [one] = hits.as_slice() {
            return Ok(Some((*one).clone()));
        }
        if hits.is_empty() {
            return Ok(None);
        }
        bits *= 2;
    }
    let _ = f;
    Ok(None)
}

/// The value of the root as a machine complex number (approximate).
pub fn approx(e: &Embedding) -> (f64, f64) {
    (rat_f64(&e.re), rat_f64(&e.im))
}

/// `f` as a square-free monic polynomial (its roots are the roots of `f`).
pub fn square_free_part(f: &[Rat], m: &mut Meter) -> WorkResult<QPoly> {
    qpoly::square_free(f, m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qpoly::of_i64;
    use mc_poly::work::Unlimited;

    #[test]
    fn quintic_roots() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let f = of_i64(&[-1, -1, 0, 0, 0, 1]);
        let rs = roots(&f, &mut m).unwrap().unwrap();
        assert_eq!(rs.len(), 5);
        assert!(rs[0].real && !rs[1].real);
        assert!((approx(&rs[0]).0 - 1.1673039782614187).abs() < 1e-12);
        // complex pairs: equal real parts, negative imaginary part first
        assert_eq!(rs[1].re, rs[2].re);
        assert!(approx(&rs[1]).1 < 0.0 && approx(&rs[2]).1 > 0.0);
        let z = enclose(&f, &rs[0], 200, &mut m).unwrap().unwrap();
        assert!(z.re.rad.log2() < -150.0);
    }

    #[test]
    fn three_real() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // (x^2 - 2)(x - 1) expanded: x^3 - x^2 - 2x + 2
        let rs = roots(&of_i64(&[2, -2, -1, 1]), &mut m).unwrap().unwrap();
        assert!(rs.iter().all(|e| e.real));
        let v: Vec<f64> = rs.iter().map(|e| approx(e).0).collect();
        assert!((v[0] + 2f64.sqrt()).abs() < 1e-9 && (v[1] - 1.0).abs() < 1e-9 && (v[2] - 2f64.sqrt()).abs() < 1e-9);
    }
}
