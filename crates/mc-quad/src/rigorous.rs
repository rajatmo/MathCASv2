//! Guaranteed enclosures (`rigorous: true`, decision N2.10): Clenshaw–Curtis quadrature on balls with a bound of
//! its error from a bound of the integrand on a Bernstein ellipse (Petras 2002; Johansson 2018 uses Gauss–Legendre
//! with the same kind of bound).
//!
//! If f is analytic in the ellipse E_rho with foci -1, 1 and |f| <= M there, its Chebyshev coefficients satisfy
//! |a_k| <= 2 M rho^-k. The (n+1)-point Clenshaw–Curtis rule (n even) integrates T_0 ... T_n exactly, its weights
//! are positive and sum to 2, so |I_n(T_k)| <= 2, and |I(T_k)| <= 2/(k^2 - 1); hence
//!
//! ```text
//! |I - I_n| <= sum_{k > n} 2 M rho^-k (2 + 2/(k^2 - 1)) <= 4 M (1 + 1/(n(n+2))) rho^-n / (rho - 1)
//! ```
//!
//! on [-1, 1], times the half-length on [a, b]. M comes from one evaluation of f on a complex ball (a rectangle)
//! containing the scaled ellipse. The evaluation proves analyticity there only when f is built from analytic
//! operations: sums, products, quotients (a divisor ball containing 0 gives no value), integer and other powers,
//! exp, log, sqrt, the trigonometric and hyperbolic functions and their inverses, whose complex-ball versions give
//! no value on a region touching a branch cut or a pole (`mc-numeric`'s decision N0C.2). Any other function in
//! the body (abs, a conditional, ...) makes the integral unverifiable. The nodes cos(k pi/n) and the weights are
//! balls, f at the nodes is evaluated on balls, so the rule's value is enclosed too.
//!
//! Each interval chooses rho from a few candidates and the smallest number of nodes that meets its share of the
//! tolerance; an interval for which no candidate works is bisected.
use crate::hp::{ball_f64, BEnd};
use mc_api::names;
use mc_arith::num::{Ball, Float, Mag};
use mc_arith::{Num, Real};
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Res};
use mc_numeric::consts::Const;
use mc_numeric::elementary as el;
use mc_numfn::{Interpreted, NumFn};
use std::collections::{HashMap, HashSet};

/// The verdict of the rigorous method.
pub(crate) enum Verdict {
    /// a ball (or complex number of balls) containing the integral
    Enclosure(Num),
    /// no enclosure within the limits (a function that is not analytic, a singularity, limits reached)
    Unverified,
}

/// Candidate ellipse parameters, largest first.
const RHOS: [f64; 7] = [16.0, 8.0, 5.0, 3.5, 2.5, 1.8, 1.4];

/// The node counts tried (even; weights are cached per count).
const COUNTS: [usize; 16] = [4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768];

/// Beyond the largest count an interval is bisected; intervals of depth beyond this are not.
const MAX_DEPTH: u32 = 40;

/// The most intervals of one integral.
const MAX_INTERVALS: usize = 4000;

/// The heads whose complex-ball evaluation proves analyticity.
const ANALYTIC: &[&str] = &[
    names::ADD,
    names::MUL,
    names::POW,
    "exp",
    "log",
    // the stable forms the numeric preparation writes for log(1 + u) and exp(u) - 1: their complex-ball versions
    // are log and exp of a shifted ball, so they prove analyticity the same way
    "log1p",
    "expm1",
    "sqrt",
    "sin",
    "cos",
    "tan",
    "sinh",
    "cosh",
    "tanh",
    "atan",
    "asin",
    "acos",
];

/// Is the body built from numbers, symbols and analytic heads only?
pub(crate) fn analytic<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<bool> {
    let h = cx.heap();
    let heads: Vec<T<'h>> = ANALYTIC.iter().map(|s| h.sym_term(h.sym(s))).collect();
    let mut seen = HashSet::new();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        cx.tick(1)?;
        if let View::App { head, args } = h.view(u) {
            if !heads.contains(&head) {
                return Ok(false);
            }
            stack.extend_from_slice(args);
        }
    }
    Ok(true)
}

fn num(b: Ball, p: u64) -> Num {
    Num::Real(Real::Ball(b, p))
}

/// An upper bound of |b|.
fn mag(b: &Ball) -> Mag {
    Mag::from_float_up(&b.mid.abs()).add(&b.rad)
}

/// An upper bound of |v| (for a complex number, |re| + |im|); `None` for a value that is not a ball number.
fn bound(v: &Num, p: u64) -> Option<Mag> {
    let part = |r: &Real| match r.to_kind(mc_arith::Kind::Ball, p) {
        Real::Ball(b, _) => Some(mag(&b)),
        _ => None,
    };
    match v {
        Num::Real(r) => part(r),
        Num::Complex(a, b) => Some(part(a)?.add(&part(b)?)),
    }
}

