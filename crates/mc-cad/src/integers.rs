//! `reduce` and `find_instance` over the integers in several variables (item 11B).
//!
//! - **Pell equations** `x^2 - D y^2 == 1` (D > 0 not a square) with `x > 0` and `y > 0`: every solution is
//!   `x + y sqrt(D) = (x_1 + y_1 sqrt(D))^k`, k >= 1, from the fundamental solution `(x_1, y_1)` found by the
//!   continued fraction of `sqrt(D)` (Lagrange; Niven, Zuckerman and Montgomery, *An Introduction to the Theory of
//!   Numbers*, section 7.8). `reduce` gives the closed form with an integer parameter `c(1) >= 1`;
//!   `find_instance` walks the powers (and the signs of x and y) and keeps those that satisfy the whole condition.
//! - **Bounded problems**: bounds first, from the real decomposition (the minimum and maximum of each variable
//!   over the real solution set, `crate::optimize`), then every integer point of the box (the last variable from
//!   the integer roots of an equation when one has it); `reduce` lists them all (complete, since the real set is
//!   bounded), `find_instance` the first ones in lexicographic order.
//! - **Bounded search** for `find_instance` of unbounded problems: the integer points with every coordinate at
//!   most `SEARCH_BOUND` in absolute value, by increasing maximum norm; none found leaves the call unevaluated
//!   (no claim is made beyond the bound).
//!
//! Every point is checked exactly against the whole condition before it is returned (decision CAD.19).
use crate::cad::{Coord, Problem};
use crate::formula::{Fm, Rel};
use crate::front::Front;
use crate::natives::metered;
use crate::optimize::{extreme, Extreme};
use mc_algebra::realroot::{rational_real_roots, RealAlg};
use mc_api::{names, Store, T};
use mc_arith::num::{Int, Rat};
use mc_eval::{NativeCx, Res};
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The most integer points of a box that are tried.
const MAX_POINTS: u64 = 400_000;
/// The bound of the search of `find_instance` for unbounded problems (maximum norm).
const SEARCH_BOUND: i64 = 60;
/// The most powers of the fundamental unit that `find_instance` tries.
const MAX_POWERS: u64 = 400;

/// The integer solutions found.
enum Sols {
    /// all of them (a bounded problem)
    All(Vec<Vec<Z>>),
    /// a Pell family: D, x_1, y_1, and `None` for all signs, else the signs of x and y (true: negative) and the
    /// least power
    Pell(Z, Z, Z, Option<(bool, bool, u32)>),
    /// some of them (the first ones found)
    Some(Vec<Vec<Z>>),
    /// `a x + b y == c` alone: a particular solution and the step, `(x0, y0, dx, dy)`
    Line(Z, Z, Z, Z),
}

/// The problem of `cond` in `vars` without parameters or quantifiers.
fn read<'h>(fr: &Front<'h>, cond: T<'h>, vars: &[T<'h>], m: &mut Meter) -> WorkResult<Option<Problem>> {
    let Ok(p) = fr.problem(cond, vars, m)? else { return Ok(None) };
    if p.prob.nfree != vars.len() || p.prob.nv != vars.len() {
        return Ok(None);
    }
    Ok(Some(p.prob))
}

/// Does the formula hold at an integer point?
fn holds(p: &Problem, pt: &[Z]) -> bool {
    fn val(q: &Poly, pt: &[Z]) -> Z {
        let mut r = q.clone();
        for (v, a) in pt.iter().enumerate() {
            r = r.eval_var(v, a);
        }
        r.as_constant().unwrap_or(Z::ZERO)
    }
    fn sign(z: &Z) -> i32 {
        if z.is_zero() {
            0
        } else if z.is_neg() {
            -1
        } else {
            1
        }
    }
    fn ev(f: &Fm, p: &Problem, pt: &[Z]) -> bool {
        match f {
            Fm::Const(b) => *b,
            Fm::Atom(i) => {
                let a = &p.atoms[*i];
                let d = sign(&val(&a.den, pt));
                d != 0 && a.rel.holds(sign(&val(&a.num, pt)) * d)
            }
            Fm::And(v) => v.iter().all(|g| ev(g, p, pt)),
            Fm::Or(v) => v.iter().any(|g| ev(g, p, pt)),
            Fm::Not(g) => !ev(g, p, pt),
        }
    }
    ev(&p.matrix, p, pt)
}

