//! Roots of analytic functions as exact objects: `root([f, x0])` stands for the root of the function `f` that the
//! Newton iteration from `x0` reaches, when that root is simple and isolated with proof.
//!
//! - **Isolation** (Krawczyk's operator, Krawczyk, Computing 4 (1969); Moore, Kearfott and Cloud, *Introduction to
//!   Interval Analysis*, SIAM 2009, section 8.2; Rump, Acta Numerica 19 (2010), section 13): for a box Z with
//!   centre c, a point Y near 1/f'(c) and an enclosure F' of f' on Z, `K(Z) = c - Y f(c) + (1 - Y F')(Z - c)`.
//!   When K(Z) lies in the interior of Z, f has exactly one zero in Z, and it lies in K(Z). A complex box is a pair
//!   of real intervals and the complex product of rectangles is the real 2 x 2 interval product with the matrix
//!   `[[a, -b], [b, a]]` of an analytic f, so the real theorem applies to complex boxes. A real starting value of a
//!   function that is real on the real line gives a real interval (the root is proved real). The enclosures are
//!   those of `mc_numeric` (ball arithmetic over the whole box), so no tolerance decides anything.
//! - **The object**: `root([&(g(&1)), x0])` with g the canonical form of f (below) and x0 the machine number
//!   nearest the root (both parts for a complex root). Creating it proves the isolation; `n` refines the box to the
//!   digits asked (Newton's iteration at a higher precision, then Krawczyk's test again inside the old box), and
//!   comparisons go through `n`. A starting value without a simple isolated root nearby (no root, a double root, a
//!   cluster, a pole or a cut in the way) leaves the call unevaluated.
//! - **Canonical form of f**: the numerator of `together(f)` (powers `b^(k + e)` with an integer k first split into
//!   `b^k b^e`, so that `b^(sin(x) - 1)` and `b^sin(x)/b` give one numerator), its factor that vanishes at the root
//!   (the other factors are proved nonzero on the box, and the denominator too), expanded, scaled to coprime
//!   integer (Gaussian) coefficients, times the unit whose printed form has no leading minus sign (then the shortest). Two objects for the same root of
//!   functions that differ by such factors print alike.
//! - **All roots in a region**: the certified counter (`transroots`) counts the roots of f in a rectangle; a
//!   rectangle with one root is handed to the isolation, others are cut in two until every piece has none or one.
//!   A disk is covered by its square. On a real interval, pieces where f excludes 0 are dropped and pieces where f'
//!   excludes 0 hold one root when f changes sign; the rest are bisected.
//! - **One-variable extrema on a closed bounded interval**: the candidates are the ends and the critical points
//!   (all roots of f' on the interval, isolated as above); the least value is the candidate whose enclosure lies
//!   below those of all others (refined at higher precisions); a tie that the enclosures do not separate declines.
use crate::cx::Tick;
use crate::transroots::{rectangle, span, split_point, take, Fns, Prec};
use mc_api::{names, Store, View, T};
use mc_arith::num::{Ball, Float, Mag, Nat, StopScope};
use mc_arith::number::{Kind, Real};
use mc_arith::{Arith, Num};
use mc_core::Heap;
use mc_eval::{NativeCx, Reply, Res};
use mc_numeric::cball::CBall;
use mc_numeric::numeval::{Graph, Names};
use mc_poly::work::{Halt, Meter, WorkResult};
use std::cell::Cell;
use std::cmp::Ordering;
use std::sync::atomic::{AtomicBool, Ordering as AO};
use std::sync::Arc;

/// Set once a root object of an analytic function has been made in this process: `n` and the comparisons look for
/// such objects only after that.
static MADE: AtomicBool = AtomicBool::new(false);

/// Whether a root object of an analytic function was ever made (a cheap test before scanning terms for one).
pub fn any_made() -> bool {
    MADE.load(AO::Relaxed)
}

/// The most boxes one search may examine.
const MAX_BOXES: u64 = 200_000;
/// The working precisions tried in turn (bits).
const PRECISIONS: [u64; 3] = [64, 128, 256];
/// The bits of the box a root object is made from.
const OBJECT_BITS: u64 = 80;

// ---------------------------------------------------------------------------------------------------------------
// numbers

/// The exact dyadic value of a finite machine real.
pub(crate) fn f64_float(v: f64) -> Option<Float> {
    if !v.is_finite() {
        return None;
    }
    if v == 0.0 {
        return Some(Float::zero());
    }
    let bits = v.to_bits();
    let neg = bits >> 63 == 1;
    let e = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    let (man, exp) = if e == 0 { (frac, -1074) } else { (frac | (1u64 << 52), e - 1075) };
    Some(Float::new(neg, Nat::from_u64(man), exp))
}

/// The nearest machine real of a dyadic number.
fn float_f64(x: &Float) -> f64 {
    mc_arith::num::float::dyadic_to_f64(x.neg, &x.man, x.exp)
}

/// A point (dyadic midpoint) for a real number: exact, machine or ball (its midpoint).
fn real_point(r: &Real, p: u64) -> Option<Float> {
    match r {
        Real::Machine(v) => f64_float(*v),
        Real::Ball(b, _) => Some(b.mid.clone()),
        Real::Exact(_) => match r.to_kind(Kind::Ball, p) {
            Real::Ball(b, _) => Some(b.mid),
            _ => None,
        },
    }
}

/// The point of a number as a complex ball with exact parts.
fn num_point(z: &Num, p: u64) -> Option<CBall> {
    match z {
        Num::Real(r) => Some(CBall::real(Ball::exact(real_point(r, p)?))),
        Num::Complex(a, b) => Some(CBall::new(Ball::exact(real_point(a, p)?), Ball::exact(real_point(b, p)?))),
    }
}

/// The exact midpoint of a box.
fn mid(z: &CBall) -> CBall {
    CBall::new(Ball::exact(z.re.mid.clone()), Ball::exact(z.im.mid.clone()))
}

/// The binary exponent of the larger part of the midpoint (a small number for 0).
fn top(z: &CBall) -> i64 {
    let t = |x: &Float| if x.is_zero() { -100_000 } else { x.top() };
    t(&z.re.mid).max(t(&z.im.mid))
}

/// The box with centre `c` and half-width `r` in each part (a real interval when `real`).
fn boxed(c: &CBall, r: &Mag, real: bool) -> CBall {
    let re = Ball { mid: c.re.mid.clone(), rad: *r };
    let im = if real { Ball::exact(Float::zero()) } else { Ball { mid: c.im.mid.clone(), rad: *r } };
    CBall::new(re, im)
}

/// `a` lies in the interior of `b`.
fn interior(a: &Ball, b: &Ball) -> bool {
    let (al, ah) = a.ends();
    let (bl, bh) = b.ends();
    al.cmp_exact(&bl) == Ordering::Greater && ah.cmp_exact(&bh) == Ordering::Less
}

/// `a` lies in the closed ball `b`.
fn within(a: &Ball, b: &Ball) -> bool {
    let (al, ah) = a.ends();
    let (bl, bh) = b.ends();
    al.cmp_exact(&bl) != Ordering::Less && ah.cmp_exact(&bh) != Ordering::Greater
}

/// The box `k` lies in the interior of `z` (in the real part only, with a real `k`, when `real`).
fn inside(k: &CBall, z: &CBall, real: bool) -> bool {
    if real {
        k.is_real() && interior(&k.re, &z.re)
    } else {
        interior(&k.re, &z.re) && interior(&k.im, &z.im)
    }
}

