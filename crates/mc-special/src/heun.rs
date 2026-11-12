//! The Heun functions: `heun_g`, `heun_c`, `heun_b`, `heun_d`, `heun_t`, the solutions of the
//! general, confluent, biconfluent, doubly-confluent and triconfluent Heun equations normalized at a point
//! (DLMF 31.2, 31.12), written as A(z) y'' + B(z) y' + C(z) y = 0 with polynomial A, B, C:
//!
//! - `heun_g(a, q, alpha, beta, gamma, delta, z)`: z (z - 1)(z - a) y'' + (gamma (z - 1)(z - a) + delta z (z - a) +
//!   epsilon z (z - 1)) y' + (alpha beta z - q) y = 0, epsilon = alpha + beta + 1 - gamma - delta, y(0) = 1 (the
//!   local solution of exponent 0 at the regular singularity 0, DLMF 31.3.1);
//! - `heun_c(q, alpha, gamma, delta, epsilon, z)`: z (z - 1) y'' + (gamma (z - 1) + delta z + epsilon z (z - 1)) y' +
//!   (alpha z - q) y = 0, y(0) = 1;
//! - `heun_b(q, alpha, gamma, delta, epsilon, z)`: z y'' + (gamma + delta z + epsilon z^2) y' + (alpha z - q) y = 0,
//!   y(0) = 1;
//! - `heun_d(q, alpha, gamma, delta, epsilon, z)`: z^2 y'' + (epsilon z^2 + delta z + gamma) y' + (alpha z - q) y = 0,
//!   y(1) = 1, y'(1) = 0;
//! - `heun_t(q, alpha, gamma, delta, epsilon, z)`: y'' + (gamma + delta z + epsilon z^2) y' + (alpha z - q) y = 0,
//!   y(0) = 1, y'(0) = 0.
//!
//! Method: the Taylor (or, at a regular singular start, Frobenius) series from the recurrence of the polynomial
//! equation, summed at the working precision, and analytic continuation along the segment from the start to z by
//! re-expansion at points at most half the distance to the nearest singularity (and at most 1/2 apart); a segment
//! passing within 0.3 of a finite singularity goes around it on the side of z (above for a real z or a z in the upper
//! half plane: the principal values on the cuts are the limits from the upper half plane; below for Im z < 0).
use crate::cover::{mag, parts, work};
use mc_api::T;
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;
type C = (f64, f64);

/// The equation: coefficients (in powers of z) of A, B, C.
struct Eq<'h> {
    a: Vec<T<'h>>,
    b: Vec<T<'h>>,
    c: Vec<T<'h>>,
}

/// The coefficients of p(z0 + t) in powers of t (the Taylor shift by repeated synthetic division).
fn shift<'h>(fx: &mut Fx<'_, '_, '_, 'h>, p: &[T<'h>], z0: T<'h>) -> Res<Vec<T<'h>>> {
    let mut c = p.to_vec();
    let n = c.len();
    for i in 0..n {
        for j in (i..n.saturating_sub(1)).rev() {
            c[j] = fx.eval(fx.add(&[c[j], fx.mul(&[z0, c[j + 1]])]))?;
        }
    }
    Ok(c)
}

/// The most terms of one series.
const MAX_TERMS: usize = 6000;

