//! Every real root of one analytic equation in one real unknown on the real line, a half-line or an open interval
//! whose ends are singular points of the equation: `reduce(exp(x) - 5 x + log(2 x) + 1 == 0, x, reals)`,
//! `solve(bessel_i(0, x) == 2 and x > 0, x)`, `n_solve(exp(2 exp(x)) - log(x^2 + 1) - 20 x == 11, x, reals)`.
//! What `solve`, `reduce` and `n_solve` try when their other methods leave such an equation unsolved; the answer is
//! given only when the list of roots is proved complete.
//!
//! - **The domain**: the real domain of f = lhs - rhs (`function_domain`) and the bounds on x in the conditions, an
//!   interval (lo, hi) with ends that are exact dyadic numbers or infinite. An end where f has a nonzero value is a
//!   regular end; an end where f has no value is singular.
//! - **The unbounded parts are proved free of roots** (levels with growing bounded parts): with g the function on
//!   [t0, inf) (g(t) = f(t) right of the bounded part [a, b], f(-t) left of it, f(c + 1/t) or f(c - 1/t) at a
//!   singular end c), g has no root there when an enclosure of g over the whole half-line excludes 0 (the extended
//!   interval enclosures of `tails`: monotone functions with their limits, rational functions beyond the Cauchy
//!   bounds of their critical points and poles, bounded oscillating parts), or by Taylor's theorem when for a sign s
//!   and some k <= 3, s g^(j)(t0) > 0 for j < k and s g^(k) >= 0 on the whole half-line (then s g^(k-1), ..., s g
//!   increase from positive values).
//! - **The bounded part** [a, b] (f proved nonzero at both ends) is bisected: a piece where an enclosure of f
//!   excludes 0 holds no root; a piece where an enclosure of f' excludes 0 holds one root when f changes sign
//!   between its ends and none otherwise (Moore, Kearfott and Cloud, *Introduction to Interval Analysis*, SIAM 2009,
//!   chapter 8; the exclusion test and the monotonicity test). Any other piece is cut in two; a piece that shrinks
//!   to a few bits (a double root, a cluster, a point without a real value) leaves the equation unsolved.
//! - **The roots** are the root objects of `mc_algebra::transroot` (Krawczyk's test proves each one again in its
//!   piece), or their machine values for `n_solve`.
use crate::tails::{args_of, contains, dyadic, has_other_symbols, head_name, Encl, Tail};
use mc_algebra::transroot::{self, Form};
use mc_api::{names, Store, View, T};
use mc_arith::num::{Ball, Float, Mag, Nat, StopScope};
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use mc_numeric::cball::CBall;
use mc_numeric::numeval::{Graph, Island, Names};
use std::cell::Cell;
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Working precision of the bisection (bits).
const BITS: u64 = 128;
/// The levels tried: half-width 2^(L+1) of the bounded part, distance 4^-(L+1) from a singular end.
const LEVELS: u32 = 7;
/// The most pieces one bisection may examine.
const MAX_PIECES: u64 = 4_000;
/// The highest derivative of the Taylor test on a half-line.
const MAX_ORDER: usize = 3;
/// The largest condition (nodes of its tree) taken.
const MAX_NODES: usize = 2_000;

thread_local! {
    /// Set while a real-line solve runs: the root objects it makes call `solve` again, which must not come back here.
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
}

/// An end of the domain: its value, its term, closed or open.
#[derive(Clone)]
struct End<'h> {
    v: Float,
    t: T<'h>,
    closed: bool,
}

/// The domain (lo, hi) of x, each end finite or not.
#[derive(Clone, Default)]
struct Dom<'h> {
    lo: Option<End<'h>>,
    hi: Option<End<'h>>,
}

impl<'h> Dom<'h> {
    /// Narrow by `x > v` (`strict`) or `x >= v`.
    fn lower(&mut self, e: End<'h>) {
        let keep = match &self.lo {
            None => false,
            Some(o) => match o.v.cmp_exact(&e.v) {
                Ordering::Greater => true,
                Ordering::Less => false,
                Ordering::Equal => !o.closed,
            },
        };
        if !keep {
            self.lo = Some(e);
        }
    }
    /// Narrow by `x < v` or `x <= v`.
    fn upper(&mut self, e: End<'h>) {
        let keep = match &self.hi {
            None => false,
            Some(o) => match o.v.cmp_exact(&e.v) {
                Ordering::Less => true,
                Ordering::Greater => false,
                Ordering::Equal => !o.closed,
            },
        };
        if !keep {
            self.hi = Some(e);
        }
    }
}