/// The box `k` lies in the closed box `z`.
fn subset(k: &CBall, z: &CBall) -> bool {
    within(&k.re, &z.re) && within(&k.im, &z.im)
}

/// The box has `bits` correct bits relative to its size.
fn accurate(z: &CBall, bits: u64) -> bool {
    let size = top(z).max(-(bits as i64) * 4);
    [&z.re, &z.im].iter().all(|b| b.rad.is_zero() || b.rad.log2() < (size - bits as i64) as f64)
}

// ---------------------------------------------------------------------------------------------------------------
// isolation

/// The functions f and f' (and more) as numeric graphs in `x`; `None` when one has no numeric graph.
pub(crate) fn graphs<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, ds: &[T<'h>], x: T<'h>, library: Option<&[&str]>) -> Option<Fns<'h>> {
    let View::Sym(xs) = h.view(x) else { return None };
    let nm = Names::new(h);
    let all = h.call(h.sym(names::LIST), ds);
    let g = Graph::build_with_vars(h, ar, &nm, all, &[xs]);
    let mut is = vec![];
    for &d in ds {
        let island = g.island(g.node(d)?);
        if let Some(lib) = library {
            if g.library_names(&island).iter().any(|n| !lib.contains(n)) {
                return None;
            }
        }
        is.push(island);
    }
    Some(Fns { g, is })
}

/// Krawczyk's operator on the box `z`.
fn krawczyk(fns: &Fns, z: &CBall, p: u64) -> Option<CBall> {
    let c = mid(z);
    let fc = fns.at(0, &c, p)?;
    let dc = fns.at(1, &c, p)?;
    let y = mid(&mid(&dc).recip(p)?);
    let dz = fns.at(1, z, p)?;
    // f itself must have an enclosure on the box: the enclosures refuse boxes that meet a pole or a branch cut, so
    // f is analytic on z (an enclosure of f' alone does not show that: 1/z is the derivative of log across its cut)
    fns.at(0, z, p)?;
    let one = CBall::from_i64(1);
    let lin = one.sub(&y.mul(&dz, p), p).mul(&z.sub(&c, p), p);
    Some(c.sub(&y.mul(&fc, p), p).add(&lin, p))
}

/// Newton's iteration from `start` at precision `p` (real when `real`): the point reached and the binary exponent
/// of the last step, or `None` when it does not settle.
fn newton(fns: &Fns, start: &CBall, real: bool, p: u64, m: &mut Meter) -> WorkResult<Option<(CBall, i64)>> {
    let mut c = mid(start);
    for _ in 0..120 {
        m.step(16)?;
        let (Some(fc), Some(dc)) = (fns.at(0, &c, p), fns.at(1, &c, p)) else { return Ok(None) };
        if fc.re.mid.is_zero() && fc.im.mid.is_zero() {
            return Ok(Some((c, -100_000)));
        }
        let Some(step) = mid(&fc).div(&mid(&dc), p) else { return Ok(None) };
        let step = if real { CBall::real(Ball::exact(step.re.mid.clone())) } else { mid(&step) };
        c = mid(&c.sub(&step, p));
        let s = top(&step);
        if s < top(&c).max(0) - p as i64 + 8 {
            return Ok(Some((c, s)));
        }
    }
    Ok(None)
}

/// A box around the root that Newton's iteration from `start` reaches, proved by Krawczyk's test to hold exactly
/// one root; `None` when not proved at precision `p`.
pub(crate) fn isolate(fns: &Fns, start: &CBall, real: bool, p: u64, m: &mut Meter) -> WorkResult<Option<CBall>> {
    let Some((c, s)) = newton(fns, start, real, p, m)? else { return Ok(None) };
    let base = (s + 2).max(top(&c).max(0) - p as i64 + 24);
    for k in 0..8 {
        m.step(16)?;
        let z = boxed(&c, &Mag::pow2(base + 3 * k), real);
        let Some(kz) = krawczyk(fns, &z, p) else { continue };
        if inside(&kz, &z, real) {
            return Ok(Some(kz));
        }
    }
    Ok(None)
}

/// The box `z` (one root, proved) narrowed to `bits` correct bits; `None` when not reached.
pub(crate) fn refine(fns: &Fns, z: &CBall, real: bool, bits: u64, m: &mut Meter) -> WorkResult<Option<CBall>> {
    if accurate(z, bits) {
        return Ok(Some(z.clone()));
    }
    let mut p = bits + 40;
    for _ in 0..4 {
        if let Some(k) = isolate(fns, &mid(z), real, p, m)? {
            if subset(&k, z) && accurate(&k, bits) {
                return Ok(Some(k));
            }
        }
        p *= 2;
    }
    Ok(None)
}

/// Is f real on the real line near the real point `c` (its value there has an exact zero imaginary part)?
fn real_function(fns: &Fns, c: &CBall, p: u64) -> bool {
    c.is_real() && fns.at(0, c, p).is_some_and(|v| v.is_real()) && fns.at(1, c, p).is_some_and(|v| v.is_real())
}

/// A root box of f near `start`: real when the start is real and f is real there, else complex.
fn isolate_any(fns: &Fns, start: &CBall, m: &mut Meter) -> WorkResult<Option<(CBall, bool)>> {
    for &p in &PRECISIONS {
        let real = real_function(fns, &mid(start), p);
        if let Some(z) = isolate(fns, start, real, p, m)? {
            return Ok(Some((z, real)));
        }
    }
    Ok(None)
}

/// The roots of the real function f in the open interval (a, b) (exact ends where f is proved nonzero), as proved
/// root intervals in increasing order; `None` when not proved.
pub(crate) fn real_roots(fns: &Fns, a: &Float, b: &Float, m: &mut Meter) -> WorkResult<Option<Vec<Ball>>> {
    'prec: for &p in &PRECISIONS {
        let bud = Cell::new(MAX_BOXES);
        let sign = |x: &Float| fns.sign(x, p);
        let (Some(sa), Some(sb)) = (sign(a), sign(b)) else { continue };
        let size = b.sub_exact(a).top();
        let mut stack = vec![(a.clone(), b.clone(), sa, sb)];
        let mut out: Vec<Ball> = vec![];
        while let Some((l, r, sl, sr)) = stack.pop() {
            m.step(8)?;
            if !take(&bud) {
                return Ok(None);
            }
            let x = span(&l, &r);
            let w = r.sub_exact(&l);
            match fns.real(0, &x, p) {
                Some(v) if !v.contains_zero() => continue,
                // no enclosure on a box this small: a pole or a point outside the domain
                None if w.top() < size - 40 => return Ok(None),
                _ => {}
            }
            if let Some(dv) = fns.real(1, &x, p) {
                if !dv.contains_zero() {
                    if sl == sr {
                        continue;
                    }
                    // monotone with a change of sign: one root inside
                    let z = CBall::real(x.clone());
                    let found = match krawczyk(fns, &z, p) {
                        Some(k) if inside(&k, &z, true) => Some(k),
                        _ => isolate(fns, &z, true, p, m)?.filter(|k| subset(k, &z)),
                    };
                    if let Some(k) = found {
                        out.push(k.re);
                        continue;
                    }
                }
            }
            let scale = l.top().max(r.top()).max(size);
            if w.is_zero() || w.top() < scale - p as i64 + 12 {
                continue 'prec;
            }
            let mut cut = None;
            for k in [8, 7, 9, 6, 10, 5, 11] {
                let s = split_point(&l, &r, k);
                if let Some(ss) = sign(&s) {
                    cut = Some((s, ss));
                    break;
                }
            }
            let Some((s, ss)) = cut else { continue 'prec };
            stack.push((s.clone(), r, ss, sr));
            stack.push((l, s, sl, ss));
        }
        out.sort_by(|u, v| u.mid.cmp_exact(&v.mid));
        return Ok(Some(out));
    }
    Ok(None)
}

