//! `simplify(e)`: the cheapest equivalent form found by a bounded search.
//!
//! The plan (`00-plan.md`, layer 2) replaces piles of heuristics by *a rule set plus a cost measure*: explore
//! equivalent forms, keep the cheapest. This is the first, search-based form of that idea (decision 2B.1); an
//! e-graph (equality saturation) can replace the search behind the same function later.
//!
//! - **Cost**: the leaf count ([`leaf_count`]), ties kept in the order found (the input first, so `simplify` never
//!   trades a form for an equally large one).
//! - **Moves**: equivalences valid for all values of the variables (decision 2B.2): `together`, `cancel`, `factor`,
//!   `expand` (package K), `trig_reduce`, the Pythagorean substitutions followed by `expand` or `together`,
//!   `tan`/`sec`/... as sines and cosines followed by `together`, `exp_to_trig`, products of exponentials as one
//!   exponential, and the sign convention for sines and cosines of sums (decision 2B.4).
//!   Inside `assuming` (decision A.6), `refine` under the context's facts is one more move, and the input and the
//!   result are refined.
//! - **Search**: breadth first to [`DEPTH`] moves, keeping the [`BEAM`] cheapest new forms per level; forms are
//!   hash-consed, so duplicates are found by handle.
//! - **Inside out**: the arguments of other functions are simplified first (`sin((x^2 - 1)/(x - 1))` becomes
//!   `sin(1 + x)`), and lists element by element.
use crate::facts::Facts;
use crate::refine::refine;
use crate::term::{apply, leaf_count, rewrite, B};
use crate::trig::{exp_combine, exp_to_trig, normal_signs, pythagoras, to_sin_cos, trig_reduce, Pythagoras};
use mc_api::{Store, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashSet;

/// Most moves from the input.
pub const DEPTH: usize = 3;
/// Forms kept per level of the search.
pub const BEAM: usize = 6;

/// A move of the search.
#[derive(Clone, Copy, Debug)]
enum Move {
    Together,
    Cancel,
    Factor,
    Expand,
    TrigReduce,
    SinExpand,
    CosExpand,
    SinTogether,
    CosTogether,
    SinCosTogether,
    ExpToTrig,
    ExpCombine,
    Signs,
    /// `refine` under the facts of the context (only inside `assuming`)
    Refine,
}

const MOVES: [Move; 13] = [
    Move::Together,
    Move::Cancel,
    Move::Factor,
    Move::Expand,
    Move::TrigReduce,
    Move::SinExpand,
    Move::CosExpand,
    Move::SinTogether,
    Move::CosTogether,
    Move::SinCosTogether,
    Move::ExpToTrig,
    Move::ExpCombine,
    Move::Signs,
];

fn step<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, facts: &mut Option<&mut Facts<'_, 'h>>, m: Move, t: T<'h>) -> Res<T<'h>> {
    match m {
        Move::Refine => match facts {
            Some(f) => refine(cx, b, f, t),
            None => Ok(t),
        },
        Move::Together => apply(cx, b, "together", t),
        Move::Cancel => apply(cx, b, "cancel", t),
        Move::Factor => apply(cx, b, "factor", t),
        Move::Expand => apply(cx, b, "expand", t),
        Move::TrigReduce => trig_reduce(cx, b, t),
        Move::SinExpand => {
            let r = pythagoras(cx, b, t, Pythagoras::ToSin)?;
            apply(cx, b, "expand", r)
        }
        Move::CosExpand => {
            let r = pythagoras(cx, b, t, Pythagoras::ToCos)?;
            apply(cx, b, "expand", r)
        }
        Move::SinTogether => {
            let r = pythagoras(cx, b, t, Pythagoras::ToSin)?;
            apply(cx, b, "together", r)
        }
        Move::CosTogether => {
            let r = pythagoras(cx, b, t, Pythagoras::ToCos)?;
            apply(cx, b, "together", r)
        }
        Move::SinCosTogether => {
            let r = to_sin_cos(cx, b, t)?;
            apply(cx, b, "together", r)
        }
        Move::ExpToTrig => exp_to_trig(cx, b, t),
        Move::ExpCombine => exp_combine(cx, b, t),
        Move::Signs => normal_signs(cx, b, t),
    }
}

/// Is `t` worth searching: a compound term that is not a number?
fn searchable<'h>(b: &B<'h>, t: T<'h>) -> bool {
    b.call_of(t).is_some() && !b.is_number(t)
}