/// The value and derivative at z0 + h of the solution with the series coefficients y_k at z0, `None` if the series
/// does not settle. `regular`: z0 is a regular singular point (A(z0) = 0) and the solution is the one of exponent 0
/// with y(z0) = 1; otherwise y(z0) = y0, y'(z0) = d0.
#[allow(clippy::too_many_arguments)]
fn step<'h>(fx: &mut Fx<'_, '_, '_, 'h>, eq: &Eq<'h>, z0: T<'h>, h: T<'h>, y0: T<'h>, d0: T<'h>, regular: bool, w: u64) -> Res<Option<(T<'h>, T<'h>)>> {
    let a = shift(fx, &eq.a, z0)?;
    let b = shift(fx, &eq.b, z0)?;
    let c = shift(fx, &eq.c, z0)?;
    let at = |v: &[T<'h>], i: usize| v.get(i).copied();
    let mut y: Vec<T<'h>> = if regular { vec![fx.int(1)] } else { vec![y0, d0] };
    let eps = 2f64.powi(-(w.min(1000) as i32));
    let mag_h = mag(fx, h);
    let mut val = fx.int(0);
    let mut der = fx.int(0);
    let mut hk = fx.int(1); // h^k
    let mut hk1 = fx.int(0); // k h^(k-1)
    let mut small = 0;
    for k in 0..MAX_TERMS {
        if k % 64 == 63 {
            fx.tick(1)?;
        }
        // the coefficient y_k from the equation of t^n, n = k - 2 (ordinary) or k - 1 (regular)
        if k >= y.len() {
            let n = if regular { k - 1 } else { k - 2 };
            let mut rest = vec![];
            let mut lead = None;
            for i in 0..a.len().max(b.len()).max(c.len()) {
                // A_i (n - i + 2)(n - i + 1) y_(n-i+2)
                if let Some(ai) = at(&a, i) {
                    if n + 2 >= i {
                        let j = n + 2 - i;
                        let f = (j * (j.saturating_sub(1))) as i64;
                        if j > k {
                            // A(z0) = 0 at a regular singular start
                            if fx.num(ai).is_none_or(|v| !v.is_zero()) {
                                return Ok(None);
                            }
                        } else if j == k {
                            lead = Some(fx.add(&[lead.unwrap_or(fx.int(0)), fx.mul(&[ai, fx.int(f)])]));
                        } else if f != 0 {
                            rest.push(fx.mul(&[ai, fx.int(f), y[j]]));
                        }
                    }
                }
                if let Some(bi) = at(&b, i) {
                    if n + 1 >= i {
                        let j = n + 1 - i;
                        if j == k {
                            lead = Some(fx.add(&[lead.unwrap_or(fx.int(0)), fx.mul(&[bi, fx.int(j as i64)])]));
                        } else if j > 0 {
                            rest.push(fx.mul(&[bi, fx.int(j as i64), y[j]]));
                        }
                    }
                }
                if let Some(ci) = at(&c, i) {
                    if n >= i {
                        let j = n - i;
                        if j == k {
                            lead = Some(fx.add(&[lead.unwrap_or(fx.int(0)), ci]));
                        } else {
                            rest.push(fx.mul(&[ci, y[j]]));
                        }
                    }
                }
            }
            let Some(lead) = lead else { return Ok(None) };
            let lead = fx.eval(lead)?;
            if fx.num(lead).is_none_or(|v| v.is_zero()) {
                return Ok(None);
            }
            let yk = fx.eval(fx.neg(fx.div(fx.add(&rest), lead)))?;
            if fx.num(yk).is_none() {
                return Ok(None);
            }
            y.push(yk);
        }
        let t = fx.eval(fx.mul(&[y[k], hk]))?;
        val = fx.eval(fx.add(&[val, t]))?;
        if k > 0 {
            der = fx.eval(fx.add(&[der, fx.mul(&[y[k], hk1])]))?;
        }
        hk1 = fx.eval(fx.mul(&[fx.int(k as i64 + 1), hk]))?;
        hk = fx.eval(fx.mul(&[hk, h]))?;
        let mt = mag(fx, t).max(mag(fx, fx.mul(&[y[k], hk1])) * mag_h);
        if k >= 4 && mt <= eps * (mag(fx, val) + mag(fx, der) * mag_h).max(1e-300) {
            small += 1;
            if small >= 3 {
                return Ok(Some((val, der)));
            }
        } else {
            small = 0;
        }
    }
    Ok(None)
}

/// The path from `start` to `z`: the corner points (a detour above every finite singularity the segment passes within
/// 0.3 of), `None` when z is a singularity.
pub(crate) fn path(start: C, z: C, sing: &[C]) -> Option<Vec<C>> {
    let d = |p: C, q: C| (p.0 - q.0).hypot(p.1 - q.1);
    if sing.iter().any(|&s| d(s, z) < 1e-12) {
        return None;
    }
    let mut pts = vec![start];
    // singularities near the segment, in the order met
    let (dx, dy) = (z.0 - start.0, z.1 - start.1);
    let len2 = dx * dx + dy * dy;
    let mut near: Vec<(f64, C)> = vec![];
    for &s in sing {
        if d(s, start) < 1e-12 || len2 == 0.0 {
            continue;
        }
        let t = ((s.0 - start.0) * dx + (s.1 - start.1) * dy) / len2;
        if t > 0.0 && t < 1.0 {
            let p = (start.0 + t * dx, start.1 + t * dy);
            if d(p, s) < 0.3 {
                near.push((t, s));
            }
        }
    }
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    // the detour keeps to the side of z (a target in the lower half plane is reached without crossing the real axis;
    // a real z beyond a singularity is the limit from above)
    let side = if z.1 < 0.0 { -0.35 } else { 0.35 };
    for (_, s) in near {
        pts.push((s.0 - 0.35, s.1 + side));
        pts.push((s.0 + 0.35, s.1 + side));
    }
    pts.push(z);
    Some(pts)
}

/// A machine pair as a number at w bits.
fn num_c<'h>(fx: &Fx<'_, '_, '_, 'h>, p: C, w: u64) -> T<'h> {
    use mc_arith::num::Ball;
    use mc_arith::{Num, Real};
    let b = |v: f64| Real::Ball(Ball::exact(crate::ball::float_of(v)), w);
    if p.1 == 0.0 {
        fx.ar.number(&Num::Real(b(p.0)))
    } else {
        fx.ar.number(&Num::Complex(b(p.0), b(p.1)))
    }
}

/// The solution at z (a working-precision number) continued from `start`: `regular` as in [`step`], else y(start)
/// = 1, y'(start) = 0.
fn solve<'h>(fx: &mut Fx<'_, '_, '_, 'h>, eq: &Eq<'h>, start: C, regular: bool, z: T<'h>, sing: &[C], w: u64) -> R<'h> {
    let Some(zf) = parts(fx, z) else { return Ok(None) };
    let Some(pts) = path(start, zf, sing) else { return Ok(None) };
    let d = |p: C, q: C| (p.0 - q.0).hypot(p.1 - q.1);
    let mut cur = start;
    let mut cur_t = num_c(fx, start, w);
    let mut y = fx.int(1);
    let mut dy = fx.int(0);
    let mut first = regular;
    let mut steps = 0;
    for (i, &target) in pts.iter().enumerate().skip(1) {
        let last = i == pts.len() - 1;
        loop {
            // the radius: the distance to the nearest singularity other than the current point
            let r = sing.iter().map(|&s| d(s, cur)).filter(|&x| x > 1e-12).fold(f64::INFINITY, f64::min);
            let hmax = (0.5 * r).min(0.5);
            let dist = d(cur, target);
            let (next, next_t) = if dist <= hmax {
                (target, if last { crate::cover::mid_term(fx, z) } else { num_c(fx, target, w) })
            } else {
                let f = hmax / dist;
                let p = (cur.0 + f * (target.0 - cur.0), cur.1 + f * (target.1 - cur.1));
                (p, num_c(fx, p, w))
            };
            let h = fx.eval(fx.sub(next_t, cur_t))?;
            let Some((v, dv)) = step(fx, eq, cur_t, h, y, dy, first, w)? else { return Ok(None) };
            first = false;
            // midpoints: the radii of a ball continuation grow by the wrapping effect far beyond the error (the
            // error is estimated by the caller from two working precisions)
            y = crate::cover::mid_term(fx, v);
            dy = crate::cover::mid_term(fx, dv);
            cur = next;
            cur_t = next_t;
            steps += 1;
            if steps > 4000 {
                return Ok(None);
            }
            if next == target {
                break;
            }
        }
    }
    Ok(Some(y))
}

/// A ball number carried at w bits (other terms unchanged).
fn retag<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, w: u64) -> T<'h> {
    use mc_arith::{Num, Real};
    let g = |x: Real| match x {
        Real::Ball(b, _) => Real::Ball(b, w),
        x => x,
    };
    match fx.num(t) {
        Some(Num::Real(x)) => fx.ar.number(&Num::Real(g(x))),
        Some(Num::Complex(x, y)) => fx.ar.number(&Num::Complex(g(x), g(y))),
        None => t,
    }
}

