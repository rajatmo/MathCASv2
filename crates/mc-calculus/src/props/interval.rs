//! Signs of an expression over a whole interval of the line, also an unbounded one or one whose ends the expression
//! does not reach (round 43b). The sign, monotonicity and convexity of a function of one variable are the signs of
//! f, f' and f'' over its domain; these methods prove them where `refine` alone does not:
//!
//! - **Facts**: `refine` of g > 0 (g >= 0) under a < x < b.
//! - **Factors**: g = n/d (`together`) or a product: the signs of the factors, each proved the same way; a power
//!   u^k by the sign of u.
//! - **From an end**: g tends to a finite L at an end of the interval (a one-sided limit, or the limit at infinity)
//!   and g' has one sign inside: then g - L has one sign (g increases away from the left end, ...), and L >= 0 with g
//!   increasing from the left end gives g > 0. This is the tail argument for an unbounded interval (erfc decreases
//!   to 0 at infinity, so it is positive) and the argument at an end where g is not defined (acosh at 1).
//! - **Enclosures** over a bounded interval ([`super::proof::sign_on`]).
//! - **Splitting** an unbounded interval at a point: the methods above on each part.
//!
//! A sign is *strict* when g has no zero inside, and otherwise its zeros are isolated: g is then real-analytic on the
//! interval (no step function, every subterm defined there) and not identically 0, so a weak sign of f' still makes f
//! strictly monotonic.
//!
//! For a property of f on its domain ([`shape_on_domain`]) the domain is reduced to intervals; monotonicity and
//! convexity need one interval, and f continuous on it (f has no jumps or kinks: the caller checks), so that the sign
//! of f' (f'') on the open interval decides it on the closed one, also where f' is not defined at an end (acosh at
//! 1). Isolated points inside where f' is not defined while f is (sinh(x)/x at 0 for the derivative of
//! sinh_integral) split the interval for monotonicity: f is continuous there, so monotone parts of one direction join.
//!
//! Literature: the mean value theorem and its consequences for monotone functions (W. Rudin, "Principles of
//! Mathematical Analysis", 3rd ed., McGraw-Hill 1976, theorem 5.11); convexity from the second derivative (S. Boyd,
//! L. Vandenberghe, "Convex Optimization", Cambridge 2004, section 3.1.4).
use super::{conditions, conj, contains_call, is_call, nonsmooth, proof};
use crate::common::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// How many methods may nest (a factor of a derivative of a part of the interval ...).
const MAX_DEPTH: u32 = 4;

/// An open interval of the line; `None` for an infinite end.
#[derive(Clone, Copy)]
pub(crate) struct Iv<'h> {
    pub lo: Option<T<'h>>,
    pub hi: Option<T<'h>>,
}

/// The rules that write the reciprocal and quotient functions as quotients.
const QUOTIENTS: &str = "[tan(u_) -> sin(u)/cos(u), cot(u_) -> cos(u)/sin(u), sec(u_) -> 1/cos(u), csc(u_) -> 1/sin(u), \
     tanh(u_) -> sinh(u)/cosh(u), coth(u_) -> cosh(u)/sinh(u), sech(u_) -> 1/cosh(u), csch(u_) -> 1/sinh(u)]";

/// A proved sign: the direction (1 or -1) and whether it is strict (no zero inside; otherwise isolated zeros).
type Sign = (i8, bool);

/// The facts of the open interval.
fn facts_of<'h>(b: &Build<'h>, x: T<'h>, iv: Iv<'h>) -> Vec<T<'h>> {
    let h = b.heap();
    let mut f = vec![h.call(h.sym("element"), &[x, h.sym_term(h.sym("reals"))])];
    if let Some(l) = iv.lo {
        f.push(b.call("less", &[l, x]));
    }
    if let Some(u) = iv.hi {
        f.push(b.call("less", &[x, u]));
    }
    f
}

fn refine_holds<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, facts: &[T<'h>], rel: &str, g: T<'h>) -> Res<bool> {
    let r = cx.eval_assuming(facts, b.call("refine", &[b.call(rel, &[g, b.zero()])]))?;
    Ok(cx.is_true(r))
}

