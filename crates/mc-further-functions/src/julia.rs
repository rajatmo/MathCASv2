//! `julia_set_iteration_count`: the number of iterations of a rational map until a point is caught by an attracting
//! cycle (or escapes, for a polynomial).
//!
//! The map f = N/D acts on the Riemann sphere; points are kept in homogeneous coordinates [a : b] (z = a/b) so that
//! poles and infinity need no special case. Every attracting cycle attracts a critical point (Fatou; Milnor,
//! "Dynamics in one complex variable", Thm. 8.6), so the cycles are found by iterating the critical points (the
//! zeros of N'D - ND', and infinity) and detecting a repetition; a cycle is kept when its multiplier, estimated by
//! the contraction of a small perturbation over one period, is below 1.
//!
//! A point z_k is caught when it lies within delta_p = delta / max(1, |f'(p)|)^2 of a point p of an attracting cycle
//! (delta = 1/32; points of a cycle at a pole are not used as targets, the cycle is caught one step later), or, for
//! infinity, when |z_k| > 1/delta. For a polynomial of degree d >= 2 infinity is caught at the escape radius
//! R = max(4, 1 + sum |a_k/a_d|), beyond which every orbit tends to infinity. The count is the first k caught
//! (0 when z itself is), max_iterations + 1 when no k <= max_iterations is.
use crate::num::{self, C};
use mc_arith::num::{Ball, Float};
use mc_arith::{Arith, Kind, Num, Real};
use mc_core::{Store, T};
use mc_eval::{Native, NativeCx, Reply, Res};

const DELTA: f64 = 1.0 / 32.0;

/// A point of the sphere in homogeneous coordinates.
type P = (C, C);

struct Map {
    num: Vec<C>,
    den: Vec<C>,
    /// the degree of the map, max(deg N, deg D)
    deg: usize,
    /// the escape radius when f is a polynomial
    escape: Option<f64>,
    /// the attracting cycle points: (point, catching radius); infinity is (None, 1/delta)
    targets: Vec<(Option<C>, f64)>,
}

fn hom(c: &[C], deg: usize, (a, b): P) -> C {
    // sum c_k a^k b^(deg - k)
    let mut s = (0.0, 0.0);
    let mut ap = (1.0, 0.0);
    let mut bp = vec![(1.0, 0.0); deg + 1];
    for k in 1..=deg {
        bp[k] = num::mul(bp[k - 1], b);
    }
    for k in 0..=deg {
        if let Some(&ck) = c.get(k) {
            s = num::add(s, num::mul(ck, num::mul(ap, bp[deg - k])));
        }
        ap = num::mul(ap, a);
    }
    s
}

fn normalize((a, b): P) -> P {
    let m = num::abs(a).max(num::abs(b));
    if m == 0.0 || !m.is_finite() {
        return (a, b);
    }
    ((a.0 / m, a.1 / m), (b.0 / m, b.1 / m))
}

fn chordal(p: P, q: P) -> f64 {
    // |a1 b2 - a2 b1| / (|p| |q|)
    let x = num::sub(num::mul(p.0, q.1), num::mul(q.0, p.1));
    let np = (num::abs(p.0).powi(2) + num::abs(p.1).powi(2)).sqrt();
    let nq = (num::abs(q.0).powi(2) + num::abs(q.1).powi(2)).sqrt();
    num::abs(x) / (np * nq)
}

fn finite(p: P) -> Option<C> {
    let z = num::div(p.0, p.1);
    (num::abs(p.1) > 1e-300 && z.0.is_finite() && z.1.is_finite()).then_some(z)
}

impl Map {
    fn apply(&self, p: P) -> P {
        normalize((hom(&self.num, self.deg, p), hom(&self.den, self.deg, p)))
    }

    fn new(num: Vec<C>, den: Vec<C>) -> Option<Map> {
        let num = num::trim(num);
        let den = num::trim(den);
        if den.is_empty() {
            return None;
        }
        let deg = num.len().max(den.len()).saturating_sub(1);
        let escape = (den.len() == 1 && num.len() >= 3).then(|| {
            let d = num.len() - 1;
            let lead = num::abs(num[d]);
            4f64.max(1.0 + num[..d].iter().map(|&c| num::abs(c) / lead).sum::<f64>())
        });
        let mut m = Map { num, den, deg, escape, targets: vec![] };
        if deg >= 1 {
            m.find_cycles();
        }
        Some(m)
    }