/// The cheapest form of `t` the search finds (its arguments already simplified).
fn search<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, facts: &mut Option<&mut Facts<'_, 'h>>, t: T<'h>) -> Res<T<'h>> {
    if !searchable(b, t) {
        return Ok(t);
    }
    let h = b.heap();
    let mut best = (leaf_count(h, t), t);
    let mut seen: FxHashSet<T<'h>> = FxHashSet::default();
    seen.insert(t);
    let mut level = vec![t];
    for _ in 0..DEPTH {
        let mut next: Vec<(u64, T<'h>)> = Vec::new();
        for &u in &level {
            let refine = facts.is_some().then_some(Move::Refine);
            for m in MOVES.into_iter().chain(refine) {
                let r = step(cx, b, facts, m, u)?;
                // a move that fails (a limit inside a kernel, a failure term) is not a form
                if !seen.insert(r) || h_is_failure(b, r) {
                    continue;
                }
                let c = leaf_count(h, r);
                if c < best.0 {
                    best = (c, r);
                }
                next.push((c, r));
            }
        }
        if next.is_empty() {
            break;
        }
        // stable: equal costs keep the order found
        next.sort_by_key(|&(c, _)| c);
        next.truncate(BEAM);
        level = next.into_iter().map(|(_, r)| r).collect();
    }
    Ok(best.1)
}

/// Is `t` a failure term?
fn h_is_failure<'h>(b: &B<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    matches!(b.call_of(t), Some((s, _)) if s == h.sym(mc_api::names::FAILURE))
}

/// `simplify(e)`. With `facts` (inside `assuming`, decision A.6) the search also refines, and the input and the
/// result are refined; without, the moves are only the equivalences valid for every value.
pub fn simplify<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, mut facts: Option<&mut Facts<'_, 'h>>, t: T<'h>) -> Res<T<'h>> {
    let facts = &mut facts;
    let t = prepare(cx, b, t)?;
    let t = match facts {
        Some(f) => refine(cx, b, f, t)?,
        None => t,
    };
    let r = inside_out(cx, b, facts, t)?;
    match facts {
        Some(f) => refine(cx, b, f, r),
        None => Ok(r),
    }
}

/// The search on the arguments of non-arithmetic functions first, then on the whole term.
fn inside_out<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, facts: &mut Option<&mut Facts<'_, 'h>>, t: T<'h>) -> Res<T<'h>> {
    // inside out: every argument of a function other than the arithmetic (and every element of a list) is
    // simplified before the call it is in; the arithmetic is searched as a whole
    let arith = [b.hd.add, b.hd.mul, b.hd.pow, b.hd.complex];
    let inner = rewrite(cx, b, t, &mut |cx, b, node| {
        let Some((s, args)) = b.call_of(node) else { return Ok(None) };
        if arith.contains(&s) || s == b.hd.list {
            return Ok(None);
        }
        let mut changed = false;
        let mut out = Vec::with_capacity(args.len());
        for &a in args {
            let r = if s == b.hd.list || !searchable(b, a) { a } else { search(cx, b, facts, a)? };
            changed |= r != a;
            out.push(r);
        }
        Ok(changed.then(|| b.call(s, &out)))
    })?;
    let inner = cx.eval(inner)?;
    if let Some((s, items)) = b.call_of(inner) {
        if s == b.hd.list {
            let mut out = Vec::with_capacity(items.len());
            for &a in items {
                out.push(search(cx, b, facts, a)?);
            }
            return Ok(b.call(s, &out));
        }
    }
    search(cx, b, facts, inner)
}

/// The functions with mirror symmetry, `f(conjugate(z)) = conjugate(f(z))` (decision 2B.5: the identity is used
/// as the reference uses it, also for the functions with a branch cut on the negative reals).
const MIRROR: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "sinh", "cosh", "tanh", "coth", "sech", "csch", "exp", "gamma",
    "factorial", "erf", "erfc", "erfi", "exp_integral_e", "exp_integral_ei", "binomial", "beta",
];

