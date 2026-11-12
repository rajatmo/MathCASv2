//! The Weierstrass elliptic functions of the invariants g2, g3: `weierstrass_p`,
//! `weierstrass_p_prime`, `weierstrass_zeta`, `weierstrass_sigma` and `inverse_weierstrass_p`, all as
//! `f(u, [g2, g3])`.
//!
//! Method (DLMF 23.6, 23.2, 20.2):
//!
//! 1. The roots e1, e2, e3 of 4 t^3 - g2 t - g3: Durand-Kerner in machine complex numbers, then Newton's method at
//!    the working precision on exact midpoints, a last step on the balls so that the radii of g2 and g3 reach the
//!    roots.
//! 2. A pair of half-periods: for a labelling of the roots, m = (e2 - e3)/(e1 - e3), s = sqrt(e1 - e3), omega1 =
//!    K(m)/s and omega3 = i K(1 - m)/s (from p(u) = e3 + s^2/sn^2(s u | m), DLMF 23.6.16, and the periods 2K, 2iK'
//!    of sn^2). Of the six labellings the one with the largest Im(tau), tau = omega3/omega1, is used (the smallest
//!    nome q = e^(i pi tau), the fastest theta series); the functions depend only on the lattice, so any basis
//!    gives the same values.
//! 3. The argument reduced into the period parallelogram, u = u' + 2 m omega1 + 2 n omega3, and the theta series
//!    of theta_1 and its first three derivatives at v = pi u'/(2 omega1) (DLMF 20.2.1):
//!    eta1 = -pi^2 theta_1'''(0)/(12 omega1 theta_1'(0)) (23.6.8), zeta(u) = eta1 u/omega1 + (pi/(2 omega1))
//!    theta_1'(v)/theta_1(v) (23.6.9), sigma(u) = (2 omega1/pi) exp(eta1 u^2/(2 omega1)) theta_1(v)/theta_1'(0)
//!    (23.6.9), p = -zeta', p' = -zeta''. zeta and sigma are carried back by their quasi-periodicity (23.2.14,
//!    23.2.20) with eta3 = (eta1 omega3 - i pi/2)/omega1 (Legendre's relation, 23.2.14).
//! 4. `inverse_weierstrass_p(p, [g2, g3])` = R_F(p - e1, p - e2, p - e3) (DLMF 23.6.31 with 19.25.35), the u with
//!    p(u) = p in the period parallelogram given by the principal branches; `inverse_weierstrass_p([p, q], [g2,
//!    g3])` takes the sign with p'(u) = q.
use crate::cover::{mag, mid_term, pi_at, theta_terms, work};
use mc_api::T;
use mc_arith::num::Ball;
use mc_arith::{Kind, Num, Real};
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;
type C = (f64, f64);

fn cmul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}
fn cdiv(a: C, b: C) -> C {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}
fn csub(a: C, b: C) -> C {
    (a.0 - b.0, a.1 - b.1)
}
fn cabs(a: C) -> f64 {
    a.0.hypot(a.1)
}
fn csqrt(a: C) -> C {
    let r = cabs(a);
    let re = ((r + a.0) / 2.0).max(0.0).sqrt();
    let im = ((r - a.0) / 2.0).max(0.0).sqrt();
    (re, if a.1 < 0.0 { -im } else { im })
}

/// The roots of t^3 - (g2/4) t - g3/4 in machine numbers (Durand-Kerner).
pub(crate) fn roots_f64(g2: C, g3: C) -> Option<[C; 3]> {
    let c1 = (-g2.0 / 4.0, -g2.1 / 4.0);
    let c0 = (-g3.0 / 4.0, -g3.1 / 4.0);
    let p = |x: C| {
        let x2 = cmul(x, x);
        let x3 = cmul(x2, x);
        (x3.0 + cmul(c1, x).0 + c0.0, x3.1 + cmul(c1, x).1 + c0.1)
    };
    let scale = 1.0 + cabs(g2).sqrt() + cabs(g3).cbrt();
    let mut z = [(0.4 * scale, 0.9 * scale), cmul((0.4, 0.9), (0.4 * scale, 0.9 * scale)), cmul((0.4, 0.9), cmul((0.4, 0.9), (0.4 * scale, 0.9 * scale)))];
    for _ in 0..500 {
        let mut moved = 0.0f64;
        for i in 0..3 {
            let mut den = (1.0, 0.0);
            for j in 0..3 {
                if i != j {
                    den = cmul(den, csub(z[i], z[j]));
                }
            }
            let d = cdiv(p(z[i]), den);
            if !d.0.is_finite() || !d.1.is_finite() {
                return None;
            }
            z[i] = csub(z[i], d);
            moved = moved.max(cabs(d));
        }
        if moved <= 1e-17 * scale {
            break;
        }
    }
    Some(z)
}

/// K(m) in machine numbers by the arithmetic-geometric mean, K = pi/(2 agm(1, sqrt(1 - m))).
pub(crate) fn k_f64(m: C) -> C {
    let (mut a, mut b) = ((1.0, 0.0), csqrt(csub((1.0, 0.0), m)));
    for _ in 0..60 {
        let an = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let mut bn = csqrt(cmul(a, b));
        // the right choice of the root: the one nearer to the new arithmetic mean
        if cabs(csub(an, bn)) > cabs((an.0 + bn.0, an.1 + bn.1)) {
            bn = (-bn.0, -bn.1);
        }
        a = an;
        b = bn;
    }
    cdiv((std::f64::consts::FRAC_PI_2, 0.0), a)
}

/// K(m) at w bits, pi/(2 agm(1, sqrt(1 - m))), each geometric mean the root nearer to the arithmetic mean (the
/// principal K off the cuts).
fn k_agm<'h>(fx: &mut Fx<'_, '_, '_, 'h>, m: T<'h>, w: u64) -> R<'h> {
    let mut a = fx.int(1);
    let mut b = fx.eval(fx.call("sqrt", &[fx.sub(fx.int(1), m)]))?;
    // quadratic convergence: once |a - b| < 2^(-w/2) |a|, one more step reaches 2^-w (the differences of balls
    // stop at their radii, so the test is not made at 2^-w itself)
    let eps = 2f64.powf(-((w.min(1000) as f64) / 2.0));
    let more = 1 + (w as f64 / (w.min(1000) as f64 / 2.0)).log2().ceil() as usize;
    let mut left: Option<usize> = None;
    for _ in 0..(64 + 2 * (w as f64).log2() as usize) {
        if fx.num(b).is_none() {
            return Ok(None);
        }
        let an = fx.eval(fx.mul(&[fx.rat(1, 2), fx.add(&[a, b])]))?;
        let mut bn = fx.eval(fx.call("sqrt", &[fx.mul(&[a, b])]))?;
        if mag(fx, fx.sub(an, bn)) > mag(fx, fx.add(&[an, bn])) {
            bn = fx.eval(fx.neg(bn))?;
        }
        let small = mag(fx, fx.sub(an, bn)) <= eps * mag(fx, an);
        a = an;
        b = bn;
        if left == Some(0) {
            let pi = pi_at(fx, w)?;
            let k = fx.eval(fx.div(pi, fx.mul(&[fx.int(2), a])))?;
            return Ok(fx.num(k).map(|_| k));
        }
        left = match left {
            Some(k) => Some(k - 1),
            None if small => Some(more - 1),
            None => None,
        };
    }
    Ok(None)
}

/// A number at w bits from machine parts.
fn ball_c<'h>(fx: &Fx<'_, '_, '_, 'h>, z: C, w: u64) -> T<'h> {
    let b = |v: f64| Real::Ball(Ball::exact(crate::ball::float_of(v)), w);
    if z.1 == 0.0 {
        return fx.ar.number(&Num::Real(b(z.0)));
    }
    fx.ar.number(&Num::Complex(b(z.0), b(z.1)))
}

/// The parts of a number as `f64`s.
fn cparts<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<C> {
    crate::cover::parts(fx, t)
}

/// The lattice data at the working precision: half-periods omega1, omega3, the nome q and q^(1/4), and eta1, eta3.
struct Lattice<'h> {
    w1: T<'h>,
    w3: T<'h>,
    tau: C,
    q: T<'h>,
    q4: T<'h>,
    eta1: T<'h>,
    eta3: T<'h>,
    pi: T<'h>,
    w: u64,
}