/// Is g defined (and real) at every point of the open interval, and free of step functions (so real-analytic there)?
fn analytic_on<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, facts: &[T<'h>]) -> Res<bool> {
    if nonsmooth(b, g, x) || contains_call(b, g, "d") || contains_call(b, g, "derivative") {
        return Ok(false);
    }
    let Some(cs) = conditions(cx, b, g, &[x], false)? else { return Ok(false) };
    for k in cs {
        let r = cx.eval_assuming(facts, b.call("refine", &[k]))?;
        if !cx.is_true(r) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The sign of g on the open interval (module comment), `None` when not proved.
pub(crate) fn sign_in<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, iv: Iv<'h>, depth: u32) -> Res<Option<Sign>> {
    cx.tick(1)?;
    if depth > MAX_DEPTH || super::out_of_time(cx) {
        return Ok(None);
    }
    if is_free(b, g, x) {
        return Ok(match proof::closed_sign(cx, b, g)? {
            Some(s @ (1 | -1)) => Some((s, true)),
            _ => None,
        });
    }
    let facts = facts_of(b, x, iv);
    if !analytic_on(cx, b, g, x, &facts)? {
        return Ok(None);
    }
    if refine_holds(cx, b, &facts, "greater", g)? {
        return Ok(Some((1, true)));
    }
    if refine_holds(cx, b, &facts, "less", g)? {
        return Ok(Some((-1, true)));
    }
    if let Some(s) = by_factors(cx, b, g, x, iv, depth)? {
        return Ok(Some(s));
    }
    let ge = refine_holds(cx, b, &facts, "greater_equal", g)?;
    let le = refine_holds(cx, b, &facts, "less_equal", g)?;
    match (ge, le) {
        (true, false) => return Ok(Some((1, false))),
        (false, true) => return Ok(Some((-1, false))),
        (true, true) => return Ok(None),
        _ => {}
    }
    if let Some(s) = from_an_end(cx, b, g, x, iv, depth)? {
        return Ok(Some(s));
    }
    if let (Some(l), Some(u)) = (iv.lo, iv.hi) {
        if let Some(s) = enclosed(cx, b, g, x, l, u)? {
            return Ok(Some(s));
        }
        return Ok(None);
    }
    split(cx, b, g, x, iv, depth)
}

/// The sign of g as a quotient or product of factors of known signs.
fn by_factors<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, iv: Iv<'h>, depth: u32) -> Res<Option<Sign>> {
    let h = b.heap();
    // the reciprocal and quotient functions as quotients of sin, cos, sinh, cosh (whose signs refine knows)
    let quotients = cx.eval(b.call("to_expression", &[h.string(QUOTIENTS)]))?;
    let g2 = cx.eval(b.call("rewrite", &[g, quotients]))?;
    let g2 = cleared(cx, b, g2, x, iv)?.unwrap_or(g2);
    let t = cx.eval(b.call("together", &[g2]))?;
    let n = cx.eval(b.call("numerator", &[t]))?;
    let d = cx.eval(b.call("denominator", &[t]))?;
    let mut factors: Vec<T<'h>> = vec![];
    for (part, _) in [(n, false), (d, true)] {
        match h.view(part) {
            View::App { head, args } if b.sym_of(head) == Some(b.hd.mul) => factors.extend(args.iter().copied()),
            _ => factors.push(part),
        }
    }
    let factors: Vec<T<'h>> = factors.into_iter().filter(|&f| f != b.one()).collect();
    // nothing smaller to decide
    if factors.len() < 2 && factors.first().is_none_or(|&f| f == g) {
        if let [f] = factors.as_slice() {
            return power_sign(cx, b, *f, x, iv, depth);
        }
        return Ok(None);
    }
    let (mut s, mut strict) = (1i8, true);
    for f in factors {
        let r = match power_sign(cx, b, f, x, iv, depth)? {
            Some(r) => r,
            None => match sign_in(cx, b, f, x, iv, depth + 1)? {
                Some(r) => r,
                None => return Ok(None),
            },
        };
        s *= r.0;
        strict &= r.1;
    }
    Ok(Some((s, strict)))
}