/// Reads the bounds on x in `c` (a relation or a chain) into `dom`; `false` when `c` is something else.
fn read_bounds<'h>(en: &Encl<'_, 'h>, c: T<'h>, x: T<'h>, dom: &mut Dom<'h>) -> bool {
    let h = en.h;
    let Some(name) = head_name(h, c) else { return false };
    let (less, strict) = match name.as_str() {
        "less" => (true, true),
        "less_equal" => (true, false),
        "greater" => (false, true),
        "greater_equal" => (false, false),
        _ => return false,
    };
    let a = args_of(h, c);
    if a.len() < 2 {
        return false;
    }
    for w in a.windows(2) {
        let (l, r) = if less { (w[0], w[1]) } else { (w[1], w[0]) };
        let (bound, is_lower) = if r == x && !contains(h, l, x) {
            (l, true)
        } else if l == x && !contains(h, r, x) {
            (r, false)
        } else {
            return false;
        };
        let Some(v) = dyadic(en, bound) else { return false };
        let e = End { v, t: bound, closed: !strict };
        if is_lower {
            dom.lower(e);
        } else {
            dom.upper(e);
        }
    }
    true
}

/// f and f' as numeric graphs in x.
struct Fx<'h> {
    g: Graph<'h>,
    is: [Island; 2],
}

impl Fx<'_> {
    /// An enclosure of f^(j) over the real ball `b` (`None`: no real enclosure).
    fn real(&self, j: usize, b: &Ball) -> Option<Ball> {
        let z = self.g.eval_complex_at(&self.is[j], BITS, &[CBall::real(b.clone())])?;
        z.is_real().then_some(z.re)
    }
    /// The sign of f at the exact point `v` when proved (`true`: positive).
    fn sign(&self, v: &Float) -> Option<bool> {
        let b = self.real(0, &Ball::exact(v.clone()))?;
        (!b.contains_zero()).then_some(!b.mid.neg)
    }
}

/// The ball [l, r].
fn span(l: &Float, r: &Float) -> Ball {
    let s = l.add_exact(r);
    let w = r.sub_exact(l);
    Ball { mid: Float::new(s.neg, s.man, s.exp - 1), rad: Mag::from_float_up(&Float::new(false, w.man, w.exp - 1)) }
}

/// l + (r - l) k/16, exactly.
fn split_point(l: &Float, r: &Float, k: i64) -> Float {
    let w = r.sub_exact(l).mul_exact(&Float::from_i64(k));
    l.add_exact(&Float::new(w.neg, w.man, w.exp - 4))
}

/// 2^e.
fn pow2(e: i64) -> Float {
    Float::new(false, Nat::from_u64(1), e)
}

/// The pieces of [a, b] (f nonzero at both ends) that hold exactly one root each, in increasing order; `None` when
/// a piece is not decided.
fn isolate(fx: &Fx, a: &Float, b: &Float, budget: &mut u64) -> Option<Vec<(Float, Float)>> {
    let (sa, sb) = (fx.sign(a)?, fx.sign(b)?);
    let size = b.sub_exact(a).top();
    let mut stack = vec![(a.clone(), b.clone(), sa, sb)];
    let mut out = vec![];
    while let Some((l, r, sl, sr)) = stack.pop() {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let piece = span(&l, &r);
        let w = r.sub_exact(&l);
        let scale = l.top().max(r.top()).max(size);
        let tiny = w.top() < scale - 90;
        match fx.real(0, &piece) {
            Some(v) if !v.contains_zero() => continue,
            None if tiny => return None,
            _ => {}
        }
        if fx.real(1, &piece).is_some_and(|d| !d.contains_zero()) {
            if sl != sr {
                out.push((l, r, sl));
            }
            continue;
        }
        if tiny {
            return None;
        }
        let mut cut = None;
        for k in [8, 7, 9, 6, 10, 5, 11] {
            let s = split_point(&l, &r, k);
            if let Some(ss) = fx.sign(&s) {
                cut = Some((s, ss));
                break;
            }
        }
        let (s, ss) = cut?;
        stack.push((s.clone(), r, ss, sr));
        stack.push((l, s, sl, ss));
    }
    out.sort_by(|u, v| u.0.cmp_exact(&v.0));
    // each piece narrowed by the sign at midpoints (to about 60 bits), so that Newton's iteration from its middle
    // stays inside
    Some(
        out.into_iter()
            .map(|(mut l, mut r, sl)| {
                for _ in 0..200 {
                    let w = r.sub_exact(&l);
                    if w.top() < l.top().max(r.top()).max(0) - 60 {
                        break;
                    }
                    let m = split_point(&l, &r, 8);
                    match fx.sign(&m) {
                        Some(s) if s == sl => l = m,
                        Some(_) => r = m,
                        None => break,
                    }
                }
                (l, r)
            })
            .collect(),
    )
}