/// The roots of 4 t^3 - g2 t - g3 at w bits, `None` when two coincide (a degenerate lattice).
fn roots<'h>(fx: &mut Fx<'_, '_, '_, 'h>, g2: T<'h>, g3: T<'h>, w: u64) -> Res<Option<([T<'h>; 3], [C; 3])>> {
    let (Some(g2f), Some(g3f)) = (cparts(fx, g2), cparts(fx, g3)) else { return Ok(None) };
    let Some(mut z) = roots_f64(g2f, g3f) else { return Ok(None) };
    let scale = 1.0 + cabs(g2f).sqrt() + cabs(g3f).cbrt();
    // real invariants: the real roots exactly real (a real ball, not a complex one straddling the axis), so that
    // square roots of their differences are taken off the branch cut
    if matches!(fx.num(g2), Some(Num::Real(_))) && matches!(fx.num(g3), Some(Num::Real(_))) {
        for r in z.iter_mut() {
            if r.1.abs() < 1e-9 * scale {
                r.1 = 0.0;
            }
        }
    }
    for i in 0..3 {
        for j in 0..i {
            if cabs(csub(z[i], z[j])) < 1e-7 * scale {
                return Ok(None);
            }
        }
    }
    let steps = ((w as f64 / 40.0).log2().ceil().max(0.0) as usize) + 3;
    let mut out = [fx.int(0); 3];
    for (k, &z0) in z.iter().enumerate() {
        let mut x = ball_c(fx, z0, w);
        for s in 0..=steps {
            let x2 = fx.mul(&[x, x]);
            let p = fx.sub(fx.sub(fx.mul(&[fx.int(4), x2, x]), fx.mul(&[g2, x])), g3);
            let dp = fx.sub(fx.mul(&[fx.int(12), x2]), g2);
            let nx = fx.eval(fx.sub(x, fx.div(p, dp)))?;
            if fx.num(nx).is_none() {
                return Ok(None);
            }
            // the last step keeps the radii (those of g2, g3), the others are taken at their midpoints
            x = if s < steps { mid_term(fx, nx) } else { nx };
        }
        out[k] = x;
    }
    Ok(Some((out, z)))
}

/// The lattice of g2, g3 at w bits.
fn lattice<'h>(fx: &mut Fx<'_, '_, '_, 'h>, g2: T<'h>, g3: T<'h>, w: u64) -> Res<Option<Lattice<'h>>> {
    let Some((e, ef)) = roots(fx, g2, g3, w)? else { return Ok(None) };
    // the labelling with the largest Im tau
    const PERMS: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    let mut best: Option<([usize; 3], f64)> = None;
    for p in PERMS {
        let (e1, e2, e3) = (ef[p[0]], ef[p[1]], ef[p[2]]);
        let m = cdiv(csub(e2, e3), csub(e1, e3));
        let on_cut = m.1.abs() <= 1e-12 * cabs(m).max(1.0) && (m.0 <= 0.0 || m.0 >= 1.0);
        if on_cut {
            continue;
        }
        let tau = cdiv(cmul((0.0, 1.0), k_f64(csub((1.0, 0.0), m))), k_f64(m));
        let im = tau.1.abs();
        if im.is_finite() && best.is_none_or(|(_, b)| im > b) {
            best = Some((p, im));
        }
    }
    let Some((p, im_tau)) = best else { return Ok(None) };
    if im_tau < 0.05 {
        return Ok(None);
    }
    let (e1, e2, e3) = (e[p[0]], e[p[1]], e[p[2]]);
    let m = fx.eval(fx.div(fx.sub(e2, e3), fx.sub(e1, e3)))?;
    let s = fx.eval(fx.call("sqrt", &[fx.sub(e1, e3)]))?;
    let Some(k) = k_agm(fx, m, w)? else { return Ok(None) };
    let Some(kp) = k_agm(fx, fx.sub(fx.int(1), m), w)? else { return Ok(None) };
    for v in [m, s, k, kp] {
        if fx.num(v).is_none() {
            return Ok(None);
        }
    }
    let w1 = fx.eval(fx.div(k, s))?;
    let mut w3 = fx.eval(fx.div(fx.mul(&[fx.i(), kp]), s))?;
    let mut tau_t = fx.eval(fx.div(w3, w1))?;
    let Some(mut tau) = cparts(fx, tau_t) else { return Ok(None) };
    if tau.1 < 0.0 {
        w3 = fx.eval(fx.neg(w3))?;
        tau_t = fx.eval(fx.neg(tau_t))?;
        tau = (-tau.0, -tau.1);
    }
    let pi = pi_at(fx, w)?;
    let i = fx.i();
    let q = fx.eval(fx.call("exp", &[fx.mul(&[i, pi, tau_t])]))?;
    let q4 = fx.eval(fx.call("exp", &[fx.mul(&[fx.rat(1, 4), i, pi, tau_t])]))?;
    if fx.num(q).is_none() || fx.num(q4).is_none() {
        return Ok(None);
    }
    let mut lat = Lattice { w1, w3, tau, q, q4, eta1: w1, eta3: w1, pi, w };
    let Some(th) = theta1(fx, &lat, fx.int(0))? else { return Ok(None) };
    // eta1 = -pi^2 theta1'''(0)/(12 omega1 theta1'(0))
    let eta1 = fx.eval(fx.neg(fx.div(fx.mul(&[pi, pi, th[3]]), fx.mul(&[fx.int(12), w1, th[1]]))))?;
    let eta3 = fx.eval(fx.div(fx.sub(fx.mul(&[eta1, w3]), fx.mul(&[fx.rat(1, 2), i, pi])), w1))?;
    lat.eta1 = eta1;
    lat.eta3 = eta3;
    Ok(Some(lat))
}

/// theta_1 and its first three derivatives in v at v (the nome of the lattice): 2 sum (-1)^n q^((n+1/2)^2) times
/// sin((2n+1) v), (2n+1) cos, -(2n+1)^2 sin, -(2n+1)^3 cos.
fn theta1<'h>(fx: &mut Fx<'_, '_, '_, 'h>, lat: &Lattice<'h>, v: T<'h>) -> Res<Option<[T<'h>; 4]>> {
    let Some(qf) = cparts(fx, lat.q) else { return Ok(None) };
    let im_v = cparts(fx, v).map_or(0.0, |p| p.1);
    let Some(nt) = theta_terms(qf, im_v, lat.w) else { return Ok(None) };
    let mut s = [fx.int(0); 4];
    let mut qk = lat.q4; // q^((n+1/2)^2) = q^(1/4) q^(n(n+1))
    let q2 = fx.mul(&[lat.q, lat.q]);
    let mut ratio = q2; // q^(2n+2)
    for n in 0..=nt as i64 {
        if n % 32 == 31 {
            fx.tick(1)?;
        }
        let k = 2 * n + 1;
        let arg = fx.mul(&[fx.int(k), v]);
        let sn = fx.eval(fx.call("sin", &[arg]))?;
        let cs = fx.eval(fx.call("cos", &[arg]))?;
        let sign = if n % 2 == 0 { 1 } else { -1 };
        let c = fx.mul(&[fx.int(sign), qk]);
        let terms = [fx.mul(&[c, sn]), fx.mul(&[c, fx.int(k), cs]), fx.mul(&[c, fx.int(-k * k), sn]), fx.mul(&[c, fx.int(-k * k * k), cs])];
        for (acc, t) in s.iter_mut().zip(terms) {
            *acc = fx.eval(fx.add(&[*acc, t]))?;
        }
        qk = fx.eval(fx.mul(&[qk, ratio]))?;
        ratio = fx.mul(&[ratio, q2]);
    }
    let mut out = [fx.int(0); 4];
    for j in 0..4 {
        out[j] = fx.eval(fx.mul(&[fx.int(2), s[j]]))?;
        if fx.num(out[j]).is_none() {
            return Ok(None);
        }
    }
    Ok(Some(out))
}

/// Which function.
#[derive(Clone, Copy, PartialEq)]
enum Wf {
    P,
    PPrime,
    Zeta,
    Sigma,
}