    fn find_cycles(&mut self) {
        // the critical points: zeros of N'D - ND', and infinity
        let crit = num::psub(&num::pmul(&num::deriv(&self.num), &self.den), &num::pmul(&self.num, &num::deriv(&self.den)));
        let mut starts: Vec<P> = num::roots(&crit).into_iter().map(|z| (z, (1.0, 0.0))).collect();
        starts.push(((1.0, 0.0), (0.0, 0.0)));
        let mut cycles: Vec<Vec<P>> = vec![];
        for s in starts {
            let mut p = normalize(s);
            for _ in 0..4000 {
                p = self.apply(p);
            }
            if !(p.0 .0.is_finite() && p.0 .1.is_finite() && p.1 .0.is_finite() && p.1 .1.is_finite()) {
                continue;
            }
            let mut orbit = vec![p];
            let mut q = p;
            let mut period = None;
            for k in 1..=64 {
                q = self.apply(q);
                if chordal(q, p) < 1e-9 {
                    period = Some(k);
                    break;
                }
                orbit.push(q);
            }
            let Some(per) = period else { continue };
            if cycles.iter().any(|c| c.iter().any(|&r| chordal(r, p) < 1e-7)) {
                continue;
            }
            // the multiplier: contraction of a perturbation over one period
            let h = 1e-7;
            let pert = normalize((num::add(p.0, (h * num::abs(p.1).max(1e-3), 0.0)), num::add(p.1, (-h * num::abs(p.0).max(1e-3), 0.0))));
            let d0 = chordal(pert, p);
            let mut x = pert;
            for _ in 0..per {
                x = self.apply(x);
            }
            let mut y = p;
            for _ in 0..per {
                y = self.apply(y);
            }
            let lam = chordal(x, y) / d0;
            if lam < 0.999 {
                cycles.push(orbit);
            }
        }
        for c in cycles {
            for p in c {
                match finite(p) {
                    // infinity: |z| > 1/delta (for a polynomial the escape radius is used instead)
                    None => {
                        if self.escape.is_none() {
                            self.targets.push((None, 1.0 / DELTA));
                        }
                    }
                    Some(z) => {
                        let dv = num::horner(&self.den, z);
                        if num::abs(dv) <= 1e-12 * num::abs(num::horner(&self.num, z)).max(1e-300) {
                            continue; // a pole: not a target
                        }
                        // f' = (N'D - ND')/D^2
                        let dn = num::horner(&num::deriv(&self.num), z);
                        let dd = num::horner(&num::deriv(&self.den), z);
                        let nv = num::horner(&self.num, z);
                        let fp = num::abs(num::div(num::sub(num::mul(dn, dv), num::mul(nv, dd)), num::mul(dv, dv)));
                        self.targets.push((Some(z), DELTA / fp.max(1.0).powi(2)));
                    }
                }
            }
        }
    }

    fn caught(&self, p: P) -> bool {
        let z = finite(p);
        if let (Some(r), Some(z)) = (self.escape, z) {
            if num::abs(z) > r {
                return true;
            }
        }
        if self.escape.is_some() && z.is_none() {
            return true;
        }
        self.targets.iter().any(|&(t, r)| match (t, z) {
            (Some(t), Some(z)) => num::abs(num::sub(z, t)) < r,
            (None, Some(z)) => num::abs(z) > r,
            (None, None) => true,
            (Some(_), None) => false,
        })
    }

    fn count(&self, z: C, max: u64) -> u64 {
        let mut p = (z, (1.0, 0.0));
        for k in 0..=max {
            if self.caught(p) {
                return k;
            }
            p = self.apply(p);
            if !(p.0 .0.is_finite() && p.0 .1.is_finite()) {
                return max + 1;
            }
        }
        max + 1
    }
}

// ---------------------------------------------------------------- verified counts (round 36)

