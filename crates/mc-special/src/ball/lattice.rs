//! Complex-ball enclosures of the modular functions and of the Weierstrass functions, for boxes of complex arguments
//! (`centered_interval` arguments with complex parts, through [`super::boxes::complex_box`]):
//! - `dedekind_eta(tau)` by Euler's pentagonal series eta = sum (-1)^n e^(i pi tau (6n - 1)^2/12) over all integers n
//!   (DLMF 27.14.4 with q^(1/24) of 23.15.9); `modular_lambda(tau)` = theta_2^4/theta_3^4 and Klein's
//!   `klein_invariant_j(tau)` = 4 (1 - lambda + lambda^2)^3/(27 lambda^2 (1 - lambda)^2), the theta constants at the
//!   nome q = e^(i pi tau) (DLMF 23.15.2, 23.15.7, 20.2.2-4);
//! - the lattice of invariants g2, g3: the roots e1, e2, e3 of 4 t^3 - g2 t - g3 by an interval Newton step (the
//!   root of every polynomial of the box lies in x - p(x)/p'(X) when that lies in the box X: the mean value form of
//!   p over the convex X, and the fixed point of a continuous map of X into itself; Moore, Kearfott and Cloud,
//!   "Introduction to Interval Analysis", SIAM 2009, 8.1), the half-periods omega1 = K(m)/s, omega3 = i K(1 - m)/s
//!   (m = (e2 - e3)/(e1 - e3), s = sqrt(e1 - e3), DLMF 23.6.16) with the labelling of the machine path, and p, p',
//!   zeta, sigma by the theta series of theta_1 at v = pi u/(2 omega1) (DLMF 23.6.8-9, 23.2.14, 23.2.20), as the term
//!   path of `crate::weierstrass` computes them;
//! - the reduced half-periods (`weierstrass_half_period_w1`, `_w2`, `_w3`) as the integer combinations of the
//!   lattice basis that the machine reduction chooses at the centre (declined near a boundary of the fundamental
//!   domain, where the choice could change inside the box), and the invariants of a pair of half-periods by
//!   e1 = c (theta_2^4 + 2 theta_4^4), e2 = c (theta_2^4 - theta_4^4), e3 = -c (2 theta_2^4 + theta_4^4),
//!   c = pi^2/(12 omega1^2), g2 = 2 (e1^2 + e2^2 + e3^2), g3 = 4 e1 e2 e3 (DLMF 23.6.2-4), tau first reduced by exact
//!   integer steps of the basis.
//!
//! Every quantity is carried with its gradient in the box arguments (forward differentiation on balls: each value and
//! each partial derivative is an enclosure over the whole box; the root's gradient by implicit differentiation,
//! de = (e dg2 + dg3)/(12 e^2 - g2), and K'(m) = (E - (1 - m) K)/(2 m (1 - m)), DLMF 19.4.1). The result is the
//! narrower, part by part, of the natural enclosure and the mean value form f(c) + sum_i f_i(B) (B_i - c_i) (the
//! segment from the centre c to a point of the convex box B stays in B; Moore, Kearfott and Cloud 6.1): the natural
//! form widens with the length of the computation, the mean value form only to second order in the radius.
//!
//! Every series is cut where its terms are below 2^-w; the rest is bounded by twice its first term (the ratio of the
//! bounds of consecutive terms is below 1/2 from there on), the derivative of the rest in tau and v by the same with
//! the factors pi (n + 1/2)^2 and 2n + 1, and added to the radii of the value and of the gradient.
use super::{f64_of, float_of, mag_up};
use mc_arith::num::{stopped, Ball, Mag};
use mc_numeric::cball::{self, CBall};
use mc_numeric::consts::Const;

type C = (f64, f64);

fn cm(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}
fn cd(a: C, b: C) -> C {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}
fn cs(a: C, b: C) -> C {
    (a.0 - b.0, a.1 - b.1)
}
fn ca(a: C) -> f64 {
    a.0.hypot(a.1)
}

/// The midpoint of a complex ball as machine numbers.
fn mid(z: &CBall) -> C {
    (f64_of(&z.re.mid), f64_of(&z.im.mid))
}

fn cb(z: C) -> CBall {
    CBall::new(Ball::exact(float_of(z.0)), Ball::exact(float_of(z.1)))
}

fn int(v: i64) -> CBall {
    CBall::from_i64(v)
}

/// A bound as a magnitude (doubled for the roundings of its machine computation).
fn mag_f64(v: f64) -> Option<Mag> {
    (v.is_finite() && v >= 0.0).then(|| Mag::from_float_up(&float_of(2.0 * v + f64::MIN_POSITIVE)))
}

/// The ball widened by `e` in both parts.
fn widen(z: CBall, e: &Mag) -> CBall {
    CBall::new(super::widen(z.re, e), super::widen(z.im, e))
}

