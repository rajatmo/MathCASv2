//! Associations made from lists: `counts` and `group_by`; and the failure tests `is_failure`, `failure_tag`.
use crate::assoc::from_distinct;
use crate::ctx::{map_calls, take, Lib, Meter};
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};
use rustc_hash::FxHashMap;

/// Distinct keys in order of first occurrence, with the positions of the elements of each.
pub(crate) fn classes<'h>(cx: &mut NativeCx<'_, '_, 'h>, keys: &[T<'h>]) -> Res<(Vec<T<'h>>, Vec<Vec<usize>>)> {
    let mut order = Vec::new();
    let mut members: Vec<Vec<usize>> = Vec::new();
    let mut at: FxHashMap<T<'h>, usize> = FxHashMap::default();
    let mut meter = Meter::default();
    for (i, &k) in keys.iter().enumerate() {
        meter.step(cx)?;
        let p = *at.entry(k).or_insert_with(|| {
            order.push(k);
            members.push(Vec::new());
            order.len() - 1
        });
        members[p].push(i);
    }
    Ok((order, members))
}

/// `counts(xs)`: the association from each distinct element of `xs` (any compound term) to the number of times it
/// occurs, in order of first occurrence.
pub(crate) fn counts<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs] = *args else {
        return Ok(Reply::Decline);
    };
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let (keys, members) = classes(cx, xs)?;
    let entries: Vec<T<'h>> = keys.iter().zip(&members).map(|(&k, m)| lib.rule(h, k, h.int(m.len() as i64))).collect();
    Ok(Reply::Done(from_distinct(lib, h, &keys, &entries)))
}

/// `group_by(xs, f)`: the association from each distinct value `f(x)` to the list of the elements `x` with that
/// value, keys in order of first occurrence, elements in order; `group_by(xs, f, g)`: each list replaced by `g`
/// applied to it. A failure from `f` or `g` is the result. On a dataset the rows are grouped (`group_dataset`).
pub(crate) fn group_by<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, f, g) = match *args {
        [xs, f] => (xs, f, None),
        [xs, f, g] => (xs, f, Some(g)),
        _ => return Ok(Reply::Decline),
    };
    // a dataset groups its rows (package data)
    if let Some(tab) = crate::table::Table::of(&lib.d, h, xs) {
        return crate::aggregate::group_dataset(lib, cx, &tab, f, g);
    }
    // an association groups its entries by `f` of their values, each group an association (decision io-core.12)
    let entries = lib.as_assoc(h, xs).filter(|es| es.iter().all(|&e| lib.entry(h, e).is_some()));
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let tested: Vec<T<'h>> = match entries {
        Some(es) => es.iter().map(|&e| lib.value(h, e)).collect(),
        None => xs.to_vec(),
    };
    let fx = map_calls(cx, f, &tested)?;
    if let Some(&fail) = fx.iter().find(|&&v| lib.is_failure(h, v)) {
        return Ok(Reply::Done(fail));
    }
    let (keys, members) = classes(cx, &fx)?;
    let mut groups: Vec<T<'h>> = Vec::with_capacity(keys.len());
    for m in &members {
        cx.tick(m.len() as u64 / 16)?;
        let items: Vec<T<'h>> = m.iter().map(|&i| xs[i]).collect();
        groups.push(if entries.is_some() { lib.assoc(h, &items) } else { cx.list(&items) });
    }
    if let Some(g) = g {
        groups = map_calls(cx, g, &groups)?;
        if let Some(&fail) = groups.iter().find(|&&v| lib.is_failure(h, v)) {
            return Ok(Reply::Done(fail));
        }
    }
    let entries: Vec<T<'h>> = keys.iter().zip(&groups).map(|(&k, &v)| lib.rule(h, k, v)).collect();
    Ok(Reply::Done(from_distinct(lib, h, &keys, &entries)))
}

/// `is_failure(x)`: whether `x` is a failure `failure(...)` (it receives failures as values).
pub(crate) fn is_failure<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    Ok(Reply::Done(cx.bool(lib.is_failure(h, x))))
}

/// `failure_tag(x)`: the tag of a failure `failure(tag, ...)`; anything else (including `failure()`) is
/// `failure(argument, failure_tag, 1)`.
pub(crate) fn failure_tag<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    Ok(Reply::Done(match h.view(x) {
        View::App { head, args: &[tag, ..] } if head == h.sym_term(lib.s.failure) => tag,
        _ => lib.bad_arg(h, t, 1),
    }))
}
