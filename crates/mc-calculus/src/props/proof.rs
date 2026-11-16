//! Proofs and refutations of the property functions from enclosures (round 43): a property is answered only when a
//! proof stands behind it, never from values at sample points.
//!
//! - **Values at points** ([`closed_sign`], [`point_sign`]): a closed numeric term is evaluated exactly when it is a
//!   number, else to 30 digits; its sign is proved when the ball (or the exact value) excludes 0, and it is 0 only
//!   when the term is exactly 0. A machine number carries no error bound and proves nothing. Sample points of the
//!   search loops are machine numbers; a witness is confirmed at the exact rational next to it.
//! - **Enclosures over an interval** ([`enclose`], [`sign_on`]): the variable becomes the real ball of the interval
//!   (machine ends taken exactly) and the expression is evaluated in ball arithmetic, which bounds every point of a
//!   ball argument (the natural interval extension); symbolic constants become 30-digit balls. [`sign_on`] covers
//!   the interval by cells (bisection) until every cell's enclosure has one sign.
//! - **Domains** ([`bounded_pieces`]): a condition on one variable reduced to finitely many bounded intervals, each
//!   with exact ends; the closed hull of each is used (an enclosure over a closed superset proves the subset).
//!
//! Literature: R. E. Moore, R. B. Kearfott, M. J. Cloud, "Introduction to Interval Analysis", SIAM 2009, chapters 5
//! and 6 (the natural extension, subdivision); F. Johansson, "Arb: efficient arbitrary-precision midpoint-radius
//! interval arithmetic", IEEE Transactions on Computers 66 (2017) 1281-1292 (ball arithmetic).
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_arith::num::float::{dyadic_to_f64, f64_to_dyadic};
use mc_arith::num::{Ball, Float, Mag};
use mc_arith::{Num, Real};
use mc_eval::{NativeCx, Res};

/// The precision of the balls of intervals (bits).
const BITS: u64 = 64;
/// At most this many enclosures settle one interval in [`sign_on`].
const MAX_CELLS: usize = 400;
/// The smallest cell of [`sign_on`] is 2^-MAX_DEPTH of the interval.
const MAX_DEPTH: u32 = 24;

fn float_of(v: f64) -> Float {
    match f64_to_dyadic(v) {
        Some((neg, man, exp)) => Float::new(neg, man, exp),
        None => Float::zero(),
    }
}

fn f64_of(f: &Float) -> f64 {
    dyadic_to_f64(f.neg, &f.man, f.exp)
}

/// Machine bounds of a real enclosure, rounded outwards: an exact real, a ball, or a complex number whose imaginary
/// part is exactly 0. `None` for a machine number (no error bound), a value that may be complex, or no number.
fn real_bounds(r: &Real) -> Option<(f64, f64)> {
    match r {
        Real::Ball(bl, _) => {
            let (l, h) = bl.ends();
            let (lo, hi) = (f64_of(&l), f64_of(&h));
            let exact = |v: f64, f: &Float| float_of(v).cmp_exact(f) == std::cmp::Ordering::Equal;
            let lo = if exact(lo, &l) { lo } else { lo.next_down() };
            let hi = if exact(hi, &h) { hi } else { hi.next_up() };
            (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
        }
        Real::Exact(q) => {
            let v = r.to_f64();
            if q.is_zero() {
                return Some((0.0, 0.0));
            }
            v.is_finite().then(|| (v.next_down(), v.next_up()))
        }
        Real::Machine(_) => None,
    }
}

/// The bounds of the number `t` (see [`real_bounds`]).
pub(crate) fn bounds<'h>(b: &Build<'h>, t: T<'h>) -> Option<(f64, f64)> {
    match b.ar.num(t)? {
        Num::Real(r) => real_bounds(&r),
        Num::Complex(re, im) => {
            let zero = match &im {
                Real::Exact(q) => q.is_zero(),
                _ => false,
            };
            if zero {
                real_bounds(&re)
            } else {
                None
            }
        }
    }
}

/// The rules that replace the symbolic constants of `e` by 30-digit balls.
fn constant_rules<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, e: T<'h>) -> Res<Vec<T<'h>>> {
    let h = b.heap();
    let mut out = vec![];
    for s in ["pi", "euler_gamma", "catalan", "golden_ratio", "degree", "glaisher", "khinchin", "e"] {
        let c = h.sym_term(h.sym(s));
        if contains(b, e, c) {
            let v = cx.eval(b.call("n", &[c, b.int(30)]))?;
            out.push(h.call(h.sym(names::RULE), &[c, v]));
        }
    }
    Ok(out)
}