/// The Clenshaw–Curtis weights of n + 1 points on [-1, 1] (n even) and the nodes cos(k pi / n), as balls.
fn weights(cx: &mut NativeCx<'_, '_, '_>, n: usize, p: u64) -> Res<Option<(Vec<Ball>, Vec<Ball>)>> {
    let Some(pi) = Const::Pi.ball(p) else { return Ok(None) };
    // c[m] = cos(m pi / n) for m in 0..2n, from m <= n/2 by symmetry
    let mut c = vec![Ball::from_i64(0); 2 * n];
    for m in 0..=n / 2 {
        cx.tick(1)?;
        let Some(v) = el::cos(&pi.mul(&Ball::from_ratio(m as i64, n as i64, p), p), p) else { return Ok(None) };
        c[m] = v.clone();
        c[n - m] = v.neg();
        c[n + m] = v.neg();
        if m > 0 {
            c[2 * n - m] = v;
        }
    }
    let coef: Vec<Ball> = (1..n / 2).map(|j| Ball::from_ratio(2, (4 * j * j - 1) as i64, p)).collect();
    let last = Ball::from_ratio(1, (n * n - 1) as i64, p);
    let two_n = Ball::from_ratio(2, n as i64, p);
    let mut w = vec![Ball::from_i64(0); n + 1];
    w[0] = last.clone();
    w[n] = last.clone();
    for k in 1..=n / 2 {
        cx.tick(1)?;
        let mut s = Ball::from_i64(1);
        for (j, cj) in coef.iter().enumerate() {
            s = s.sub(&cj.mul(&c[(2 * (j + 1) * k) % (2 * n)], p), p);
        }
        let sign = if k % 2 == 0 { last.clone() } else { last.neg() };
        s = s.sub(&sign, p);
        let v = two_n.mul(&s, p);
        w[k] = v.clone();
        w[n - k] = v;
    }
    let nodes = (0..=n).map(|k| c[k].clone()).collect();
    Ok(Some((nodes, w)))
}

/// An interval [m - r, m + r] with exact midpoint and half-length.
struct Iv {
    m: Ball,
    r: Ball,
    depth: u32,
}