/// Is g (a function of x) free of roots on [t0, inf)?
fn tail_free<'h>(cx: &mut NativeCx<'_, '_, 'h>, en: &Encl<'_, 'h>, g: T<'h>, t0: &Float) -> Res<bool> {
    let h = en.h;
    let x = en.x;
    let bound = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<(i32, i32)>> {
        let mut tail = Tail { cx, en: Encl { h: en.h, ar: en.ar, nm: en.nm, x }, t0: t0.clone() };
        Ok(tail.bound(t)?.map(|iv| (iv.lo.sign(), iv.hi.sign())))
    };
    let excludes = |b: Option<(i32, i32)>, s: i32| b.is_some_and(|(lo, hi)| if s > 0 { lo >= 0 } else { hi <= 0 });
    if let Some((lo, hi)) = bound(cx, g)? {
        if lo > 0 || hi < 0 {
            return Ok(true);
        }
    }
    let at = |t: T<'h>| en.at(t, &Ball::exact(t0.clone())).filter(|v| !v.contains_zero()).map(|v| if v.mid.neg { -1 } else { 1 });
    let Some(s) = at(g) else { return Ok(false) };
    let mut d = g;
    for k in 1..=MAX_ORDER {
        cx.tick(1)?;
        d = cx.eval(h.call(h.sym("d"), &[d, x]))?;
        if excludes(bound(cx, d)?, s) {
            return Ok(true);
        }
        if k == MAX_ORDER || at(d) != Some(s) {
            return Ok(false);
        }
    }
    Ok(false)
}

/// `f` with x replaced by `by`, evaluated.
fn subst<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, by: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, by])]))
}

/// The real domain of f from `function_domain` into `dom`; `false` when it is not one interval.
fn natural_domain<'h>(cx: &mut NativeCx<'_, '_, 'h>, en: &Encl<'_, 'h>, f: T<'h>, x: T<'h>, dom: &mut Dom<'h>) -> Res<bool> {
    let h = cx.heap();
    let d = cx.eval(h.call(h.sym("function_domain"), &[f, x]))?;
    if matches!(h.view(d), View::Sym(s) if h.sym_name(s) == "true") {
        return Ok(true);
    }
    let parts: Vec<T<'h>> = match head_name(h, d).as_deref() {
        Some("and") => args_of(h, d).to_vec(),
        _ => vec![d],
    };
    Ok(parts.into_iter().all(|c| read_bounds(en, c, x, dom)))
}