/// Every argument a number and one inexact?
fn numeric_args<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> bool {
    crate::cover::numeric(fx, a)
}

/// Which Heun function.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    G,
    C,
    B,
    D,
    T,
}

fn heun<'h>(fx: &mut Fx<'_, '_, '_, 'h>, args: &[T<'h>], kind: Kind) -> R<'h> {
    let z = *args.last().unwrap();
    // the normalization point
    let (start, regular) = match kind {
        Kind::D => ((1.0, 0.0), false),
        Kind::T => ((0.0, 0.0), false),
        _ => ((0.0, 0.0), true),
    };
    if (kind == Kind::D && fx.is_int(z, 1)) || (kind != Kind::D && fx.is_int(z, 0)) {
        return Ok(Some(fx.int(1)));
    }
    if !numeric_args(fx, args) {
        return Ok(None);
    }
    // machine arguments: the double-double kernel (round 29), the ball path when it declines
    if let Some((ms, real)) = crate::fast29::machine_args(fx, args) {
        let code = match kind {
            Kind::G => 0,
            Kind::C => 1,
            Kind::B => 2,
            Kind::D => 3,
            Kind::T => 4,
        };
        let n = ms.len();
        if let Some((v, detour)) = crate::fast29::heun(code, &ms[..n - 1], ms[n - 1]) {
            return Ok(crate::fast29::machine_out(fx, v, real && !detour));
        }
    }
    let Some(zf) = parts(fx, z) else { return Ok(None) };
    // the working precision doubled while the value lacks the digits of the arguments' kind (the re-expansions near an
    // irregular singularity cancel many bits)
    let mut extra = 40 + (zf.0.hypot(zf.1) * 8.0) as u64;
    // (balls with radii cannot always reach the digits: the last value is then returned with the radius it has)
    let mut last = None;
    for _ in 0..4 {
        let Some((v, ok)) = heun_at(fx, args, kind, start, regular, zf, extra)? else { return Ok(None) };
        if ok {
            return Ok(Some(v));
        }
        last = Some(v);
        extra = extra * 2 + 64;
    }
    Ok(last)
}