fn contains<'h>(b: &Build<'h>, t: T<'h>, s: T<'h>) -> bool {
    let h = b.heap();
    if t == s {
        return true;
    }
    match h.view(t) {
        View::App { head, args } => head == s || args.iter().any(|&a| contains(b, a, s)),
        _ => false,
    }
}

/// The sign of a closed real term, proved: `Some(0)` only when it is exactly 0, `Some(1)` or `Some(-1)` when its
/// exact value or its 30-digit ball excludes 0; `None` otherwise (undecided, not real, or no number).
pub(crate) fn closed_sign<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<Option<i8>> {
    cx.tick(1)?;
    // past the search budget of the call: nothing more is decided (no answer, never a different one)
    if super::out_of_time(cx) {
        return Ok(None);
    }
    let v = cx.eval(t)?;
    if v == b.zero() {
        return Ok(Some(0));
    }
    let bd = match b.ar.num(v) {
        Some(n) if n.is_exact() => bounds(b, v),
        // a machine number (or a ball already) carries what it carries: no 30-digit value made from it
        Some(_) => bounds(b, v),
        None => {
            let w = cx.eval(b.call("n", &[v, b.int(30)]))?;
            bounds(b, w)
        }
    };
    Ok(match bd {
        Some((lo, _)) if lo > 0.0 => Some(1),
        Some((_, hi)) if hi < 0.0 => Some(-1),
        // an enclosure of 0 (or none): exactly 0 only when the term simplifies to 0
        Some(_) => {
            let s = cx.eval(b.call("simplify", &[v]))?;
            (s == b.zero()).then_some(0)
        }
        None => None,
    })
}

/// The machine number `v` as the exact binary rational it is (a sample point made exact, so that a witness found at
/// it is confirmed at a point of the line).
pub(crate) fn exact_point<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: f64) -> Res<T<'h>> {
    let h = b.heap();
    if v == 0.0 {
        return Ok(b.zero());
    }
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        return Ok(b.int(v as i64));
    }
    cx.eval(b.call("set_precision", &[h.real(v), h.sym_term(h.sym("inf"))]))
}

/// `t` at `x = q`.
pub(crate) fn at<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, x: T<'h>, q: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    cx.eval(h.call(h.sym("rewrite"), &[t, h.call(h.sym(names::RULE), &[x, q])]))
}

/// Do all the conditions hold at the exact point `x = q`?
pub(crate) fn holds_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, conds: &[T<'h>], x: T<'h>, q: T<'h>) -> Res<bool> {
    for &k in conds {
        let r = at(cx, b, k, x, q)?;
        if !cx.is_true(r) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The proved sign of `f` at the exact point `x = q` inside the conditions, `None` when not proved.
pub(crate) fn point_sign<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, q: T<'h>, conds: &[T<'h>]) -> Res<Option<i8>> {
    if !holds_at(cx, b, conds, x, q)? {
        return Ok(None);
    }
    let v = at(cx, b, f, x, q)?;
    closed_sign(cx, b, v)
}

/// [`point_sign`] for a function known to be real at the points where `conds` hold (its real domain is read from
/// the table): a value computed as a complex ball whose imaginary part only encloses 0 has the sign of its real
/// part (J(1/2 + i t) through complex intermediates).
pub(crate) fn point_sign_real<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, q: T<'h>, conds: &[T<'h>]) -> Res<Option<i8>> {
    if !holds_at(cx, b, conds, x, q)? {
        return Ok(None);
    }
    let v = at(cx, b, f, x, q)?;
    if let Some(s) = closed_sign(cx, b, v)? {
        return Ok(Some(s));
    }
    let w = cx.eval(b.call("n", &[v, b.int(30)]))?;
    let re = cx.eval(b.call("re", &[w]))?;
    Ok(match bounds(b, re) {
        Some((lo, _)) if lo > 0.0 => Some(1),
        Some((_, hi)) if hi < 0.0 => Some(-1),
        _ => None,
    })
}

/// The ball of [lo, hi] (machine ends taken exactly).
pub(crate) fn ball_of<'h>(b: &Build<'h>, lo: f64, hi: f64) -> T<'h> {
    let (l, r) = (float_of(lo), float_of(hi));
    let s = l.add_exact(&r);
    let mid = Float::new(s.neg, s.man.clone(), s.exp - 1);
    let rad = Mag::from_float_up(&r.sub_exact(&mid));
    b.ar.number(&Num::Real(Real::Ball(Ball { mid, rad }, BITS)))
}

