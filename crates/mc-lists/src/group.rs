//! Grouping by identity or by a key: `tally`, `gather`, `gather_by`, `unique_by`, `count_by`.
//!
//! Terms are hash-consed, so equal elements (or keys) are one term: every function here is one pass with a hash
//! map, and groups come in the order of their first element (decision 1A.7). Keys `f(x)` are evaluated as a group,
//! like `map`; a failure among them is the result.
use crate::ctx::{apply, keys, take, Lib, Meter};
use mc_core::{Store, T};
use mc_eval::{NativeCx, Reply, Res};
use rustc_hash::FxHashMap;

/// The groups of `xs` by `key[i]`: the distinct keys in order of first occurrence, and the elements of each.
fn groups<'h>(cx: &mut NativeCx<'_, '_, 'h>, xs: &[T<'h>], key: &[T<'h>]) -> Res<(Vec<T<'h>>, Vec<Vec<T<'h>>>)> {
    let mut index: FxHashMap<T<'h>, usize> = FxHashMap::default();
    let (mut ks, mut gs): (Vec<T<'h>>, Vec<Vec<T<'h>>>) = (vec![], vec![]);
    let mut m = Meter::default();
    for (&x, &k) in xs.iter().zip(key) {
        m.step(cx)?;
        let i = *index.entry(k).or_insert_with(|| {
            ks.push(k);
            gs.push(vec![]);
            ks.len() - 1
        });
        gs[i].push(x);
    }
    Ok((ks, gs))
}

/// The list `xs` (argument 1) and the key function (argument 2) of a `_by` function, with the keys evaluated.
/// `Err` holds the reply: unevaluated, an argument failure, or a key's failure.
#[allow(clippy::type_complexity)]
fn keyed<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    xs: T<'h>,
    f: T<'h>,
) -> Res<Result<(&'h [T<'h>], Vec<T<'h>>), Reply<'h>>> {
    let h = cx.heap();
    let xs = match lib.operand(h, t, xs, 1) {
        Ok((_, xs)) => xs,
        Err(r) => return Ok(Err(r)),
    };
    Ok(match keys(lib, cx, f, xs)? {
        Ok(ks) => Ok((xs, ks)),
        Err(e) => Err(Reply::Done(e)),
    })
}

/// `tally(xs)`: the list of pairs `[x, n]`, each distinct element with the number of its occurrences.
pub(crate) fn tally<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs] = *args else { return Ok(Reply::Decline) };
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let (ks, gs) = groups(cx, xs, xs)?;
    let out: Vec<T<'h>> = ks.iter().zip(&gs).map(|(&k, g)| cx.list(&[k, h.int(g.len() as i64)])).collect();
    Ok(Reply::Done(cx.list(&out)))
}

/// `gather(xs)`: the lists of identical elements.
pub(crate) fn gather<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs] = *args else { return Ok(Reply::Decline) };
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let (_, gs) = groups(cx, xs, xs)?;
    let out: Vec<T<'h>> = gs.iter().map(|g| cx.list(g)).collect();
    Ok(Reply::Done(cx.list(&out)))
}

/// `gather_by(xs, f)`: the lists of elements with identical `f(x)`, in order of their first element;
/// `gather_by(xs, f, r)`: each list reduced by `r`, `[r([x, ...]), ...]`. The association from keys to groups is
/// package 1B's `group_by` (decision 1A.8).
pub(crate) fn gather_by<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, f, r) = match *args {
        [xs, f] => (xs, f, None),
        [xs, f, r] => (xs, f, Some(r)),
        _ => return Ok(Reply::Decline),
    };
    let (xs, ks) = take!(keyed(lib, cx, t, xs, f)?);
    let (_, gs) = groups(cx, xs, &ks)?;
    let mut out = Vec::with_capacity(gs.len());
    for g in &gs {
        let mut v = cx.list(g);
        if let Some(r) = r {
            cx.tick(1)?;
            v = apply(cx, r, &[v])?;
            if lib.is_failure(h, v) {
                return Ok(Reply::Done(v));
            }
        }
        out.push(v);
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// `unique_by(xs, f)`: the elements with a value `f(x)` not seen before, in order (the first of each key is kept).
/// The `by` form of `unique` (decision 1A.9).
pub(crate) fn unique_by<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, f] = *args else { return Ok(Reply::Decline) };
    let (head, _) = take!(lib.operand(h, t, xs, 1));
    let (xs, ks) = take!(keyed(lib, cx, t, xs, f)?);
    let (_, gs) = groups(cx, xs, &ks)?;
    let out: Vec<T<'h>> = gs.iter().map(|g| g[0]).collect();
    Ok(lib.rebuilt(h, head, &out))
}

/// `count_by(xs, f)`: the list of rules `k -> n`, each distinct key `k = f(x)` with its number of elements.
pub(crate) fn count_by<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, f] = *args else { return Ok(Reply::Decline) };
    let (xs, ks) = take!(keyed(lib, cx, t, xs, f)?);
    let (ks, gs) = groups(cx, xs, &ks)?;
    let out: Vec<T<'h>> = ks.iter().zip(&gs).map(|(&k, g)| lib.rule(h, k, h.int(g.len() as i64))).collect();
    Ok(Reply::Done(cx.list(&out)))
}