/// `solve`, `reduce` or `n_solve` (by `form`) of one analytic equation in one real unknown over its real domain
/// (`reals` or bounds on x in the conditions): every root, proved; `None` when the call is not of this kind or the
/// list is not proved complete.
pub(crate) fn solve_line<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, args: &[T<'h>], form: Form) -> Res<Option<T<'h>>> {
    if ACTIVE.with(|a| a.get()) {
        return Ok(None);
    }
    ACTIVE.with(|a| a.set(true));
    let r = solve_line_at(cx, stop, args, form);
    ACTIVE.with(|a| a.set(false));
    r
}

/// The function, the unknown and the pieces with one root each, in increasing order.
type Pieces<'h> = (T<'h>, T<'h>, Vec<(Float, Float)>);

/// The roots of the equation of a `solve` call (`args`) over the real domain of the unknown, isolated; `None` when
/// the call is not of this kind or the list is not proved complete.
fn pieces<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, args: &[T<'h>]) -> Res<Option<Pieces<'h>>> {
    let h: &'h Heap = cx.heap();
    let is_sym = |t: T<'h>, n: &str| matches!(h.view(t), View::Sym(s) if h.sym_name(s) == n);
    let (cond, xs, reals) = match args {
        [c, x] => (*c, *x, false),
        [c, x, d] if is_sym(*d, "reals") => (*c, *x, true),
        _ => return Ok(None),
    };
    if !small(h, cond, MAX_NODES) {
        return Ok(None);
    }
    let x = match cx.as_list(xs) {
        Some(&[x]) => x,
        Some(_) => return Ok(None),
        None => xs,
    };
    if !matches!(h.view(x), View::Sym(_)) {
        return Ok(None);
    }
    let parts: Vec<T<'h>> = match head_name(h, cond).as_deref() {
        Some("and") => args_of(h, cond).to_vec(),
        _ => vec![cond],
    };
    let ar = Arith::new(h);
    let nm = Names::new(h);
    let en = Encl { h, ar: &ar, nm: &nm, x };
    let mut eq = None;
    let mut dom = Dom::default();
    let mut bounded = false;
    for c in parts {
        if head_name(h, c).as_deref() == Some("equal") {
            let &[l, r] = args_of(h, c) else { return Ok(None) };
            if eq.is_some() {
                return Ok(None);
            }
            eq = Some((l, r));
            continue;
        }
        if !read_bounds(&en, c, x, &mut dom) {
            return Ok(None);
        }
        bounded = true;
    }
    let Some((l, r)) = eq else { return Ok(None) };
    if !reals && !bounded {
        return Ok(None);
    }
    let f = cx.eval(h.call(h.sym("add"), &[l, h.call(h.sym("mul"), &[h.int(-1), r])]))?;
    if !contains(h, f, x) || has_other_symbols(h, f, x) {
        return Ok(None);
    }
    if !natural_domain(cx, &en, f, x, &mut dom)? {
        return Ok(None);
    }
    if let (Some(a), Some(b)) = (&dom.lo, &dom.hi) {
        if a.v.cmp_exact(&b.v) != Ordering::Less {
            return Ok(None);
        }
    }
    let _scope = StopScope::enter(Some(stop));
    let df = cx.eval(h.call(h.sym("d"), &[f, x]))?;
    let View::Sym(xsym) = h.view(x) else { return Ok(None) };
    let both = h.call(h.sym(names::LIST), &[f, df]);
    let g = Graph::build_with_vars(h, &ar, &nm, both, &[xsym]);
    let (Some(nf), Some(nd)) = (g.node(f), g.node(df)) else { return Ok(None) };
    let is = [g.island(nf), g.island(nd)];
    let fx = Fx { g, is };
    // the ends: regular (f nonzero there) or singular (no value); a value that may be 0 leaves the call
    let regular = |e: &Option<End<'h>>| -> Option<bool> {
        match e {
            None => Some(false),
            Some(e) => match fx.real(0, &Ball::exact(e.v.clone())) {
                Some(v) if v.contains_zero() => None,
                Some(_) => Some(true),
                None => Some(false),
            },
        }
    };
    let (Some(reg_lo), Some(reg_hi)) = (regular(&dom.lo), regular(&dom.hi)) else { return Ok(None) };
    let neg_x = h.call(h.sym("mul"), &[h.int(-1), x]);
    let inv_x = h.call(h.sym("pow"), &[x, h.int(-1)]);
    // a regular end where the enclosures of f or f' fail on every piece next to it (sqrt(x) at 0) is taken as
    // singular next (f nonzero at the end: the same roots)
    let mut variants = vec![(reg_lo, reg_hi)];
    if reg_lo || reg_hi {
        variants.push((false, false));
    }
    let mut found = None;
    for (reg_lo, reg_hi) in variants {
        found = None;
        for level in 0..LEVELS {
            cx.tick(16)?;
            if cx.stopped() {
                return Ok(None);
            }
            let w = pow2(level as i64 + 1);
            let near = 2 * (level as i64 + 1);
            // the bounded part [a, b] and the half-lines to prove free of roots: (g, t0)
            let mut tails: Vec<(T<'h>, Float)> = vec![];
            let a = match (&dom.lo, reg_lo) {
                (Some(e), true) => e.v.clone(),
                (Some(e), false) => {
                    tails.push((subst(cx, f, x, h.call(h.sym("add"), &[e.t, inv_x]))?, pow2(near)));
                    e.v.add_exact(&pow2(-near))
                }
                (None, _) => {
                    let m = dom.hi.as_ref().map_or(w.neg(), |e| e.v.sub_exact(&w));
                    let a = if m.cmp_exact(&w.neg()) == Ordering::Less { m } else { w.neg() };
                    tails.push((subst(cx, f, x, neg_x)?, a.neg()));
                    a
                }
            };
            let b = match (&dom.hi, reg_hi) {
                (Some(e), true) => e.v.clone(),
                (Some(e), false) => {
                    let by = h.call(h.sym("add"), &[e.t, h.call(h.sym("mul"), &[h.int(-1), inv_x])]);
                    tails.push((subst(cx, f, x, by)?, pow2(near)));
                    e.v.sub_exact(&pow2(-near))
                }
                (None, _) => {
                    let m = dom.lo.as_ref().map_or(w.clone(), |e| e.v.add_exact(&w));
                    let b = if m.cmp_exact(&w) == Ordering::Greater { m } else { w.clone() };
                    tails.push((f, b.clone()));
                    b
                }
            };
            if a.cmp_exact(&b) != Ordering::Less || fx.sign(&a).is_none() || fx.sign(&b).is_none() {
                continue;
            }
            let mut free = true;
            for (g, t0) in &tails {
                if !tail_free(cx, &en, *g, t0)? {
                    free = false;
                    break;
                }
            }
            if !free {
                continue;
            }
            let mut budget = MAX_PIECES;
            found = Some(isolate(&fx, &a, &b, &mut budget));
            break;
        }
        if matches!(found, Some(Some(_))) {
            break;
        }
    }
    Ok(match found {
        Some(Some(pieces)) => Some((f, x, pieces)),
        _ => None,
    })
}

/// The number of real roots of `f` (a function of x alone) on its real domain when every one is proved simple.
pub(crate) fn count_line<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, f: T<'h>, x: T<'h>) -> Res<Option<usize>> {
    if ACTIVE.with(|a| a.get()) {
        return Ok(None);
    }
    let h = cx.heap();
    if cx.as_list(f).is_some() || !small(h, f, MAX_NODES) {
        return Ok(None);
    }
    let args = [h.call(h.sym("equal"), &[f, h.int(0)]), x, h.sym_term(h.sym("reals"))];
    ACTIVE.with(|a| a.set(true));
    let r = pieces(cx, stop, &args);
    ACTIVE.with(|a| a.set(false));
    Ok(r?.map(|p| p.2.len()))
}