/// The integer interval `[floor(lo), ceil(hi)]` around a real algebraic bound (`lower`: floor of the value).
fn int_bound(c: &Coord, lower: bool, m: &mut Meter) -> WorkResult<Option<i64>> {
    let mut a = match c {
        Coord::Rat(q) => RealAlg::rational(q),
        Coord::Alg(a) => a.clone(),
    };
    a.refine(&Rat::new(Int::from_i64(1), Int::from_i64(4)), m)?;
    let (fl, _) = if lower { a.iv.lo.floor_fract() } else { a.iv.hi.floor_fract() };
    let Some(v) = fl.to_i64() else { return Ok(None) };
    Ok(Some(if lower { v } else { v + 1 }))
}

/// The box of the real solution set: for each variable its integer range; `None` when unbounded or undecided,
/// `Some(None)`-like empty when infeasible (an empty vector of ranges with `infeasible`).
#[allow(clippy::type_complexity)]
fn real_box<'h>(fr: &Front<'h>, cond: T<'h>, vars: &[T<'h>], m: &mut Meter) -> WorkResult<Option<Option<Vec<(i64, i64)>>>> {
    let h = fr.ar.store();
    let tv = h.sym_term(h.sym("cad$objective"));
    let mut free = vec![tv];
    free.extend(vars.iter().copied());
    // the 2 n bounds are 2 n decompositions: in parallel (roadmap item 16A), in order until one decides the box
    // (infeasible, unbounded, undecided)
    let bound = |k: usize, m: &mut Meter| -> WorkResult<Result<i64, bool>> {
        let fr = Front::new(h);
        let (x, sign) = (vars[k / 2], if k.is_multiple_of(2) { 1i64 } else { -1 });
        let obj = if sign > 0 { x } else { fr.ar.neg(x) };
        let f = h.call(h.sym("and"), &[h.call(h.sym("equal"), &[tv, obj]), cond]);
        let Some(e) = extreme(&fr, f, &free, vars.len(), m)? else { return Ok(Err(false)) };
        let v = match e {
            Extreme::Infeasible => return Ok(Err(true)),
            Extreme::Unbounded => return Ok(Err(false)),
            Extreme::Infimum(c) | Extreme::Attained(c, _) => c,
        };
        // min of x: floor; min of -x is -max(x): the ceiling of max(x) is -floor(min(-x))
        let Some(b) = int_bound(&v, true, m)? else { return Ok(Err(false)) };
        Ok(Ok(if sign > 0 { b } else { -b }))
    };
    let bs = crate::par::until(2 * vars.len(), m, &|r: &WorkResult<Result<i64, bool>>| !matches!(r, Ok(Ok(_))), &bound)?;
    let mut out = vec![];
    for (k, b) in bs.iter().enumerate() {
        match b {
            Ok(b) if k.is_multiple_of(2) => out.push((*b, 0)),
            Ok(b) => out.last_mut().expect("a lower bound first").1 = *b,
            Err(true) => return Ok(Some(None)),
            Err(false) => return Ok(None),
        }
    }
    Ok(Some(Some(out)))
}

/// The integer roots of a polynomial in one variable (dense, integer coefficients) in `[lo, hi]`.
fn integer_roots(c: &[Z], lo: i64, hi: i64, m: &mut Meter) -> WorkResult<Vec<i64>> {
    let q: Vec<Rat> = c.iter().map(|z| Rat::from_int(z.to_int())).collect();
    let mut out = vec![];
    for (mut r, _) in rational_real_roots(&q, m)? {
        r.refine(&Rat::new(Int::from_i64(1), Int::from_i64(4)), m)?;
        let (a, _) = r.iv.lo.floor_fract();
        let Some(a) = a.to_i64() else { continue };
        for k in a..=a + 1 {
            if k < lo || k > hi {
                continue;
            }
            let v = c.iter().rev().fold(Z::ZERO, |acc, z| acc.mul(&Z::from_i128(k as i128)).add(z));
            if v.is_zero() && !out.contains(&k) {
                out.push(k);
            }
        }
    }
    out.sort_unstable();
    Ok(out)
}

