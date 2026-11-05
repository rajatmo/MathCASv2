//! All roots of a polynomial in one variable numerically, at machine precision or as balls (decision 3B.8).
//!
//! Coefficients are taken as exact rationals (machine reals and ball midpoints are dyadic rationals), so for real
//! coefficients the real roots are found exactly: square-free decomposition, isolation by Descartes' rule
//! ([`crate::realroots`]), and bisection with exact signs to the precision asked (a ball is the isolating interval,
//! so it is rigorous). The non-real roots start from the Aberth–Ehrlich iteration in machine complex arithmetic (all
//! roots at once, cubic convergence, Gauss–Seidel updates); the d - r approximations of largest imaginary part are
//! the complex ones (r = the exact number of real roots), refined by Newton's method in complex balls with doubling
//! precision; the enclosure radius is `d |p(z)| / |p'(z)|` (a disc of that radius about any z holds a root).
//!
//! The order is the order of root objects: real roots ascending, then the others by real part, then imaginary part.
use crate::realroots::{isolate, refine, Interval};
use mc_arith::num::float::f64_to_dyadic;
use mc_arith::num::{Ball, Float, Int, Mag, Nat, Rat};
use mc_arith::{Kind, Num, Real};
use mc_numeric::cball::CBall;
use mc_poly::factor::factor;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};

/// The precision of the roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Precision {
    /// machine reals and complex machine numbers
    Machine,
    /// balls of this many bits
    Bits(u64),
}

/// A complex machine number.
#[derive(Clone, Copy, Debug, PartialEq)]
struct C {
    re: f64,
    im: f64,
}