/// The roots of f in the closed rectangle [x0, x1] x [y0, y1] (exact corners; none on the boundary), as proved root
/// boxes; `None` when not proved.
pub(crate) fn rect_roots(fns: &Fns, corners: [&Float; 4], m: &mut Meter) -> WorkResult<Option<Vec<CBall>>> {
    for &p in &PRECISIONS[..2] {
        let bud = Cell::new(MAX_BOXES);
        if let Some(v) = rect_at(fns, corners, Prec { p, bud: &bud }, 24, m)? {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

fn rect_at(fns: &Fns, [x0, x1, y0, y1]: [&Float; 4], w: Prec, depth: u32, m: &mut Meter) -> WorkResult<Option<Vec<CBall>>> {
    let Some(n) = rectangle(fns, [x0, x1, y0, y1], w, m)? else { return Ok(None) };
    if n == 0 {
        return Ok(Some(vec![]));
    }
    let rect = CBall::new(span(x0, x1), span(y0, y1));
    if n == 1 {
        let found = match krawczyk(fns, &rect, w.p) {
            Some(k) if inside(&k, &rect, false) => Some(k),
            _ => isolate(fns, &mid(&rect), false, w.p, m)?.filter(|k| subset(k, &rect)),
        };
        if let Some(k) = found {
            return Ok(Some(vec![k]));
        }
    }
    if depth == 0 {
        return Ok(None);
    }
    let wide = x1.sub_exact(x0).cmp_exact(&y1.sub_exact(y0)) != Ordering::Less;
    for k in [8, 7, 9, 6, 10] {
        let halves = if wide {
            let s = split_point(x0, x1, k);
            [[x0.clone(), s.clone(), y0.clone(), y1.clone()], [s, x1.clone(), y0.clone(), y1.clone()]]
        } else {
            let s = split_point(y0, y1, k);
            [[x0.clone(), x1.clone(), y0.clone(), s.clone()], [x0.clone(), x1.clone(), s, y1.clone()]]
        };
        let mut all = vec![];
        let mut ok = true;
        for hh in &halves {
            match rect_at(fns, [&hh[0], &hh[1], &hh[2], &hh[3]], w, depth - 1, m)? {
                Some(v) => all.extend(v),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && all.len() as u64 == n {
            return Ok(Some(all));
        }
        if w.bud.get() == 0 {
            return Ok(None);
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------------------------------
// the evaluator side

/// Runs a numeric search with the evaluator's fuel and interrupt; `Ok(None)` when it stops for want of memory or
/// on the arithmetic's own stop.
fn metered<'h, R>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, f: impl FnOnce(&mut Meter) -> WorkResult<Option<R>>) -> Res<Option<R>> {
    let _scope = StopScope::enter(Some(stop));
    let mut tick = Tick { cx, stop, abort: None };
    let r = {
        let mut m = Meter::new(&mut tick);
        f(&mut m)
    };
    match r {
        Ok(v) => Ok(v),
        Err(Halt::Stopped) => match tick.abort {
            Some(a) => Err(a),
            None => Ok(None),
        },
        Err(Halt::Memory) => Ok(None),
    }
}

fn sym<'h>(h: &'h Heap, name: &str) -> T<'h> {
    h.sym_term(h.sym(name))
}

fn head_is(h: &Heap, t: T<'_>, name: &str) -> bool {
    matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym(name)))
}

fn args_of<'h>(h: &'h Heap, t: T<'h>, name: &str) -> Option<&'h [T<'h>]> {
    match h.view(t) {
        View::App { head, args } if head == h.sym_term(h.sym(name)) => Some(args),
        _ => None,
    }
}

fn contains(h: &Heap, t: T<'_>, x: T<'_>) -> bool {
    let mut stack = vec![t];
    let mut seen = std::collections::HashSet::new();
    while let Some(u) = stack.pop() {
        if u == x {
            return true;
        }
        if !seen.insert(u) {
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// The variable and body of the function of a root object (`&(...)`, `x => ...`, or a function applied to `&1`).
fn function_body<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let h = cx.heap();
    if let Some(&[params, body]) = args_of(h, f, "fn") {
        if let Some(&[x]) = cx.as_list(params) {
            if matches!(h.view(x), View::Sym(_)) {
                // the body of a function is held: evaluated here (`im` becomes a number)
                let body = cx.eval(body)?;
                return Ok(Some((x, body)));
            }
        }
        return Ok(None);
    }
    if !matches!(h.view(f), View::Sym(_)) {
        return Ok(None);
    }
    let x = sym(h, "&1");
    let body = cx.eval(h.app(f, &[x]))?;
    Ok((contains(h, body, x) && !head_is(h, body, &h.sym_name(match h.view(f) {
        View::Sym(s) => s,
        _ => return Ok(None),
    })))
    .then_some((x, body)))
}

/// The derivative of `body` in `x`, evaluated.
fn derivative<'h>(cx: &mut NativeCx<'_, '_, 'h>, body: T<'h>, x: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    cx.eval(h.call(h.sym("d"), &[body, x]))
}

/// `t` with the subterms in `rules` (term, replacement) replaced, evaluated.
fn replace<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, rules: &[(T<'h>, T<'h>)]) -> Res<T<'h>> {
    if rules.is_empty() {
        return Ok(t);
    }
    let h = cx.heap();
    let rs: Vec<T<'h>> = rules.iter().map(|&(a, b)| h.call(h.sym(names::RULE), &[a, b])).collect();
    let list = h.call(h.sym(names::LIST), &rs);
    cx.eval(h.call(h.sym("rewrite"), &[t, list]))
}

/// Powers `b^(k + e)` (b depends on x, k a nonzero integer, e not a number) as `b^k K` with a new symbol K for
/// `b^e`, and lone powers `b^e` with a non-numeric e as K: the rules K -> b^e to undo it.
fn split_powers<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, x: T<'h>) -> Res<(T<'h>, Vec<(T<'h>, T<'h>)>)> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let pow = h.sym_term(h.sym("pow"));
    let add = h.sym_term(h.sym("add"));
    let mut found: Vec<(T<'h>, T<'h>, T<'h>)> = vec![];
    let mut stack = vec![t];
    let mut seen = std::collections::HashSet::new();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { continue };
        if head == pow && args.len() == 2 && contains(h, args[0], x) && contains(h, args[1], x) {
            if let View::App { head: eh, args: es } = h.view(args[1]) {
                if eh == add && matches!(h.view(es[0]), View::Int(k) if k != 0) && es[1..].iter().all(|&e| ar.num(e).is_none()) {
                    let rest = if es.len() == 2 { es[1] } else { h.app(add, &es[1..]) };
                    found.push((u, args[0], rest));
                }
            }
        }
        stack.extend_from_slice(args);
    }
    if found.is_empty() {
        return Ok((t, vec![]));
    }
    let mut fwd = vec![];
    let mut back = vec![];
    for (i, (u, b, rest)) in found.into_iter().enumerate() {
        let k = sym(h, &format!("root$k{i}"));
        let View::App { args, .. } = h.view(u) else { continue };
        let View::App { args: es, .. } = h.view(args[1]) else { continue };
        let pw = cx.eval(h.call(h.sym("pow"), &[b, rest]))?;
        fwd.push((u, h.call(h.sym("mul"), &[h.call(h.sym("pow"), &[b, es[0]]), k])));
        back.push((k, pw));
    }
    // innermost first is not needed: the replaced powers are kernels of the outer term
    let s = replace(cx, t, &fwd)?;
    Ok((s, back))
}