/// An upper bound of the modulus (as a magnitude).
fn abs_mag(z: &CBall) -> Mag {
    mag_up(&z.re).add(&mag_up(&z.im))
}

/// An upper bound of the modulus as a machine number.
fn abs_up(z: &CBall) -> f64 {
    f64_of(&abs_mag(z).to_float()) * (1.0 + 1e-12)
}

/// A lower bound of the imaginary part as a machine number (`None` when it is not positive).
fn im_low(z: &CBall) -> Option<f64> {
    let lo = f64_of(&super::lower(&z.im));
    (lo > 1e-300).then_some(lo * (1.0 - 1e-12))
}

/// The centre of a box (exact, radius 0).
fn centre(z: &CBall) -> CBall {
    CBall::new(Ball::exact(z.re.mid.clone()), Ball::exact(z.im.mid.clone()))
}

// ---------------------------------------------------------------- values with gradients

/// A value and its partial derivatives in the box arguments, each an enclosure over the box.
#[derive(Clone)]
struct J {
    v: CBall,
    d: Vec<CBall>,
}

impl J {
    fn cst(v: CBall, n: usize) -> J {
        J { v, d: vec![int(0); n] }
    }
    fn var(v: CBall, i: usize, n: usize) -> J {
        let mut d = vec![int(0); n];
        d[i] = int(1);
        J { v, d }
    }
    fn n(&self) -> usize {
        self.d.len()
    }
    fn lin(&self, o: &J, f: impl Fn(&CBall, &CBall) -> CBall) -> J {
        J { v: f(&self.v, &o.v), d: self.d.iter().zip(&o.d).map(|(a, b)| f(a, b)).collect() }
    }
    fn map_d(&self, v: CBall, f: impl Fn(&CBall) -> CBall) -> J {
        J { v, d: self.d.iter().map(f).collect() }
    }
    fn add(&self, o: &J, w: u64) -> J {
        self.lin(o, |a, b| a.add(b, w))
    }
    fn sub(&self, o: &J, w: u64) -> J {
        self.lin(o, |a, b| a.sub(b, w))
    }
    fn neg(&self) -> J {
        self.map_d(self.v.neg(), CBall::neg)
    }
    fn mul_i(&self) -> J {
        self.map_d(self.v.mul_i(), CBall::mul_i)
    }
    fn scale(&self, c: &CBall, w: u64) -> J {
        self.map_d(self.v.mul(c, w), |a| a.mul(c, w))
    }
    fn k(&self, k: i64, w: u64) -> J {
        self.scale(&int(k), w)
    }
    fn mul(&self, o: &J, w: u64) -> J {
        J { v: self.v.mul(&o.v, w), d: self.d.iter().zip(&o.d).map(|(a, b)| a.mul(&o.v, w).add(&b.mul(&self.v, w), w)).collect() }
    }
    fn square(&self, w: u64) -> J {
        let two = self.v.mul(&int(2), w);
        self.map_d(self.v.square(w), |a| a.mul(&two, w))
    }
    fn recip(&self, w: u64) -> Option<J> {
        let r = self.v.recip(w)?;
        let r2 = r.square(w).neg();
        Some(self.map_d(r, |a| a.mul(&r2, w)))
    }
    fn div(&self, o: &J, w: u64) -> Option<J> {
        Some(self.mul(&o.recip(w)?, w))
    }
    fn exp(&self, w: u64) -> Option<J> {
        let e = cball::exp(&self.v, w)?;
        Some(self.map_d(e.clone(), |a| a.mul(&e, w)))
    }
    fn sin_cos(&self, w: u64) -> Option<(J, J)> {
        let (s, c) = cball::sin_cos(&self.v, w)?;
        let ms = s.neg();
        Some((self.map_d(s, |a| a.mul(&c, w)), self.map_d(c.clone(), |a| a.mul(&ms, w))))
    }
    fn sqrt(&self, w: u64) -> Option<J> {
        let s = cball::sqrt(&self.v, w)?;
        let h = s.mul(&int(2), w).recip(w)?;
        Some(self.map_d(s, |a| a.mul(&h, w)))
    }
    /// The value widened by `e` and each derivative by `e` times the bound of the derivative of `x` (the rest of a
    /// series in x, with its derivative bounded by `e` too).
    fn widen_by(&mut self, e: &Mag, xs: &[(&J, &Mag)]) {
        self.v = widen(self.v.clone(), e);
        for (i, d) in self.d.iter_mut().enumerate() {
            let mut m = Mag::ZERO;
            for (x, f) in xs {
                m = m.add(&abs_mag(&x.d[i]).mul(f));
            }
            *d = widen(d.clone(), &m);
        }
    }
}