impl C {
    fn new(re: f64, im: f64) -> C {
        C { re, im }
    }
    fn add(self, o: C) -> C {
        C::new(self.re + o.re, self.im + o.im)
    }
    fn sub(self, o: C) -> C {
        C::new(self.re - o.re, self.im - o.im)
    }
    fn mul(self, o: C) -> C {
        C::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
    fn div(self, o: C) -> C {
        let d = o.re * o.re + o.im * o.im;
        C::new((self.re * o.re + self.im * o.im) / d, (self.im * o.re - self.re * o.im) / d)
    }
    fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
}

/// The exact value of a real number (machine reals and ball midpoints are dyadic).
pub(crate) fn real_to_rat(x: &Real) -> Option<Rat> {
    match x {
        Real::Exact(q) => Some(q.clone()),
        Real::Machine(v) => {
            let (neg, man, exp) = f64_to_dyadic(*v)?;
            Some(dyadic(neg, man, exp))
        }
        Real::Ball(b, _) => Some(dyadic(b.mid.neg, b.mid.man.clone(), b.mid.exp)),
    }
}

fn dyadic(neg: bool, man: Nat, exp: i64) -> Rat {
    let n = Int::new(neg, man);
    if exp >= 0 {
        Rat::from_int(n.shl(exp as u64))
    } else {
        Rat::new(n, Int::from_nat(Nat::from_u64(1).shl((-exp) as u64)))
    }
}

/// (re, im) of a number as exact rationals.
pub(crate) fn num_to_rats(x: &Num) -> Option<(Rat, Rat)> {
    match x {
        Num::Real(r) => Some((real_to_rat(r)?, Rat::from_i64(0))),
        Num::Complex(a, b) => Some((real_to_rat(a)?, real_to_rat(b)?)),
    }
}

fn rat_f64(q: &Rat) -> f64 {
    Real::Exact(q.clone()).to_f64()
}

fn rat_ball(q: &Rat, p: u64) -> Ball {
    match Real::Exact(q.clone()).to_kind(Kind::Ball, p) {
        Real::Ball(b, _) => b,
        _ => unreachable!("an exact number as a ball"),
    }
}

fn f64_ball(v: f64) -> Ball {
    match f64_to_dyadic(v) {
        Some((neg, man, exp)) => Ball::exact(Float::new(neg, man, exp)),
        None => Ball::from_i64(0),
    }
}

#[cfg(test)]
fn ball_f64(b: &Ball) -> f64 {
    mc_arith::num::float::dyadic_to_f64(b.mid.neg, &b.mid.man, b.mid.exp)
}

/// An upper bound of a ball's absolute value.
fn mag_up(b: &Ball) -> Mag {
    Mag::from_float_up(&b.mid).add(&b.rad)
}

/// All roots of `sum coefs[k] x^k` (leading coefficient nonzero, degree at least 1), with multiplicity, in the
/// order of root objects. `None` when a coefficient is not a finite number.
pub(crate) fn numeric_roots(coefs: &[Num], prec: Precision, m: &mut Meter) -> WorkResult<Option<Vec<Num>>> {
    let Some(parts) = coefs.iter().map(num_to_rats).collect::<Option<Vec<_>>>() else { return Ok(None) };
    let real = parts.iter().all(|(_, b)| b.is_zero());
    if real {
        let rs: Vec<Rat> = parts.into_iter().map(|(a, _)| a).collect();
        return real_poly_roots(&rs, prec, m).map(Some);
    }
    let cs: Vec<(Rat, Rat)> = parts;
    let approx = aberth(&cs.iter().map(|(a, b)| C::new(rat_f64(a), rat_f64(b))).collect::<Vec<_>>(), m)?;
    let mut out: Vec<(f64, f64, Num)> = vec![];
    for z in approx {
        let v = refine_complex(&cs, z, prec, m)?;
        out.push((z.re, z.im, v));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    Ok(Some(out.into_iter().map(|x| x.2).collect()))
}

/// The roots of a polynomial with rational coefficients.
fn real_poly_roots(rs: &[Rat], prec: Precision, m: &mut Meter) -> WorkResult<Vec<Num>> {
    // the integer polynomial over the common denominator
    let mut den = Z::ONE;
    for q in rs {
        let d = Z::from_int(Int::from_nat(q.den.clone()));
        den = den.mul(&d).div_exact(&den.gcd(&d));
    }
    let ints: Vec<Z> = rs
        .iter()
        .map(|q| Z::from_int(q.num.clone()).mul(&den.div_exact(&Z::from_int(Int::from_nat(q.den.clone())))))
        .collect();
    let p = Poly::from_dense(1, 0, &ints);
    let sqf = factor(&p, true, m)?;
    let mut reals: Vec<(Rat, Num)> = vec![];
    let mut complex: Vec<(f64, f64, Num)> = vec![];
    for (f, e) in &sqf.factors {
        let dense = f.to_dense(0);
        let ivs = isolate(&dense, m)?;
        let d = dense.len() - 1;
        for iv in &ivs {
            let (key, v) = refine_real(&dense, iv, prec, m)?;
            for _ in 0..*e {
                reals.push((key.clone(), v.clone()));
            }
        }
        if ivs.len() == d {
            continue;
        }
        let cs: Vec<(Rat, Rat)> = dense.iter().map(|z| (Rat::from_int(z.to_int()), Rat::from_i64(0))).collect();
        let mut approx = aberth(&cs.iter().map(|(a, _)| C::new(rat_f64(a), 0.0)).collect::<Vec<_>>(), m)?;
        // the d - r approximations farthest from the real axis are the complex roots, in conjugate pairs
        approx.sort_by(|a, b| b.im.abs().total_cmp(&a.im.abs()));
        let upper: Vec<C> = approx[..d - ivs.len()].iter().filter(|z| z.im > 0.0).copied().collect();
        for z in upper {
            let v = refine_complex(&cs, z, prec, m)?;
            let conj = match &v {
                Num::Complex(a, b) => Num::Complex(a.clone(), b.neg()),
                other => other.clone(),
            };
            for _ in 0..*e {
                complex.push((z.re, -z.im, conj.clone()));
                complex.push((z.re, z.im, v.clone()));
            }
        }
    }
    reals.sort_by(|a, b| a.0.cmp_value(&b.0));
    complex.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    Ok(reals.into_iter().map(|x| x.1).chain(complex.into_iter().map(|x| x.2)).collect())
}

/// A real root from its isolating interval: (a rational key for ordering, the value).
fn refine_real(p: &[Z], iv: &Interval, prec: Precision, m: &mut Meter) -> WorkResult<(Rat, Num)> {
    if iv.is_exact() {
        let v = match prec {
            Precision::Machine => Real::Machine(rat_f64(&iv.lo)),
            Precision::Bits(b) => Real::Exact(iv.lo.clone()).to_kind(Kind::Ball, b),
        };
        return Ok((iv.lo.clone(), Num::Real(v)));
    }
    // relative width 2^-(bits + 8), measured against the larger endpoint magnitude (at least 2^-1000)
    let bits = match prec {
        Precision::Machine => 60,
        Precision::Bits(b) => b + 8,
    };
    let scale = {
        let a = rat_f64(&iv.lo).abs().max(rat_f64(&iv.hi).abs()).max(1e-300);
        a.log2().floor() as i64
    };
    let e = scale - bits as i64;
    let width = if e >= 0 {
        Rat::from_int(Int::from_i64(1).shl(e as u64))
    } else {
        Rat::new(Int::from_i64(1), Int::from_nat(Nat::from_u64(1).shl((-e) as u64)))
    };
    let iv = refine(p, iv, &width, m)?;
    let mid = iv.mid();
    let v = match prec {
        Precision::Machine => Real::Machine(rat_f64(&mid)),
        Precision::Bits(b) => {
            let mut ball = rat_ball(&mid, b);
            let half = iv.hi.sub(&iv.lo).mul(&Rat::new(Int::from_i64(1), Int::from_i64(2)));
            ball.rad = ball.rad.add(&mag_up(&rat_ball(&half, 64)));
            Real::Ball(ball, b)
        }
    };
    Ok((mid, Num::Real(v)))
}

/// p(z) and p'(z) in complex balls at precision w.
fn eval_cball(cs: &[(Ball, Ball)], z: &CBall, w: u64) -> (CBall, CBall) {
    let n = cs.len();
    let mut p = CBall::new(cs[n - 1].0.clone(), cs[n - 1].1.clone());
    let mut d = CBall::from_i64(0);
    for k in (0..n - 1).rev() {
        d = d.mul(z, w).add(&p, w);
        p = p.mul(z, w).add(&CBall::new(cs[k].0.clone(), cs[k].1.clone()), w);
    }
    (p, d)
}

/// A complex root refined from the machine approximation z.
fn refine_complex(cs: &[(Rat, Rat)], z: C, prec: Precision, m: &mut Meter) -> WorkResult<Num> {
    let n = cs.len() - 1;
    match prec {
        Precision::Machine => {
            let c: Vec<C> = cs.iter().map(|(a, b)| C::new(rat_f64(a), rat_f64(b))).collect();
            let mut z = z;
            for _ in 0..3 {
                m.step(n as u64)?;
                let (p, d) = horner(&c, z);
                if d.abs() == 0.0 {
                    break;
                }
                z = z.sub(p.div(d));
            }
            Ok(Num::complex(Real::Machine(z.re), Real::Machine(z.im)))
        }
        Precision::Bits(target) => {
            let w_final = target + 32;
            let mut w = 64u64;
            let mut zb = CBall::new(f64_ball(z.re), f64_ball(z.im));
            let mut extra = 3;
            loop {
                m.step((n as u64) * (w / 64 + 1))?;
                let bc: Vec<(Ball, Ball)> = cs.iter().map(|(a, b)| (rat_ball(a, w), rat_ball(b, w))).collect();
                let (p, d) = eval_cball(&bc, &zb, w);
                let Some(step) = p.div(&d, w) else { break };
                let next = zb.sub(&step, w);
                zb = CBall::new(Ball::exact(next.re.mid), Ball::exact(next.im.mid));
                if w >= w_final {
                    extra -= 1;
                    if extra == 0 {
                        break;
                    }
                } else {
                    w = (w * 2).min(w_final);
                }
            }
            // the enclosure: a disc of radius n |p(z)| / |p'(z)| about z holds a root
            let bc: Vec<(Ball, Ball)> = cs.iter().map(|(a, b)| (rat_ball(a, w_final), rat_ball(b, w_final))).collect();
            let (p, d) = eval_cball(&bc, &zb, w_final);
            let pa = mc_numeric::cball::abs(&p, w_final);
            let da = mc_numeric::cball::abs(&d, w_final);
            let rad = match (pa, da) {
                (Some(pa), Some(da)) => {
                    let lo = Mag::from_float_down(&da.mid).sub_down(&da.rad);
                    if lo.is_zero() {
                        None
                    } else {
                        Some(mag_up(&pa).mul(&Mag::from_float_up(&Float::from_i64(n as i64))).div_up(&lo))
                    }
                }
                _ => None,
            };
            let rad = rad.unwrap_or_else(|| Mag::from_float_up(&Float::from_i64(1)));
            let re = Ball { mid: zb.re.mid.clone(), rad };
            let im = Ball { mid: zb.im.mid.clone(), rad };
            let z = CBall::new(re, im).round(target);
            Ok(Num::complex(Real::Ball(z.re, target), Real::Ball(z.im, target)))
        }
    }
}

/// p(z), p'(z) in machine arithmetic.
fn horner(c: &[C], z: C) -> (C, C) {
    let n = c.len();
    let mut p = c[n - 1];
    let mut d = C::new(0.0, 0.0);
    for k in (0..n - 1).rev() {
        d = d.mul(z).add(p);
        p = p.mul(z).add(c[k]);
    }
    (p, d)
}

/// All roots by the Aberth–Ehrlich iteration in machine complex arithmetic (degree at least 1).
fn aberth(c: &[C], m: &mut Meter) -> WorkResult<Vec<C>> {
    let n = c.len() - 1;
    let lead = c[n];
    let c: Vec<C> = c.iter().map(|x| x.div(lead)).collect();
    if n == 1 {
        return Ok(vec![C::new(-c[0].re, -c[0].im)]);
    }
    // initial points on a circle of the Fujiwara bound's radius, rotated off the axes
    let r = (0..n)
        .map(|k| (c[k].abs() / if k == 0 { 2.0 } else { 1.0 }).powf(1.0 / (n - k) as f64))
        .fold(0.0f64, f64::max)
        * 2.0;
    let r = if r.is_finite() && r > 0.0 { r } else { 1.0 };
    let mut z: Vec<C> = (0..n)
        .map(|k| {
            let t = 2.0 * std::f64::consts::PI * k as f64 / n as f64 + 0.4;
            C::new(r * t.cos(), r * t.sin())
        })
        .collect();
    let mut done = vec![false; n];
    for _ in 0..500 {
        m.step((n * n) as u64)?;
        let mut moved = false;
        for k in 0..n {
            if done[k] {
                continue;
            }
            let (p, d) = horner(&c, z[k]);
            if p.abs() == 0.0 {
                done[k] = true;
                continue;
            }
            let ratio = p.div(d);
            let mut s = C::new(0.0, 0.0);
            for j in 0..n {
                if j != k {
                    s = s.add(C::new(1.0, 0.0).div(z[k].sub(z[j])));
                }
            }
            let w = ratio.div(C::new(1.0, 0.0).sub(ratio.mul(s)));
            if !(w.re.is_finite() && w.im.is_finite()) {
                continue;
            }
            z[k] = z[k].sub(w);
            if w.abs() <= 1e-15 * z[k].abs().max(1e-300) {
                done[k] = true;
            } else {
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    Ok(z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_poly::work::Unlimited;

    fn ints(v: &[i64]) -> Vec<Num> {
        v.iter().map(|&x| Num::int(x)).collect()
    }

    #[test]
    fn quintic() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let rs = numeric_roots(&ints(&[-1, -1, 0, 0, 0, 1]), Precision::Machine, &mut m).unwrap().unwrap();
        assert_eq!(rs.len(), 5);
        let Num::Real(Real::Machine(x)) = rs[0] else { panic!("real first") };
        assert!((x - 1.1673039782614187).abs() < 1e-15);
        let Num::Complex(Real::Machine(a), Real::Machine(b)) = rs[1] else { panic!("complex") };
        assert!((a + 0.7648844336005847).abs() < 1e-14 && (b + 0.3524715460317262).abs() < 1e-14);
        // 50 digits
        let rs = numeric_roots(&ints(&[-1, -1, 0, 0, 0, 1]), Precision::Bits(170), &mut m).unwrap().unwrap();
        let Num::Complex(Real::Ball(a, _), _) = &rs[3] else { panic!("complex ball") };
        assert!((ball_f64(a) - 0.1812324444698754).abs() < 1e-15);
        assert!(a.rad.log2() < -150.0);
    }

    #[test]
    fn multiple_roots() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // (x - 1)^2 (x^2 + 1) = x^4 - 2x^3 + 2x^2 - 2x + 1
        let rs = numeric_roots(&ints(&[1, -2, 2, -2, 1]), Precision::Machine, &mut m).unwrap().unwrap();
        assert_eq!(rs.len(), 4);
        assert!(matches!(rs[0], Num::Real(Real::Machine(x)) if x == 1.0));
        assert!(matches!(rs[1], Num::Real(Real::Machine(x)) if x == 1.0));
        assert!(matches!(rs[2], Num::Complex(_, Real::Machine(y)) if (y + 1.0).abs() < 1e-15));
    }
}
