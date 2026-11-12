//! `count_roots(p, x)` and `count_roots(p, [x, a, b])`: the number of roots of a polynomial, with multiplicity, on
//! the real line, in a closed real interval, or in a closed rectangle of the complex plane with corners `a` and `b`
//! (item F2, decision F2.8).
//!
//! Real counts come from [`crate::realroot::real_roots`] (coefficients may be algebraic numbers in a real field)
//! with exact comparisons at the ends. Rectangle counts (rational coefficients): roots on the boundary are found
//! exactly, as the real roots of `gcd(U, V)` where `p(c + i t) = U(t) + i V(t)` on each side; the other roots are
//! placed by certified enclosures (`embed`), refined until every enclosure lies inside or outside the rectangle
//! except those of the boundary roots. No count depends on a tolerance.
use crate::algnum::AlgNum;
use crate::convert::{to_alg, Memo};
use crate::cx::{Env, Out};
use crate::embed;
use crate::natives::read_over_field;
use crate::qpoly::{self, q, QPoly};
use crate::realroot::{self, RealAlg};
use mc_api::{Store, View, T};
use mc_arith::num::{Mag, Rat};
use mc_arith::number::Real;
use mc_arith::Num;
use mc_poly::work::{Meter, WorkResult};
use std::cmp::Ordering;

/// The highest precision of the rectangle classification (bits).
const MAX_BITS: u64 = 1 << 13;

/// An end of a counting range.
enum End {
    NegInf,
    PosInf,
    Real(RealAlg),
    Complex(Rat, Rat),
}

/// The end `t`; `None` when it is not a number this function handles.
fn end<'h>(env: &Env<'h>, t: T<'h>, m: &mut Meter) -> WorkResult<Option<End>> {
    let h = env.h();
    let inf = env.sym("inf");
    if t == inf {
        return Ok(Some(End::PosInf));
    }
    if let View::App { head, args: &[c, i] } = h.view(t) {
        if head == env.hd.mul && i == inf && env.ar.num(c).is_some_and(|n| n.as_rat().is_some_and(|r| r.num.neg)) {
            return Ok(Some(End::NegInf));
        }
    }
    match env.ar.num(t) {
        Some(Num::Real(Real::Exact(r))) => return Ok(Some(End::Real(RealAlg::rational(&r)))),
        Some(Num::Complex(Real::Exact(a), Real::Exact(b))) => return Ok(Some(End::Complex(a, b))),
        Some(_) => return Ok(None),
        None => {}
    }
    let mut memo = Memo::default();
    let Some(a) = to_alg(env, t, &mut memo, m)? else { return Ok(None) };
    Ok(real_alg_of(&a, m)?.map(End::Real))
}

/// A real algebraic number as a [`RealAlg`]; `None` when it is not real or could not be located.
pub fn real_alg_of(a: &AlgNum, m: &mut Meter) -> WorkResult<Option<RealAlg>> {
    if let Some(c) = a.as_rat() {
        return Ok(Some(RealAlg::rational(&c)));
    }
    // a number that is not real has no sign
    if a.sign(m)?.is_none() {
        return Ok(None);
    }
    let mp = a.minpoly(m)?;
    let mut cands: Vec<RealAlg> = realroot::rational_real_roots(&mp, m)?.into_iter().map(|(r, _)| r).collect();
    let mut bits = 64;
    while bits <= MAX_BITS {
        let Some(z) = a.enclose(bits, m)? else { return Ok(None) };
        let (lo, hi) = ball_bounds(&z.re);
        cands.retain(|r| r.iv.hi.cmp_value(&lo) != Ordering::Less && r.iv.lo.cmp_value(&hi) != Ordering::Greater);
        if cands.len() == 1 {
            return Ok(cands.pop());
        }
        if cands.is_empty() {
            return Ok(None);
        }
        for r in cands.iter_mut() {
            for _ in 0..4 {
                r.halve();
            }
        }
        bits *= 2;
    }
    Ok(None)
}

/// The rational bounds of a ball.
fn ball_bounds(b: &mc_arith::num::Ball) -> (Rat, Rat) {
    let mid = embed::float_rat(&b.mid);
    let r = embed::float_rat(&Mag::to_float(&b.rad));
    (mid.sub(&r), mid.add(&r))
}