/// f(u) at working-precision u on the lattice.
fn eval_at<'h>(fx: &mut Fx<'_, '_, '_, 'h>, lat: &Lattice<'h>, u: T<'h>, which: Wf) -> R<'h> {
    let pi = lat.pi;
    let c = fx.eval(fx.div(pi, fx.mul(&[fx.int(2), lat.w1])))?;
    let v = fx.eval(fx.mul(&[c, u]))?;
    let Some(vf) = cparts(fx, v) else { return Ok(None) };
    // reduce v by n pi tau and m pi: u' = u - 2 m omega1 - 2 n omega3
    let n = (vf.1 / (std::f64::consts::PI * lat.tau.1)).round();
    let vr = (vf.0 - n * std::f64::consts::PI * lat.tau.0, vf.1 - n * std::f64::consts::PI * lat.tau.1);
    let m = (vr.0 / std::f64::consts::PI).round();
    if !n.is_finite() || !m.is_finite() || n.abs() > 1e12 || m.abs() > 1e12 {
        return Ok(None);
    }
    let (ni, mi) = (n as i64, m as i64);
    let shift = fx.add(&[fx.mul(&[fx.int(2 * mi), lat.w1]), fx.mul(&[fx.int(2 * ni), lat.w3])]);
    let u1 = fx.eval(fx.sub(u, shift))?;
    let v1 = fx.eval(fx.mul(&[c, u1]))?;
    let Some(th) = theta1(fx, lat, v1)? else { return Ok(None) };
    let [t0, t1, t2, t3] = th;
    if fx.num(t0).is_some_and(|x| x.is_zero()) {
        return Ok(None);
    }
    let ratio = fx.div(lat.eta1, lat.w1);
    let val = match which {
        Wf::P => {
            // -eta1/omega1 - c^2 (t2 t0 - t1^2)/t0^2
            let num = fx.sub(fx.mul(&[t2, t0]), fx.mul(&[t1, t1]));
            fx.sub(fx.neg(ratio), fx.mul(&[c, c, fx.div(num, fx.mul(&[t0, t0]))]))
        }
        Wf::PPrime => {
            // -c^3 (t3 t0^2 - 3 t0 t1 t2 + 2 t1^3)/t0^3
            let num = fx.add(&[fx.mul(&[t3, t0, t0]), fx.mul(&[fx.int(-3), t0, t1, t2]), fx.mul(&[fx.int(2), t1, t1, t1])]);
            fx.neg(fx.mul(&[c, c, c, fx.div(num, fx.mul(&[t0, t0, t0]))]))
        }
        Wf::Zeta => {
            let z1 = fx.add(&[fx.mul(&[ratio, u1]), fx.mul(&[c, fx.div(t1, t0)])]);
            fx.add(&[z1, fx.mul(&[fx.int(2 * mi), lat.eta1]), fx.mul(&[fx.int(2 * ni), lat.eta3])])
        }
        Wf::Sigma => {
            let Some(th0) = theta1(fx, lat, fx.int(0))? else { return Ok(None) };
            let g = fx.call("exp", &[fx.div(fx.mul(&[lat.eta1, u1, u1]), fx.mul(&[fx.int(2), lat.w1]))]);
            let s1 = fx.div(fx.mul(&[g, t0]), fx.mul(&[c, th0[1]]));
            // sigma(u' + 2 m w1 + 2 n w3) = (-1)^(m + n + m n) exp((2 m eta1 + 2 n eta3)(u' + m w1 + n w3)) sigma(u')
            let sign = if (mi + ni + mi * ni).rem_euclid(2) == 0 { 1 } else { -1 };
            let h = fx.add(&[fx.mul(&[fx.int(2 * mi), lat.eta1]), fx.mul(&[fx.int(2 * ni), lat.eta3])]);
            let at = fx.add(&[u1, fx.mul(&[fx.int(mi), lat.w1]), fx.mul(&[fx.int(ni), lat.w3])]);
            fx.mul(&[fx.int(sign), fx.call("exp", &[fx.mul(&[h, at])]), s1])
        }
    };
    let v = fx.eval(val)?;
    Ok(fx.num(v).map(|_| v))
}

/// The invariants of a call: [g2, g3] as two terms.
fn invariants<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
    match fx.as_list(t)? {
        [g2, g3] => Some((*g2, *g3)),
        _ => None,
    }
}

/// Are all of the terms numbers, one of them inexact?
fn numeric_terms<'h>(fx: &Fx<'_, '_, '_, 'h>, ts: &[T<'h>]) -> bool {
    let nums: Option<Vec<Num>> = ts.iter().map(|&t| fx.num(t)).collect();
    nums.is_some_and(|ns| ns.iter().any(|n| n.kind() != Kind::Exact))
}

/// Real arguments with a real lattice give real values of p, p', zeta and sigma.
fn all_real<'h>(fx: &Fx<'_, '_, '_, 'h>, ts: &[T<'h>]) -> bool {
    ts.iter().all(|&t| matches!(fx.num(t), Some(Num::Real(_))))
}

fn weierstrass<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: Wf) -> R<'h> {
    let Some((g2, g3)) = invariants(fx, a[1]) else { return Ok(None) };
    let u = a[0];
    if let Some(us) = fx.as_list(u) {
        let items: Vec<T<'h>> = us.iter().map(|&x| fx.call(name_of(which), &[x, a[1]])).collect();
        return Ok(Some(fx.list(&items)));
    }
    if fx.is_int(u, 0) {
        return Ok(Some(match which {
            Wf::Sigma => fx.int(0),
            _ => fx.sym("complex_infinity"),
        }));
    }
    if fx.is_int(g2, 0) && fx.is_int(g3, 0) {
        // the degenerate lattice: p = 1/u^2, zeta = 1/u, sigma = u
        return Ok(Some(match which {
            Wf::P => fx.pow(u, fx.int(-2)),
            Wf::PPrime => fx.mul(&[fx.int(-2), fx.pow(u, fx.int(-3))]),
            Wf::Zeta => fx.pow(u, fx.int(-1)),
            Wf::Sigma => u,
        }));
    }
    if let Some(v) = degenerate(fx, u, g2, g3, which)? {
        return Ok(Some(v));
    }
    // p and zeta odd/even: p(-u) = p(u)
    if let Some(nu) = fx.negated(u) {
        if fx.num(u).is_none() {
            let f = fx.call(name_of(which), &[nu, a[1]]);
            return Ok(Some(if which == Wf::P { f } else { fx.neg(f) }));
        }
    }
    let args = [u, g2, g3];
    if !numeric_terms(fx, &args) {
        return Ok(None);
    }
    // machine arguments: the double-double kernel (round 29), the ball path when it declines
    if let Some((ms, real)) = crate::fast29::machine_args(fx, &args) {
        let code = match which {
            Wf::P => 0,
            Wf::PPrime => 1,
            Wf::Zeta => 2,
            Wf::Sigma => 3,
        };
        if let Some(v) = crate::fast29::weierstrass(ms[0], ms[1], ms[2], real, code) {
            return Ok(crate::fast29::machine_out(fx, v, real));
        }
    }
    let Some((wk, xs)) = work(fx, &args, 48) else { return Ok(None) };
    let Some(lat) = lattice(fx, xs[1], xs[2], wk.w)? else { return Ok(None) };
    let Some(v) = eval_at(fx, &lat, xs[0], which)? else { return Ok(None) };
    let real = all_real(fx, &args);
    Ok(wk.out(fx, v, real))
}