/// The equation and its finite singularities for the parameters `xs` (z last, unused).
fn build<'h>(fx: &mut Fx<'_, '_, '_, 'h>, xs: &[T<'h>], kind: Kind) -> Res<Option<(Eq<'h>, Vec<C>)>> {
    let int = |fx: &Fx<'_, '_, '_, 'h>, v: i64| fx.int(v);
    let (eq, sing): (Eq<'h>, Vec<C>) = match kind {
        Kind::G => {
            let [a, q, al, be, ga, de, _] = xs[..] else { return Ok(None) };
            let ep = fx.eval(fx.sub(fx.add(&[al, be, int(fx, 1)]), fx.add(&[ga, de])))?;
            // z(z - 1)(z - a) = z^3 - (1 + a) z^2 + a z
            let a_p = vec![int(fx, 0), a, fx.neg(fx.add(&[int(fx, 1), a])), int(fx, 1)];
            // gamma (z^2 - (1 + a) z + a) + delta (z^2 - a z) + epsilon (z^2 - z)
            let b0 = fx.mul(&[ga, a]);
            let b1 = fx.neg(fx.add(&[fx.mul(&[ga, fx.add(&[int(fx, 1), a])]), fx.mul(&[de, a]), ep]));
            let b2 = fx.add(&[ga, de, ep]);
            let c_p = vec![fx.neg(q), fx.mul(&[al, be])];
            let Some(af) = parts(fx, a) else { return Ok(None) };
            (Eq { a: a_p, b: vec![b0, b1, b2], c: c_p }, vec![(0.0, 0.0), (1.0, 0.0), af])
        }
        Kind::C => {
            let [q, al, ga, de, ep, _] = xs[..] else { return Ok(None) };
            // z^2 - z; gamma (z - 1) + delta z + epsilon (z^2 - z); alpha z - q
            let a_p = vec![int(fx, 0), int(fx, -1), int(fx, 1)];
            let b_p = vec![fx.neg(ga), fx.sub(fx.add(&[ga, de]), ep), ep];
            (Eq { a: a_p, b: b_p, c: vec![fx.neg(q), al] }, vec![(0.0, 0.0), (1.0, 0.0)])
        }
        Kind::B => {
            let [q, al, ga, de, ep, _] = xs[..] else { return Ok(None) };
            (Eq { a: vec![int(fx, 0), int(fx, 1)], b: vec![ga, de, ep], c: vec![fx.neg(q), al] }, vec![(0.0, 0.0)])
        }
        Kind::D => {
            let [q, al, ga, de, ep, _] = xs[..] else { return Ok(None) };
            (Eq { a: vec![int(fx, 0), int(fx, 0), int(fx, 1)], b: vec![ga, de, ep], c: vec![fx.neg(q), al] }, vec![(0.0, 0.0)])
        }
        Kind::T => {
            let [q, al, ga, de, ep, _] = xs[..] else { return Ok(None) };
            (Eq { a: vec![int(fx, 1)], b: vec![ga, de, ep], c: vec![fx.neg(q), al] }, vec![])
        }
    };
    let mut eq = eq;
    for v in eq.a.iter_mut().chain(eq.b.iter_mut()).chain(eq.c.iter_mut()) {
        let e = fx.eval(*v)?;
        *v = crate::cover::mid_term(fx, e);
    }
    Ok(Some((eq, sing)))
}

