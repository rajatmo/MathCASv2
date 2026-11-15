//! Injectivity of a function of one real variable from the pieces of its domain and the intermediate value
//! theorem (round 43b).
//!
//! - **Pieces with limits** ([`by_pieces`]): the domain reduced to intervals, f continuous on each (no jumps). The
//!   image of a piece contains the open interval between the limits of f at its two ends; when the open intervals of
//!   two pieces overlap, some value is taken on both: not injective (`exp_integral_ei` takes every negative value on
//!   x < 0 and every real on x > 0). When f is strictly monotone on every piece (the sign of f' proved inside,
//!   [`super::interval`]), the image of a piece is exactly the interval between those limits (with an end value
//!   where the end belongs to the piece), and pairwise disjoint images make f injective (`csch`: (-inf, 0) and
//!   (0, inf)).
//! - **Two stretches** ([`two_stretches`]): disjoint segments [p, q] and [r, s] on each of which f is continuous
//!   (no pole or step inside: its conditions hold on the segment, or an enclosure of f over it exists), with
//!   [f(p), f(q)] and [f(r), f(s)] (ordered) meeting: a value in both is taken in both segments. The segments come
//!   from machine samples; the values at their exact ends are compared by proved signs.
//! - **A turning point**: p < q < r with f(q) above (below) both f(p) and f(r), f continuous on [p, r]; the samples
//!   next to q are taken far enough out that the differences are seen.
//!
//! Literature: the intermediate value theorem and the continuity of the inverse of a strictly monotone function
//! (W. Rudin, "Principles of Mathematical Analysis", 3rd ed., McGraw-Hill 1976, theorems 4.23 and 4.17).
use super::interval;
use super::{conj, defined_on, proof, Call};
use crate::term::Build;
use mc_api::{Store, T};
use mc_eval::{NativeCx, Res};
use std::cmp::Ordering;

/// A value at an end of a piece: the limit (or the value), and whether the piece attains it.
#[derive(Clone, Copy)]
struct End<'h> {
    v: T<'h>,
    attained: bool,
}

/// Compare two values that are real numbers or `inf`, `-inf`: `None` when not decided.
fn compare<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, v: T<'h>) -> Res<Option<Ordering>> {
    let h = b.heap();
    let inf = h.sym_term(h.sym("inf"));
    let ninf = cx.eval(b.neg(inf))?;
    let rank = |t: T<'h>| if t == inf { 1 } else if t == ninf { -1 } else { 0 };
    match (rank(u), rank(v)) {
        (0, 0) => {}
        (a, c) if a != 0 || c != 0 => return Ok(Some(if a == c { Ordering::Equal } else { a.cmp(&c) })),
        _ => {}
    }
    Ok(match proof::closed_sign(cx, b, b.sub(u, v))? {
        Some(1) => Some(Ordering::Greater),
        Some(-1) => Some(Ordering::Less),
        Some(0) => Some(Ordering::Equal),
        _ => None,
    })
}

/// Is `t` a real number (exact, or a closed form with a real 30-digit value) or an infinity?
fn real_or_infinite<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<bool> {
    let h = b.heap();
    let inf = h.sym_term(h.sym("inf"));
    if t == inf || t == cx.eval(b.neg(inf))? {
        return Ok(true);
    }
    if proof::bounds(b, t).is_some() {
        return Ok(true);
    }
    let w = cx.eval(b.call("n", &[t, b.int(30)]))?;
    Ok(proof::bounds(b, w).is_some())
}