/// A sum whose terms carry negative powers u^(-e) of positive bases u (fractional ones, which `together` leaves):
/// the sum times the product of the u^e, expanded, which has the same sign. `None` for another term.
fn cleared<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, iv: Iv<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let View::App { head, args } = h.view(g) else { return Ok(None) };
    if b.sym_of(head) != Some(b.hd.add) {
        return Ok(None);
    }
    // the largest negative exponent of each base
    let mut most: Vec<(T<'h>, T<'h>)> = vec![];
    for &t in args {
        let fs: Vec<T<'h>> = match h.view(t) {
            View::App { head, args } if b.sym_of(head) == Some(b.hd.mul) => args.to_vec(),
            _ => vec![t],
        };
        for f in fs {
            let View::App { head, args: &[u, e] } = h.view(f) else { continue };
            if b.sym_of(head) != Some(b.hd.pow) || is_free(b, u, x) || !matches!(h.view(e), View::Int(_) | View::Rational { .. }) {
                continue;
            }
            if b.ar.compare(e, b.zero()) != Some(mc_arith::Cmp3::Less) {
                continue;
            }
            let ne = cx.eval(b.neg(e))?;
            match most.iter_mut().find(|(v, _)| *v == u) {
                Some(m) => {
                    if b.ar.compare(ne, m.1) == Some(mc_arith::Cmp3::Greater) {
                        m.1 = ne;
                    }
                }
                None => most.push((u, ne)),
            }
        }
    }
    if most.is_empty() {
        return Ok(None);
    }
    let facts = facts_of(b, x, iv);
    let mut m = vec![];
    for (u, e) in most {
        if !refine_holds(cx, b, &facts, "greater", u)? {
            return Ok(None);
        }
        m.push(b.pow(u, e));
    }
    m.push(g);
    Ok(Some(cx.eval(b.call("expand", &[b.mul(&m)]))?))
}

/// The sign of a power u^k by the sign of u (`None` for another term).
fn power_sign<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, iv: Iv<'h>, depth: u32) -> Res<Option<Sign>> {
    let h = b.heap();
    let View::App { head, args: &[u, k] } = h.view(f) else { return Ok(None) };
    if b.sym_of(head) != Some(b.hd.pow) || !is_free(b, k, x) {
        return Ok(None);
    }
    let Some((su, strict)) = sign_in(cx, b, u, x, iv, depth + 1)? else { return Ok(None) };
    Ok(match h.view(k) {
        View::Int(e) if e % 2 == 0 => Some((1, strict)),
        View::Int(_) => Some((su, strict)),
        // a fractional power is real here only for a nonnegative base: its principal value is then nonnegative
        View::Rational { .. } | View::Real(_) if su == 1 => Some((1, strict)),
        _ => None,
    })
}

/// A one-sided limit of g at an end of the interval (from inside), when finite and real with a proved sign.
fn end_limit<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, end: Option<T<'h>>, left: bool) -> Res<Option<i8>> {
    let Some(l) = side_value(cx, b, g, x, end, left)? else { return Ok(None) };
    // a finite real number
    let w = cx.eval(b.call("n", &[l, b.int(30)]))?;
    if proof::bounds(b, w).is_none() && proof::bounds(b, l).is_none() {
        return Ok(None);
    }
    proof::closed_sign(cx, b, l)
}

/// The limit of g at an end of the interval from inside (`left`: the lower end; `None`: infinite), free of x;
/// `None` when not found.
pub(crate) fn side_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, end: Option<T<'h>>, left: bool) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let inf = h.sym_term(h.sym("inf"));
    let at = match end {
        Some(e) => e,
        None if left => cx.eval(b.neg(inf))?,
        None => inf,
    };
    let rule = h.call(h.sym(names::RULE), &[x, at]);
    let l = if end.is_none() {
        cx.eval(b.call("limit", &[g, rule]))?
    } else {
        let dir = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("direction")), h.sym_term(h.sym(if left { "from_above" } else { "from_below" }))]);
        cx.eval(b.call("limit", &[g, rule, h.call(h.sym("options"), &[dir])]))?
    };
    // an undecided limit, or `indeterminate` (an oscillation), or one that depends on a direction it was not given
    if is_call(b, l, "limit") || is_call(b, l, names::FAILURE) || !is_free(b, l, x) || is_call(b, l, "interval") || l == h.sym_term(h.sym("indeterminate")) {
        return Ok(None);
    }
    Ok(Some(l))
}