/// Every integer point of the box satisfying the formula (at most `limit`), lexicographically; `None` when the
/// box is too large. The slices of the first coordinate are searched in parallel (roadmap item 16A) and their points
/// taken in order.
fn enumerate(p: &Problem, bx: &[(i64, i64)], limit: usize, m: &mut Meter) -> WorkResult<Option<Vec<Vec<Z>>>> {
    let n = bx.len();
    if n < 2 || bx[0].1 <= bx[0].0 || bx.iter().any(|(lo, hi)| hi < lo) {
        return enumerate_box(p, bx, limit, m);
    }
    // the size check of the whole box first
    if enumerate_box(p, bx, 0, &mut Meter::new(&mut mc_poly::work::Unlimited))?.is_none() {
        return Ok(None);
    }
    let (lo, hi) = bx[0];
    let k = (hi - lo + 1) as usize;
    let slices = crate::par::until(k, m, &|r: &WorkResult<Option<Vec<Vec<Z>>>>| !matches!(r, Ok(Some(v)) if v.len() < limit), &|i, m| {
        let mut b = bx.to_vec();
        b[0] = (lo + i as i64, lo + i as i64);
        enumerate_box(p, &b, limit, m)
    })?;
    let mut out = vec![];
    for s in slices {
        let Some(s) = s else { return Ok(None) };
        out.extend(s);
        if out.len() >= limit {
            out.truncate(limit);
            break;
        }
    }
    Ok(Some(out))
}

/// [`enumerate`] on one thread; with `limit` 0 only the size check.
fn enumerate_box(p: &Problem, bx: &[(i64, i64)], limit: usize, m: &mut Meter) -> WorkResult<Option<Vec<Vec<Z>>>> {
    let n = bx.len();
    let last = n - 1;
    // an equation in the last variable gives its candidates
    let eq_last = p.matrix.conjuncts().into_iter().find_map(|c| match c {
        Fm::Atom(i) if p.atoms[*i].rel == Rel::Eq && (1..=40).contains(&p.atoms[*i].num.degree(last).unwrap_or(0)) => Some(&p.atoms[*i]),
        _ => None,
    });
    let mut size: u64 = 1;
    for (i, (lo, hi)) in bx.iter().enumerate() {
        if hi < lo {
            return Ok(Some(vec![]));
        }
        if i < last || eq_last.is_none() {
            size = size.saturating_mul((hi - lo + 1) as u64);
        }
    }
    if size > MAX_POINTS {
        return Ok(None);
    }
    if limit == 0 {
        return Ok(Some(vec![]));
    }
    let mut out = vec![];
    let mut pt: Vec<i64> = bx.iter().map(|b| b.0).collect();
    loop {
        m.step(1)?;
        let cands: Vec<i64> = match eq_last {
            Some(a) => {
                let mut q = a.num.clone();
                for (v, &x) in pt.iter().enumerate().take(last) {
                    q = q.eval_var(v, &Z::from_i128(x as i128));
                }
                if q.is_zero() {
                    (bx[last].0..=bx[last].1).collect()
                } else {
                    integer_roots(&q.to_dense(last), bx[last].0, bx[last].1, m)?
                }
            }
            None => (bx[last].0..=bx[last].1).collect(),
        };
        for y in cands {
            pt[last] = y;
            let z: Vec<Z> = pt.iter().map(|&v| Z::from_i128(v as i128)).collect();
            if holds(p, &z) {
                out.push(z);
                if out.len() >= limit {
                    return Ok(Some(out));
                }
            }
        }
        // the next point of the first n - 1 coordinates
        let mut i = last;
        loop {
            if i == 0 {
                return Ok(Some(out));
            }
            i -= 1;
            if pt[i] < bx[i].1 {
                pt[i] += 1;
                for j in i + 1..last {
                    pt[j] = bx[j].0;
                }
                break;
            }
        }
    }
}