/// The images of the pieces of the domain (module comment): `Some(false)` when two overlap, `Some(true)` when f is
/// strictly monotone on each and they are disjoint, `None` otherwise. f must be continuous on its domain.
pub(super) fn by_pieces<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, conds: &[T<'h>]) -> Res<Option<bool>> {
    let h = b.heap();
    if conds.is_empty() {
        return Ok(None);
    }
    let all = conj(cx, conds)?;
    let r = cx.eval(h.call(h.sym("reduce"), &[all, x, h.sym_term(h.sym("reals"))]))?;
    if cx.is_true(r) || cx.is_false(r) {
        return Ok(None);
    }
    let Some(pieces) = super::range::pieces(b, r, x) else { return Ok(None) };
    if pieces.len() < 2 || pieces.len() > 8 {
        return Ok(None);
    }
    // the ends of each image, in the order of the piece's ends
    let mut images: Vec<(End<'h>, End<'h>)> = vec![];
    for p in &pieces {
        let mut ends = vec![];
        for (e, left) in [(p.lo, true), (p.hi, false)] {
            let end = match e {
                Some((t, true)) => End { v: proof::at(cx, b, f, x, t)?, attained: true },
                Some((t, false)) => match interval::side_value(cx, b, f, x, Some(t), left)? {
                    Some(v) => End { v, attained: false },
                    None => return Ok(None),
                },
                None => match interval::side_value(cx, b, f, x, None, left)? {
                    Some(v) => End { v, attained: false },
                    None => return Ok(None),
                },
            };
            if !real_or_infinite(cx, b, end.v)? {
                return Ok(None);
            }
            ends.push(end);
        }
        // ordered: low end first
        let (a, c) = (ends[0], ends[1]);
        images.push(match compare(cx, b, a.v, c.v)? {
            Some(Ordering::Greater) => (c, a),
            Some(_) => (a, c),
            None => return Ok(None),
        });
    }
    // two open images that overlap: max of the lows below min of the highs
    for i in 0..images.len() {
        for j in i + 1..images.len() {
            let (li, hi_) = images[i];
            let (lj, hj) = images[j];
            let low = match compare(cx, b, li.v, lj.v)? {
                Some(Ordering::Less) => lj.v,
                Some(_) => li.v,
                None => continue,
            };
            let high = match compare(cx, b, hi_.v, hj.v)? {
                Some(Ordering::Less) => hi_.v,
                Some(_) => hj.v,
                None => continue,
            };
            if compare(cx, b, low, high)? == Some(Ordering::Less) {
                return Ok(Some(false));
            }
        }
    }
    // strictly monotone on every piece
    for p in &pieces {
        let mut cs = conds.to_vec();
        if let Some((l, _)) = p.lo {
            cs.push(b.call("greater_equal", &[x, l]));
        }
        if let Some((u, _)) = p.hi {
            cs.push(b.call("less_equal", &[x, u]));
        }
        let d = cx.eval(b.call("d", &[f, x]))?;
        if interval::shape_on_domain(cx, b, f, d, x, &cs, 1, true)?.is_none() {
            return Ok(None);
        }
    }
    // pairwise disjoint: one image entirely below the other (a shared end attained by at most one)
    for i in 0..images.len() {
        for j in i + 1..images.len() {
            let (li, hi_) = images[i];
            let (lj, hj) = images[j];
            let below = |cx: &mut NativeCx<'_, '_, 'h>, top: End<'h>, bottom: End<'h>| -> Res<bool> {
                Ok(match compare(cx, b, top.v, bottom.v)? {
                    Some(Ordering::Less) => true,
                    Some(Ordering::Equal) => !(top.attained && bottom.attained),
                    _ => false,
                })
            };
            if !(below(cx, hi_, lj)? || below(cx, hj, li)?) {
                return Ok(None);
            }
        }
    }
    Ok(Some(true))
}