/// The lattice degenerate to a strip (round 37): exact invariants with g2 != 0 and discriminant g2^3 - 27 g3^2 = 0,
/// that is g2 = 12 c^2, g3 = -8 c^3 with c = -3 g3/(2 g2) (a double root e = c of 4t^3 - g2 t - g3). With
/// k = sqrt(3c) (the case Delta = 0 of Abramowitz and Stegun 18.12; checked by p'^2 = 4p^3 - g2 p - g3): p = c + k^2 csch(k u)^2, p' = -2 k^3 csch(k u)^2
/// coth(k u), zeta = -c u + k coth(k u), sigma = sinh(k u)/k exp(-c u^2/2) (zeta' = -p and sigma'/sigma = zeta,
/// with the poles and the zero at u = 0 normalised as for every lattice). `None` for other invariants.
fn degenerate<'h>(fx: &mut Fx<'_, '_, '_, 'h>, u: T<'h>, g2: T<'h>, g3: T<'h>, which: Wf) -> R<'h> {
    let exact = |t: T<'h>| fx.num(t).is_some_and(|n| n.kind() == Kind::Exact);
    if !(exact(g2) && exact(g3)) || fx.is_int(g2, 0) {
        return Ok(None);
    }
    let disc = fx.eval(fx.sub(fx.pow(g2, fx.int(3)), fx.mul(&[fx.int(27), fx.pow(g3, fx.int(2))])))?;
    if !fx.is_int(disc, 0) {
        return Ok(None);
    }
    let c = fx.eval(fx.div(fx.mul(&[fx.int(-3), g3]), fx.mul(&[fx.int(2), g2])))?;
    let k = fx.eval(fx.call("sqrt", &[fx.mul(&[fx.int(3), c])]))?;
    let ku = fx.mul(&[k, u]);
    let csch2 = fx.pow(fx.call("csch", &[ku]), fx.int(2));
    Ok(Some(match which {
        Wf::P => fx.add(&[c, fx.mul(&[fx.pow(k, fx.int(2)), csch2])]),
        Wf::PPrime => fx.mul(&[fx.int(-2), fx.pow(k, fx.int(3)), csch2, fx.call("coth", &[ku])]),
        Wf::Zeta => fx.add(&[fx.neg(fx.mul(&[c, u])), fx.mul(&[k, fx.call("coth", &[ku])])]),
        Wf::Sigma => {
            let e = fx.call("exp", &[fx.mul(&[fx.rat(-1, 2), c, fx.pow(u, fx.int(2))])]);
            fx.mul(&[fx.call("sinh", &[ku]), fx.pow(k, fx.int(-1)), e])
        }
    }))
}

fn name_of(which: Wf) -> &'static str {
    match which {
        Wf::P => "weierstrass_p",
        Wf::PPrime => "weierstrass_p_prime",
        Wf::Zeta => "weierstrass_zeta",
        Wf::Sigma => "weierstrass_sigma",
    }
}

fn weierstrass_p<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    weierstrass(fx, a, Wf::P)
}
fn weierstrass_p_prime<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    weierstrass(fx, a, Wf::PPrime)
}
fn weierstrass_zeta<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    weierstrass(fx, a, Wf::Zeta)
}
fn weierstrass_sigma<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    weierstrass(fx, a, Wf::Sigma)
}

/// u with p(u) = p: R_F(p - e1, p - e2, p - e3); with [p, q] the sign of u makes p'(u) = q.
fn inverse_weierstrass_p<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let Some((g2, g3)) = invariants(fx, a[1]) else { return Ok(None) };
    let (p, q) = match fx.as_list(a[0]) {
        Some([p, q]) => (*p, Some(*q)),
        Some(_) => return Ok(None),
        None => (a[0], None),
    };
    let mut args = vec![p, g2, g3];
    if let Some(q) = q {
        args.push(q);
    }
    if !numeric_terms(fx, &args) {
        return Ok(None);
    }
    if let Some((ms, real)) = crate::fast29::machine_args(fx, &args) {
        if let Some(v) = crate::fast29::inverse_p(ms[0], ms[1], ms[2], ms.get(3).copied(), real) {
            let r = real && v.im == 0.0;
            return Ok(crate::fast29::machine_out(fx, v, r));
        }
    }
    let extra = 32 + (mag(fx, p).max(1.0).log2() as u64);
    let Some((wk, xs)) = work(fx, &args, extra) else { return Ok(None) };
    let Some((e, _)) = roots(fx, xs[1], xs[2], wk.w)? else { return Ok(None) };
    let p = xs[0];
    let d = [fx.sub(p, e[0]), fx.sub(p, e[1]), fx.sub(p, e[2])];
    let mut ds = [fx.int(0); 3];
    for (o, t) in ds.iter_mut().zip(d) {
        *o = fx.eval(t)?;
    }
    let Some(mut u) = crate::cover28::carlson_rf_at(fx, ds[0], ds[1], ds[2], wk.w)? else { return Ok(None) };
    if all_real(fx, &args) {
        u = chop(fx, u, wk.w);
    }
    // the sign: p'(u) = q, or p'(u) = +sqrt(4 p^3 - g2 p - g3) (principal root) without q
    let target = match q {
        Some(_) => xs[3],
        None => {
            let c = fx.sub(fx.sub(fx.mul(&[fx.int(4), p, p, p]), fx.mul(&[xs[1], p])), xs[2]);
            fx.eval(fx.call("sqrt", &[c]))?
        }
    };
    if let Some(lat) = lattice(fx, xs[1], xs[2], wk.w)? {
        let Some(dp) = eval_at(fx, &lat, u, Wf::PPrime)? else { return Ok(None) };
        let (Some(dpf), Some(tf)) = (cparts(fx, dp), cparts(fx, target)) else { return Ok(None) };
        if cabs(csub(dpf, tf)) > cabs((dpf.0 + tf.0, dpf.1 + tf.1)) {
            u = fx.eval(fx.neg(u))?;
        }
    } else if q.is_some() {
        return Ok(None);
    }
    // real arguments: a negative real u moved by the real period 2 R_F(0, e - e', e - e'') (e the largest real root)
    if q.is_none() && all_real(fx, &args) {
        if let Some((ur, ui)) = cparts(fx, u) {
            if ui == 0.0 && ur < 0.0 {
                let mut ef: Vec<(f64, usize)> = e.iter().enumerate().filter_map(|(k, &r)| match fx.num(r) {
                    Some(Num::Real(x)) => Some((x.to_f64(), k)),
                    _ => None,
                }).collect();
                ef.sort_by(|a, b| b.0.total_cmp(&a.0));
                if let Some(&(_, k)) = ef.first() {
                    let others: Vec<T<'h>> = (0..3).filter(|&j| j != k).map(|j| fx.sub(e[k], e[j])).collect();
                    let o0 = fx.eval(others[0])?;
                    let o1 = fx.eval(others[1])?;
                    if let Some(wr) = crate::cover28::carlson_rf_at(fx, fx.int(0), o0, o1, wk.w)? {
                        u = fx.eval(fx.add(&[u, fx.mul(&[fx.int(2), wr])]))?;
                    }
                }
            }
        }
    }
    if all_real(fx, &args) {
        u = chop(fx, u, wk.w);
    }
    let real = all_real(fx, &args) && cparts(fx, u).is_some_and(|z| z.1 == 0.0);
    Ok(wk.out(fx, u, real))
}

/// A complex number whose imaginary part is below 2^-(w - 32) of its modulus as its real part (R_F of real
/// arguments formed from complex conjugate roots).
fn chop<'h>(fx: &Fx<'_, '_, '_, 'h>, u: T<'h>, w: u64) -> T<'h> {
    match fx.num(u) {
        Some(Num::Complex(x, y)) if y.to_f64().abs() <= 2f64.powi(-(w.min(1000) as i32 - 32)) * x.to_f64().abs().max(1e-300) => fx.ar.number(&Num::Real(x)),
        _ => u,
    }
}

/// The Weierstrass invariants of the curve y^2 = x^3 + a x^2 + b x: with x = X - a/3 the curve is y^2 = X^3 + p X + q,
/// p = b - a^2/3, q = 2 a^3/27 - a b/3, and X = p(u), y = p'(u)/2 for g2 = -4 p, g3 = -4 q.
fn curve_invariants<'h>(fx: &Fx<'_, '_, '_, 'h>, a: T<'h>, b: T<'h>) -> (T<'h>, T<'h>) {
    let g2 = fx.sub(fx.mul(&[fx.rat(4, 3), a, a]), fx.mul(&[fx.int(4), b]));
    let g3 = fx.add(&[fx.mul(&[fx.rat(-8, 27), a, a, a]), fx.mul(&[fx.rat(4, 3), a, b])]);
    (g2, g3)
}