/// The number of distinct subterms of `t`, counted up to `limit`.
fn term_size(h: &Heap, t: T<'_>, limit: usize) -> usize {
    let mut stack = vec![t];
    let mut seen = std::collections::HashSet::new();
    while let Some(u) = stack.pop() {
        if seen.len() >= limit {
            break;
        }
        if !seen.insert(u) {
            continue;
        }
        if let View::App { args, .. } = h.view(u) {
            stack.extend_from_slice(args);
        }
    }
    seen.len()
}

/// The terms of a sum (or the one term).
fn terms_of<'h>(h: &'h Heap, t: T<'h>) -> Vec<T<'h>> {
    match args_of(h, t, "add") {
        Some(a) => a.to_vec(),
        None => vec![t],
    }
}

/// The numeric coefficient of a term (1 when it has none).
fn coefficient<'h>(ar: &Arith<'h, Heap>, h: &'h Heap, t: T<'h>) -> Num {
    if let Some(n) = ar.num(t) {
        return n;
    }
    if let Some(a) = args_of(h, t, "mul") {
        if let Some(n) = a.first().and_then(|&c| ar.num(c)) {
            return n;
        }
    }
    Num::int(1)
}

/// The canonical function of a root: the factor of the numerator of `together(body)` that vanishes on the root box
/// `z`, expanded, with coprime integer coefficients and a chosen unit; `None` when the factor is not found
/// (no factor or more than one vanishes there, a vanishing factor is a power, or the denominator may vanish).
fn canonical_body<'h>(cx: &mut NativeCx<'_, '_, 'h>, body: T<'h>, x: T<'h>, z: &CBall) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let (split, back) = split_powers(cx, body, x)?;
    let tg = cx.eval(h.call(h.sym("together"), &[split]))?;
    let num = cx.eval(h.call(h.sym("numerator"), &[tg]))?;
    let den = cx.eval(h.call(h.sym("denominator"), &[tg]))?;
    // the denominator is nonzero on the box
    if contains(h, den, x) {
        let d = replace(cx, den, &back)?;
        let Some(f) = graphs(h, &ar, &[d], x, None) else { return Ok(None) };
        let ok = PRECISIONS.iter().any(|&p| f.at(0, z, p).is_some_and(|v| !v.contains_zero()));
        if !ok {
            return Ok(None);
        }
    }
    // the factors of the numerator that vanish on the box
    let size = term_size(h, num, 601);
    let fac = if size <= 600 { cx.eval(h.call(h.sym("factor"), &[num]))? } else { num };
    let factors: Vec<(T<'h>, i64)> = match args_of(h, fac, "mul") {
        Some(a) => a.to_vec(),
        None => vec![fac],
    }
    .into_iter()
    .filter(|&u| contains(h, u, x))
    .map(|u| match args_of(h, u, "pow") {
        Some(&[b, e]) => match h.view(e) {
            View::Int(k) => (b, k),
            _ => (u, 1),
        },
        _ => (u, 1),
    })
    .collect();
    let mut vanishing = vec![];
    for &(u, k) in &factors {
        let v = replace(cx, u, &back)?;
        let Some(f) = graphs(h, &ar, &[v], x, None) else { return Ok(None) };
        let nonzero = PRECISIONS.iter().any(|&p| f.at(0, z, p).is_some_and(|w| !w.contains_zero()));
        if !nonzero {
            vanishing.push((u, k));
        }
    }
    let [(chosen, 1)] = vanishing.as_slice() else { return Ok(None) };
    let e = cx.eval(h.call(h.sym("expand"), &[*chosen]))?;
    let e = replace(cx, e, &back)?;
    // coprime integer coefficients and a chosen unit
    let terms = terms_of(h, e);
    let mut dens = vec![];
    let mut nums = vec![];
    let mut complex = false;
    for &u in &terms {
        let c = coefficient(&ar, h, u);
        complex |= matches!(c, Num::Complex(..));
        let parts: Vec<Real> = match c {
            Num::Real(r) => vec![r],
            Num::Complex(a, b) => vec![a, b],
        };
        for r in parts {
            let Real::Exact(q) = r else { return Ok(None) };
            if q.is_zero() {
                continue;
            }
            let t = ar.number(&Num::rat(q));
            nums.push(h.call(h.sym("numerator"), &[t]));
            dens.push(h.call(h.sym("denominator"), &[t]));
        }
    }
    if nums.is_empty() {
        return Ok(None);
    }
    let l = cx.eval(h.call(h.sym("lcm"), &dens))?;
    let g = cx.eval(h.call(h.sym("gcd"), &nums))?;
    let scale = cx.eval(h.call(h.sym("mul"), &[l, h.call(h.sym("pow"), &[h.call(h.sym("abs"), &[g]), h.int(-1)])]))?;
    if ar.num(scale).is_none_or(|s| !s.is_exact() || s.is_zero()) {
        return Ok(None);
    }
    // the unit: of the multiples by 1, -1 (and i, -i when a coefficient is not real) the one whose printed form
    // does not begin with a minus sign, then the shortest, then the least string: a choice that depends only on the
    // set of multiples, so functions that differ by a unit agree
    let i = ar.number(&Num::i());
    let mut units = vec![ar.int(1), ar.int(-1)];
    if complex {
        units.push(i);
        units.push(ar.neg(i));
    }
    let mut best: Option<((bool, usize, String), T<'h>)> = None;
    for u in units {
        let c = cx.eval(h.call(h.sym("expand"), &[h.call(h.sym("mul"), &[scale, u, e])]))?;
        let s = cx.eval(h.call(h.sym("to_string"), &[c]))?;
        let View::Str(s) = h.view(s) else { return Ok(None) };
        let s = s.to_string();
        let key = (s.starts_with('-'), s.len(), s);
        if best.as_ref().is_none_or(|(k, _)| key < *k) {
            best = Some((key, c));
        }
    }
    Ok(best.map(|(_, c)| c))
}

/// The machine number nearest the box's midpoint.
/// (A part whose box holds 0 is 0.0: a root on an axis that the box does not prove off it.)
fn machine_point<'h>(ar: &Arith<'h, Heap>, z: &CBall, real: bool) -> T<'h> {
    let part = |b: &Ball| if b.contains_zero() { 0.0 } else { float_f64(&b.mid) };
    let re = part(&z.re);
    if real {
        ar.number(&Num::Real(Real::Machine(re)))
    } else {
        ar.number(&Num::Complex(Real::Machine(re), Real::Machine(part(&z.im))))
    }
}