/// A square root of the box z: the principal one, or i sqrt(-z) when z lies about the negative real axis (each
/// is a continuous root over a box off its cut); `None` when neither applies.
fn some_sqrt(z: &CBall, w: u64) -> Option<CBall> {
    if f64_of(&z.re.mid) >= 0.0 {
        cball::sqrt(z, w)
    } else {
        Some(cball::sqrt(&z.neg(), w)?.mul_i())
    }
}

/// K(m) and E(m) by the arithmetic-geometric mean (DLMF 19.8.1, 19.8.6): a_0 = 1, b_0 = sqrt(1 - m), each geometric
/// mean the root nearer to the arithmetic mean (the principal K and E off the cuts, as the machine path takes them),
/// K = pi/(2 M), E = K (1 - sum 2^(n-1) c_n^2), c_0^2 = m, c_(n+1) = (a_n - b_n)/2. Once |a_n - b_n| <= |a_n|/8,
/// a_(n+1) - b_(n+1) = (sqrt a_n - sqrt b_n)^2/2 with |sqrt a_n + sqrt b_n|^2 >= 3 |a_n|, so the differences fall by
/// a factor 1/48 or more at each step and every later mean moves a by half of one: M lies within |a_n - b_n| of a_n,
/// and the rest of the sum within 2^(n+1) |c_(n+1)|^2.
fn agm_ke(m: &CBall, w: u64) -> Option<(CBall, CBall)> {
    let mut a = int(1);
    let mut b = some_sqrt(&int(1).sub(m, w), w)?;
    let mut sum = m.mul(&CBall::real(Ball::from_ratio(1, 2, w)), w); // 2^-1 c_0^2
    let mut p2 = Ball::from_i64(1); // 2^(n-1) for c_(n+1)... starts at 2^0 for c_1
    for _ in 0..200 {
        if stopped() {
            return None;
        }
        let d = a.sub(&b, w);
        let (dm, am) = (abs_up(&d), ca(mid(&a)));
        // converged to the working precision, or (once |a - b| <= |a|/8 over the box) to the width of the box
        let md = ca(mid(&d));
        let rd = f64_of(&d.re.rad.to_float()) + f64_of(&d.im.rad.to_float());
        if dm <= am * 2f64.powi(-(w as i32) - 8) || (dm <= am / 8.0 && md <= rd) {
            let dmag = mag_f64(dm)?;
            let mm = widen(a, &dmag);
            let c = d.mul(&CBall::real(Ball::from_ratio(1, 2, w)), w);
            let rest = mag_f64(abs_up(&c).powi(2) * f64_of(&p2.mid) * 4.0)?;
            let pi = CBall::real(Const::Pi.ball(w)?);
            let k = pi.div(&mm.mul(&int(2), w), w)?;
            let e = k.mul(&int(1).sub(&widen(sum, &rest), w), w);
            mc_api::work::charge(64);
            return Some((k, e));
        }
        let an = a.add(&b, w).mul(&CBall::real(Ball::from_ratio(1, 2, w)), w);
        let mut bn = some_sqrt(&a.mul(&b, w), w)?;
        if ca(cs(mid(&an), mid(&bn))) > ca((mid(&an).0 + mid(&bn).0, mid(&an).1 + mid(&bn).1)) {
            bn = bn.neg();
        }
        // c_(n+1) = (a_n - b_n)/2, weighted 2^n
        let c = d.mul(&CBall::real(Ball::from_ratio(1, 2, w)), w);
        sum = sum.add(&c.square(w).mul_real(&p2, w), w);
        p2 = p2.mul(&Ball::from_i64(2), w);
        a = an;
        b = bn;
    }
    None
}

fn kk(m: &J, w: u64) -> Option<J> {
    let (k, e) = agm_ke(&m.v, w)?;
    let om = int(1).sub(&m.v, w);
    let dk = e.sub(&om.mul(&k, w), w).div(&m.v.mul(&om, w).mul(&int(2), w), w)?;
    Some(m.map_d(k, |a| a.mul(&dk, w)))
}

/// Where to cut a series whose n-th term (with the derivatives that are needed) is below e^logb(n): the last n
/// taken and the bound of the rest; `None` when it does not settle within 6000 terms.
fn cut(logb: impl Fn(f64) -> f64, w: u64) -> Option<(usize, Mag)> {
    let eps = -(w as f64) * std::f64::consts::LN_2;
    for n in 0..6000usize {
        let (a, b, c) = (logb(n as f64 + 1.0), logb(n as f64 + 2.0), logb(n as f64 + 3.0));
        if a < eps && b - a < -std::f64::consts::LN_2 && c - b <= b - a {
            mc_api::work::charge(8 * n as u64 + 8);
            return Some((n, mag_f64(2.0 * a.exp())?));
        }
    }
    None
}