/// elliptic_log([x, y], [a, b]): the integral of dt/(2 y) from infinity to (x, y) on y^2 = x^3 + a x^2 + b x, the u with
/// p(u) = x + a/3 and p'(u) = 2 y for the invariants of [`curve_invariants`].
fn elliptic_log<'h>(fx: &mut Fx<'_, '_, '_, 'h>, args: &[T<'h>]) -> R<'h> {
    let (Some([x, y]), Some([a, b])) = (fx.as_list(args[0]), fx.as_list(args[1])) else { return Ok(None) };
    let (mut x, mut y, mut a, mut b) = (*x, *y, *a, *b);
    // closed forms among numbers (y = -i sqrt(15) with a machine b): numerically at the kind of the numbers
    let nums: Vec<T<'h>> = [x, y, a, b].into_iter().filter(|&t| fx.num(t).is_some()).collect();
    if nums.len() < 4 && numeric_terms(fx, &nums) {
        for t in [&mut x, &mut y, &mut a, &mut b] {
            if fx.num(*t).is_none() {
                *t = crate::cover::numify(fx, *t, &nums)?;
            }
        }
    }
    if !numeric_terms(fx, &[x, y, a, b]) {
        return Ok(None);
    }
    let (g2, g3) = curve_invariants(fx, a, b);
    let p = fx.eval(fx.add(&[x, fx.mul(&[fx.rat(1, 3), a])]))?;
    let q = fx.eval(fx.mul(&[fx.int(2), y]))?;
    let (g2, g3) = (fx.eval(g2)?, fx.eval(g3)?);
    let pq = fx.list(&[p, q]);
    let gs = fx.list(&[g2, g3]);
    let v = fx.eval(fx.call("inverse_weierstrass_p", &[pq, gs]))?;
    Ok(fx.num(v).map(|_| v))
}

/// elliptic_exp(u, [a, b]): the point [x, y] of y^2 = x^3 + a x^2 + b x with elliptic_log([x, y], [a, b]) = u: [p(u) -
/// a/3, p'(u)/2].
fn elliptic_exp<'h>(fx: &mut Fx<'_, '_, '_, 'h>, args: &[T<'h>]) -> R<'h> {
    let Some([a, b]) = fx.as_list(args[1]) else { return Ok(None) };
    let (a, b) = (*a, *b);
    let u = args[0];
    if !numeric_terms(fx, &[u, a, b]) {
        return Ok(None);
    }
    let (g2, g3) = curve_invariants(fx, a, b);
    let (g2, g3) = (fx.eval(g2)?, fx.eval(g3)?);
    let gs = fx.list(&[g2, g3]);
    let p = fx.eval(fx.call("weierstrass_p", &[u, gs]))?;
    let dp = fx.eval(fx.call("weierstrass_p_prime", &[u, gs]))?;
    if fx.num(p).is_none() || fx.num(dp).is_none() {
        return Ok(None);
    }
    let x = fx.eval(fx.sub(p, fx.mul(&[fx.rat(1, 3), a])))?;
    let y = fx.eval(fx.mul(&[fx.rat(1, 2), dp]))?;
    Ok(Some(fx.list(&[x, y])))
}

// ---------------------------------------------------------------- lattice data (round 29)

/// A lattice quantity of the invariants: the half-periods [omega1, omega3], one half-period omega_k, a root e_k =
/// p(omega_k) or a quasi-period eta_k = zeta(omega_k) (k = 1, 2, 3; omega2 = -omega1 - omega3).
#[derive(Clone, Copy, PartialEq)]
enum Lq {
    Periods,
    W(usize),
    E(usize),
    Eta(usize),
}

/// The half-period pair normalized: omega1 with Re > 0 (or Re = 0 and Im < 0).
fn sign_normal<'h>(fx: &mut Fx<'_, '_, '_, 'h>, o1: T<'h>, o3: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let Some(a) = cparts(fx, o1) else { return Ok(None) };
    let eps = 1e-10 * cabs(a);
    if a.0 < -eps || (a.0.abs() <= eps && a.1 > 0.0) {
        return Ok(Some((fx.eval(fx.neg(o1))?, fx.eval(fx.neg(o3))?)));
    }
    Ok(Some((o1, o3)))
}

/// The reduced half-periods of the lattice of (w1, w3): tau = omega3/omega1 in the fundamental domain, Re tau in
/// (-1/2, 1/2], |tau| >= 1, on |tau| = 1 Re tau >= 0; omega1 with Re > 0 (or Re = 0 and Im < 0); at tau = i the pair
/// whose omega1 has the larger real part (then the smaller imaginary part). The boundary
/// choices are fixed by tests (half-periods of the invariants (2, 0), (-2, 0), (-3, 1), (1, 2)).
fn reduce_basis<'h>(fx: &mut Fx<'_, '_, '_, 'h>, w1: T<'h>, w3: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let (mut o1, mut o3) = (w1, w3);
    let eps = 1e-10;
    let tau_of = |fx: &Fx<'_, '_, '_, 'h>, o1: T<'h>, o3: T<'h>| -> Option<C> { Some(cdiv(cparts(fx, o3)?, cparts(fx, o1)?)) };
    let Some(t) = tau_of(fx, o1, o3) else { return Ok(None) };
    if t.1.abs() < 1e-12 * cabs(t) {
        return Ok(None);
    }
    if t.1 < 0.0 {
        o3 = fx.eval(fx.neg(o3))?;
    }
    for _ in 0..200 {
        let Some(t) = tau_of(fx, o1, o3) else { return Ok(None) };
        let n = (t.0 + 0.5 - eps).floor();
        if n != 0.0 {
            o3 = fx.eval(fx.sub(o3, fx.mul(&[fx.int(n as i64), o1])))?;
        }
        let t = (t.0 - n, t.1);
        if cabs(t) < 1.0 - eps {
            let m = fx.eval(fx.neg(o1))?;
            o1 = o3;
            o3 = m;
        } else {
            break;
        }
    }
    let Some((mut o1, mut o3)) = sign_normal(fx, o1, o3)? else { return Ok(None) };
    let Some(t) = tau_of(fx, o1, o3) else { return Ok(None) };
    if (cabs(t) - 1.0).abs() <= eps {
        // on |tau| = 1 the pair (omega3, -omega1) is reduced too: the one whose omega1 has the larger real part, then
        // the one with Re tau > 0, then the smaller imaginary part of omega1
        let m = fx.eval(fx.neg(o1))?;
        let Some((b1, b3)) = sign_normal(fx, o3, m)? else { return Ok(None) };
        let (Some(a), Some(b), Some(tb)) = (cparts(fx, o1), cparts(fx, b1), tau_of(fx, b1, b3)) else { return Ok(None) };
        let tol = eps * cabs(a);
        let take = if (b.0 - a.0).abs() > tol {
            b.0 > a.0
        } else if (tb.0 - t.0).abs() > eps {
            tb.0 > t.0
        } else {
            b.1 < a.1
        };
        if take {
            o1 = b1;
            o3 = b3;
        }
    }
    Ok(Some((o1, o3)))
}

/// A number whose imaginary part is below 2^-(w - 24) of its modulus as its real part.
fn chop_rel<'h>(fx: &Fx<'_, '_, '_, 'h>, u: T<'h>, w: u64) -> T<'h> {
    let small = 2f64.powi(-(w.min(1000) as i32 - 24));
    match fx.num(u) {
        Some(Num::Complex(x, y)) => {
            let m = x.to_f64().hypot(y.to_f64());
            if y.to_f64().abs() <= small * m {
                fx.ar.number(&Num::Real(x))
            } else if x.to_f64().abs() <= small * m && matches!(x, Real::Machine(_)) {
                // a negligible real part (the quarter-turn of real invariants with g3 < 0) as zero
                fx.ar.number(&Num::Complex(Real::Machine(0.0), y))
            } else {
                u
            }
        }
        Some(Num::Real(x)) if x.to_f64().abs() <= small => u,
        _ => u,
    }
}