/// The root object of the root of `body` (a function of `x`) in the proved root box `z` (`real`: proved real):
/// the canonical function, the root isolated again for it, and the machine starting value. `None` when the
/// canonical form or the isolation is not reached.
pub fn make_object<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, body: T<'h>, x: T<'h>, z: &CBall, real: bool) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let slot = sym(h, "&1");
    let Some(cb) = canonical_body(cx, body, x, z)? else { return Ok(None) };
    let cb = if x == slot { cb } else { replace(cx, cb, &[(x, slot)])? };
    // a polynomial factor: the root is algebraic, given exactly by solve
    if !transcendental(h, &ar, cb, slot) {
        let call = h.call(h.sym("solve"), &[h.call(h.sym("equal"), &[cb, h.int(0)]), slot]);
        return algebraic_value(cx, call, z);
    }
    let dcb = derivative(cx, cb, slot)?;
    let Some(fns) = graphs(h, &ar, &[cb, dcb], slot, None) else { return Ok(None) };
    let start = mid(z);
    let zz = z.clone();
    let found = metered(cx, stop, |m| {
        for &p in &PRECISIONS {
            if let Some(k) = isolate(&fns, &start, real, p, m)? {
                // the factor's root in the old box is the root (the other factors and the denominator are nonzero
                // there)
                if subset(&k, &zz) {
                    return refine(&fns, &k, real, OBJECT_BITS, m);
                }
            }
        }
        Ok(None)
    })?;
    let Some(k) = found else { return Ok(None) };
    // a real root of a small function that solve gives exactly on a short interval around it (sin(x) at pi)
    if real && term_size(h, cb, 41) <= 40 {
        let c = float_f64(&k.re.mid);
        let d = 1e-9 * c.abs().max(1.0);
        if let (Some(lo), Some(hi)) = (mc_numeric::value::machine_rat(c - d), mc_numeric::value::machine_rat(c + d)) {
            let (lo, hi) = (ar.number(&Num::rat(lo)), ar.number(&Num::rat(hi)));
            let cond = h.call(
                h.sym("and"),
                &[h.call(h.sym("equal"), &[cb, h.int(0)]), h.call(h.sym("less"), &[lo, slot]), h.call(h.sym("less"), &[slot, hi])],
            );
            let call = h.call(h.sym("solve"), &[cond, slot, sym(h, "reals")]);
            if let Some(v) = algebraic_value(cx, call, &k)? {
                return Ok(Some(v));
            }
        }
    }
    MADE.store(true, AO::Relaxed);
    let f = h.call(h.sym("fn"), &[h.call(h.sym(names::LIST), &[slot]), cb]);
    let spec = h.call(h.sym(names::LIST), &[f, machine_point(&ar, &k, real)]);
    Ok(Some(h.call(h.sym("root"), &[spec])))
}

/// The root in the box `z` (where the function has exactly one root), exactly: the solution of the `solve` call
/// whose enclosure meets the box, when exactly one does and every solution has a numeric value.
fn algebraic_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, call: T<'h>, z: &CBall) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let s = cx.eval(call)?;
    let Some(items) = cx.as_list(s).map(|v| v.to_vec()) else { return Ok(None) };
    let mut hit = None;
    for it in items {
        let Some(&[r]) = cx.as_list(it) else { return Ok(None) };
        let Some(&[_, v]) = args_of(h, r, names::RULE) else { return Ok(None) };
        let nv = cx.eval(h.call(h.sym("n"), &[v, h.int(40)]))?;
        let Some(b) = ar.num(nv).and_then(|n| match n {
            Num::Real(Real::Ball(a, _)) => Some(CBall::real(a)),
            Num::Real(r @ Real::Exact(_)) => match r.to_kind(Kind::Ball, 160) {
                Real::Ball(a, _) => Some(CBall::real(a)),
                _ => None,
            },
            Num::Complex(Real::Ball(a, _), Real::Ball(b, _)) => Some(CBall::new(a, b)),
            _ => None,
        }) else {
            return Ok(None);
        };
        let meets = |u: &Ball, w: &Ball| {
            let (ul, uh) = u.ends();
            let (wl, wh) = w.ends();
            ul.cmp_exact(&wh) != Ordering::Greater && wl.cmp_exact(&uh) != Ordering::Greater
        };
        if meets(&b.re, &z.re) && meets(&b.im, &z.im) {
            if hit.is_some() {
                return Ok(None);
            }
            hit = Some(v);
        }
    }
    Ok(hit)
}

/// The parts of a root object `root([f, x0])`: variable, body, start point.
fn object_parts<'h>(cx: &mut NativeCx<'_, '_, 'h>, spec: T<'h>) -> Res<Option<(T<'h>, T<'h>, CBall)>> {
    let h = cx.heap();
    let Some(&[f, x0]) = cx.as_list(spec) else { return Ok(None) };
    let ar = Arith::new(h);
    let Some(n) = ar.num(x0) else { return Ok(None) };
    let Some(start) = num_point(&n, 128) else { return Ok(None) };
    let Some((x, body)) = function_body(cx, f)? else { return Ok(None) };
    Ok(Some((x, body, start)))
}

/// `root([f, x0])`: the canonical root object, or unevaluated when no simple root is isolated near x0.
pub fn root_native<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, t: T<'h>, spec: T<'h>) -> Res<Reply<'h>> {
    let Some((x, body, start)) = object_parts(cx, spec)? else { return Ok(Reply::Decline) };
    let h = cx.heap();
    let ar = Arith::new(h);
    // a polynomial is the algebraic root object's business
    if !transcendental(h, &ar, body, x) {
        return Ok(Reply::Decline);
    }
    let df = derivative(cx, body, x)?;
    let Some(fns) = graphs(h, &ar, &[body, df], x, None) else { return Ok(Reply::Decline) };
    let Some((z, real)) = metered(cx, stop, |m| isolate_any(&fns, &start, m))? else { return Ok(Reply::Decline) };
    let Some(obj) = make_object(cx, stop, body, x, &z, real)? else { return Ok(Reply::Decline) };
    Ok(if obj == t { Reply::Decline } else { Reply::Done(obj) })
}

/// Does `body` have x inside something other than sums, products and integer powers?
fn transcendental<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, body: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![body];
    while let Some(u) = stack.pop() {
        if !contains(h, u, x) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { continue };
        let name = match h.view(head) {
            View::Sym(s) => h.sym_name(s),
            _ => return true,
        };
        match name.as_str() {
            "add" | "mul" => stack.extend_from_slice(args),
            "pow" if args.len() == 2 && ar.num(args[1]).is_some_and(|n| n.as_int().is_some()) => stack.push(args[0]),
            _ => return true,
        }
    }
    false
}

/// The value of a root object at `bits` bits (`None`: machine precision): a box with the root.
fn object_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, spec: T<'h>, bits: u64) -> Res<Option<(CBall, bool)>> {
    let Some((x, body, start)) = object_parts(cx, spec)? else { return Ok(None) };
    let h = cx.heap();
    let ar = Arith::new(h);
    let df = derivative(cx, body, x)?;
    let Some(fns) = graphs(h, &ar, &[body, df], x, None) else { return Ok(None) };
    metered(cx, stop, |m| {
        let Some((z, real)) = isolate_any(&fns, &start, m)? else { return Ok(None) };
        Ok(refine(&fns, &z, real, bits, m)?.map(|k| (k, real)))
    })
}