/// e^(i pi x tau) and an upper bound of its modulus.
fn nome(tau: &J, x: (i64, i64), w: u64) -> Option<(J, f64)> {
    let pi = CBall::real(Const::Pi.ball(w)?);
    let f = CBall::real(Ball::from_ratio(x.0, x.1, w));
    let q = tau.scale(&pi.mul(&f, w), w).mul_i().exp(w)?;
    let r = (-std::f64::consts::PI * im_low(&tau.v)? * x.0 as f64 / x.1 as f64).exp() * (1.0 + 1e-12);
    Some((q, r))
}

/// theta_2^4, theta_3^4 and theta_4^4 at the nome e^(i pi tau) (Im tau > 0): theta_2 = 2 q^(1/4) sum q^(n(n+1)),
/// theta_3 = 1 + 2 sum q^(n^2), theta_4 = 1 + 2 sum (-1)^n q^(n^2) (DLMF 20.2.2-4).
fn theta_fourth(tau: &J, w: u64) -> Option<[J; 3]> {
    let (q, r) = nome(tau, (1, 1), w)?;
    let (q4, _) = nome(tau, (1, 4), w)?;
    let lr = r.ln();
    // q^(n^2) and q^(n(n+1)), with the derivative in tau: the factor pi (n + 1)^2
    let (n, tail) = cut(|k| k * k * lr + (1.0 + std::f64::consts::PI * (k + 1.0).powi(2)).ln(), w)?;
    let nv = tau.n();
    let one = J::cst(int(1), nv);
    let (mut s2, mut s3, mut s4) = (one.clone(), one.clone(), one.clone());
    let q2 = q.square(w);
    let (mut qn2, mut qnn) = (one.clone(), one.clone());
    let (mut a, mut b) = (q.clone(), q2.clone());
    for k in 1..=n {
        if stopped() {
            return None;
        }
        qn2 = qn2.mul(&a, w);
        qnn = qnn.mul(&b, w);
        a = a.mul(&q2, w);
        b = b.mul(&q2, w);
        s2 = s2.add(&qnn, w);
        let t = qn2.k(2, w);
        s3 = s3.add(&t, w);
        s4 = if k % 2 == 1 { s4.sub(&t, w) } else { s4.add(&t, w) };
    }
    let t2x = tail.mul_2exp(1);
    s2.widen_by(&tail, &[(tau, &tail)]);
    s3.widen_by(&t2x, &[(tau, &t2x)]);
    s4.widen_by(&t2x, &[(tau, &t2x)]);
    let t2 = s2.mul(&q4, w).k(2, w);
    let f = |t: J| t.square(w).square(w);
    Some([f(t2), f(s3), f(s4)])
}

/// The product prod (1 - Q^n), Q = e^(2 pi i tau), by Euler's pentagonal series sum (-1)^k Q^(k(3k-1)/2).
fn pentagonal(tau: &J, w: u64) -> Option<J> {
    // k(3k - 1)/2 >= k^2 for k != 0: the terms are below R^(k^2), R = |Q|, their derivatives in tau below
    // pi k |3k - 1| R^(k^2) <= 5 pi k^2 R^(k^2); both sides of the sum
    let lr = -2.0 * std::f64::consts::PI * im_low(&tau.v)?;
    let (n, tail) = cut(|k| k * k * lr + (1.0 + 5.0 * std::f64::consts::PI * k * k).ln(), w)?;
    let pi = CBall::real(Const::Pi.ball(w)?);
    let mut s = J::cst(int(0), tau.n());
    for k in -(n as i64)..=(n as i64) {
        if stopped() {
            return None;
        }
        let c = pi.mul(&int(k * (3 * k - 1)), w);
        let t = tau.scale(&c, w).mul_i().exp(w)?;
        s = if k.rem_euclid(2) == 0 { s.add(&t, w) } else { s.sub(&t, w) };
    }
    let t2 = tail.mul_2exp(1);
    s.widen_by(&t2, &[(tau, &t2)]);
    Some(s)
}

/// eta(tau) = e^(i pi tau/12) prod (1 - Q^n).
fn eta(tau: &J, w: u64) -> Option<J> {
    let (q24, _) = nome(tau, (1, 12), w)?;
    Some(q24.mul(&pentagonal(tau, w)?, w))
}

/// A modular function of the box tau with its derivative (Im tau above 0.05 in the box).
fn modular_j(name: &str, tau: &J, w: u64) -> Option<J> {
    if im_low(&tau.v)? < 0.05 {
        return None;
    }
    match name {
        "dedekind_eta" => eta(tau, w),
        "modular_lambda" => {
            let [t2, t3, _] = theta_fourth(tau, w)?;
            t2.div(&t3, w)
        }
        _ => {
            // J = E4^3/(1728 Delta), E4 = (theta_2^8 + theta_3^8 + theta_4^8)/2, Delta = eta^24 = Q prod (1 - Q^n)^24
            // (DLMF 23.15.7 with 23.15.10 and 27.14.4 written without the factor lambda, which a box can reach 0 in)
            let [t2, t3, t4] = theta_fourth(tau, w)?;
            let e4 = t2.square(w).add(&t3.square(w), w).add(&t4.square(w), w).scale(&CBall::real(Ball::from_ratio(1, 2, w)), w);
            let p = pentagonal(tau, w)?;
            let p24 = p.square(w).mul(&p, w).square(w).square(w).square(w);
            let pi = CBall::real(Const::Pi.ball(w)?);
            let qinv = tau.scale(&pi.mul(&int(-2), w), w).mul_i().exp(w)?;
            e4.square(w).mul(&e4, w).mul(&qinv, w).div(&p24.k(1728, w), w)
        }
    }
}