/// A Pell equation `x^2 - D y^2 == 1` (times a constant) among the conjuncts: `(D, index of x, index of y)`.
fn pell(p: &Problem) -> Option<(Z, usize, usize)> {
    if p.nv != 2 {
        return None;
    }
    for c in p.matrix.conjuncts() {
        let Fm::Atom(i) = c else { continue };
        let a = &p.atoms[*i];
        if a.rel != Rel::Eq || !a.den.is_constant() {
            continue;
        }
        let mut cx = Z::ZERO;
        let mut cy = Z::ZERO;
        let mut c0 = Z::ZERO;
        let mut ok = true;
        for (e, c) in a.num.terms() {
            match (e[0], e[1]) {
                (2, 0) => cx = c.clone(),
                (0, 2) => cy = c.clone(),
                (0, 0) => c0 = c.clone(),
                _ => ok = false,
            }
        }
        if !ok || cx.is_zero() || cy.is_zero() || c0.is_zero() {
            continue;
        }
        // cx x^2 + cy y^2 + c0 == 0 with x^2 - D y^2 == 1: cx = -c0, cy = c0 D (or with x and y exchanged)
        for (ax, ay, ix, iy) in [(&cx, &cy, 0, 1), (&cy, &cx, 1, 0)] {
            if ax.add(&c0).is_zero() && ay.divisible(&c0) {
                let d = ay.div_exact(&c0);
                if !d.is_neg() && !d.is_zero() && !is_square(&d) {
                    return Some((d, ix, iy));
                }
            }
        }
    }
    None
}

fn is_square(d: &Z) -> bool {
    let n = d.to_int().mag;
    let r = n.isqrt();
    r.mul(&r) == n
}

/// The fundamental solution of `x^2 - D y^2 == 1` by the continued fraction of `sqrt(D)` (`None` beyond a bound).
fn fundamental(d: &Z, m: &mut Meter) -> WorkResult<Option<(Z, Z)>> {
    let Some(dd) = d.to_int().to_i64() else { return Ok(None) };
    let a0 = (dd as f64).sqrt() as i64;
    let a0 = (a0 - 2..=a0 + 2).filter(|k| *k >= 0 && k * k <= dd).max().unwrap_or(0);
    let (mut mm, mut den, mut a) = (0i64, 1i64, a0);
    let (mut p0, mut p1) = (Z::ONE, Z::from_i128(a0 as i128));
    let (mut q0, mut q1) = (Z::ZERO, Z::ONE);
    for _ in 0..100_000 {
        m.step(1)?;
        if p1.mul(&p1).sub(&d.mul(&q1).mul(&q1)).is_one() {
            return Ok(Some((p1, q1)));
        }
        mm = den * a - mm;
        den = (dd - mm * mm) / den;
        a = (a0 + mm) / den;
        let za = Z::from_i128(a as i128);
        let p2 = za.mul(&p1).add(&p0);
        let q2 = za.mul(&q1).add(&q0);
        (p0, p1, q0, q1) = (p1, p2, q1, q2);
    }
    Ok(None)
}