/// The root objects of analytic functions inside `t` (shared subterms once).
fn objects<'h>(h: &'h Heap, t: T<'h>) -> Vec<T<'h>> {
    let root = sym(h, "root");
    let list = sym(h, names::LIST);
    let mut out = vec![];
    let mut stack = vec![t];
    let mut seen = std::collections::HashSet::new();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) || h.packed(u).is_some() {
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            if head == root && args.len() == 1 && matches!(h.view(args[0]), View::App { head: l, .. } if l == list) {
                out.push(u);
                continue;
            }
            stack.extend_from_slice(args);
        }
    }
    out
}

/// `t` with each root object of an analytic function replaced by its value: machine numbers (`digits` None) or
/// balls of `digits` decimal digits and some guard digits; `None` when there is none or one has no value.
pub fn numeric_objects<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, t: T<'h>, digits: Option<u64>) -> Res<Option<T<'h>>> {
    if !any_made() {
        return Ok(None);
    }
    let h = cx.heap();
    let objs = objects(h, t);
    if objs.is_empty() {
        return Ok(None);
    }
    let ar = Arith::new(h);
    let mut rules = vec![];
    for o in objs {
        let View::App { args, .. } = h.view(o) else { continue };
        let bits = match digits {
            None => 64,
            Some(d) => mc_numeric::numeval::first_precision(d) + 16,
        };
        let Some((z, real)) = object_value(cx, stop, args[0], bits)? else { return Ok(None) };
        let v = match digits {
            None => machine_point(&ar, &z, real),
            Some(_) => {
                let z = if real { CBall::real(z.re) } else { z };
                mc_numeric::numeval::cball_term(&ar, z, bits)
            }
        };
        rules.push((o, v));
    }
    Ok(Some(replace(cx, t, &rules)?))
}

// ---------------------------------------------------------------------------------------------------------------
// producers

/// A dyadic point of a closed real number term at 128 bits, exactly when the number is dyadic.
fn exact_float<'h>(h: &'h Heap, t: T<'h>) -> Option<Float> {
    let ar = Arith::new(h);
    match ar.num(t)? {
        Num::Real(Real::Exact(q)) => {
            let Real::Ball(b, _) = Real::Exact(q).to_kind(Kind::Ball, 256) else { return None };
            b.rad.is_zero().then_some(b.mid)
        }
        _ => None,
    }
}

/// The roots of `f` (a function of `x`) in the rectangle [x0, x1] x [y0, y1] as (box, real) pairs.
fn roots_in_rectangle<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, f: T<'h>, x: T<'h>, c: [Float; 4]) -> Res<Option<Vec<(CBall, bool)>>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let df = derivative(cx, f, x)?;
    let Some(fns) = graphs(h, &ar, &[f, df], x, Some(crate::transroots::COUNT_LIBRARY)) else { return Ok(None) };
    metered(cx, stop, |m| {
        let Some(boxes) = rect_roots(&fns, [&c[0], &c[1], &c[2], &c[3]], m)? else { return Ok(None) };
        let mut out = vec![];
        for z in boxes {
            // a box across the real line of a function real there: the root is real when a real one is in the box
            if z.im.contains_zero() {
                let c0 = CBall::real(Ball::exact(z.re.mid.clone()));
                if real_function(&fns, &c0, 128) {
                    if let Some(k) = isolate(&fns, &c0, true, 128, m)? {
                        if subset(&k, &z) {
                            out.push((k, true));
                            continue;
                        }
                    }
                }
            }
            out.push((z, false));
        }
        Ok(Some(out))
    })
}