/// Closed forms at exact invariants: [a, 0] with a > 0 (the lemniscatic case scaled, omega1 = Gamma(1/4)^2/(4
/// sqrt(pi)) a^(-1/4), omega3 = i omega1) and [0, b] with b > 0 (the equianharmonic case, omega1 = Gamma(1/3)^3/(4 pi)
/// b^(-1/6), omega3 = e^(i pi/3) omega1), by p(u; l^4 g2, l^6 g3) = l^-2 p(u/l; g2, g3) (DLMF 23.10.17).
fn lattice_exact<'h>(fx: &Fx<'_, '_, '_, 'h>, g2: T<'h>, g3: T<'h>, q: Lq) -> Option<T<'h>> {
    let (r2, r3) = (fx.rat_of(g2)?, fx.rat_of(g3)?);
    let i = fx.i();
    let pi = fx.sym("pi");
    let half = |k: usize, w1: T<'h>, w3: T<'h>| match k {
        1 => w1,
        3 => w3,
        _ => fx.neg(fx.add(&[w1, w3])),
    };
    if r3.is_zero() && !r2.is_zero() && !r2.num.neg {
        // l = a^(1/4)
        let l = fx.pow(g2, fx.rat(1, 4));
        let w1 = fx.div(fx.mul(&[fx.rat(1, 4), fx.pow(fx.call("gamma", &[fx.rat(1, 4)]), fx.int(2)), fx.pow(pi, fx.rat(-1, 2))]), l);
        let w3 = fx.mul(&[i, w1]);
        let e1 = fx.mul(&[fx.rat(1, 2), fx.sqrt(g2)]);
        let eta1 = fx.mul(&[l, fx.pow(pi, fx.rat(3, 2)), fx.pow(fx.call("gamma", &[fx.rat(1, 4)]), fx.int(-2))]);
        let eta3 = fx.mul(&[fx.neg(i), eta1]);
        return Some(match q {
            Lq::Periods => fx.list(&[w1, w3]),
            Lq::W(k) => half(k, w1, w3),
            Lq::E(1) => e1,
            Lq::E(2) => fx.int(0),
            Lq::E(_) => fx.neg(e1),
            Lq::Eta(k) => half(k, eta1, eta3),
        });
    }
    if r2.is_zero() && !r3.is_zero() && !r3.num.neg {
        let l = fx.pow(g3, fx.rat(1, 6));
        let w1 = fx.div(fx.mul(&[fx.rat(1, 4), fx.pow(fx.call("gamma", &[fx.rat(1, 3)]), fx.int(3)), fx.pow(pi, fx.int(-1))]), l);
        let rho = fx.call("exp", &[fx.mul(&[fx.rat(1, 3), i, pi])]);
        let w3 = fx.mul(&[rho, w1]);
        let e1 = fx.pow(fx.mul(&[fx.rat(1, 4), g3]), fx.rat(1, 3));
        let eta1 = fx.mul(&[l, fx.int(2), fx.pow(pi, fx.int(2)), fx.pow(fx.int(3), fx.rat(-1, 2)), fx.pow(fx.call("gamma", &[fx.rat(1, 3)]), fx.int(-3))]);
        let eta3 = fx.mul(&[fx.call("exp", &[fx.mul(&[fx.rat(-1, 3), i, pi])]), eta1]);
        let cis = |k: i64| fx.call("exp", &[fx.mul(&[fx.rat(2 * k, 3), i, pi])]);
        return Some(match q {
            Lq::Periods => fx.list(&[w1, w3]),
            Lq::W(k) => half(k, w1, w3),
            Lq::E(1) => e1,
            Lq::E(2) => fx.mul(&[cis(1), e1]),
            Lq::E(_) => fx.mul(&[cis(-1), e1]),
            Lq::Eta(k) => half(k, eta1, eta3),
        });
    }
    // the degenerate lattice of real invariants with a double root (g2^3 = 27 g3^2, g3 > 0): roots 2c, -c, -c with
    // c = sqrt(g2/12), p = -c + 3c/sin^2(sqrt(3c) u), omega1 = pi/(2 sqrt(3c)), eta1 = (pi/2) sqrt(c/3); omega3 infinite
    if !r3.is_zero() && !r3.num.neg && r2.mul(&r2).mul(&r2) == r3.mul(&r3).mul(&mc_arith::num::Rat::from_i64(27)) {
        let c = fx.sqrt(fx.mul(&[fx.rat(1, 12), g2]));
        return match q {
            Lq::W(1) => Some(fx.div(pi, fx.mul(&[fx.int(2), fx.sqrt(fx.mul(&[fx.int(3), c]))]))),
            Lq::E(1) => Some(fx.mul(&[fx.int(2), c])),
            Lq::E(_) => Some(fx.neg(c)),
            Lq::Eta(1) => Some(fx.mul(&[fx.rat(1, 2), pi, fx.sqrt(fx.mul(&[fx.rat(1, 3), c]))])),
            _ => None,
        };
    }
    None
}

/// weierstrass_half_periods, _half_period_w1/w2/w3, weierstrass_e1/e2/e3, weierstrass_eta1/eta2/eta3 of [g2, g3]:
/// the reduced half-periods of the lattice of [`lattice`] (real invariants with g3 < 0: those of [g2, -g3] times -i,
/// since p(u; g2, -g3) = -p(i u; g2, g3), DLMF 23.10.18), e_k = p(omega_k), eta_k = zeta(omega_k).
fn lattice_quantity<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], q: Lq) -> R<'h> {
    let Some((g2, g3)) = invariants(fx, a[0]) else { return Ok(None) };
    if let Some(v) = lattice_exact(fx, g2, g3, q) {
        return Ok(Some(v));
    }
    let args = [g2, g3];
    if !numeric_terms(fx, &args) {
        return Ok(None);
    }
    if let Some(v) = lattice_quantity_machine(fx, &args, q)? {
        return Ok(Some(v));
    }
    let Some((wk, xs)) = work(fx, &args, 48) else { return Ok(None) };
    let real = all_real(fx, &args);
    let (g2w, g3w) = (xs[0], xs[1]);
    let neg = real && fx.f64_of(g3w).is_some_and(|v| v < 0.0);
    let g3b = if neg { fx.eval(fx.neg(g3w))? } else { g3w };
    let Some(latb) = lattice(fx, g2w, g3b, wk.w)? else { return Ok(None) };
    let Some((mut o1, mut o3)) = reduce_basis(fx, latb.w1, latb.w3)? else { return Ok(None) };
    if neg {
        let mi = fx.neg(fx.i());
        let r1 = fx.eval(fx.mul(&[mi, o1]))?;
        let r3 = fx.eval(fx.mul(&[mi, o3]))?;
        // the boundary choices are made in the turned frame
        let Some((n1, n3)) = reduce_basis(fx, r1, r3)? else { return Ok(None) };
        o1 = n1;
        o3 = n3;
    }
    let o2 = fx.eval(fx.neg(fx.add(&[o1, o3])))?;
    let om = [o1, o2, o3];
    let w = wk.w;
    let out = |fx: &Fx<'_, '_, '_, 'h>, v: T<'h>| -> Option<T<'h>> {
        let v = chop_rel(fx, v, w);
        let r = matches!(fx.num(v), Some(Num::Real(_)));
        wk.out(fx, v, r).map(|o| chop_rel(fx, o, 53))
    };
    let v = match q {
        Lq::Periods => {
            let (Some(x), Some(y)) = (out(fx, o1), out(fx, o3)) else { return Ok(None) };
            return Ok(Some(fx.list(&[x, y])));
        }
        Lq::W(k) => om[k - 1],
        Lq::E(k) | Lq::Eta(k) => {
            let lat = if neg {
                let Some(l) = lattice(fx, g2w, g3w, wk.w)? else { return Ok(None) };
                l
            } else {
                latb
            };
            let which = if matches!(q, Lq::E(_)) { Wf::P } else { Wf::Zeta };
            let Some(v) = eval_at(fx, &lat, om[k - 1], which)? else { return Ok(None) };
            // a root that vanishes (e2 of [g2, 0]) is zero to the working precision relative to the others
            if which == Wf::P && wk.machine {
                let scale = lat_scale(fx, &lat);
                if mag(fx, v) <= 2f64.powi(-(w.min(1000) as i32 - 24)) * scale {
                    return Ok(Some(fx.real(0.0)));
                }
            }
            v
        }
    };
    Ok(out(fx, v))
}