/// The integer half-line of each of x and y when every other conjunct is a linear condition on one of them and
/// each is restricted once: `Some((x <= 0 side, y <= 0 side, least power))`. The solutions are then
/// `(+-X_c, +-Y_c)` with `X_c + Y_c sqrt(D) = u^c`: `X_c >= 1` always and `Y_c >= 1` for `c >= 1`, `Y_0 = 0`, so the
/// least power is 1 when y must be nonzero (`y >= 1` or `y <= -1`) and 0 otherwise; a condition that excludes
/// every `x` with `|x| >= 1` of its sign is not a half-line of this kind and gives `None`.
fn quadrant(p: &Problem, ix: usize, iy: usize, skip: &Poly) -> Option<(bool, bool, u32)> {
    // per variable: (negative side, strict)
    let mut seen: [Option<(bool, bool)>; 2] = [None, None];
    for c in p.matrix.conjuncts() {
        let Fm::Atom(i) = c else { return None };
        let a = &p.atoms[*i];
        if a.num == *skip && a.rel == Rel::Eq {
            continue;
        }
        if !a.den.is_constant() || !matches!(a.rel, Rel::Gt | Rel::Ge | Rel::Lt | Rel::Le) {
            return None;
        }
        let d = a.num.degrees();
        let v = (0..p.nv).find(|&v| d[v] > 0)?;
        if d[v] != 1 || (0..p.nv).any(|w| w != v && d[w] > 0) {
            return None;
        }
        let holds_at = |k: i128| {
            let mut pt = vec![Z::ZERO; p.nv];
            pt[v] = Z::from_i128(k);
            let (mut q, mut r) = (a.num.clone(), a.den.clone());
            for (w, z) in pt.iter().enumerate() {
                q = q.eval_var(w, z);
                r = r.eval_var(w, z);
            }
            let sg = |z: Z| if z.is_zero() { 0 } else if z.is_neg() { -1 } else { 1 };
            a.rel.holds(sg(q.as_constant().unwrap_or(Z::ZERO)) * sg(r.as_constant().unwrap_or(Z::ZERO)))
        };
        // a linear condition holds on a half-line of the integers: read it from the points -2..2
        let pattern: Vec<bool> = (-2..=2).map(holds_at).collect();
        let side = match pattern.as_slice() {
            [false, false, false, true, true] => (false, true),
            [false, false, true, true, true] => (false, false),
            [true, true, false, false, false] => (true, true),
            [true, true, true, false, false] => (true, false),
            _ => return None,
        };
        let slot = if v == ix { 0 } else if v == iy { 1 } else { return None };
        if seen[slot].is_some() {
            return None;
        }
        seen[slot] = Some(side);
    }
    let ((nx, _), (ny, sy)) = (seen[0]?, seen[1]?);
    Some((nx, ny, if sy { 1 } else { 0 }))
}

/// One linear equation `a x + b y == c` in two variables and nothing else: no solution unless `gcd(a, b)`
/// divides `c`; otherwise `x = x0 + (b/g) k`, `y = y0 - (a/g) k` with the extended Euclidean algorithm, `x0` the
/// least nonnegative.
fn line(p: &Problem) -> Option<Sols> {
    if p.nv != 2 {
        return None;
    }
    let cs = p.matrix.conjuncts();
    let [Fm::Atom(i)] = cs.as_slice() else { return None };
    let at = &p.atoms[*i];
    if at.rel != Rel::Eq || !at.den.is_constant() || at.num.total_degree() != Some(1) {
        return None;
    }
    let (mut a, mut b, mut c) = (Z::ZERO, Z::ZERO, Z::ZERO);
    for (e, k) in at.num.terms() {
        match (e[0], e[1]) {
            (1, 0) => a = k.clone(),
            (0, 1) => b = k.clone(),
            (0, 0) => c = k.neg(),
            _ => return None,
        }
    }
    if a.is_zero() || b.is_zero() {
        return None;
    }
    let to = |z: &Z| z.to_int().to_i64().map(|v| v as i128);
    let (ai, bi, ci) = (to(&a)?, to(&b)?, to(&c)?);
    // extended gcd: s a + t b = g
    let (mut r0, mut r1, mut s0, mut s1, mut t0, mut t1) = (ai, bi, 1i128, 0i128, 0i128, 1i128);
    while r1 != 0 {
        let q = r0.div_euclid(r1);
        (r0, r1) = (r1, r0 - q * r1);
        (s0, s1) = (s1, s0 - q * s1);
        (t0, t1) = (t1, t0 - q * t1);
    }
    let (g, s, t) = if r0 < 0 { (-r0, -s0, -t0) } else { (r0, s0, t0) };
    if ci % g != 0 {
        return Some(Sols::All(vec![]));
    }
    let (dx, dy) = (bi / g, -ai / g);
    let k = ci / g;
    let (x0, y0) = (s.checked_mul(k)?, t.checked_mul(k)?);
    // the least nonnegative x0: shift by multiples of the step
    let m = dx.abs();
    let j = x0.div_euclid(m) * dx.signum();
    let (x0, y0) = (x0 - j * dx, y0 - j * dy);
    let z = Z::from_i128;
    Some(Sols::Line(z(x0), z(y0), z(dx.abs()), z(dy * dx.signum())))
}