/// The radius of a ball number (the larger of its parts), 0 for other terms.
fn radius<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> f64 {
    use mc_arith::{Num, Real};
    let r = |x: &Real| match x {
        Real::Ball(b, _) => crate::ball::f64_of(&b.rad.to_float()),
        _ => 0.0,
    };
    match fx.num(t) {
        Some(Num::Real(x)) => r(&x),
        Some(Num::Complex(x, y)) => r(&x).max(r(&y)),
        None => 0.0,
    }
}

/// The value at the working precision with `extra` bits, and whether it has the digits asked for. The continuation
/// runs on midpoints; the radius is twice the difference of the values at w and w + 48 bits, plus the effect of the
/// arguments' radii, |v(x_i + K r_i) - v(x_i)|/K for each argument with a radius r_i (K = 2^16, a first-order
/// estimate).
#[allow(clippy::too_many_arguments)]
fn heun_at<'h>(fx: &mut Fx<'_, '_, '_, 'h>, args: &[T<'h>], kind: Kind, start: C, regular: bool, zf: C, extra: u64) -> Res<Option<(T<'h>, bool)>> {
    let Some((wk, xs)) = work(fx, args, extra) else { return Ok(None) };
    let w = wk.w;
    let mids: Vec<T<'h>> = xs.iter().map(|&t| crate::cover::mid_term(fx, t)).collect();
    let Some((eq, sing)) = build(fx, &mids, kind)? else { return Ok(None) };
    let zt = mids[mids.len() - 1];
    let Some(v1) = solve(fx, &eq, start, regular, zt, &sing, w)? else { return Ok(None) };
    let w2 = w + 48;
    let mids2: Vec<T<'h>> = mids.iter().map(|&t| retag(fx, t, w2)).collect();
    let Some((eq2, _)) = build(fx, &mids2, kind)? else { return Ok(None) };
    let Some(v2) = solve(fx, &eq2, start, regular, mids2[mids2.len() - 1], &sing, w2)? else { return Ok(None) };
    let (v1, v2) = (crate::cover::mid_term(fx, v1), crate::cover::mid_term(fx, v2));
    let diff = fx.eval(fx.sub(v1, v2))?;
    let mut err = 2.0 * mag(fx, diff) + mag(fx, v2) * 2f64.powi(-(w.min(1000) as i32));
    let k = 65536.0;
    for i in 0..xs.len() {
        let r = radius(fx, xs[i]);
        if r == 0.0 {
            continue;
        }
        let mut ys = mids.clone();
        let Some(xf) = parts(fx, mids[i]) else { return Ok(None) };
        let _ = xf;
        let dx = num_c(fx, (r * k, 0.0), w);
        ys[i] = fx.eval(fx.add(&[mids[i], dx]))?;
        let Some((eq3, sing3)) = build(fx, &ys, kind)? else { return Ok(None) };
        let Some(v3) = solve(fx, &eq3, start, regular, ys[ys.len() - 1], &sing3, w)? else { return Ok(None) };
        let v3 = crate::cover::mid_term(fx, v3);
        let d = fx.eval(fx.sub(v3, v1))?;
        err += 2.0 * mag(fx, d) / k;
    }
    if !err.is_finite() {
        return Ok(None);
    }
    let m = mc_arith::num::Mag::from_float_up(&crate::ball::float_of(err.max(1e-300)));
    let v = crate::cover::map_balls(fx, v2, |b| crate::ball::widen(b.clone(), &m));
    let goal = if wk.machine { 16 } else { ((wk.p as f64 * std::f64::consts::LOG10_2) as u32).saturating_sub(2) };
    let ok = crate::cover::accurate(fx, v, goal);
    // real parameters and a real z on a path along the real axis without a detour give real values
    let detour = path(start, zf, &sing).is_some_and(|p| p.len() > 2);
    let real = wk.real && !detour;
    Ok(wk.out(fx, v, real).map(|v| (v, ok)))
}