/// `g` with its symbolic constants as balls, for evaluation on balls.
pub(crate) fn with_ball_constants<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>) -> Res<T<'h>> {
    let rules = constant_rules(cx, b, g)?;
    if rules.is_empty() {
        return Ok(g);
    }
    let h = b.heap();
    cx.eval(h.call(h.sym("rewrite"), &[g, cx.list(&rules)]))
}

/// An enclosure of the values of `g` (its constants already balls) for `x` in [lo, hi]: `None` when the ball
/// evaluation gives no real ball (a pole or a branch point inside, complex values, a function without a ball form).
pub(crate) fn enclose<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, lo: f64, hi: f64) -> Res<Option<(f64, f64)>> {
    cx.tick(1)?;
    if super::out_of_time(cx) {
        return Ok(None);
    }
    let ball = if lo == hi { exact_point(cx, b, lo)? } else { ball_of(b, lo, hi) };
    let mut v = at(cx, b, g, x, ball)?;
    if b.ar.num(v).is_none() {
        // a constant brought in by a function's own formula
        v = with_ball_constants(cx, b, v)?;
    }
    if lo == hi && b.ar.num(v).is_none_or(|n| n.is_exact()) && bounds(b, v).is_none() {
        // a closed form at an exact point (sin(3/2)): its 30-digit ball
        let w = cx.eval(b.call("n", &[v, b.int(30)]))?;
        return Ok(bounds(b, w));
    }
    Ok(bounds(b, v))
}

/// The sign of `g` on [lo, hi] proved by enclosures over cells: `Some(1)` when g >= 0 there (g > 0 with `strict`),
/// `Some(-1)` when g <= 0 (g < 0), `Some(0)` when every enclosure is exactly 0; `None` when not proved within the
/// budget of cells. A cell is enclosed by g on its ball, met with the mean value forms from its ends,
/// g(l) + g'([l, r]) [0, r - l] and g(r) - g'([l, r]) [0, r - l] (they settle a cell at an end where g is exactly 0,
/// as -sin(x) at 0, which no ball around the end separates from 0).
/// `ends`: the exact ends of the interval (their values are taken there exactly: sin at pi is 0, which the machine
/// end just above pi does not show).
pub(crate) fn sign_on<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    x: T<'h>,
    (lo, hi): (f64, f64),
    ends: Option<(T<'h>, T<'h>)>,
    strict: bool,
) -> Res<Option<i8>> {
    if !(lo.is_finite() && hi.is_finite() && lo <= hi) {
        return Ok(None);
    }
    let dg = {
        let h = b.heap();
        let d = cx.eval(h.call(h.sym("d"), &[g, x]))?;
        let free = !contains(b, d, h.sym_term(h.sym("d"))) && !contains(b, d, h.sym_term(h.sym("derivative")));
        if free {
            Some(with_ball_constants(cx, b, d)?)
        } else {
            None
        }
    };
    let g = with_ball_constants(cx, b, g)?;
    let min = (hi - lo) * 2f64.powi(-(MAX_DEPTH as i32));
    // 2: exactly 0, 1: >= 0 (> 0), -1: <= 0 (< 0)
    let settle = |e: Option<(f64, f64)>| -> Option<i8> {
        match e {
            Some((a, c)) if a == 0.0 && c == 0.0 && !strict => Some(2),
            Some((a, _)) if (strict && a > 0.0) || (!strict && a >= 0.0) => Some(1),
            Some((_, c)) if (strict && c < 0.0) || (!strict && c <= 0.0) => Some(-1),
            _ => None,
        }
    };
    // the cells seen: some with g >= 0 (not all 0), some with g <= 0 (not all 0)
    let (mut pos, mut neg) = (false, false);
    let mut stack = vec![(lo, hi)];
    let mut cells = 0usize;
    while let Some((l, r)) = stack.pop() {
        cells += 1;
        if cells > MAX_CELLS {
            return Ok(None);
        }
        let mut s = settle(enclose(cx, b, g, x, l, r)?);
        if s.is_none() && l < r {
            if let Some(dg) = dg {
                let gl = match ends {
                    Some((e, _)) if l == lo => value_at_exact(cx, b, g, x, e)?,
                    _ => enclose(cx, b, g, x, l, l)?,
                };
                let gr = match ends {
                    Some((_, e)) if r == hi => value_at_exact(cx, b, g, x, e)?,
                    _ => enclose(cx, b, g, x, r, r)?,
                };
                if let (Some((p, q)), Some((gl0, gl1)), Some((gr0, gr1))) = (enclose(cx, b, dg, x, l, r)?, gl, gr) {
                    let w = (r - l).next_up();
                    let (pw, qw) = ((p * w).next_down().min((p * w).next_up()), (q * w).next_up().max((q * w).next_down()));
                    // from the left end: g(l) + [min(0, p w), max(0, q w)]; from the right end: g(r) - the same
                    let left = ((gl0 + pw.min(0.0)).next_down(), (gl1 + qw.max(0.0)).next_up());
                    let right = ((gr0 - qw.max(0.0)).next_down(), (gr1 - pw.min(0.0)).next_up());
                    let (a, c) = (left.0.max(right.0), left.1.min(right.1));
                    // an exact 0 at an end gives the bound 0 itself (no rounding past it)
                    let a = if (gl0 == 0.0 && gl1 == 0.0 && p >= 0.0) || (gr0 == 0.0 && gr1 == 0.0 && q <= 0.0) { a.max(0.0) } else { a };
                    let c = if (gl0 == 0.0 && gl1 == 0.0 && q <= 0.0) || (gr0 == 0.0 && gr1 == 0.0 && p >= 0.0) { c.min(0.0) } else { c };
                    if a <= c {
                        s = settle(Some((a, c)));
                    }
                }
            }
        }
        match s {
            Some(2) => continue,
            Some(1) => {
                pos = true;
                if neg {
                    return Ok(None);
                }
                continue;
            }
            Some(_) => {
                neg = true;
                if pos {
                    return Ok(None);
                }
                continue;
            }
            None => {}
        }
        let m = 0.5 * (l + r);
        if r - l <= min || m <= l || m >= r {
            return Ok(None);
        }
        stack.push((m, r));
        stack.push((l, m));
    }
    Ok(Some(match (pos, neg) {
        (true, _) => 1,
        (_, true) => -1,
        _ => 0,
    }))
}