/// A region of the complex plane read from conditions on one unknown: re/im bounds and disks.
#[derive(Default)]
struct Region<'h> {
    re: [Option<(T<'h>, bool)>; 2],
    im: [Option<(T<'h>, bool)>; 2],
    disks: Vec<(T<'h>, T<'h>, bool)>,
}

/// Reads `a op b` into the region (`op` one of the order relations); false when not understood.
fn read_bound<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, c: T<'h>, r: &mut Region<'h>) -> Res<bool> {
    let h = cx.heap();
    let View::App { head, args } = h.view(c) else { return Ok(false) };
    let View::Sym(s) = h.view(head) else { return Ok(false) };
    let name = h.sym_name(s);
    let (less, strict) = match name.as_str() {
        "less" => (true, true),
        "less_equal" => (true, false),
        "greater" => (false, true),
        "greater_equal" => (false, false),
        _ => return Ok(false),
    };
    // chains a <= u <= b
    for w in args.windows(2) {
        let (lo, hi) = if less { (w[0], w[1]) } else { (w[1], w[0]) };
        // lo < hi with one side a number term
        let num = |t: T<'h>| !contains(h, t, x);
        if num(lo) && !num(hi) {
            if !put(cx, x, hi, lo, true, strict, r)? {
                return Ok(false);
            }
        } else if num(hi) && !num(lo) {
            if !put(cx, x, lo, hi, false, strict, r)? {
                return Ok(false);
            }
        } else {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `u > v` (`lower`: v is a lower bound of u) or `u < v` into the region.
fn put<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, u: T<'h>, v: T<'h>, lower: bool, strict: bool, r: &mut Region<'h>) -> Res<bool> {
    let h = cx.heap();
    let closed = !strict;
    if let Some(&[a]) = args_of(h, u, "re") {
        if a == x {
            r.re[usize::from(!lower)] = Some((v, closed));
            return Ok(true);
        }
    }
    if let Some(&[a]) = args_of(h, u, "im_part") {
        if a == x {
            r.im[usize::from(!lower)] = Some((v, closed));
            return Ok(true);
        }
    }
    if let Some(&[a]) = args_of(h, u, "abs") {
        if !lower {
            // |a| < v with a = x - c
            let c = cx.eval(h.call(h.sym("add"), &[x, h.call(h.sym("mul"), &[h.int(-1), a])]))?;
            if !contains(h, c, x) {
                r.disks.push((c, v, closed));
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// The rectangle covering the region, with exact dyadic corners (outward), or `None` when unbounded.
fn cover<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>) -> Res<Option<[Float; 4]>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let mut lo: [Option<f64>; 2] = [None, None];
    let mut hi: [Option<f64>; 2] = [None, None];
    for (k, side) in [&r.re, &r.im].iter().enumerate() {
        for (j, bound) in side.iter().enumerate() {
            let Some((t, _)) = bound else { continue };
            let v = cx.eval(h.call(h.sym("n"), &[*t]))?;
            let Some(v) = ar.num(v).and_then(|n| n.as_real().map(|x| x.to_f64())) else { return Ok(None) };
            if j == 0 {
                lo[k] = Some(v);
            } else {
                hi[k] = Some(v);
            }
        }
    }
    for &(c, rad, _) in &r.disks {
        let cv = cx.eval(h.call(h.sym("n"), &[c]))?;
        let rv = cx.eval(h.call(h.sym("n"), &[rad]))?;
        let (Some(cn), Some(rn)) = (ar.num(cv), ar.num(rv).and_then(|n| n.as_real().map(|x| x.to_f64()))) else { return Ok(None) };
        let (cr, ci) = match cn {
            Num::Real(a) => (a.to_f64(), 0.0),
            Num::Complex(a, b) => (a.to_f64(), b.to_f64()),
        };
        lo[0] = Some(lo[0].map_or(cr - rn, |v: f64| v.max(cr - rn)));
        hi[0] = Some(hi[0].map_or(cr + rn, |v: f64| v.min(cr + rn)));
        lo[1] = Some(lo[1].map_or(ci - rn, |v: f64| v.max(ci - rn)));
        hi[1] = Some(hi[1].map_or(ci + rn, |v: f64| v.min(ci + rn)));
    }
    let (Some(a), Some(b), Some(c), Some(d)) = (lo[0], hi[0], lo[1], hi[1]) else { return Ok(None) };
    if !(a < b && c < d) || [a, b, c, d].iter().any(|v| !v.is_finite() || v.abs() > 1e6) {
        return Ok(None);
    }
    // outward to multiples of 2^-20 (a little more, so a bound that is a root's coordinate is not a corner)
    let q = |v: f64, up: bool| -> Option<Float> {
        let s = v * 1048576.0;
        let s = if up { s.ceil() + 1.0 } else { s.floor() - 1.0 };
        f64_float(s / 1048576.0)
    };
    let (Some(a), Some(b), Some(c), Some(d)) = (q(a, false), q(b, true), q(c, false), q(d, true)) else { return Ok(None) };
    Ok(Some([a, b, c, d]))
}

/// Whether the root box `z` lies in the region: `Some(true)` inside, `Some(false)` outside, `None` undecided.
fn in_region<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Region<'h>, z: &CBall) -> Res<Option<bool>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let p = 256;
    let ball = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<CBall>> {
        let v = cx.eval(h.call(h.sym("n"), &[t, h.int(70)]))?;
        Ok(ar.num(v).and_then(|n| match n {
            Num::Real(Real::Ball(b, _)) => Some(CBall::real(b)),
            Num::Real(Real::Exact(q)) => match Real::Exact(q).to_kind(Kind::Ball, p) {
                Real::Ball(b, _) => Some(CBall::real(b)),
                _ => None,
            },
            Num::Complex(Real::Ball(a, _), Real::Ball(b, _)) => Some(CBall::new(a, b)),
            _ => None,
        }))
    };
    let mut all = true;
    for (k, side) in [&r.re, &r.im].iter().enumerate() {
        let part = if k == 0 { &z.re } else { &z.im };
        for (j, bound) in side.iter().enumerate() {
            let Some((t, closed)) = bound else { continue };
            let Some(b) = ball(cx, *t)? else { return Ok(None) };
            if !b.is_real() {
                return Ok(None);
            }
            // lower bound (j == 0): part >= b; upper: part <= b
            let ord = part.cmp3(&b.re);
            let (inside, outside) = match (j, ord) {
                (0, Some(Ordering::Greater)) => (true, false),
                (0, Some(Ordering::Less)) => (false, true),
                (1, Some(Ordering::Less)) => (true, false),
                (1, Some(Ordering::Greater)) => (false, true),
                _ => (false, false),
            };
            let _ = closed;
            if outside {
                return Ok(Some(false));
            }
            if !inside {
                all = false;
            }
        }
    }
    for &(c, rad, _) in &r.disks {
        let (Some(cb), Some(rb)) = (ball(cx, c)?, ball(cx, rad)?) else { return Ok(None) };
        let d = z.sub(&cb, p);
        let Some(m) = mc_numeric::cball::abs(&d, p) else { return Ok(None) };
        match m.cmp3(&rb.re) {
            Some(Ordering::Less) => {}
            Some(Ordering::Greater) => return Ok(Some(false)),
            _ => all = false,
        }
    }
    Ok(all.then_some(true))
}

/// What a region solve gives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Form {
    /// `solve`: `[[x -> r1], ...]`
    Rules,
    /// `reduce`: `x == r1 or ...`
    Condition,
    /// `n_solve`: rules with machine numbers
    Numbers,
}

/// `solve`, `reduce` and `n_solve` of one analytic equation in one unknown with conditions on `re(x)`, `im_part(x)`
/// or `abs(x - c)` that bound a region of the complex plane: every root in the region, isolated and proved.
/// `None` when the call is not of this kind or a root is not proved.
pub fn solve_region<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, args: &[T<'h>], form: Form) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let (cond, xs) = match args {
        [c, x] => (*c, *x),
        [c, x, d] if *d == sym(h, "complexes") => (*c, *x),
        _ => return Ok(None),
    };
    let x = match cx.as_list(xs) {
        Some(&[x]) => x,
        Some(_) => return Ok(None),
        None => xs,
    };
    if !matches!(h.view(x), View::Sym(_)) {
        return Ok(None);
    }
    let Some(parts) = args_of(h, cond, "and") else { return Ok(None) };
    let mut eq = None;
    let mut region = Region::default();
    for &c in parts {
        if let Some(&[l, r]) = args_of(h, c, "equal") {
            if eq.is_some() {
                return Ok(None);
            }
            eq = Some((l, r));
            continue;
        }
        if !read_bound(cx, x, c, &mut region)? {
            return Ok(None);
        }
    }
    let Some((l, r)) = eq else { return Ok(None) };
    let ar = Arith::new(h);
    let f = cx.eval(h.call(h.sym("add"), &[l, h.call(h.sym("mul"), &[h.int(-1), r])]))?;
    if !transcendental(h, &ar, f, x) || has_other_symbols(h, f, x) {
        return Ok(None);
    }
    let Some(corners) = cover(cx, &region)? else { return Ok(None) };
    let Some(found) = roots_in_rectangle(cx, stop, f, x, corners)? else { return Ok(None) };
    let df = derivative(cx, f, x)?;
    let Some(fns) = graphs(h, &ar, &[f, df], x, None) else { return Ok(None) };
    let mut kept: Vec<(CBall, bool)> = vec![];
    for (z, real) in found {
        let mut z = z;
        let mut decided = None;
        for bits in [64, 160, 320] {
            let zz = z.clone();
            if let Some(k) = metered(cx, stop, |m| refine(&fns, &zz, real, bits, m))? {
                z = k;
            }
            decided = in_region(cx, &region, &z)?;
            if decided.is_some() {
                break;
            }
        }
        match decided {
            Some(true) => kept.push((z, real)),
            Some(false) => {}
            None => return Ok(None),
        }
    }
    kept.sort_by(|a, b| {
        let key = |z: &CBall| (float_f64(&z.re.mid), float_f64(&z.im.mid));
        let (ka, kb) = (key(&a.0), key(&b.0));
        // exact objects: by real then imaginary part (the order of root objects); numbers: by real part, then the
        // upper half-plane first
        let im = if form == Form::Numbers { kb.1.total_cmp(&ka.1) } else { ka.1.total_cmp(&kb.1) };
        // real parts compared to 9 decimals (conjugate roots are isolated separately, their midpoints differ in
        // the last bits)
        let q = |v: f64| (v * 1e9).round();
        q(ka.0).total_cmp(&q(kb.0)).then(im)
    });
    let mut items = vec![];
    for (z, real) in &kept {
        let v = match form {
            Form::Numbers => machine_point(&ar, z, *real),
            _ => match make_object(cx, stop, f, x, z, *real)? {
                Some(o) => o,
                None => return Ok(None),
            },
        };
        items.push(v);
    }
    let rule = |v: T<'h>| h.call(h.sym(names::LIST), &[h.call(h.sym(names::RULE), &[x, v])]);
    Ok(Some(match form {
        Form::Rules | Form::Numbers => h.call(h.sym(names::LIST), &items.iter().map(|&v| rule(v)).collect::<Vec<_>>()),
        Form::Condition => {
            let eqs: Vec<T<'h>> = items.iter().map(|&v| h.call(h.sym("equal"), &[x, v])).collect();
            match eqs.len() {
                0 => sym(h, "false"),
                1 => eqs[0],
                _ => h.call(h.sym("or"), &eqs),
            }
        }
    }))
}

/// Does `t` hold a symbol other than `x` and the numeric constants?
fn has_other_symbols<'h>(h: &'h Heap, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) if u != x && !matches!(h.sym_name(s).as_str(), "pi" | "e" | "euler_gamma" | "catalan" | "im" | "golden_ratio") => return true,
            View::App { args, .. } => stack.extend_from_slice(args),
            _ => {}
        }
    }
    false
}