/// The solutions of `cond` over the integers (`count` of them for `find_instance`, all for `reduce`).
fn solve<'h>(fr: &Front<'h>, cond: T<'h>, vars: &[T<'h>], count: Option<usize>, m: &mut Meter) -> WorkResult<Option<Sols>> {
    let Some(p) = read(fr, cond, vars, m)? else { return Ok(None) };
    let n = vars.len();
    if let Some((d, ix, iy)) = pell(&p) {
        let Some((x1, y1)) = fundamental(&d, m)? else { return Ok(None) };
        let eqp = p.matrix.conjuncts().iter().find_map(|c| match c {
            Fm::Atom(i) if p.atoms[*i].rel == Rel::Eq => Some(p.atoms[*i].num.clone()),
            _ => None,
        });
        match count {
            None => {
                if ix == 0 && p.matrix.conjuncts().len() == 1 {
                    return Ok(Some(Sols::Pell(d, x1, y1, None)));
                }
                if ix == 0 {
                    if let Some(q) = quadrant(&p, ix, iy, eqp.as_ref().expect("the equation")) {
                        return Ok(Some(Sols::Pell(d, x1, y1, Some(q))));
                    }
                }
            }
            Some(k) => {
                // the powers of the fundamental unit, every sign
                let mut out = vec![];
                let (mut xk, mut yk) = (Z::ONE, Z::ZERO);
                for _ in 0..MAX_POWERS {
                    m.step(1)?;
                    (xk, yk) = (xk.mul(&x1).add(&d.mul(&yk).mul(&y1)), xk.mul(&y1).add(&yk.mul(&x1)));
                    for (sx, sy) in [(false, false), (false, true), (true, false), (true, true)] {
                        let vx = if sx { xk.neg() } else { xk.clone() };
                        let vy = if sy { yk.neg() } else { yk.clone() };
                        let mut pt = vec![Z::ZERO; 2];
                        pt[ix] = vx;
                        pt[iy] = vy;
                        if holds(&p, &pt) && !out.contains(&pt) {
                            out.push(pt);
                            if out.len() >= k {
                                return Ok(Some(Sols::Some(out)));
                            }
                        }
                    }
                    if xk.to_int().mag.bits() > 4000 {
                        break;
                    }
                }
                // (1, 0) and (-1, 0) are solutions too
                for sx in [Z::ONE, Z::ONE.neg()] {
                    let mut pt = vec![Z::ZERO; 2];
                    pt[ix] = sx;
                    if holds(&p, &pt) && !out.contains(&pt) && out.len() < k {
                        out.push(pt);
                    }
                }
                if !out.is_empty() {
                    return Ok(Some(Sols::Some(out)));
                }
            }
        }
    }
    // bounds from the real decomposition, for problems of moderate degree
    let low_degree = p.atoms.iter().all(|a| a.num.total_degree().unwrap_or(0) <= 12 && a.den.total_degree().unwrap_or(0) <= 12);
    let bx = if low_degree { real_box(fr, cond, vars, m)? } else { None };
    if count.is_none() {
        if let Some(l) = line(&p) {
            return Ok(Some(l));
        }
    }
    match bx {
        Some(None) => return Ok(Some(Sols::All(vec![]))),
        Some(Some(bx)) => {
            let limit = count.unwrap_or(usize::MAX);
            if let Some(pts) = enumerate(&p, &bx, limit, m)? {
                // fewer than asked for: the box was searched completely
                return Ok(Some(if pts.len() < limit { Sols::All(pts) } else { Sols::Some(pts) }));
            }
            if count.is_none() {
                return Ok(None);
            }
        }
        None => {
            if count.is_none() {
                return Ok(None);
            }
        }
    }
    // find_instance: a bounded search by increasing maximum norm
    let k = count.unwrap_or(1);
    let mut out: Vec<Vec<Z>> = vec![];
    for r in 0..=SEARCH_BOUND {
        let bx: Vec<(i64, i64)> = vec![(-r, r); n];
        if ((2 * r + 1) as u64).saturating_pow(n as u32) > MAX_POINTS {
            break;
        }
        let Some(pts) = enumerate(&p, &bx, usize::MAX, m)? else { break };
        for q in pts {
            let on_shell = q.iter().any(|z| z.abs() == Z::from_i128(r as i128));
            if on_shell && !out.contains(&q) {
                out.push(q);
                if out.len() >= k {
                    return Ok(Some(Sols::Some(out)));
                }
            }
        }
    }
    Ok(if out.is_empty() { None } else { Some(Sols::Some(out)) })
}