/// A complex ball: real and imaginary parts.
type CB = (Ball, Ball);

/// The parts of an exact, ball or machine number as reals (a machine real is the dyadic number it is).
fn parts_of<'h>(h: &'h mc_core::Heap, t: T<'h>) -> Option<(Real, Real)> {
    match Arith::new(h).num(t)? {
        Num::Real(r) => Some((r, Real::Exact(mc_arith::num::Rat::from_int(mc_arith::num::Int::from_i64(0))))),
        Num::Complex(a, b) => Some((a, b)),
    }
}

fn is_machine(r: &Real) -> bool {
    matches!(r, Real::Machine(_))
}

fn to_ball(r: &Real, p: u64) -> Option<Ball> {
    let r = match r {
        Real::Machine(v) => {
            let (neg, man, exp) = mc_arith::num::float::f64_to_dyadic(*v)?;
            return Some(Ball::exact(Float::new(neg, man, exp)));
        }
        r => r.to_kind(Kind::Ball, p),
    };
    match r {
        Real::Ball(b, _) => Some(b),
        _ => None,
    }
}

fn ball_f64(v: f64) -> Ball {
    to_ball(&Real::Machine(v), 64).unwrap_or_else(|| Ball::from_i64(0))
}

fn cb_mul(a: &CB, b: &CB, p: u64) -> CB {
    (a.0.mul(&b.0, p).sub(&a.1.mul(&b.1, p), p), a.0.mul(&b.1, p).add(&a.1.mul(&b.0, p), p))
}

fn abs2(a: &CB, p: u64) -> Ball {
    a.0.mul(&a.0, p).add(&a.1.mul(&a.1, p), p)
}

/// The count of a polynomial map (coefficients `coef`, constant first) from `z` in complex ball arithmetic at `p`
/// bits: every decision (escape beyond the radius, entry into a target disc) is taken only when the ball is on one
/// side; `None` when one is undecided (the precision is too low for this orbit).
fn ball_run<'h>(cx: &mut NativeCx<'_, '_, 'h>, map: &Map, coef: &[(Real, Real)], z: &(Real, Real), max: u64, p: u64) -> Res<Option<u64>> {
    let bc = |c: &(Real, Real)| Some((to_ball(&c.0, p)?, to_ball(&c.1, p)?));
    let Some(cs) = coef.iter().map(bc).collect::<Option<Vec<CB>>>() else { return Ok(None) };
    let Some(mut w) = bc(z) else { return Ok(None) };
    let Some(r) = map.escape else { return Ok(None) };
    let r2 = ball_f64(r * r);
    let targets: Vec<(CB, Ball)> =
        map.targets.iter().filter_map(|&(t, rad)| t.map(|t| ((ball_f64(t.0), ball_f64(t.1)), ball_f64(rad * rad)))).collect();
    for k in 0..=max {
        if k % 256 == 255 {
            cx.tick(1)?;
        }
        match abs2(&w, p).cmp3(&r2) {
            Some(std::cmp::Ordering::Greater) => return Ok(Some(k)),
            Some(_) => {}
            None => return Ok(None),
        }
        for (t, rr) in &targets {
            let dz = (w.0.sub(&t.0, p), w.1.sub(&t.1, p));
            match abs2(&dz, p).cmp3(rr) {
                Some(std::cmp::Ordering::Less) => return Ok(Some(k)),
                Some(_) => {}
                None => return Ok(None),
            }
        }
        // Horner: c_d z^d + ... + c_0
        let mut acc = cs[cs.len() - 1].clone();
        for c in cs[..cs.len() - 1].iter().rev() {
            let m = cb_mul(&acc, &w, p);
            acc = (m.0.add(&c.0, p), m.1.add(&c.1, p));
        }
        w = acc;
    }
    Ok(Some(max + 1))
}

/// The verified count: [`ball_run`] at p0 bits, then at doubled precisions up to 2^16 bits; `None` when the orbit
/// stays undecided (the count is then not given).
fn verified_count<'h>(cx: &mut NativeCx<'_, '_, 'h>, map: &Map, coef: &[(Real, Real)], z: &(Real, Real), max: u64, p0: u64) -> Res<Option<u64>> {
    let mut p = p0.max(64);
    while p <= 1 << 16 {
        if let Some(k) = ball_run(cx, map, coef, z, max, p)? {
            return Ok(Some(k));
        }
        p *= 2;
    }
    Ok(None)
}