/// An enclosure of the integral of `f` (the interpreted backend at `p` bits; `body` the term as given) over the
/// finite range `ends`, with an absolute error aim of 2^target.
pub(crate) fn integrate<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut Interpreted<'h>,
    _expr: T<'h>,
    ends: &[BEnd],
    p: u64,
    target: f64,
) -> Res<Verdict> {
    let body = f.body();
    if !analytic(cx, body)? || NumFn::<Num>::inputs(f) != 1 {
        return Ok(Verdict::Unverified);
    }
    let mut pts = Vec::with_capacity(ends.len());
    for e in ends {
        match e {
            BEnd::At(b) => pts.push(b.clone()),
            BEnd::Inf(_) => return Ok(Verdict::Unverified),
        }
    }
    let zero = Num::int(0);
    let half = Ball::from_ratio(1, 2, p);
    let mut total = zero.clone();
    let mut extra = Mag::ZERO;
    // the ends' uncertainty: |f| on the end ball times its radius
    for b in &pts {
        if !b.rad.is_zero() {
            let mut y = [zero.clone()];
            if !NumFn::<Num>::eval(f, cx, &[num(b.clone(), p)], &mut y)? {
                return Ok(Verdict::Unverified);
            }
            let Some(m) = bound(&y[0], p) else { return Ok(Verdict::Unverified) };
            extra = extra.add(&m.mul(&b.rad));
        }
    }
    let exact = |b: &Ball| Ball::exact(b.mid.clone());
    let width: f64 = pts.windows(2).map(|w| (w[1].mid.sub_exact(&w[0].mid)).abs().top() as f64).fold(f64::NEG_INFINITY, f64::max);
    let mut cache: HashMap<usize, (Vec<Ball>, Vec<Ball>)> = HashMap::new();
    let mut stack: Vec<Iv> = Vec::new();
    for w in pts.windows(2).rev() {
        let (a, b) = (exact(&w[0]), exact(&w[1]));
        let m = a.add(&b, p).mul(&half, p);
        let r = b.sub(&a, p).mul(&half, p);
        stack.push(Iv { m, r, depth: 0 });
    }
    let mut count = 0usize;
    while let Some(iv) = stack.pop() {
        cx.tick(1)?;
        count += 1;
        if count > MAX_INTERVALS {
            return Ok(Verdict::Unverified);
        }
        if iv.r.mid.is_zero() {
            continue;
        }
        // this interval's share of the tolerance: proportional to its length (2^-depth of its piece, the pieces
        // weighted by the widest), with a factor 1/4 of margin for the ends and roundoff
        let r_log2 = iv.r.mid.abs().top() as f64;
        let tol = target - 2.0 + (r_log2 + 1.0 - width);
        // the cheapest ellipse
        let mut best: Option<(usize, f64)> = None;
        for &rho in &RHOS {
            let a = (rho + 1.0 / rho) * 0.5 * (1.0 + 1e-12);
            let bb = (rho - 1.0 / rho) * 0.5 * (1.0 + 1e-12);
            let ra = mag(&iv.r).mul(&Mag::from_float_up(&ball_f64(a).mid));
            let rb = mag(&iv.r).mul(&Mag::from_float_up(&ball_f64(bb).mid));
            let re = Ball { mid: iv.m.mid.clone(), rad: iv.m.rad.add(&ra) };
            let im = Ball { mid: Float::zero(), rad: rb };
            let z = Num::Complex(Real::Ball(re, p), Real::Ball(im, p));
            let mut y = [zero.clone()];
            if !NumFn::<Num>::eval(f, cx, &[z], &mut y)? {
                continue;
            }
            let Some(m) = bound(&y[0], p) else { continue };
            // log2 of 4 (1 + 1/(n(n+2))) M |r| / (rho - 1), then n with that times rho^-n below 2^tol
            let c = 2.0001 + m.log2() + r_log2 + 1.0 - (rho - 1.0).log2();
            let need = (c - tol) / rho.log2();
            if let Some(&n) = COUNTS.iter().find(|&&n| n as f64 >= need) {
                let cost = n as f64;
                if best.is_none_or(|(bn, _)| n < bn) {
                    best = Some((n, rho));
                }
                if cost <= 8.0 {
                    break;
                }
            }
        }
        let Some((n, rho)) = best else {
            if iv.depth >= MAX_DEPTH {
                return Ok(Verdict::Unverified);
            }
            let h = iv.r.mul(&half, p);
            stack.push(Iv { m: iv.m.add(&h, p), r: h.clone(), depth: iv.depth + 1 });
            stack.push(Iv { m: iv.m.sub(&h, p), r: h, depth: iv.depth + 1 });
            continue;
        };
        if let std::collections::hash_map::Entry::Vacant(e) = cache.entry(n) {
            match weights(cx, n, p)? {
                Some(v) => {
                    e.insert(v);
                }
                None => return Ok(Verdict::Unverified),
            }
        }
        let (nodes, ws) = &cache[&n];
        let xs: Vec<Num> = nodes.iter().map(|c| num(iv.m.add(&iv.r.mul(c, p), p), p)).collect();
        let mut ys = vec![zero.clone(); xs.len()];
        if !NumFn::<Num>::eval_flat(f, cx, &xs, &mut ys)? {
            return Ok(Verdict::Unverified);
        }
        let mut s = zero.clone();
        for (y, w) in ys.iter().zip(ws) {
            s = s.add(&y.mul(&num(w.clone(), p)));
        }
        total = total.add(&s.mul(&num(iv.r.clone(), p)));
        // the error bound of this interval, recomputed for the chosen rho and n
        let mut y = [zero.clone()];
        let a = (rho + 1.0 / rho) * 0.5 * (1.0 + 1e-12);
        let bb = (rho - 1.0 / rho) * 0.5 * (1.0 + 1e-12);
        let re = Ball { mid: iv.m.mid.clone(), rad: iv.m.rad.add(&mag(&iv.r).mul(&Mag::from_float_up(&ball_f64(a).mid))) };
        let im = Ball { mid: Float::zero(), rad: mag(&iv.r).mul(&Mag::from_float_up(&ball_f64(bb).mid)) };
        if !NumFn::<Num>::eval(f, cx, &[Num::Complex(Real::Ball(re, p), Real::Ball(im, p))], &mut y)? {
            return Ok(Verdict::Unverified);
        }
        let Some(m) = bound(&y[0], p) else { return Ok(Verdict::Unverified) };
        let nn = n as f64;
        let factor = 4.0 * (1.0 + 1.0 / (nn * (nn + 2.0))) / (rho - 1.0) * (1.0 + 1e-9);
        // an upper bound of rho^-n: 2^ceil(-n log2 rho)
        let e = (-(nn * rho.log2() * (1.0 - 1e-12))).ceil() as i64;
        let err = m.mul(&mag(&iv.r)).mul(&Mag::from_float_up(&ball_f64(factor).mid)).mul(&Mag::pow2(e));
        extra = extra.add(&err);
    }
    Ok(Verdict::Enclosure(widen_mag(&total, &extra, p)))
}

/// The number with the radius `e` added to each part.
fn widen_mag(v: &Num, e: &Mag, p: u64) -> Num {
    let w = |r: &Real| match r.to_kind(mc_arith::Kind::Ball, p) {
        Real::Ball(b, _) => Real::Ball(Ball { mid: b.mid, rad: b.rad.add(e) }, p),
        other => other,
    };
    match v {
        Num::Real(r) => Num::Real(w(r)),
        Num::Complex(a, b) => Num::Complex(w(a), w(b)),
    }
}