/// The sampled values `vals` (x ascending, inside the domain) of f: two disjoint stretches whose value ranges meet,
/// or a turning point, confirmed (module comment). `true` when f is proved not injective.
pub(super) fn two_stretches<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, conds: &[T<'h>], vals: &[(f64, f64)]) -> Res<bool> {
    let f = c.f;
    let tol = |a: f64, c: f64| 1e-9 * (1.0 + a.abs() + c.abs());
    // turning points: at each sample, the nearest samples on each side whose values differ by more than the
    // tolerance
    let mut turns: Vec<(usize, usize, usize)> = vec![];
    for i in 0..vals.len() {
        let yi = vals[i].1;
        let Some(j) = (0..i).rev().find(|&j| (vals[j].1 - yi).abs() > tol(vals[j].1, yi)) else { continue };
        let Some(k) = (i + 1..vals.len()).find(|&k| (vals[k].1 - yi).abs() > tol(vals[k].1, yi)) else { continue };
        let (yj, yk) = (vals[j].1, vals[k].1);
        if (yi > yj && yi > yk) || (yi < yj && yi < yk) {
            // one candidate per turning region
            if turns.last().is_some_and(|&(_, _, k0)| k0 >= i) {
                continue;
            }
            turns.push((j, i, k));
        }
    }
    turns.sort_by(|a, b2| vals[a.1].0.abs().total_cmp(&vals[b2.1].0.abs()));
    let mut tries = 0;
    for (j, i, k) in turns {
        tries += 1;
        if tries > super::MAX_CONFIRM {
            break;
        }
        let (p, q, r) = (vals[j].0, vals[i].0, vals[k].0);
        let (Some(s1), Some(s2)) = (super::confirmed_diff(cx, b, f, x, conds, &[p, q], 1)?, super::confirmed_diff(cx, b, f, x, conds, &[q, r], 1)?) else { continue };
        if s1 != 0 && s2 == -s1 {
            let (ep, er) = (proof::exact_point(cx, b, p)?, proof::exact_point(cx, b, r)?);
            if defined_on(cx, b, f, x, conds, (ep, p), (er, r), true)? {
                return Ok(true);
            }
        }
    }
    // two disjoint stretches of consecutive samples whose value ranges meet (the largest overlaps first)
    let stretches: Vec<(usize, f64, f64)> = vals.windows(2).enumerate().map(|(i, w)| (i, w[0].1.min(w[1].1), w[0].1.max(w[1].1))).collect();
    let mut cands: Vec<(f64, usize, usize)> = vec![];
    for a in 0..stretches.len() {
        for c2 in a + 2..stretches.len() {
            let (_, la, ha) = stretches[a];
            let (_, lc, hc) = stretches[c2];
            let (lo, hi) = (la.max(lc), ha.min(hc));
            let width = hi - lo;
            if width > tol(lo, hi) {
                let scale = (ha - la).max(hc - lc);
                cands.push((width / scale.max(f64::MIN_POSITIVE), a, c2));
            }
        }
    }
    cands.sort_by(|u, v| v.0.total_cmp(&u.0));
    let mut tries = 0;
    for (_, a, c2) in cands {
        tries += 1;
        if tries > super::MAX_CONFIRM {
            break;
        }
        let (p, q) = (vals[a].0, vals[a + 1].0);
        let (r, s) = (vals[c2].0, vals[c2 + 1].0);
        let mut pts = vec![];
        let mut ok = true;
        for v in [p, q, r, s] {
            let e = proof::exact_point(cx, b, v)?;
            if !proof::holds_at(cx, b, conds, x, e)? {
                ok = false;
                break;
            }
            pts.push(e);
        }
        if !ok {
            continue;
        }
        let mut fs = vec![];
        for &e in &pts {
            fs.push(proof::at(cx, b, f, x, e)?);
        }
        // the order within each stretch, then the overlap: low_a <= high_c and low_c <= high_a
        let ord = |cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>, v: T<'h>| -> Res<Option<i8>> { proof::closed_sign(cx, b, b.sub(u, v)) };
        let (Some(o1), Some(o2)) = (ord(cx, fs[0], fs[1])?, ord(cx, fs[2], fs[3])?) else { continue };
        let (la, ha) = if o1 > 0 { (fs[1], fs[0]) } else { (fs[0], fs[1]) };
        let (lc, hc) = if o2 > 0 { (fs[3], fs[2]) } else { (fs[2], fs[3]) };
        let (Some(u1), Some(u2)) = (ord(cx, la, hc)?, ord(cx, lc, ha)?) else { continue };
        if u1 > 0 || u2 > 0 {
            continue;
        }
        if defined_on(cx, b, f, x, conds, (pts[0], p), (pts[1], q), true)? && defined_on(cx, b, f, x, conds, (pts[2], r), (pts[3], s), true)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The domain of f as the condition of a call (the monotonicity of f on its domain, which the call without a
/// condition reads as on the whole line).
pub(super) fn on_domain<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &Call<'h>, conds: &[T<'h>]) -> Res<Option<Call<'h>>> {
    if c.cond.is_some() || conds.is_empty() {
        return Ok(None);
    }
    let k = conj(cx, conds)?;
    Ok(Some(Call { f: c.f, cond: Some(k), vars: c.vars.clone(), complex: c.complex, integers: c.integers, facts: c.facts.clone(), strict: true }))
}