/// `count_roots(p, x)`, `count_roots(p, [x, a, b])`.
pub(crate) fn count_roots<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [p, spec] = args else {
        return Ok(env.arg_failure("count_roots(p, x) or count_roots(p, [x, a, b])"));
    };
    let (x, range) = match env.as_list(*spec) {
        Some(&[x, a, b]) => (x, Some((a, b))),
        Some(_) => return Ok(env.arg_failure("the range is [x, a, b]")),
        None => (*spec, None),
    };
    if !matches!(env.h().view(x), View::Sym(_)) {
        return Ok(env.arg_failure("the variable must be a symbol"));
    }
    // not a polynomial with algebraic coefficients in x: not this function's business (transcendental equations)
    let of = match read_over_field(env, &[*p], env.sym("automatic"), m)? {
        Ok(of) => of,
        Err(_) => return Ok(Out::Decline),
    };
    if of.x != x {
        return Ok(Out::Decline);
    }
    let k = of.k.clone();
    let poly = of.polys[0].clone();
    if poly.is_empty() {
        return Ok(Out::Decline);
    }
    let ends = match range {
        None => (End::NegInf, End::PosInf),
        Some((a, b)) => {
            let (Some(ea), Some(eb)) = (end(env, a, m)?, end(env, b, m)?) else { return Ok(Out::Decline) };
            (ea, eb)
        }
    };
    let complex = matches!(ends.0, End::Complex(..)) || matches!(ends.1, End::Complex(..));
    let count = if complex {
        let gauss = |e: End| -> Option<(Rat, Rat)> {
            match e {
                End::Complex(a, b) => Some((a, b)),
                End::Real(r) => r.as_rat().map(|c| (c, q(0))),
                _ => None,
            }
        };
        let (Some((ar, ai)), Some((br, bi))) = (gauss(ends.0), gauss(ends.1)) else { return Ok(Out::Decline) };
        let Some(pq) = crate::kpoly::as_rational(&k, &poly) else { return Ok(Out::Decline) };
        let (x0, x1) = ordered(ar, br);
        let (y0, y1) = ordered(ai, bi);
        match count_rectangle(&pq, (&x0, &x1), (&y0, &y1), m)? {
            Some(n) => n,
            None => return Ok(Out::Decline),
        }
    } else {
        {
            let Some(roots) = realroot::real_roots(&k, &poly, m)? else { return Ok(Out::Decline) };
            let (mut lo, mut hi) = ends;
            if let (End::Real(a), End::Real(b)) = (&mut lo, &mut hi) {
                if a.compare(b, m)? == Ordering::Greater {
                    std::mem::swap(a, b);
                }
            }
            let mut n = 0u64;
            for (mut r, mult) in roots {
                let above = match &mut lo {
                    End::NegInf => true,
                    End::Real(a) => r.compare(a, m)? != Ordering::Less,
                    _ => false,
                };
                let below = match &mut hi {
                    End::PosInf => true,
                    End::Real(b) => r.compare(b, m)? != Ordering::Greater,
                    _ => false,
                };
                if above && below {
                    n += mult as u64;
                }
            }
            n
        }
    };
    Ok(Out::Done(env.int(count as i64)))
}

fn ordered(a: Rat, b: Rat) -> (Rat, Rat) {
    if a.cmp_value(&b) == Ordering::Greater {
        (b, a)
    } else {
        (a, b)
    }
}

/// The roots of `p` (rational coefficients) in the closed rectangle `[x0, x1] x [y0, y1]`, with multiplicity.
pub fn count_rectangle(p: &[Rat], xs: (&Rat, &Rat), ys: (&Rat, &Rat), m: &mut Meter) -> WorkResult<Option<u64>> {
    let mut total = 0;
    for (f, e) in qpoly::factor(p, m)? {
        let Some(n) = count_rectangle_sf(&f, xs, ys, m)? else { return Ok(None) };
        total += n * e as u64;
    }
    Ok(Some(total))
}