fn solve_line_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, args: &[T<'h>], form: Form) -> Res<Option<T<'h>>> {
    let h: &'h Heap = cx.heap();
    let Some((f, x, pieces)) = pieces(cx, stop, args)? else { return Ok(None) };
    let mut items = vec![];
    for (l, r) in pieces {
        let z = CBall::real(span(&l, &r));
        let Some(o) = transroot::make_object(cx, stop, f, x, &z, true)? else { return Ok(None) };
        let v = if form == Form::Numbers { cx.eval(h.call(h.sym("n"), &[o]))? } else { o };
        if form == Form::Numbers && !matches!(h.view(v), View::Real(_)) {
            return Ok(None);
        }
        items.push(v);
    }
    let rule = |v: T<'h>| h.call(h.sym(names::LIST), &[h.call(h.sym(names::RULE), &[x, v])]);
    Ok(Some(match form {
        Form::Rules | Form::Numbers => h.call(h.sym(names::LIST), &items.iter().map(|&v| rule(v)).collect::<Vec<_>>()),
        Form::Condition => {
            let eqs: Vec<T<'h>> = items.iter().map(|&v| h.call(h.sym("equal"), &[x, v])).collect();
            match eqs.len() {
                0 => h.sym_term(h.sym("false")),
                1 => eqs[0],
                _ => h.call(h.sym("or"), &eqs),
            }
        }
    }))
}

/// Has the tree of `t` (shared subterms counted at each use) at most `limit` nodes? Derivatives and enclosures of a
/// larger term (a deeply shared one) are not tried.
pub(crate) fn small<'h>(h: &'h Heap, t: T<'h>, limit: usize) -> bool {
    let mut n = 0;
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        n += 1;
        if n > limit {
            return false;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    true
}

/// Is `t` the failure of an equation no method solved?
pub(crate) fn unsolved<'h>(h: &'h Heap, t: T<'h>) -> bool {
    head_name(h, t).as_deref() == Some(names::FAILURE) && args_of(h, t).first().is_some_and(|&a| matches!(h.view(a), View::Sym(s) if h.sym_name(s) == "unsolved"))
}