// ---------------------------------------------------------------- the lattice of invariants

/// Does the ball `a` lie inside `b` (both parts)?
fn inside(a: &CBall, b: &CBall) -> bool {
    let part = |x: &Ball, y: &Ball| {
        super::lower(x).cmp_exact(&super::lower(y)).is_ge() && super::upper(x).cmp_exact(&super::upper(y)).is_le()
    };
    part(&a.re, &b.re) && part(&a.im, &b.im)
}

/// p(x) = 4 x^3 - g2 x - g3 and p'(x) = 12 x^2 - g2.
fn cubic(x: &CBall, g2: &CBall, g3: &CBall, w: u64) -> (CBall, CBall) {
    let x2 = x.square(w);
    let p = x2.mul(x, w).mul(&int(4), w).sub(&g2.mul(x, w), w).sub(g3, w);
    let d = x2.mul(&int(12), w).sub(g2, w);
    (p, d)
}

/// The root of the box polynomials near the machine root z0, with its gradient.
fn root(z0: C, g2: &J, g3: &J, scale: f64, w: u64) -> Option<J> {
    let (gm2, gm3) = (centre(&g2.v), centre(&g3.v));
    let mut x = cb(z0);
    for _ in 0..((w as f64 / 40.0).log2().ceil().max(0.0) as usize + 4) {
        let (p, d) = cubic(&x, &gm2, &gm3, w);
        x = centre(&x.sub(&p.div(&d, w)?, w));
    }
    let (p, _) = cubic(&x, &g2.v, &g3.v, w);
    let mut e = Mag::from_float_up(&float_of(abs_up(&p) / (scale * scale) + 1e-30 * scale));
    for _ in 0..60 {
        let xb = widen(x.clone(), &e);
        let (_, d) = cubic(&xb, &g2.v, &g3.v, w);
        if let Some(q) = p.div(&d, w) {
            let n = x.sub(&q, w);
            if inside(&n, &xb) {
                // de = (e dg2 + dg3)/(12 e^2 - g2), the derivative over the enclosure of the root
                let (_, dn) = cubic(&n, &g2.v, &g3.v, w);
                let r = dn.recip(w)?;
                let d = g2.d.iter().zip(&g3.d).map(|(a, b)| n.mul(a, w).add(b, w).mul(&r, w)).collect();
                return Some(J { v: n, d });
            }
        }
        e = e.mul_2exp(2);
    }
    None
}

/// The lattice: half-periods omega1, omega3, tau, the nome q and q^(1/4) with a bound of |q|, eta1.
struct Lat {
    w1: J,
    w3: J,
    tau: J,
    tau_m: C,
    q: J,
    q4: J,
    r: f64,
    eta1: J,
}

