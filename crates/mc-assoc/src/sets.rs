//! Sets on lists (or on any compound terms with one head): `union`, `intersection`, `complement`, `is_subset`.
//!
//! Elements are compared by identity (`===`, so `1` and `1.0` are different elements). Results have no repeated
//! elements and are in the canonical order (decision 1B.7). Every operation hashes each element once and sorts
//! the result once: O(n log n) in the total size.
use crate::ctx::{sort_canonical, take, Lib, Meter};
use mc_core::{Store, View, T};
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

/// The test of a final `options(same_test -> f)` argument, and the arguments before it.
fn same_test<'h>(cx: &NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Option<(T<'h>, usize)> {
    let h = cx.heap();
    let (&last, _) = args.split_last()?;
    let View::App { head, args: opts } = h.view(last) else { return None };
    if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "options") {
        return None;
    }
    let [o] = opts else { return None };
    match h.view(*o) {
        View::App { args: &[k, f], .. } if matches!(h.view(k), View::Sym(s) if h.sym_name(s) == "same_test") => {
            Some((f, args.len() - 1))
        }
        _ => None,
    }
}

/// Is `f(x, y)` true? A failure of the test is `Err`.
fn same<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, y: T<'h>) -> Res<Result<bool, T<'h>>> {
    let h = cx.heap();
    cx.tick(1)?;
    let v = cx.eval(h.app(f, &[x, y]))?;
    if lib.is_failure(h, v) {
        return Ok(Err(v));
    }
    Ok(Ok(cx.is_true(v)))
}

/// The set operations with `same_test: f` (decision io-core.7): the elements of the first argument (all
/// arguments for `union`) in canonical order, an element dropped when `f(y, x)` holds for an element y kept before
/// it; then `intersection` keeps those with a match `f(x, z)` in every other argument and `complement` those with a
/// match in none.
fn with_test<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    args: &[T<'h>],
    f: T<'h>,
    op: u8,
) -> Res<Reply<'h>> {
    let (head, sets) = take!(operands(lib, cx, t, args));
    let Some((first, rest)) = sets.split_first() else { return Ok(Reply::Decline) };
    let mut all: Vec<T<'h>> = if op == b'u' { sets.iter().flat_map(|s| s.iter().copied()).collect() } else { first.to_vec() };
    sort_canonical(cx, &mut all)?;
    let mut kept: Vec<T<'h>> = vec![];
    'next: for x in all {
        for &y in &kept {
            match same(lib, cx, f, y, x)? {
                Ok(true) => continue 'next,
                Ok(false) => {}
                Err(v) => return Ok(Reply::Done(v)),
            }
        }
        kept.push(x);
    }
    if op != b'u' {
        let mut out = vec![];
        for x in kept {
            let mut found_in_all = true;
            let mut found_in_any = false;
            for s in rest {
                let mut found = false;
                for &z in s.iter() {
                    match same(lib, cx, f, x, z)? {
                        Ok(true) => {
                            found = true;
                            break;
                        }
                        Ok(false) => {}
                        Err(v) => return Ok(Reply::Done(v)),
                    }
                }
                found_in_all &= found;
                found_in_any |= found;
            }
            if (op == b'i' && found_in_all) || (op == b'c' && !found_in_any) {
                out.push(x);
            }
        }
        kept = out;
    }
    Ok(lib.rebuilt(cx.heap(), head, &kept))
}

/// `union(xs, ys, ...)`: the elements of any of the arguments; `union()` is `[]`.
pub(crate) fn union<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    if let Some((f, n)) = same_test(cx, args) {
        return with_test(lib, cx, t, &args[..n], f, b'u');
    }
    let (head, sets) = take!(operands(lib, cx, t, args));
    sorted(lib, cx, head, sets.iter().flat_map(|s| s.iter().copied()), |_| true)
}

/// `intersection(xs, ys, ...)`: the elements of every argument.
pub(crate) fn intersection<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    if let Some((f, n)) = same_test(cx, args) {
        return with_test(lib, cx, t, &args[..n], f, b'i');
    }
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
    if let Some((f, n)) = same_test(cx, args) {
        return with_test(lib, cx, t, &args[..n], f, b'c');
    }
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