fn heun_g<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    heun(fx, a, Kind::G)
}
fn heun_c<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    heun(fx, a, Kind::C)
}
fn heun_b<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    heun(fx, a, Kind::B)
}
fn heun_d<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    heun(fx, a, Kind::D)
}
fn heun_t<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    heun(fx, a, Kind::T)
}

macro_rules! def {
    ($name:literal, $ar:expr, $e:expr, $s:literal) => {
        Def { name: $name, arities: $ar, listable: true, exact: Some($e), numeric: None, summary: $s }
    };
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    def!("heun_g", &[7], heun_g, "heun_g(a, q, alpha, beta, gamma, delta, z): the local Heun function, the solution of the general Heun equation with y(0) = 1"),
    def!("heun_c", &[6], heun_c, "heun_c(q, alpha, gamma, delta, epsilon, z): the confluent Heun function, the solution of the confluent Heun equation with y(0) = 1"),
    def!("heun_b", &[6], heun_b, "heun_b(q, alpha, gamma, delta, epsilon, z): the biconfluent Heun function, the solution of the biconfluent Heun equation with y(0) = 1"),
    def!("heun_d", &[6], heun_d, "heun_d(q, alpha, gamma, delta, epsilon, z): the doubly-confluent Heun function, the solution with y(1) = 1, y'(1) = 0"),
    def!("heun_t", &[6], heun_t, "heun_t(q, alpha, gamma, delta, epsilon, z): the triconfluent Heun function, the solution with y(0) = 1, y'(0) = 0"),
];