/// How points are counted: in machine arithmetic, or verified in ball arithmetic (exact points of a polynomial
/// map, or any point with a working precision given).
struct Mode {
    /// the exact coefficients of a polynomial map (denominator 1)
    coef: Option<Vec<(Real, Real)>>,
    /// the starting precision in bits when one was given
    prec: Option<u64>,
}

/// `c30_julia_counts(num, den, points, max)`, `c30_julia_counts(num, den, points, max, working_precision)`: the
/// counts of the points (a number or nested lists of numbers) under N/D given by coefficient lists, constant first.
/// For a polynomial map, a point given exactly (or any point with a working precision in digits) is iterated in
/// ball arithmetic with the precision raised until every decision is certain (machine arithmetic loses an orbit
/// that stays near the boundary: i + 10^-309 under z^2 escapes after 2056 steps, rounding moved it out after about
/// a thousand).
pub struct JuliaCounts;

impl Native for JuliaCounts {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (n, d, pts, mx, wp) = match *args {
            [n, d, pts, mx] => (n, d, pts, mx, None),
            [n, d, pts, mx, wp] => (n, d, pts, mx, Some(wp)),
            _ => return Ok(Reply::Decline),
        };
        let prec = match wp.map(|w| h.view(w)) {
            None => None,
            Some(mc_core::View::Sym(s)) if h.sym_name(s) == "machine" => None,
            Some(mc_core::View::Int(k)) if (1..=20_000).contains(&k) => Some((k as f64 * std::f64::consts::LOG2_10).ceil() as u64 + 8),
            Some(_) => return Ok(Reply::Decline),
        };
        let exact_coef: Option<Vec<(Real, Real)>> = cx.as_list(n).and_then(|cs| cs.iter().map(|&c| parts_of(h, c)).collect());
        let (Some(nv), Some(dv)) = (num::complexes_of(cx, n)?, num::complexes_of(cx, d)?) else { return Ok(Reply::Decline) };
        let max = match h.view(mx) {
            mc_core::View::Int(m) if m >= 0 => m as u64,
            _ => return Ok(Reply::Decline),
        };
        let polynomial = dv == [(1.0, 0.0)];
        let Some(map) = Map::new(nv, dv) else { return Ok(Reply::Decline) };
        let coef = exact_coef.filter(|c| polynomial && map.escape.is_some() && !c.is_empty());
        let mode = Mode { coef, prec };
        let mut tick = 0u64;
        match walk(cx, &map, &mode, pts, max, &mut tick)? {
            Some(t) => Ok(Reply::Done(t)),
            None => Ok(Reply::Decline),
        }
    }
}

fn walk<'h>(cx: &mut NativeCx<'_, '_, 'h>, map: &Map, mode: &Mode, t: T<'h>, max: u64, tick: &mut u64) -> Res<Option<T<'h>>> {
    if let Some(items) = cx.as_list(t).map(|l| l.to_vec()) {
        let mut out = Vec::with_capacity(items.len());
        for e in items {
            match walk(cx, map, mode, e, max, tick)? {
                Some(v) => out.push(v),
                None => return Ok(None),
            }
        }
        return Ok(Some(cx.list(&out)));
    }
    if let Some(coef) = &mode.coef {
        let h = cx.heap();
        if let Some(zp) = parts_of(h, t) {
            if mode.prec.is_some() || !(is_machine(&zp.0) || is_machine(&zp.1)) {
                return Ok(verified_count(cx, map, coef, &zp, max, mode.prec.unwrap_or(128))?.map(|k| h.int(k as i64)));
            }
        }
    }
    let Some(z) = num::complex_of(cx, t)? else { return Ok(None) };
    let k = map.count(z, max);
    *tick += k + 1;
    if *tick > 20_000 {
        cx.tick(*tick / 1000)?;
        *tick = 0;
    }
    Ok(Some(cx.heap().int(k as i64)))
}