/// The lattice of the box invariants with the labelling of the machine path (the largest Im tau).
fn lattice(g2: &J, g3: &J, w: u64) -> Option<Lat> {
    let (g2f, g3f) = (mid(&g2.v), mid(&g3.v));
    let mut z = crate::weierstrass::roots_f64(g2f, g3f)?;
    let scale = 1.0 + ca(g2f).sqrt() + ca(g3f).cbrt();
    let real = g2.v.is_real() && g3.v.is_real();
    if real {
        for r in z.iter_mut() {
            if r.1.abs() < 1e-9 * scale {
                r.1 = 0.0;
            }
        }
    }
    for i in 0..3 {
        for j in 0..i {
            if ca(cs(z[i], z[j])) < 1e-7 * scale {
                return None;
            }
        }
    }
    const PERMS: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    let mut best: Option<([usize; 3], f64)> = None;
    for p in PERMS {
        let (e1, e2, e3) = (z[p[0]], z[p[1]], z[p[2]]);
        let m = cd(cs(e2, e3), cs(e1, e3));
        if m.1.abs() <= 1e-12 * ca(m).max(1.0) && (m.0 <= 0.0 || m.0 >= 1.0) {
            continue;
        }
        let k = crate::weierstrass::k_f64;
        let tau = cd(cm((0.0, 1.0), k(cs((1.0, 0.0), m))), k(m));
        let im = tau.1.abs();
        if im.is_finite() && best.is_none_or(|(_, b)| im > b) {
            best = Some((p, im));
        }
    }
    let (p, im_tau) = best?;
    if im_tau < 0.05 {
        return None;
    }
    let mut e = Vec::with_capacity(3);
    for &k in &p {
        e.push(root(z[k], g2, g3, scale, w)?);
    }
    let (e1, e2, e3) = (&e[0], &e[1], &e[2]);
    let m = e2.sub(e3, w).div(&e1.sub(e3, w), w)?;
    let s = e1.sub(e3, w).sqrt(w)?;
    let k = kk(&m, w)?;
    let one = J::cst(int(1), g2.n());
    let kp = kk(&one.sub(&m, w), w)?;
    let w1 = k.div(&s, w)?;
    let mut w3 = kp.mul_i().div(&s, w)?;
    let mut tau = w3.div(&w1, w)?;
    if f64_of(&tau.v.im.mid) < 0.0 {
        w3 = w3.neg();
        tau = tau.neg();
    }
    let (q, r) = nome(&tau, (1, 1), w)?;
    let (q4, _) = nome(&tau, (1, 4), w)?;
    let tau_m = mid(&tau.v);
    let mut lat = Lat { w1, w3, tau, tau_m, q, q4, r, eta1: one };
    let th = theta1(&lat, &J::cst(int(0), g2.n()), w)?;
    let pi = CBall::real(Const::Pi.ball(w)?);
    // eta1 = -pi^2 theta1'''(0)/(12 omega1 theta1'(0))
    lat.eta1 = th[3].scale(&pi.square(w), w).div(&lat.w1.mul(&th[1], w).k(12, w), w)?.neg();
    Some(lat)
}

/// theta_1 and its first three derivatives in v at v, on the lattice's nome.
fn theta1(lat: &Lat, v: &J, w: u64) -> Option<[J; 4]> {
    let y = abs_up(&CBall::real(v.v.im.clone()));
    let lr = lat.r.ln();
    // the n-th term of the fourth series and its derivatives: 2 |q|^((n+1/2)^2) k^4 e^(k y) (1 + pi (n+1/2)^2), k = 2n+1
    let (n, tail) = cut(
        |m| {
            let k = 2.0 * m + 1.0;
            (m + 0.5).powi(2) * lr + k * y + 4.0 * k.ln() + (1.0 + std::f64::consts::PI * (m + 0.5).powi(2)).ln() + 2f64.ln()
        },
        w,
    )?;
    let nv = v.n();
    let mut s = [J::cst(int(0), nv), J::cst(int(0), nv), J::cst(int(0), nv), J::cst(int(0), nv)];
    let q2 = lat.q.square(w);
    let mut qk = lat.q4.clone();
    let mut ratio = q2.clone();
    for k in 0..=n as i64 {
        if stopped() {
            return None;
        }
        let kk = 2 * k + 1;
        let (sn, cs) = v.k(kk, w).sin_cos(w)?;
        let c = if k % 2 == 0 { qk.clone() } else { qk.neg() };
        let ts = [c.mul(&sn, w), c.mul(&cs, w).k(kk, w), c.mul(&sn, w).k(-kk * kk, w), c.mul(&cs, w).k(-kk * kk * kk, w)];
        for (acc, t) in s.iter_mut().zip(ts) {
            *acc = acc.add(&t, w);
        }
        qk = qk.mul(&ratio, w);
        ratio = ratio.mul(&q2, w);
    }
    Some(s.map(|mut x| {
        x.widen_by(&tail, &[(&lat.tau, &tail), (v, &tail)]);
        x.k(2, w)
    }))
}

