//! Sets on lists (or on any compound terms with one head): `union`, `intersection`, `complement`, `is_subset`.
//!
//! Elements are compared by identity (`===`, so `1` and `1.0` are different elements). Results have no repeated
//! elements and are in the canonical order (decision 1B.7). Every operation hashes each element once and sorts
//! the result once: O(n log n) in the total size.
use crate::ctx::{sort_canonical, take, Lib, Meter};
use mc_core::{Store, T};
use mc_eval::{NativeCx, Reply, Res};
use rustc_hash::FxHashSet;

/// The operands of a set operation: one head shared by every argument, and their elements. A symbol argument
/// declines; an atom, or a head different from the first argument's, is `failure(argument, f, k)`.
fn operands<'h>(
    lib: &Lib,
    cx: &NativeCx<'_, '_, 'h>,
    t: T<'h>,
    args: &[T<'h>],
) -> Result<(T<'h>, Vec<&'h [T<'h>]>), Reply<'h>> {
    let h = cx.heap();
    let mut head = None;
    let mut sets = Vec::with_capacity(args.len());
    for (i, &a) in args.iter().enumerate() {
        let (g, xs) = lib.operand(h, t, a, i + 1)?;
        match head {
            None => head = Some(g),
            Some(f) if f != g => return Err(Reply::Done(lib.bad_arg(h, t, i + 1))),
            Some(_) => {}
        }
        sets.push(xs);
    }
    Ok((head.unwrap_or_else(|| h.sym_term(lib.s.list)), sets))
}

/// The set of the elements of `xs`.
fn set_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, xs: &[T<'h>]) -> Res<FxHashSet<T<'h>>> {
    let mut s = FxHashSet::with_capacity_and_hasher(xs.len(), Default::default());
    let mut meter = Meter::default();
    for &x in xs {
        meter.step(cx)?;
        s.insert(x);
    }
    Ok(s)
}

/// The distinct elements of `xs` with `keep(x)`, in canonical order, under `head`.
fn sorted<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    head: T<'h>,
    xs: impl Iterator<Item = T<'h>>,
    mut keep: impl FnMut(T<'h>) -> bool,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut seen = FxHashSet::default();
    let mut out = Vec::new();
    let mut meter = Meter::default();
    for x in xs {
        meter.step(cx)?;
        if keep(x) && seen.insert(x) {
            out.push(x);
        }
    }
    sort_canonical(cx, &mut out)?;
    Ok(lib.rebuilt(h, head, &out))
}

/// `union(xs, ys, ...)`: the elements of any of the arguments; `union()` is `[]`.
pub(crate) fn union<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let (head, sets) = take!(operands(lib, cx, t, args));
    sorted(lib, cx, head, sets.iter().flat_map(|s| s.iter().copied()), |_| true)
}

/// `intersection(xs, ys, ...)`: the elements of every argument.
pub(crate) fn intersection<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    if args.is_empty() {
        return Ok(Reply::Decline);
    }
    let (head, sets) = take!(operands(lib, cx, t, args));
    let mut others = Vec::with_capacity(sets.len() - 1);
    for s in &sets[1..] {
        others.push(set_of(cx, s)?);
    }
    sorted(lib, cx, head, sets[0].iter().copied(), |x| others.iter().all(|s| s.contains(&x)))
}

/// `complement(all, xs, ...)`: the elements of `all` in none of the other arguments.
pub(crate) fn complement<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    if args.is_empty() {
        return Ok(Reply::Decline);
    }
    let (head, sets) = take!(operands(lib, cx, t, args));
    let mut others = FxHashSet::default();
    let mut meter = Meter::default();
    for s in &sets[1..] {
        for &x in s.iter() {
            meter.step(cx)?;
            others.insert(x);
        }
    }
    sorted(lib, cx, head, sets[0].iter().copied(), |x| !others.contains(&x))
}

/// `is_subset(xs, ys)`: whether every element of `xs` is an element of `ys` (decision 1B.8).
pub(crate) fn is_subset<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    if args.len() != 2 {
        return Ok(Reply::Decline);
    }
    let (_, sets) = take!(operands(lib, cx, t, args));
    let ys = set_of(cx, sets[1])?;
    let mut meter = Meter::default();
    for &x in sets[0] {
        meter.step(cx)?;
        if !ys.contains(&x) {
            return Ok(Reply::Done(cx.bool(false)));
        }
    }
    Ok(Reply::Done(cx.bool(true)))
}