/// `p(z) = U(t) + i V(t)` on the line `z = c + i t` (vertical) or `z = t + i c` (horizontal).
fn on_line(p: &[Rat], c: &Rat, vertical: bool, m: &mut Meter) -> WorkResult<(QPoly, QPoly)> {
    let (mut u, mut v): (QPoly, QPoly) = (vec![], vec![]);
    let t: QPoly = vec![q(0), q(1)];
    for coef in p.iter().rev() {
        m.step(u.len() as u64 + v.len() as u64 + 1)?;
        let (tu, tv) = (qpoly::mul(&u, &t, m)?, qpoly::mul(&v, &t, m)?);
        let (cu, cv) = (qpoly::scale(&u, c), qpoly::scale(&v, c));
        let (nu, nv) = if vertical {
            // (u + i v)(c + i t) = (c u - t v) + i (c v + t u)
            (qpoly::sub(&cu, &tv), qpoly::add(&cv, &tu))
        } else {
            // (u + i v)(t + i c) = (t u - c v) + i (t v + c u)
            (qpoly::sub(&tu, &cv), qpoly::add(&tv, &cu))
        };
        u = qpoly::add(&nu, std::slice::from_ref(coef));
        v = nv;
    }
    Ok((u, v))
}

/// The distinct roots `t` of the square-free `p` on one side: real roots of `gcd(U, V)` in `[lo, hi]` (closed) or
/// `(lo, hi)` (open).
fn side_roots(p: &[Rat], c: &Rat, vertical: bool, lo: &Rat, hi: &Rat, closed: bool, m: &mut Meter) -> WorkResult<u64> {
    let (u, v) = on_line(p, c, vertical, m)?;
    let g = qpoly::gcd(&u, &v, m)?;
    if g.len() <= 1 {
        return Ok(0);
    }
    let mut n = 0;
    let (mut a, mut b) = (RealAlg::rational(lo), RealAlg::rational(hi));
    for (mut r, _) in realroot::rational_real_roots(&g, m)? {
        let ca = r.compare(&mut a, m)?;
        let cb = r.compare(&mut b, m)?;
        let ok = if closed { ca != Ordering::Less && cb != Ordering::Greater } else { ca == Ordering::Greater && cb == Ordering::Less };
        if ok {
            n += 1;
        }
    }
    Ok(n)
}

fn count_rectangle_sf(p: &[Rat], (x0, x1): (&Rat, &Rat), (y0, y1): (&Rat, &Rat), m: &mut Meter) -> WorkResult<Option<u64>> {
    // the boundary: vertical sides closed, horizontal sides open (the corners once)
    let mut boundary = side_roots(p, x0, true, y0, y1, true, m)?;
    if x1 != x0 {
        boundary += side_roots(p, x1, true, y0, y1, true, m)?;
    }
    boundary += side_roots(p, y0, false, x0, x1, false, m)?;
    if y1 != y0 {
        boundary += side_roots(p, y1, false, x0, x1, false, m)?;
    }
    let Some(emb) = embed::roots(p, m)? else { return Ok(None) };
    // per root: Some(true) inside the open rectangle, Some(false) outside the closed one, None undecided
    let mut state: Vec<Option<bool>> = vec![None; emb.len()];
    let classify = |re: (Rat, Rat), im: (Rat, Rat)| -> Option<bool> {
        let inside = re.0.cmp_value(x0) == Ordering::Greater
            && re.1.cmp_value(x1) == Ordering::Less
            && im.0.cmp_value(y0) == Ordering::Greater
            && im.1.cmp_value(y1) == Ordering::Less;
        let outside = re.1.cmp_value(x0) == Ordering::Less
            || re.0.cmp_value(x1) == Ordering::Greater
            || im.1.cmp_value(y0) == Ordering::Less
            || im.0.cmp_value(y1) == Ordering::Greater;
        if inside {
            Some(true)
        } else if outside {
            Some(false)
        } else {
            None
        }
    };
    for (i, e) in emb.iter().enumerate() {
        let re = (e.re.sub(&e.radius), e.re.add(&e.radius));
        let im = (e.im.sub(&e.radius), e.im.add(&e.radius));
        state[i] = classify(re, im);
    }
    let mut bits = 64;
    while state.iter().filter(|s| s.is_none()).count() as u64 > boundary {
        if bits > MAX_BITS {
            return Ok(None);
        }
        for (i, e) in emb.iter().enumerate() {
            if state[i].is_some() {
                continue;
            }
            let Some(z) = embed::enclose(p, e, bits, m)? else { return Ok(None) };
            state[i] = classify(ball_bounds(&z.re), ball_bounds(&z.im));
        }
        bits *= 2;
    }
    let inside = state.iter().filter(|s| **s == Some(true)).count() as u64;
    Ok(Some(inside + boundary))
}