/// g - L has one sign when g' has one sign inside and g tends to L at an end (module comment).
fn from_an_end<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, iv: Iv<'h>, depth: u32) -> Res<Option<Sign>> {
    let mut dg: Option<Option<Sign>> = None;
    for left in [true, false] {
        let end = if left { iv.lo } else { iv.hi };
        let Some(sl) = end_limit(cx, b, g, x, end, left)? else { continue };
        if dg.is_none() {
            let d = cx.eval(b.call("d", &[g, x]))?;
            dg = Some(if contains_call(b, d, "d") || contains_call(b, d, "derivative") { None } else { sign_in(cx, b, d, x, iv, depth + 1)? });
        }
        let Some(Some((s1, _))) = dg else { return Ok(None) };
        // from the left end g moves in the direction of g'; from the right end, against it
        let dir = if left { s1 } else { -s1 };
        if dir == 1 && sl >= 0 {
            return Ok(Some((1, true)));
        }
        if dir == -1 && sl <= 0 {
            return Ok(Some((-1, true)));
        }
    }
    Ok(None)
}

/// The sign of g by enclosures over the closed bounded interval [l, u] (weak: zeros allowed, isolated as g is
/// analytic and not identically 0 there).
fn enclosed<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, l: T<'h>, u: T<'h>) -> Res<Option<Sign>> {
    let (Some((lo, _)), Some((_, hi))) = (end_value(cx, b, l)?, end_value(cx, b, u)?) else { return Ok(None) };
    Ok(match proof::sign_on(cx, b, g, x, (lo, hi), Some((l, u)), false)? {
        Some(s @ (1 | -1)) => Some((s, false)),
        _ => None,
    })
}

/// The bounds of an exact end.
fn end_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<Option<(f64, f64)>> {
    if let Some(n) = b.ar.num(t) {
        return Ok(if n.is_exact() { proof::bounds(b, t) } else { None });
    }
    let w = cx.eval(b.call("n", &[t, b.int(30)]))?;
    Ok(proof::bounds(b, w))
}

/// An unbounded interval split at a point: one sign on both parts and at the point.
fn split<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, iv: Iv<'h>, depth: u32) -> Res<Option<Sign>> {
    let points: Vec<T<'h>> = match (iv.lo, iv.hi) {
        (None, None) => vec![b.zero()],
        (Some(l), None) => vec![cx.eval(b.add(&[l, b.one()]))?, cx.eval(b.add(&[l, b.int(8)]))?],
        (None, Some(u)) => vec![cx.eval(b.sub(u, b.one()))?, cx.eval(b.sub(u, b.int(8)))?],
        _ => return Ok(None),
    };
    for p in points {
        let gp = proof::at(cx, b, g, x, p)?;
        let Some(sp) = proof::closed_sign(cx, b, gp)? else { continue };
        let left = Iv { lo: iv.lo, hi: Some(p) };
        let Some((s1, t1)) = sign_in(cx, b, g, x, left, depth + 1)? else { continue };
        if sp != 0 && sp != s1 {
            continue;
        }
        let right = Iv { lo: Some(p), hi: iv.hi };
        let Some((s2, t2)) = sign_in(cx, b, g, x, right, depth + 1)? else { continue };
        if s1 == s2 {
            return Ok(Some((s1, t1 && t2 && sp != 0)));
        }
    }
    Ok(None)
}