/// `reduce(cond, [x, y, ...], integers)`: a Pell family with the parameter `c(1)`, or every solution of a
/// bounded problem; `None` otherwise.
pub fn reduce_integers<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, cond: T<'h>, vars: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let r = metered(cx, stop, |m| solve(&fr, cond, vars, None, m))?;
    let Some(Some(sols)) = r else { return Ok(None) };
    let int = |z: &Z| fr.ar.integer(&z.to_int());
    let t = match sols {
        Sols::All(pts) | Sols::Some(pts) => {
            let cases: Vec<T<'h>> = pts
                .iter()
                .map(|pt| {
                    let eqs: Vec<T<'h>> = vars.iter().zip(pt).map(|(&x, v)| h.call(h.sym("equal"), &[x, int(v)])).collect();
                    if eqs.len() == 1 {
                        eqs[0]
                    } else {
                        h.call(h.sym("and"), &eqs)
                    }
                })
                .collect();
            match cases.len() {
                0 => h.sym_term(h.sym(names::FALSE)),
                1 => cases[0],
                _ => h.call(h.sym("or"), &cases),
            }
        }
        Sols::Line(x0, y0, dx, dy) => {
            let c1 = h.call(h.sym("c"), &[h.int(1)]);
            let ints = h.sym_term(h.sym("integers"));
            let xv = fr.ar.add(&[int(&x0), fr.ar.mul(&[int(&dx), c1])]);
            let yv = fr.ar.add(&[int(&y0), fr.ar.mul(&[int(&dy), c1])]);
            let parts = [
                h.call(h.sym("element"), &[c1, ints]),
                h.call(h.sym("equal"), &[vars[0], xv]),
                h.call(h.sym("equal"), &[vars[1], yv]),
            ];
            h.call(h.sym("and"), &parts)
        }
        Sols::Pell(d, x1, y1, signs) => {
            // x + y sqrt(D) = u^c with u = x1 + y1 sqrt(D), c >= 1
            let c1 = h.call(h.sym("c"), &[h.int(1)]);
            let sd = fr.ar.pow(int(&d), fr.ar.div(fr.ar.int(1), fr.ar.int(2)));
            let u = fr.ar.add(&[int(&x1), fr.ar.mul(&[int(&y1), sd])]);
            let ub = fr.ar.add(&[int(&x1), fr.ar.neg(fr.ar.mul(&[int(&y1), sd]))]);
            let uc = fr.ar.pow(u, c1);
            let ubc = fr.ar.pow(ub, c1);
            let half = fr.ar.div(fr.ar.int(1), fr.ar.int(2));
            let xv = fr.ar.mul(&[half, fr.ar.add(&[ubc, uc])]);
            let two_d = Z::from_i128(2).mul(&d);
            let yv = fr.ar.mul(&[fr.ar.div(fr.ar.int(-1), int(&two_d)), fr.ar.add(&[fr.ar.neg(uc), ubc]), sd]);
            let ints = h.sym_term(h.sym("integers"));
            let el = h.call(h.sym("element"), &[c1, ints]);
            let eq = |a: T<'h>, b: T<'h>| h.call(h.sym("equal"), &[a, b]);
            match signs {
                None => {
                    // x + y sqrt(D) = +-u^c, c any integer
                    let plus = h.call(h.sym("and"), &[eq(vars[0], xv), eq(vars[1], yv)]);
                    let minus = h.call(h.sym("and"), &[eq(vars[0], fr.ar.neg(xv)), eq(vars[1], fr.ar.neg(yv))]);
                    h.call(h.sym("and"), &[el, h.call(h.sym("or"), &[plus, minus])])
                }
                Some((nx, ny, cmin)) => {
                    let sx = if nx { fr.ar.neg(xv) } else { xv };
                    let sy = if ny { fr.ar.neg(yv) } else { yv };
                    let parts = [el, h.call(h.sym("greater_equal"), &[c1, h.int(cmin as i64)]), eq(vars[0], sx), eq(vars[1], sy)];
                    h.call(h.sym("and"), &parts)
                }
            }
        }
    };
    Ok(Some(cx.eval(t)?))
}

