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
//! - **Search**: breadth first to [`DEPTH`] moves, keeping the [`BEAM`] cheapest new forms per level; forms are
//!   hash-consed, so duplicates are found by handle.
//! - **Inside out**: the arguments of other functions are simplified first (`sin((x^2 - 1)/(x - 1))` becomes
//!   `sin(1 + x)`), and lists element by element.
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

fn step<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, m: Move, t: T<'h>) -> Res<T<'h>> {
    match m {
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
fn search<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
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
            for m in MOVES {
                let r = step(cx, b, m, u)?;
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

/// `simplify(e)`.
pub fn simplify<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
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
            let r = if s == b.hd.list || !searchable(b, a) { a } else { search(cx, b, a)? };
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
                out.push(search(cx, b, a)?);
            }
            return Ok(b.call(s, &out));
        }
    }
    search(cx, b, inner)
}