/// [`lattice_quantity`] at machine invariants (round 37): the lattice of the double-double kernel of `fast29`
/// (the same labelling and AGM as [`lattice`]), its half-periods reduced by [`reduce_basis`] on machine numbers, e_k
/// and eta_k by that kernel's p and zeta at the reduced half-periods. The plots of e1, eta1 and the half-periods over
/// real invariants took 4 ms a point on the ball path and timed out. `None` (the ball path) when the kernel declines.
fn lattice_quantity_machine<'h>(fx: &mut Fx<'_, '_, '_, 'h>, args: &[T<'h>; 2], q: Lq) -> R<'h> {
    let Some((ms, real)) = crate::fast29::machine_args(fx, args) else { return Ok(None) };
    let (g2, g3) = (ms[0], ms[1]);
    let neg = real && g3.re.hi < 0.0;
    let g3b = if neg { -g3 } else { g3 };
    let Some(latb) = crate::fast29::w_lattice(g2, g3b, real) else { return Ok(None) };
    let term = |fx: &Fx<'_, '_, '_, 'h>, z: mc_numeric::machine::C64| fx.ar.number(&Num::Complex(Real::Machine(z.re), Real::Machine(z.im)));
    let (w1, w3) = (term(fx, latb.w1.to_c64()), term(fx, latb.w3.to_c64()));
    let Some((mut o1, mut o3)) = reduce_basis(fx, w1, w3)? else { return Ok(None) };
    if neg {
        let mi = fx.neg(fx.i());
        let r1 = fx.eval(fx.mul(&[mi, o1]))?;
        let r3 = fx.eval(fx.mul(&[mi, o3]))?;
        let Some((n1, n3)) = reduce_basis(fx, r1, r3)? else { return Ok(None) };
        o1 = n1;
        o3 = n3;
    }
    // the reduced pair as integer combinations of the kernel's basis (times -i in the turned frame), recomputed in
    // double-double numbers so that the half-periods and the values at them keep the kernel's accuracy
    use crate::cdd::CD;
    let (b1, b3) = if neg { ((-latb.w1).mul_i(), (-latb.w3).mul_i()) } else { (latb.w1, latb.w3) };
    let (f1, f3) = (b1.to_c64(), b3.to_c64());
    let det = f1.re * f3.im - f3.re * f1.im;
    let combine = |o: T<'h>| -> Option<CD> {
        let (x, y) = cparts(fx, o)?;
        let a = ((x * f3.im - f3.re * y) / det).round();
        let b = ((f1.re * y - x * f1.im) / det).round();
        let v = b1.scale(a) + b3.scale(b);
        let w = v.to_c64();
        ((w.re - x).hypot(w.im - y) <= 1e-9 * x.hypot(y).max(1e-300)).then_some(v)
    };
    let (Some(d1), Some(d3)) = (combine(o1), combine(o3)) else { return Ok(None) };
    let d2 = -(d1 + d3);
    let om = [d1, d2, d3];
    let out = |fx: &Fx<'_, '_, '_, 'h>, v: mc_numeric::machine::C64| -> T<'h> { chop_rel(fx, term(fx, v), 53) };
    let v = match q {
        Lq::Periods => return Ok(Some(fx.list(&[out(fx, d1.to_c64()), out(fx, d3.to_c64())]))),
        Lq::W(k) => om[k - 1].to_c64(),
        Lq::E(k) | Lq::Eta(k) => {
            let lat = if neg {
                let Some(l) = crate::fast29::w_lattice(g2, g3, real) else { return Ok(None) };
                l
            } else {
                latb
            };
            let which = if matches!(q, Lq::E(_)) { 0 } else { 2 };
            let Some(v) = crate::fast29::w_eval(&lat, om[k - 1], which) else { return Ok(None) };
            // a root that vanishes (e2 of [g2, 0]) is zero to the machine precision relative to the others
            let scale = (std::f64::consts::PI / (2.0 * lat.w1.to_c64().re.hypot(lat.w1.to_c64().im))).powi(2);
            if which == 0 && v.re.hypot(v.im) <= 1e-13 * scale {
                return Ok(Some(fx.real(0.0)));
            }
            v
        }
    };
    Ok(Some(out(fx, v)))
}

/// The size of the roots of a lattice, (pi/(2 |omega1|))^2.
fn lat_scale<'h>(fx: &Fx<'_, '_, '_, 'h>, lat: &Lattice<'h>) -> f64 {
    let w1 = cparts(fx, lat.w1).map_or(1.0, cabs);
    (std::f64::consts::PI / (2.0 * w1)).powi(2)
}

/// theta_2(0, q)^4 and theta_4(0, q)^4 at the nome q = e^(i pi tau), from q^(1/4) (DLMF 20.2.2, 20.2.4).
fn theta24<'h>(fx: &mut Fx<'_, '_, '_, 'h>, q4: T<'h>, q: T<'h>, w: u64) -> Res<Option<(T<'h>, T<'h>)>> {
    let Some(qf) = cparts(fx, q) else { return Ok(None) };
    let mq = cabs(qf);
    if mq.is_nan() || mq >= 1.0 {
        return Ok(None);
    }
    let nt = ((w as f64 * std::f64::consts::LN_2 / -mq.ln()).sqrt().ceil() as i64 + 3).max(3);
    let mut t2 = fx.int(0);
    let mut t4 = fx.int(1);
    for n in 0..=nt {
        // q^((n + 1/2)^2) = q^(1/4) q^(n^2 + n)
        let e2 = fx.mul(&[q4, fx.pow(q, fx.int(n * n + n))]);
        t2 = fx.eval(fx.add(&[t2, e2]))?;
        if n >= 1 {
            let e4 = fx.mul(&[fx.int(if n % 2 == 0 { 2 } else { -2 }), fx.pow(q, fx.int(n * n))]);
            t4 = fx.eval(fx.add(&[t4, e4]))?;
        }
    }
    let t2 = fx.eval(fx.mul(&[fx.int(2), t2]))?;
    let p2 = fx.eval(fx.pow(t2, fx.int(4)))?;
    let p4 = fx.eval(fx.pow(t4, fx.int(4)))?;
    Ok((fx.num(p2).is_some() && fx.num(p4).is_some()).then_some((p2, p4)))
}

/// weierstrass_invariants([omega1, omega3]) and its parts: with tau = omega3/omega1 reduced, e1 = c (theta2^4 + 2
/// theta4^4), e2 = c (theta2^4 - theta4^4), e3 = -c (2 theta2^4 + theta4^4), c = pi^2/(12 omega1^2) (DLMF 23.6.2-4),
/// g2 = 2 (e1^2 + e2^2 + e3^2), g3 = 4 e1 e2 e3 (23.3.7-8). Exact half-periods with tau = i or e^(i pi/3) give the
/// closed forms Gamma(1/4)^8/(256 pi^2 omega1^4), 0 and 0, Gamma(1/3)^18/(4096 pi^6 omega1^6).
fn invariants_of<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], part: usize) -> R<'h> {
    let Some([w1, w3]) = fx.as_list(a[0]).map(|l| l.to_vec()).and_then(|l| <[T<'h>; 2]>::try_from(l).ok()) else { return Ok(None) };
    let exact = fx.num(w1).is_none_or(|n| n.is_exact()) && fx.num(w3).is_none_or(|n| n.is_exact());
    if exact {
        let r = fx.eval(fx.call("n", &[fx.div(w3, w1), fx.int(30)]))?;
        if let Some(t) = cparts(fx, r) {
            let pi = fx.sym("pi");
            if cabs(csub(t, (0.0, 1.0))) < 1e-25 {
                let g2 = fx.mul(&[fx.rat(1, 256), fx.pow(fx.call("gamma", &[fx.rat(1, 4)]), fx.int(8)), fx.pow(pi, fx.int(-2)), fx.pow(w1, fx.int(-4))]);
                let g = [g2, fx.int(0)];
                return Ok(Some(if part == 0 { fx.list(&g) } else { g[part - 1] }));
            }
            if cabs(csub(t, (0.5, 0.75f64.sqrt()))) < 1e-25 {
                let g3 = fx.mul(&[fx.rat(1, 4096), fx.pow(fx.call("gamma", &[fx.rat(1, 3)]), fx.int(18)), fx.pow(pi, fx.int(-6)), fx.pow(w1, fx.int(-6))]);
                let g = [fx.int(0), g3];
                return Ok(Some(if part == 0 { fx.list(&g) } else { g[part - 1] }));
            }
        }
        return Ok(None);
    }
    let args = [w1, w3];
    if !numeric_terms(fx, &args) {
        return Ok(None);
    }
    let Some((wk, xs)) = work(fx, &args, 32) else { return Ok(None) };
    let Some((o1, o3)) = reduce_basis(fx, xs[0], xs[1])? else { return Ok(None) };
    let pi = pi_at(fx, wk.w)?;
    let i = fx.i();
    let tau = fx.eval(fx.div(o3, o1))?;
    let q = fx.eval(fx.call("exp", &[fx.mul(&[i, pi, tau])]))?;
    let q4 = fx.eval(fx.call("exp", &[fx.mul(&[fx.rat(1, 4), i, pi, tau])]))?;
    let Some((t2, t4)) = theta24(fx, q4, q, wk.w)? else { return Ok(None) };
    let c = fx.div(fx.mul(&[pi, pi]), fx.mul(&[fx.int(12), o1, o1]));
    let e1 = fx.eval(fx.mul(&[c, fx.add(&[t2, fx.mul(&[fx.int(2), t4])])]))?;
    let e2 = fx.eval(fx.mul(&[c, fx.sub(t2, t4)]))?;
    let e3 = fx.eval(fx.neg(fx.mul(&[c, fx.add(&[fx.mul(&[fx.int(2), t2]), t4])])))?;
    let g2 = fx.eval(fx.mul(&[fx.int(2), fx.add(&[fx.mul(&[e1, e1]), fx.mul(&[e2, e2]), fx.mul(&[e3, e3])])]))?;
    let g3 = fx.eval(fx.mul(&[fx.int(4), e1, e2, e3]))?;
    let w = wk.w;
    let mut g = [g2, g3];
    for v in g.iter_mut() {
        let c = chop_rel(fx, *v, w);
        let r = matches!(fx.num(c), Some(Num::Real(_)));
        let Some(o) = wk.out(fx, c, r) else { return Ok(None) };
        *v = o;
    }
    Ok(Some(if part == 0 { fx.list(&g) } else { g[part - 1] }))
}

/// elliptic_exp_prime(u, [a, b]): the derivative of elliptic_exp in u, [p'(u), p''(u)/2] = [2 y, 3 p^2 - g2/4] with p =
/// x + a/3 for the point [x, y] = elliptic_exp(u, [a, b]) and the invariants of [`curve_invariants`].
fn elliptic_exp_prime<'h>(fx: &mut Fx<'_, '_, '_, 'h>, args: &[T<'h>]) -> R<'h> {
    let Some([a, b]) = fx.as_list(args[1]) else { return Ok(None) };
    let (a, b) = (*a, *b);
    let u = args[0];
    if !numeric_terms(fx, &[u, a, b]) {
        return Ok(None);
    }
    let (g2, g3) = curve_invariants(fx, a, b);
    let (g2, g3) = (fx.eval(g2)?, fx.eval(g3)?);
    let gs = fx.list(&[g2, g3]);
    let p = fx.eval(fx.call("weierstrass_p", &[u, gs]))?;
    let dp = fx.eval(fx.call("weierstrass_p_prime", &[u, gs]))?;
    if fx.num(p).is_none() || fx.num(dp).is_none() {
        return Ok(None);
    }
    let second = fx.eval(fx.sub(fx.mul(&[fx.int(3), p, p]), fx.mul(&[fx.rat(1, 4), g2])))?;
    Ok(Some(fx.list(&[dp, second])))
}