/// Rewritings applied once before the search (decision 2B.6): `conjugate(f(z))` as `f(conjugate(z))` for the
/// functions of [`MIRROR`]; `degree` as `pi/180`; `and`/`or` with repeated arguments removed and the rest sorted;
/// `floor(n + x)`, `ceiling(n + x)` as `n + floor(x)`, `n + ceiling(x)` for integers n; `sqrt(a + b sqrt(c))`
/// denested (rationals a > 0, b, c when a^2 - b^2 c is the square of a rational).
fn prepare<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let degree = h.sym("degree");
    let mentions_degree = h.mask(t) & 1u64 << (degree.0 % 64) != 0;
    let t = if mentions_degree {
        let to = b.div(h.sym_term(h.sym("pi")), b.int(180));
        let rule = b.call(h.sym(mc_api::names::RULE), &[h.sym_term(degree), to]);
        cx.eval(b.call_named("rewrite", &[t, rule]))?
    } else {
        t
    };
    let r = rewrite(cx, b, t, &mut |cx, b, node| {
        let h = b.heap();
        let Some((s, args)) = b.call_of(node) else { return Ok(None) };
        let name = h.sym_name(s);
        match (name.as_str(), args) {
            ("conjugate", [u]) => {
                if let Some((g, inner)) = b.call_of(*u) {
                    if MIRROR.contains(&h.sym_name(g).as_str()) {
                        let conj: Vec<T<'h>> = inner.iter().map(|&a| b.call(s, &[a])).collect();
                        return Ok(Some(b.call(g, &conj)));
                    }
                }
                Ok(None)
            }
            ("and" | "or", _) if args.len() > 1 => {
                let mut v = args.to_vec();
                cx.sort(&mut v);
                v.dedup();
                Ok((v.as_slice() != args).then(|| b.call(s, &v)))
            }
            ("floor" | "ceiling", [u]) => {
                let (ints, rest): (Vec<T<'h>>, Vec<T<'h>>) =
                    b.terms(*u).into_iter().partition(|&x| matches!(h.view(x), mc_api::View::Int(_) | mc_api::View::BigInt { .. }));
                if ints.is_empty() || rest.is_empty() {
                    return Ok(None);
                }
                Ok(Some(b.add(&[b.add(&ints), b.call(s, &[b.add(&rest)])])))
            }
            _ if s == b.hd.pow => {
                if let Some(r) = denest(cx, b, args)? {
                    return Ok(Some(r));
                }
                // a compound exponent expanded when that is smaller (G51: mp^(-2 (4 - k) + 6 - k) is mp^(-2 + k))
                let [base, ex] = args else { return Ok(None) };
                if b.call_of(*ex).is_some() && !b.is_number(*ex) {
                    let e2 = apply(cx, b, "expand", *ex)?;
                    if leaf_count(h, e2) < leaf_count(h, *ex) {
                        return Ok(Some(b.pow(*base, e2)));
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}

/// `sqrt(a + b sqrt(c))` as `sqrt((a + d)/2) + sign(b) sqrt((a - d)/2)`, d = sqrt(a^2 - b^2 c) rational.
fn denest<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let [base, ex] = args else { return Ok(None) };
    let half = b.rat(1, 2);
    if *ex != half {
        return Ok(None);
    }
    let rational = |u: T<'h>| matches!(h.view(u), mc_api::View::Int(_) | mc_api::View::BigInt { .. } | mc_api::View::Rational { .. });
    let terms = b.terms(*base);
    let [a, rest] = terms.as_slice() else { return Ok(None) };
    if !rational(*a) {
        return Ok(None);
    }
    // rest = coef * c^(1/2)
    let (coef, rad) = match b.call_of(*rest) {
        Some((m, [k, r])) if m == b.hd.mul && rational(*k) => (*k, *r),
        _ => (b.int(1), *rest),
    };
    let c = match b.call_of(rad) {
        Some((p, [c, e])) if p == b.hd.pow && *e == half && rational(*c) => *c,
        _ => return Ok(None),
    };
    let pos = |u: T<'h>| b.ar.compare(u, b.int(0)).is_some_and(|o| o == mc_arith::number::Cmp3::Greater);
    if !pos(*a) {
        return Ok(None);
    }
    let disc = b.sub(b.mul(&[*a, *a]), b.mul(&[coef, coef, c]));
    if !rational(disc) || b.ar.compare(disc, b.int(0)).is_some_and(|o| o == mc_arith::number::Cmp3::Less) {
        return Ok(None);
    }
    let d = cx.eval(b.pow(disc, half))?;
    if !rational(d) {
        return Ok(None);
    }
    let p = b.pow(b.mul(&[half, b.add(&[*a, d])]), half);
    let q = b.pow(b.mul(&[half, b.sub(*a, d)]), half);
    let q = if pos(coef) { q } else { b.neg(q) };
    Ok(Some(b.add(&[p, q])))
}
