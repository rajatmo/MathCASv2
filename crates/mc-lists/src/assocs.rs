//! Associations through the list functions (decision io-core.4): an association `assoc(k1 -> v1, ...)` as the first
//! argument stands for its values.
//!
//! - [`AssocMode::Values`]: the function sees the list of the values and its result is the answer (`total`, `max`,
//!   `count`, `cases`, `all_true`, `select_first`, ...).
//! - [`AssocMode::Keyed`]: the function sees the list of the values and gives some of them back, filtered or
//!   reordered (`unique`, `sort`, `delete_cases`, `max_by`, `take_largest`, ...); each value given back takes its
//!   entry with it (equal values in their order), so the answer is the association of those entries.
//!
//! Functions that already handle associations themselves (`map`, `select`, `sort_by`, `reverse`, `take`, `drop`,
//! `length`, `part`) keep their own meaning.
use crate::ctx::Lib;
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};
use rustc_hash::FxHashMap;
use std::collections::VecDeque;

/// How a function treats an association as its first argument.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum AssocMode {
    /// as any other compound term
    Own,
    /// the list of its values; the result as it is
    Values,
    /// the list of its values; the values in the result carry their keys back
    Keyed,
}

/// The functions that see an association's values ([`AssocMode::Values`]).
const VALUES: &[&str] = &[
    "total", "max", "min", "count", "cases", "all_true", "any_true", "none_true", "select_first", "first_case",
    "member_of", "tally", "count_by", "gather_by", "first", "last", "free_of",
];

/// The functions whose results keep the association's keys ([`AssocMode::Keyed`]).
const KEYED: &[&str] =
    &["unique", "sort", "delete_cases", "max_by", "min_by", "take_largest", "take_smallest", "reverse_sort", "unique_by"];

/// The mode of the function `name`.
pub(crate) fn mode(name: &str) -> AssocMode {
    if VALUES.contains(&name) {
        AssocMode::Values
    } else if KEYED.contains(&name) {
        AssocMode::Keyed
    } else {
        AssocMode::Own
    }
}

/// The entries `(k, v)` of `x` when it is an association whose arguments are all rules `k -> v`.
pub(crate) fn entries<'h>(lib: &Lib, h: &'h Heap, x: T<'h>) -> Option<Vec<(T<'h>, T<'h>)>> {
    if h.packed(x).is_some() {
        return None;
    }
    let View::App { head, args } = h.view(x) else { return None };
    if head != h.sym_term(lib.s.assoc) {
        return None;
    }
    let rule = h.sym_term(lib.s.rule);
    args.iter()
        .map(|&r| match h.view(r) {
            View::App { head, args: &[k, v] } if head == rule => Some((k, v)),
            _ => None,
        })
        .collect()
}

/// The body of a list function.
pub(crate) type Body = crate::Body;

/// The call `t` (`f(a, rest...)`, `a` an association with entries `es`) with `a` replaced by the list of its values.
pub(crate) fn call<'h>(
    lib: &Lib,
    body: Body,
    mode: AssocMode,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    args: &[T<'h>],
    es: Vec<(T<'h>, T<'h>)>,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    // with levels, the walk sees an association's values itself (`cases(a, p, levels)`, `delete_cases(a, p, levels)`)
    if args.len() >= 3 && matches!(sym_name(h, t).as_deref(), Some("cases" | "count" | "delete_cases")) {
        return body(lib, cx, t, args);
    }
    let values: Vec<T<'h>> = es.iter().map(|&(_, v)| v).collect();
    cx.tick(values.len() as u64)?;
    let mut a = args.to_vec();
    a[0] = cx.list(&values);
    let head = match h.view(t) {
        View::App { head, .. } => head,
        _ => return Ok(Reply::Decline),
    };
    let t2 = h.app(head, &a);
    let r = body(lib, cx, t2, &a)?;
    if mode == AssocMode::Values {
        return Ok(r);
    }
    let v = match r {
        Reply::Done(v) => v,
        Reply::Again(u) => cx.eval(u)?,
        Reply::Decline => return Ok(Reply::Decline),
    };
    let Some(out) = cx.as_list(v) else { return Ok(Reply::Done(v)) };
    Ok(Reply::Done(rekey(lib, h, &es, out).unwrap_or(v)))
}

/// The association of the entries whose values are `out` (each value once, equal values in their order), or `None`
/// when some element of `out` is no value of `es`.
fn rekey<'h>(lib: &Lib, h: &'h Heap, es: &[(T<'h>, T<'h>)], out: &[T<'h>]) -> Option<T<'h>> {
    let mut by_value: FxHashMap<T<'h>, VecDeque<usize>> = FxHashMap::default();
    for (i, &(_, v)) in es.iter().enumerate() {
        by_value.entry(v).or_default().push_back(i);
    }
    let rule = lib.s.rule;
    let mut rules = Vec::with_capacity(out.len());
    for &v in out {
        let i = by_value.get_mut(&v)?.pop_front()?;
        rules.push(h.call(rule, &[es[i].0, es[i].1]));
    }
    Some(h.call(lib.s.assoc, &rules))
}

/// The name of the head of the call `t`.
fn sym_name(h: &Heap, t: T<'_>) -> Option<String> {
    match h.view(t) {
        View::App { head, .. } => match h.view(head) {
            View::Sym(s) => Some(h.sym_name(s)),
            _ => None,
        },
        _ => None,
    }
}