/// Which extremum.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Goal {
    /// `[value, [x -> point]]`
    Both,
    /// the value
    Value,
    /// the point
    Point,
}

/// The ends of a closed bounded interval condition on `x` (`a <= x <= b`, `a <= x and x <= b`): exact dyadic
/// numbers, with the terms.
fn interval<'h>(h: &'h Heap, cond: T<'h>, x: T<'h>) -> Option<((T<'h>, Float), (T<'h>, Float))> {
    let mut lo = None;
    let mut hi = None;
    let parts: Vec<T<'h>> = match args_of(h, cond, "and") {
        Some(a) => a.to_vec(),
        None => vec![cond],
    };
    for c in parts {
        let View::App { head, args } = h.view(c) else { return None };
        let View::Sym(s) = h.view(head) else { return None };
        let less = match h.sym_name(s).as_str() {
            "less_equal" => true,
            "greater_equal" => false,
            _ => return None,
        };
        for w in args.windows(2) {
            let (a, b) = if less { (w[0], w[1]) } else { (w[1], w[0]) };
            if b == x && !contains(h, a, x) {
                lo = Some((a, exact_float(h, a)?));
            } else if a == x && !contains(h, b, x) {
                hi = Some((b, exact_float(h, b)?));
            } else {
                return None;
            }
        }
    }
    let (lo, hi) = (lo?, hi?);
    (lo.1.cmp_exact(&hi.1) == Ordering::Less).then_some((lo, hi))
}

/// The global minimum (maximum when `max`) of `f` (a function of `x` alone) under `cond`, a closed bounded
/// interval: the candidates are the ends and the roots of f' between them (root objects), compared by enclosures.
/// `None` when not proved.
pub fn minimize_interval<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, f: T<'h>, cond: T<'h>, x: T<'h>, max: bool, goal: Goal) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    if !matches!(h.view(x), View::Sym(_)) || !transcendental(h, &ar, f, x) || has_other_symbols(h, f, x) {
        return Ok(None);
    }
    let g = if max { cx.eval(h.call(h.sym("mul"), &[h.int(-1), f]))? } else { f };
    let Some(((ta, a), (tb, b))) = interval(h, cond, x) else { return Ok(None) };
    let dg = derivative(cx, g, x)?;
    let ddg = derivative(cx, dg, x)?;
    let Some(fd) = graphs(h, &ar, &[dg, ddg], x, None) else { return Ok(None) };
    let Some(fg) = graphs(h, &ar, &[g], x, None) else { return Ok(None) };
    // candidates: (box, end term, critical point, or a small interval at an end where f' vanishes)
    #[derive(Clone)]
    enum Cand<'h> {
        End(T<'h>),
        Crit,
        Unsure,
    }
    let mut cands: Vec<(Ball, Cand<'h>)> = vec![(Ball::exact(a.clone()), Cand::End(ta)), (Ball::exact(b.clone()), Cand::End(tb))];
    // an end where the sign of f' is not proved (f'(a) = 0): the search starts at a point a little inside, where it
    // is, with f'' of one sign between (so f' has at most one root there); that small interval is a candidate
    // whose values are enclosed as a whole, and it may not win
    let mut ends = [a.clone(), b.clone()];
    for (k, toward) in [(0usize, 1i64), (1, -1)] {
        if fd.sign(&ends[k], 64).is_some() {
            continue;
        }
        let w = b.sub_exact(&a);
        let mut moved = None;
        for s in 6..60 {
            let step = Float::new(w.neg, w.man.clone(), w.exp - s);
            let inner = if toward > 0 { ends[k].add_exact(&step) } else { ends[k].sub_exact(&step) };
            let (l, r) = if toward > 0 { (ends[k].clone(), inner.clone()) } else { (inner.clone(), ends[k].clone()) };
            let piece = span(&l, &r);
            if fd.sign(&inner, 64).is_some() && fd.real(1, &piece, 64).is_some_and(|v| !v.contains_zero()) {
                moved = Some((inner, piece));
                break;
            }
        }
        let Some((inner, piece)) = moved else { return Ok(None) };
        ends[k] = inner;
        cands.push((piece, Cand::Unsure));
    }
    let (aa, bb) = (ends[0].clone(), ends[1].clone());
    let Some(crit) = metered(cx, stop, |m| real_roots(&fd, &aa, &bb, m))? else { return Ok(None) };
    cands.extend(crit.into_iter().map(|z| (z, Cand::Crit)));
    let mut winner = None;
    for bits in [64u64, 128, 256, 512] {
        let boxes: Vec<(Ball, bool)> = cands.iter().map(|c| (c.0.clone(), matches!(c.1, Cand::Unsure))).collect();
        let fdr = &fd;
        let refined = metered(cx, stop, |m| {
            let mut out = vec![];
            for (z, unsure) in &boxes {
                if z.rad.is_zero() || *unsure {
                    out.push(z.clone());
                    continue;
                }
                match refine(fdr, &CBall::real(z.clone()), true, bits, m)? {
                    Some(k) => out.push(k.re),
                    None => return Ok(None),
                }
            }
            Ok(Some(out))
        })?;
        let Some(refined) = refined else { return Ok(None) };
        for (c, z) in cands.iter_mut().zip(refined) {
            c.0 = z;
        }
        let vals: Option<Vec<Ball>> = cands.iter().map(|c| fg.real(0, &c.0, bits + 32)).collect();
        let Some(vals) = vals else { return Ok(None) };
        for (i, v) in vals.iter().enumerate() {
            let (_, vh) = v.ends();
            if vals.iter().enumerate().all(|(j, w)| j == i || vh.cmp_exact(&w.ends().0) == Ordering::Less) {
                winner = Some(i);
            }
        }
        if winner.is_some() {
            break;
        }
    }
    let Some(i) = winner else { return Ok(None) };
    let point = match &cands[i].1 {
        Cand::End(t) => *t,
        Cand::Unsure => return Ok(None),
        Cand::Crit => {
            let z = CBall::real(cands[i].0.clone());
            match make_object(cx, stop, dg, x, &z, true)? {
                Some(o) => o,
                None => return Ok(None),
            }
        }
    };
    // the value: f over one denominator, at the point
    let tf = cx.eval(h.call(h.sym("together"), &[f]))?;
    let value = cx.eval(h.call(h.sym("rewrite"), &[tf, h.call(h.sym(names::RULE), &[x, point])]))?;
    Ok(Some(match goal {
        Goal::Value => value,
        Goal::Point => point,
        Goal::Both => h.call(h.sym(names::LIST), &[value, h.call(h.sym(names::LIST), &[h.call(h.sym(names::RULE), &[x, point])])]),
    }))
}