macro_rules! lq {
    ($f:ident, $q:expr) => {
        fn $f<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
            lattice_quantity(fx, a, $q)
        }
    };
}
lq!(half_periods, Lq::Periods);
lq!(half_period_w1, Lq::W(1));
lq!(half_period_w2, Lq::W(2));
lq!(half_period_w3, Lq::W(3));
lq!(weierstrass_e1, Lq::E(1));
lq!(weierstrass_e2, Lq::E(2));
lq!(weierstrass_e3, Lq::E(3));
lq!(weierstrass_eta1, Lq::Eta(1));
lq!(weierstrass_eta2, Lq::Eta(2));
lq!(weierstrass_eta3, Lq::Eta(3));
fn weierstrass_invariants<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    invariants_of(fx, a, 0)
}
fn weierstrass_invariant_g2<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    invariants_of(fx, a, 1)
}
fn weierstrass_invariant_g3<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    invariants_of(fx, a, 2)
}

macro_rules! wdef {
    ($name:literal, $f:ident, $s:literal) => {
        Def { name: $name, arities: &[1], listable: false, exact: Some($f), numeric: None, summary: $s }
    };
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    wdef!("weierstrass_half_periods", half_periods, "weierstrass_half_periods([g2, g3]): the reduced half-periods [omega1, omega3] of the Weierstrass functions of invariants g2, g3"),
    wdef!("weierstrass_half_period_w1", half_period_w1, "weierstrass_half_period_w1([g2, g3]): the half-period omega1 of the invariants g2, g3"),
    wdef!("weierstrass_half_period_w2", half_period_w2, "weierstrass_half_period_w2([g2, g3]): the half-period omega2 = -omega1 - omega3 of the invariants g2, g3"),
    wdef!("weierstrass_half_period_w3", half_period_w3, "weierstrass_half_period_w3([g2, g3]): the half-period omega3 of the invariants g2, g3"),
    wdef!("weierstrass_e1", weierstrass_e1, "weierstrass_e1([g2, g3]): the root e1 = weierstrass_p(omega1, [g2, g3]) of 4 t^3 - g2 t - g3"),
    wdef!("weierstrass_e2", weierstrass_e2, "weierstrass_e2([g2, g3]): the root e2 = weierstrass_p(omega2, [g2, g3]) of 4 t^3 - g2 t - g3"),
    wdef!("weierstrass_e3", weierstrass_e3, "weierstrass_e3([g2, g3]): the root e3 = weierstrass_p(omega3, [g2, g3]) of 4 t^3 - g2 t - g3"),
    wdef!("weierstrass_eta1", weierstrass_eta1, "weierstrass_eta1([g2, g3]): the quasi-period eta1 = weierstrass_zeta(omega1, [g2, g3])"),
    wdef!("weierstrass_eta2", weierstrass_eta2, "weierstrass_eta2([g2, g3]): the quasi-period eta2 = weierstrass_zeta(omega2, [g2, g3])"),
    wdef!("weierstrass_eta3", weierstrass_eta3, "weierstrass_eta3([g2, g3]): the quasi-period eta3 = weierstrass_zeta(omega3, [g2, g3])"),
    wdef!("weierstrass_invariants", weierstrass_invariants, "weierstrass_invariants([omega1, omega3]): the invariants [g2, g3] of the lattice of half-periods omega1, omega3"),
    wdef!("weierstrass_invariant_g2", weierstrass_invariant_g2, "weierstrass_invariant_g2([omega1, omega3]): the invariant g2 of the lattice of half-periods omega1, omega3"),
    wdef!("weierstrass_invariant_g3", weierstrass_invariant_g3, "weierstrass_invariant_g3([omega1, omega3]): the invariant g3 of the lattice of half-periods omega1, omega3"),
    Def { name: "elliptic_exp_prime", arities: &[2], listable: false, exact: Some(elliptic_exp_prime), numeric: None, summary: "elliptic_exp_prime(u, [a, b]): the derivative in u of elliptic_exp(u, [a, b])" },
    Def { name: "weierstrass_p", arities: &[2], listable: false, exact: Some(weierstrass_p), numeric: None, summary: "weierstrass_p(u, [g2, g3]): the Weierstrass elliptic function p(u; g2, g3)" },
    Def { name: "weierstrass_p_prime", arities: &[2], listable: false, exact: Some(weierstrass_p_prime), numeric: None, summary: "weierstrass_p_prime(u, [g2, g3]): the derivative of the Weierstrass elliptic function p(u; g2, g3)" },
    Def { name: "weierstrass_sigma", arities: &[2], listable: false, exact: Some(weierstrass_sigma), numeric: None, summary: "weierstrass_sigma(u, [g2, g3]): the Weierstrass sigma function, sigma'/sigma = zeta" },
    Def { name: "weierstrass_zeta", arities: &[2], listable: false, exact: Some(weierstrass_zeta), numeric: None, summary: "weierstrass_zeta(u, [g2, g3]): the Weierstrass zeta function, zeta' = -p" },
    Def { name: "elliptic_log", arities: &[2], listable: false, exact: Some(elliptic_log), numeric: None, summary: "elliptic_log([x, y], [a, b]): the elliptic logarithm, the integral of dt/(2 y) from infinity to the point (x, y) of y^2 = x^3 + a x^2 + b x" },
    Def { name: "elliptic_exp", arities: &[2], listable: false, exact: Some(elliptic_exp), numeric: None, summary: "elliptic_exp(u, [a, b]): the point [x, y] of y^2 = x^3 + a x^2 + b x whose elliptic logarithm is u" },
    Def { name: "inverse_weierstrass_p", arities: &[2], listable: false, exact: Some(inverse_weierstrass_p), numeric: None, summary: "inverse_weierstrass_p(p, [g2, g3]): a u with weierstrass_p(u, [g2, g3]) = p; inverse_weierstrass_p([p, q], [g2, g3]) also with weierstrass_p_prime(u, [g2, g3]) = q" },
];