/// p (0), p' (1), zeta (2) or sigma (3) at u on the lattice.
fn eval(lat: &Lat, u: &J, which: u8, w: u64) -> Option<J> {
    let pi = CBall::real(Const::Pi.ball(w)?);
    let c = lat.w1.k(2, w).recip(w)?.scale(&pi, w);
    let vf = mid(&c.v.mul(&u.v, w));
    let pt = std::f64::consts::PI;
    let n = (vf.1 / (pt * lat.tau_m.1)).round();
    let m = ((vf.0 - n * pt * lat.tau_m.0) / pt).round();
    if !n.is_finite() || !m.is_finite() || n.abs() > 1e9 || m.abs() > 1e9 {
        return None;
    }
    let (ni, mi) = (n as i64, m as i64);
    let u1 = u.sub(&lat.w1.k(2 * mi, w).add(&lat.w3.k(2 * ni, w), w), w);
    let v1 = c.mul(&u1, w);
    let [t0, t1, t2, t3] = theta1(lat, &v1, w)?;
    let ratio = lat.eta1.div(&lat.w1, w)?;
    let eta3 = || -> Option<J> {
        let h = J::cst(pi.mul_i().mul_real(&Ball::from_ratio(1, 2, w), w), u.n());
        lat.eta1.mul(&lat.w3, w).sub(&h, w).div(&lat.w1, w)
    };
    Some(match which {
        0 => {
            let num = t2.mul(&t0, w).sub(&t1.square(w), w);
            ratio.neg().sub(&c.square(w).mul(&num.div(&t0.square(w), w)?, w), w)
        }
        1 => {
            let num = t3.mul(&t0.square(w), w).sub(&t0.mul(&t1, w).mul(&t2, w).k(3, w), w).add(&t1.square(w).mul(&t1, w).k(2, w), w);
            c.square(w).mul(&c, w).mul(&num.div(&t0.square(w).mul(&t0, w), w)?, w).neg()
        }
        2 => {
            let z1 = ratio.mul(&u1, w).add(&c.mul(&t1.div(&t0, w)?, w), w);
            z1.add(&lat.eta1.k(2 * mi, w), w).add(&eta3()?.k(2 * ni, w), w)
        }
        _ => {
            let th0 = theta1(lat, &J::cst(int(0), u.n()), w)?;
            let g = lat.eta1.mul(&u1.square(w), w).div(&lat.w1.k(2, w), w)?.exp(w)?;
            let s1 = g.mul(&t0, w).div(&c.mul(&th0[1], w), w)?;
            let sign = if (mi + ni + mi * ni).rem_euclid(2) == 0 { 1 } else { -1 };
            let h = lat.eta1.k(2 * mi, w).add(&eta3()?.k(2 * ni, w), w);
            let at = u1.add(&lat.w1.k(mi, w), w).add(&lat.w3.k(ni, w), w);
            h.mul(&at, w).exp(w)?.mul(&s1, w).k(sign, w)
        }
    })
}

/// The reduction of the machine path (`crate::weierstrass::reduce_basis`) on machine numbers, as the integer matrix
/// taking (w1, w3) to the reduced pair; `None` near a boundary of the fundamental domain.
fn reduce(w1: C, w3: C) -> Option<[[i64; 2]; 2]> {
    let eps = 1e-10;
    let margin = 1e-6;
    let mut mt = [[1i64, 0], [0, 1]];
    let val = |r: [i64; 2]| (r[0] as f64 * w1.0 + r[1] as f64 * w3.0, r[0] as f64 * w1.1 + r[1] as f64 * w3.1);
    let t = cd(w3, w1);
    if t.1.abs() < 1e-12 * ca(t) {
        return None;
    }
    if t.1 < 0.0 {
        mt[1] = [-mt[1][0], -mt[1][1]];
    }
    for _ in 0..200 {
        let t = cd(val(mt[1]), val(mt[0]));
        let n = (t.0 + 0.5 - eps).floor();
        if (t.0 + 0.5 - (t.0 + 0.5).round()).abs() < margin {
            return None;
        }
        let ni = n as i64;
        mt[1] = [mt[1][0] - ni * mt[0][0], mt[1][1] - ni * mt[0][1]];
        let t = (t.0 - n, t.1);
        if (ca(t) - 1.0).abs() < margin {
            return None;
        }
        if ca(t) < 1.0 {
            let m = [-mt[0][0], -mt[0][1]];
            mt[0] = mt[1];
            mt[1] = m;
        } else {
            break;
        }
    }
    let a = val(mt[0]);
    if a.0.abs() <= margin * ca(a) {
        return None;
    }
    if a.0 < 0.0 {
        mt = [[-mt[0][0], -mt[0][1]], [-mt[1][0], -mt[1][1]]];
    }
    Some(mt)
}