/// The bounds of `g` at the exact point `x = q` (exact, else 30 digits).
fn value_at_exact<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, q: T<'h>) -> Res<Option<(f64, f64)>> {
    if super::out_of_time(cx) {
        return Ok(None);
    }
    let v = at(cx, b, g, x, q)?;
    let v = with_ball_constants(cx, b, v)?;
    if let Some(bd) = bounds(b, v) {
        return Ok(Some(bd));
    }
    let w = cx.eval(b.call("n", &[v, b.int(30)]))?;
    Ok(bounds(b, w))
}

/// The bounded intervals of a condition on `x` (closed hulls, ends rounded outwards), or `None` when the condition
/// does not reduce to finitely many bounded intervals with numeric ends.
#[allow(clippy::type_complexity)]
pub(crate) fn bounded_pieces<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, conds: &[T<'h>], x: T<'h>) -> Res<Option<Vec<(f64, f64, T<'h>, T<'h>)>>> {
    let h = b.heap();
    if conds.is_empty() {
        return Ok(None);
    }
    let all = if conds.len() == 1 { conds[0] } else { cx.eval(h.call(h.sym("and"), conds))? };
    let r = cx.eval(h.call(h.sym("reduce"), &[all, x, h.sym_term(h.sym("reals"))]))?;
    if cx.is_false(r) || cx.is_true(r) {
        return Ok(None);
    }
    let Some(ps) = super::range::pieces(b, r, x) else { return Ok(None) };
    let mut out = vec![];
    for p in ps {
        let (Some((l, _)), Some((u, _))) = (p.lo, p.hi) else { return Ok(None) };
        let (Some((l0, _)), Some((_, u1))) = (end_bounds(cx, b, l)?, end_bounds(cx, b, u)?) else { return Ok(None) };
        if l0.partial_cmp(&u1) == Some(std::cmp::Ordering::Greater) || l0.is_nan() || u1.is_nan() {
            return Ok(None);
        }
        out.push((l0, u1, l, u));
    }
    Ok((!out.is_empty()).then_some(out))
}

/// The bounds of an exact numeric end of an interval.
fn end_bounds<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<Option<(f64, f64)>> {
    if let Some(n) = b.ar.num(t) {
        if n.is_exact() {
            return Ok(bounds(b, t));
        }
        return Ok(None);
    }
    let w = cx.eval(b.call("n", &[t, b.int(30)]))?;
    Ok(bounds(b, w))
}