/// `find_instance(cond, vars, integers)` and `find_instance(cond, vars, integers, n)`: up to `n` integer points
/// (each checked exactly); `None` when none was found within the bounds.
pub fn find_integers<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, cond: T<'h>, vars: &[T<'h>], count: usize) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let r = metered(cx, stop, |m| solve(&fr, cond, vars, Some(count), m))?;
    let Some(Some(sols)) = r else { return Ok(None) };
    let pts = match sols {
        Sols::All(p) | Sols::Some(p) => p,
        Sols::Pell(..) | Sols::Line(..) => return Ok(None),
    };
    let mut out = vec![];
    for pt in pts {
        let rules: Vec<T<'h>> = vars.iter().zip(&pt).map(|(&x, v)| h.call(h.sym(names::RULE), &[x, fr.ar.integer(&v.to_int())])).collect();
        out.push(cx.list(&rules));
    }
    Ok(Some(cx.list(&out)))
}

/// `resolve(exists(vars, body), integers)` and `resolve(for_all(vars, body), integers)` of sentences: an integer
/// point found (checked exactly) or a bounded problem searched completely decides; `None` otherwise.
pub fn resolve_integers<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, f: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    // one block of quantifiers of one kind
    let mut vars: Vec<T<'h>> = vec![];
    let mut body = f;
    let mut kind: Option<String> = None;
    while let Some((name, args)) = fr.head(body) {
        if name != "exists" && name != "for_all" {
            break;
        }
        if kind.as_deref().is_some_and(|k| k != name) {
            return Ok(None);
        }
        let Some(vs) = fr.qvars(args[0]) else { return Ok(None) };
        vars.extend(vs);
        body = match (name.as_str(), args) {
            (_, [_, b]) => *b,
            ("exists", [_, c, b]) => h.call(h.sym("and"), &[*c, *b]),
            (_, [_, c, b]) => h.call(h.sym("implies"), &[*c, *b]),
            _ => return Ok(None),
        };
        kind = Some(name);
    }
    let Some(kind) = kind else { return Ok(None) };
    if vars.is_empty() || fr.has_quantifier(body) {
        return Ok(None);
    }
    let all = kind == "for_all";
    // for_all: no counterexample
    let goal = if all { h.call(h.sym("not"), &[body]) } else { body };
    let r = metered(cx, stop, |m| solve(&fr, goal, &vars, Some(1), m))?;
    let found = match r.flatten() {
        Some(Sols::Some(p)) | Some(Sols::All(p)) if !p.is_empty() => true,
        Some(Sols::All(_)) => false,
        _ => return Ok(None),
    };
    Ok(Some(h.sym_term(h.sym(if found != all { names::TRUE } else { names::FALSE }))))
}