/// The sign (`order` 0), monotonicity (1) or convexity (2) of f on the domain given by `conds` (with f's own
/// conditions among them), from the sign of `g` (f, f' or f''); f has no jumps or kinks (the caller checks).
/// `Some(1)`, `Some(-1)`, or `None` when not proved; with `strict`, a sign must be strict (f > 0), a monotonicity
/// or convexity strict (which a weak sign of f', f'' gives).
#[allow(clippy::too_many_arguments)]
pub(crate) fn shape_on_domain<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    g: T<'h>,
    x: T<'h>,
    conds: &[T<'h>],
    order: usize,
    strict: bool,
) -> Res<Option<i64>> {
    let h = b.heap();
    let reals = h.sym_term(h.sym("reals"));
    let pieces: Vec<super::range::Piece<'h>> = if conds.is_empty() {
        vec![super::range::Piece { lo: None, hi: None }]
    } else {
        let all = conj(cx, conds)?;
        let r = cx.eval(h.call(h.sym("reduce"), &[all, x, reals]))?;
        if cx.is_true(r) {
            vec![super::range::Piece { lo: None, hi: None }]
        } else if cx.is_false(r) {
            return Ok(None);
        } else {
            match super::range::pieces(b, r, x) {
                Some(p) => p,
                None => return Ok(None),
            }
        }
    };
    if order > 0 && pieces.len() != 1 {
        return Ok(None);
    }
    let mut ans: Option<i8> = None;
    let mut agree = |s: i8| -> bool {
        match ans {
            None => {
                ans = Some(s);
                true
            }
            Some(a) => a == s,
        }
    };
    for p in &pieces {
        let iv = Iv { lo: p.lo.map(|e| e.0), hi: p.hi.map(|e| e.0) };
        if order == 0 {
            // the ends in the domain: their exact values
            for (e, closed) in [p.lo, p.hi].into_iter().flatten() {
                if !closed {
                    continue;
                }
                let fe = proof::at(cx, b, f, x, e)?;
                match proof::closed_sign(cx, b, fe)? {
                    Some(0) if !strict => {}
                    Some(s @ (1 | -1)) if agree(s) => {}
                    _ => return Ok(None),
                }
            }
            match sign_in(cx, b, g, x, iv, 0)? {
                Some((s, st)) if (st || !strict) && agree(s) => {}
                _ => return Ok(None),
            }
            continue;
        }
        // points inside where g is not defined: isolated points of f's domain split the interval (monotonicity only)
        let mut cuts: Vec<T<'h>> = vec![];
        let facts = facts_of(b, x, iv);
        if let Some(gc) = conditions(cx, b, g, &[x], false)? {
            if !gc.is_empty() {
                let mut inside = facts[1..].to_vec();
                inside.push(b.call("not", &[conj(cx, &gc)?]));
                let inside = conj(cx, &inside)?;
                let bad = cx.eval(h.call(h.sym("reduce"), &[inside, x, reals]))?;
                if !cx.is_false(bad) {
                    if order != 1 {
                        return Ok(None);
                    }
                    let Some(pts) = points_of(b, bad, x) else { return Ok(None) };
                    for q in pts {
                        // f is defined (so continuous) at the cut
                        if !proof::holds_at(cx, b, conds, x, q)? {
                            return Ok(None);
                        }
                        cuts.push(q);
                    }
                }
            }
        } else {
            return Ok(None);
        }
        cuts.sort_by(|&u, &v| match b.ar.compare(u, v) {
            Some(mc_arith::Cmp3::Less) => std::cmp::Ordering::Less,
            Some(mc_arith::Cmp3::Greater) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });
        let mut ends = vec![iv.lo];
        ends.extend(cuts.iter().map(|&q| Some(q)));
        ends.push(iv.hi);
        for w in ends.windows(2) {
            match sign_in(cx, b, g, x, Iv { lo: w[0], hi: w[1] }, 0)? {
                Some((s, _)) if agree(s) => {}
                _ => return Ok(None),
            }
        }
    }
    Ok(ans.map(i64::from))
}

/// The points of a reduced condition `x == p or x == q ...` with numeric p, q; `None` for another set.
fn points_of<'h>(b: &Build<'h>, r: T<'h>, x: T<'h>) -> Option<Vec<T<'h>>> {
    let h = b.heap();
    let View::App { head, args } = h.view(r) else { return None };
    let name = h.sym_name(b.sym_of(head)?).to_string();
    match (name.as_str(), args) {
        ("or", _) => {
            let mut out = vec![];
            for &a in args {
                out.extend(points_of(b, a, x)?);
            }
            Some(out)
        }
        ("equal", &[l, v]) if l == x && is_free(b, v, x) && b.ar.num(v).is_some_and(|n| n.is_exact()) => Some(vec![v]),
        _ => None,
    }
}