/// The function of the gradient-carrying arguments.
fn value(name: &str, a: &[J], w: u64) -> Option<J> {
    match (name, a) {
        ("dedekind_eta" | "modular_lambda" | "klein_invariant_j", [tau]) => modular_j(name, tau, w),
        ("weierstrass_p" | "weierstrass_p_prime" | "weierstrass_zeta" | "weierstrass_sigma", [u, g2, g3]) => {
            let which = match name {
                "weierstrass_p" => 0,
                "weierstrass_p_prime" => 1,
                "weierstrass_zeta" => 2,
                _ => 3,
            };
            eval(&lattice(g2, g3, w)?, u, which, w)
        }
        ("weierstrass_half_period_w1" | "weierstrass_half_period_w2" | "weierstrass_half_period_w3", [g2, g3]) => {
            // real invariants with g3 < 0 are reduced in a turned frame by the machine path: not taken here
            if g2.v.is_real() && g3.v.is_real() && g3.v.re.mid.neg {
                return None;
            }
            let lat = lattice(g2, g3, w)?;
            let mt = reduce(mid(&lat.w1.v), mid(&lat.w3.v))?;
            // the choice is constant over the box when tau moves far less than the margin of the reduction: the
            // move is at most sum |dtau/dg_i| (|Re r_i| + |Im r_i|) over the box (the mean value theorem)
            let rad = |b: &CBall| f64_of(&b.re.rad.to_float()) + f64_of(&b.im.rad.to_float());
            let moved: f64 = [g2, g3].iter().zip(&lat.tau.d).map(|(g, d)| abs_up(d) * rad(&g.v)).sum();
            if moved.is_nan() || moved >= 1e-7 {
                return None;
            }
            let comb = |r: [i64; 2]| lat.w1.k(r[0], w).add(&lat.w3.k(r[1], w), w);
            let (o1, o3) = (comb(mt[0]), comb(mt[1]));
            Some(match name.as_bytes()[name.len() - 1] {
                b'1' => o1,
                b'3' => o3,
                _ => o1.add(&o3, w).neg(),
            })
        }
        ("weierstrass_invariant_g2" | "weierstrass_invariant_g3", [w1, w3]) => {
            let (mut a, mut b) = (w1.clone(), w3.clone());
            // tau = b/a into the fundamental domain by integer steps (the lattice, hence g2 and g3, is unchanged)
            for _ in 0..200 {
                let t = cd(mid(&b.v), mid(&a.v));
                if !t.0.is_finite() || t.1 == 0.0 {
                    return None;
                }
                if t.1 < 0.0 {
                    b = b.neg();
                    continue;
                }
                let n = t.0.round();
                if n != 0.0 {
                    b = b.sub(&a.k(n as i64, w), w);
                }
                let t = (t.0 - n, t.1);
                if ca(t) < 1.0 - 1e-9 {
                    let m = a.neg();
                    a = b;
                    b = m;
                } else {
                    break;
                }
            }
            let tau = b.div(&a, w)?;
            let [t2, _, t4] = theta_fourth(&tau, w)?;
            let pi = CBall::real(Const::Pi.ball(w)?);
            let c = a.square(w).k(12, w).recip(w)?.scale(&pi.square(w), w);
            let e1 = c.mul(&t2.add(&t4.k(2, w), w), w);
            let e2 = c.mul(&t2.sub(&t4, w), w);
            let e3 = c.mul(&t2.k(2, w).add(&t4, w), w).neg();
            Some(if name.ends_with('3') {
                e1.mul(&e2, w).mul(&e3, w).k(4, w)
            } else {
                e1.square(w).add(&e2.square(w), w).add(&e3.square(w), w).k(2, w)
            })
        }
        _ => None,
    }
}

/// Does this module enclose the function?
pub fn has(name: &str) -> bool {
    matches!(
        name,
        "dedekind_eta"
            | "modular_lambda"
            | "klein_invariant_j"
            | "weierstrass_p"
            | "weierstrass_p_prime"
            | "weierstrass_zeta"
            | "weierstrass_sigma"
            | "weierstrass_half_period_w1"
            | "weierstrass_half_period_w2"
            | "weierstrass_half_period_w3"
            | "weierstrass_invariant_g2"
            | "weierstrass_invariant_g3"
    )
}

/// The narrower, part by part, of two enclosures of one number.
fn narrower(a: CBall, b: CBall) -> CBall {
    let pick = |x: Ball, y: Ball| if x.rad.log2() <= y.rad.log2() || y.rad.is_zero() && x.rad.is_zero() { x } else { y };
    CBall::new(pick(a.re, b.re), pick(a.im, b.im))
}

/// The enclosure of `name(a...)` over the boxes `a` at w bits: the natural enclosure met with the mean value form.
pub fn enclose(name: &str, a: &[CBall], w: u64) -> Option<CBall> {
    let v = enclose_c(name, a, w)?;
    // p, p', zeta and sigma are real on real arguments of real invariants (their series have real coefficients in
    // u, g2, g3, DLMF 23.9.2-3)
    let real_fn = matches!(name, "weierstrass_p" | "weierstrass_p_prime" | "weierstrass_zeta" | "weierstrass_sigma");
    if real_fn && a.iter().all(CBall::is_real) && v.im.contains_zero() {
        return Some(CBall::real(v.re));
    }
    Some(v)
}

fn enclose_c(name: &str, a: &[CBall], w: u64) -> Option<CBall> {
    let n = a.len();
    let boxes: Vec<J> = a.iter().enumerate().map(|(i, b)| J::var(b.clone(), i, n)).collect();
    let f = value(name, &boxes, w)?;
    if a.iter().all(|b| b.re.rad.is_zero() && b.im.rad.is_zero()) {
        return Some(f.v);
    }
    let centres: Vec<J> = a.iter().map(|b| J::cst(centre(b), n)).collect();
    let Some(fc) = value(name, &centres, w) else { return Some(f.v) };
    let mut mvf = fc.v;
    for (i, b) in a.iter().enumerate() {
        mvf = mvf.add(&f.d[i].mul(&b.sub(&centre(b), w), w), w);
    }
    Some(narrower(f.v, mvf))
}
