//! Associations: `assoc` (construction and normal form), `is_assoc`, `keys`, `values`, `to_rules`, `lookup`,
//! `key_exists`, `key_take`, `key_drop`, `key_sort`, `insert`, `delete_key`, `merge`, `map_values`, `select_keys`.
//!
//! An association is the term `assoc(rule(k1, v1), ..., rule(kn, vn))` with distinct keys in insertion order
//! (decision 1B.1); keys are compared by identity (`===`). Its normal form is made by the `assoc` native: the
//! arguments are entries `k -> v`, lists of entries or associations, read in order; a later entry for a key
//! replaces the value of the earlier one, which keeps its position (decision 1B.2).
use crate::ctx::{charge_copy, map_calls, sort_canonical, take, Lib, Meter};
use crate::index::{position, KeyIndex, SMALL};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Entries collected into an association in normal form: later values for a key replace earlier ones in place.
#[derive(Default)]
struct Builder<'h> {
    entries: Vec<T<'h>>,
    at: FxHashMap<T<'h>, usize>,
    /// whether some entry replaced an earlier one
    replaced: bool,
}

impl<'h> Builder<'h> {
    fn with_capacity(n: usize) -> Builder<'h> {
        let at = FxHashMap::with_capacity_and_hasher(n, Default::default());
        Builder { entries: Vec::with_capacity(n), at, replaced: false }
    }

    /// Add the entry `e` with key `k`.
    fn push(&mut self, k: T<'h>, e: T<'h>) {
        match self.at.get(&k) {
            Some(&p) => {
                self.entries[p] = e;
                self.replaced = true;
            }
            None => {
                self.at.insert(k, self.entries.len());
                self.entries.push(e);
            }
        }
    }

    /// The association, with its index cached when it is large enough to be indexed.
    fn finish(self, lib: &Lib, h: &'h Heap) -> T<'h> {
        let a = lib.assoc(h, &self.entries);
        self.indexed(lib, a)
    }

    /// `a`, the association of the entries, with its index cached when it is large enough to be indexed.
    fn indexed(self, lib: &Lib, a: T<'h>) -> T<'h> {
        if self.entries.len() > SMALL {
            let index: KeyIndex = self.at.into_iter().map(|(k, p)| (k.raw(), p as u32)).collect();
            lib.indexes.put(a, Arc::new(index));
        }
        a
    }
}

/// `assoc(e1, e2, ...)`: the association of the entries; each argument is an entry `k -> v`, a list of entries,
/// or an association. An association in normal form is returned as it is.
pub(crate) fn assoc<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // an association made (or checked) earlier in this evaluation
    if lib.indexes.contains(t) {
        return Ok(Reply::Done(t));
    }
    let mut b = Builder::with_capacity(args.len());
    let mut flat = true;
    let mut meter = Meter::default();
    for (i, &x) in args.iter().enumerate() {
        meter.step(cx)?;
        if let Some((k, _)) = lib.entry(h, x) {
            b.push(k, x);
            continue;
        }
        flat = false;
        // anything else than entries (a pattern `assoc(k_ -> v_, ___)`, a delayed rule) leaves the call as it is
        // (decision io-core.12)
        let inner = match h.view(x) {
            View::App { head, args } if head == h.sym_term(lib.s.list) || head == h.sym_term(lib.s.assoc) => args,
            View::Str(_) | View::Int(_) | View::Real(_) => return Ok(Reply::Done(lib.bad_arg(h, t, i + 1))),
            _ => return Ok(Reply::Decline),
        };
        for &e in inner {
            meter.step(cx)?;
            match lib.entry(h, e) {
                Some((k, _)) => b.push(k, e),
                None => return Ok(Reply::Decline),
            }
        }
    }
    if flat && !b.replaced {
        // already a normal form: keep the term (the evaluated call) and its index
        return Ok(Reply::Done(b.indexed(lib, t)));
    }
    Ok(Reply::Done(b.finish(lib, h)))
}

/// `is_assoc(x)`: whether `x` is an association.
pub(crate) fn is_assoc<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    // every argument an entry (an `assoc` call that stayed unevaluated is no association)
    let ok = lib.as_assoc(h, x).is_some_and(|es| es.iter().all(|&e| lib.is_rule(h, e)));
    Ok(Reply::Done(cx.bool(ok)))
}

/// The list of one part of every entry.
fn parts<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    args: &[T<'h>],
    part: fn(&Lib, &'h Heap, T<'h>) -> T<'h>,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a] = *args else {
        return Ok(Reply::Decline);
    };
    // one rule: its key or value (decision io-core.12)
    if lib.is_rule(h, a) {
        return Ok(Reply::Done(part(lib, h, a)));
    }
    // a list of rules is read like the entries of an association (decision p6core.12)
    let rules = cx.as_list(a).filter(|xs| xs.iter().all(|&x| lib.is_rule(h, x)));
    let es = match rules {
        Some(xs) => xs,
        None => take!(lib.assoc_operand(h, t, a, 1)),
    };
    charge_copy(cx, es.len())?;
    let out: Vec<T<'h>> = es.iter().map(|&e| part(lib, h, e)).collect();
    Ok(Reply::Done(cx.list(&out)))
}

/// `keys(a)`: the list of keys, in order.
pub(crate) fn keys<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    parts(lib, cx, t, args, |lib, h, e| lib.key(h, e))
}

/// `values(a)`: the list of values, in order.
pub(crate) fn values<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    parts(lib, cx, t, args, |lib, h, e| lib.value(h, e))
}

/// `to_rules(a)`: the list of entries `k -> v`, in order.
pub(crate) fn to_rules<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    parts(lib, cx, t, args, |_, _, e| e)
}

/// `lookup(a, k)`: the value of key `k`, `failure(missing_key, k)` if there is none; `lookup(a, k, d)`: `d` if there
/// is none. The key and the default are held: the key is evaluated first, the default only when the key is missing
/// (decision p6core.23).
pub(crate) fn lookup<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (a, k, default) = match *args {
        [a, k] => (a, k, None),
        [a, k, d] => (a, k, Some(d)),
        _ => return Ok(Reply::Decline),
    };
    let k = cx.eval(k)?;
    if lib.is_failure(h, k) {
        return Ok(Reply::Done(k));
    }
    let a = match rules_operand(lib, cx, t, a, &[k], default)? {
        Ok(a) => a,
        Err(r) => return Ok(r),
    };
    let es = match lib.assoc_operand(h, t, a, 1) {
        Ok(es) => es,
        // left unevaluated: the call with its key evaluated
        Err(Reply::Decline) => {
            let mut v = vec![a, k];
            v.extend(default);
            let head = match h.view(t) {
                View::App { head, .. } => head,
                _ => return Ok(Reply::Decline),
            };
            return Ok(Reply::Done(h.app(head, &v)));
        }
        Err(r) => return Ok(r),
    };
    let found = position(lib, cx, a, es, k, true)?;
    // a list of keys that is not itself a key: the value of each (decision cov-core.4)
    if found.is_none() {
        if let Some(ks) = cx.as_list(k) {
            cx.tick(ks.len() as u64)?;
            let f = h.sym_term(h.sym("lookup"));
            let each: Vec<T<'h>> = ks
                .iter()
                .map(|&k1| {
                    let mut v = vec![a, k1];
                    v.extend(default);
                    h.app(f, &v)
                })
                .collect();
            return Ok(Reply::Again(cx.list(&each)));
        }
    }
    Ok(match found {
        Some(p) => Reply::Done(lib.value(h, es[p])),
        None => match default {
            Some(d) => Reply::Again(d),
            None => Reply::Done(lib.fail(h, lib.s.missing_key, &[k])),
        },
    })
}

/// The association argument of `lookup` and `key_exists` (decision cov-core.4): a rule or a list of rules is read
/// as the association of those rules; a list of associations (or of lists of rules) gives the call on each element,
/// as `Err` with the reply; anything else is passed on unchanged.
fn rules_operand<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    a: T<'h>,
    rest: &[T<'h>],
    default: Option<T<'h>>,
) -> Res<Result<T<'h>, Reply<'h>>> {
    let h = cx.heap();
    let rule = h.sym_term(lib.s.rule);
    let rule_delayed = h.sym_term(h.sym("rule_delayed"));
    let is_rule = |x: T<'h>| matches!(h.view(x), View::App { head, args: [_, _] } if head == rule || head == rule_delayed);
    let assoc = h.sym_term(lib.s.assoc);
    if is_rule(a) {
        return Ok(Ok(cx.eval(h.app(assoc, &[a]))?));
    }
    let Some(xs) = cx.as_list(a) else { return Ok(Ok(a)) };
    if xs.is_empty() {
        return Ok(Ok(a));
    }
    if xs.iter().all(|&x| is_rule(x)) {
        return Ok(Ok(cx.eval(h.app(assoc, xs))?));
    }
    let rules_or_assoc = |x: T<'h>| {
        lib.as_assoc(h, x).is_some() || cx.as_list(x).is_some_and(|ys| !ys.is_empty() && ys.iter().all(|&y| is_rule(y)))
    };
    if xs.iter().all(|&x| rules_or_assoc(x)) {
        let View::App { head, .. } = h.view(t) else { return Ok(Ok(a)) };
        cx.tick(xs.len() as u64)?;
        let each: Vec<T<'h>> = xs
            .iter()
            .map(|&x| {
                let mut v = vec![x];
                v.extend_from_slice(rest);
                v.extend(default);
                h.app(head, &v)
            })
            .collect();
        return Ok(Err(Reply::Again(cx.list(&each))));
    }
    Ok(Ok(a))
}

/// `key_exists(a, k)`: whether `a` has the key `k`.
pub(crate) fn key_exists<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, k] = *args else {
        return Ok(Reply::Decline);
    };
    let a = match rules_operand(lib, cx, t, a, &[k], None)? {
        Ok(a) => a,
        Err(r) => return Ok(r),
    };
    // a dataset has its columns as keys (G57)
    let a = match h.view(a) {
        View::App { head, args: [cols] } if head == h.sym_term(lib.d.dataset) && lib.as_assoc(h, *cols).is_some() => *cols,
        _ => a,
    };
    let es = take!(lib.assoc_operand(h, t, a, 1));
    let found = position(lib, cx, a, es, k, true)?.is_some();
    Ok(Reply::Done(cx.bool(found)))
}

/// `key_take(a, ks)`: the entries of the keys `ks` (a list, or a single key) that `a` has, in the order of `ks`.
pub(crate) fn key_take<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, ks] = *args else {
        return Ok(Reply::Decline);
    };
    let es = take!(lib.assoc_operand(h, t, a, 1));
    let ks = lib.key_list(h, ks);
    let mut b = Builder::with_capacity(ks.len().min(es.len()));
    let mut meter = Meter::default();
    for &k in &ks {
        meter.step(cx)?;
        if let Some(p) = position(lib, cx, a, es, k, true)? {
            b.push(k, es[p]);
        }
    }
    Ok(Reply::Done(b.finish(lib, h)))
}

/// `key_drop(a, ks)`: `a` without the keys `ks` (a list, or a single key), in its order.
pub(crate) fn key_drop<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, ks] = *args else {
        return Ok(Reply::Decline);
    };
    let es = take!(lib.assoc_operand(h, t, a, 1));
    let ks = lib.key_list(h, ks);
    charge_copy(cx, ks.len())?;
    let drop: rustc_hash::FxHashSet<T<'h>> = ks.into_iter().collect();
    Ok(Reply::Done(filtered(lib, cx, a, es, |_, e| !drop.contains(&lib.key(h, e)))?))
}

/// The association of the entries `es[i]` of `a` with `keep(i, es[i])`: `a` itself when every entry is kept.
fn filtered<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    a: T<'h>,
    es: &[T<'h>],
    mut keep: impl FnMut(usize, T<'h>) -> bool,
) -> Res<T<'h>> {
    let h = cx.heap();
    let mut out = Vec::with_capacity(es.len());
    let mut meter = Meter::default();
    for (i, &e) in es.iter().enumerate() {
        meter.step(cx)?;
        if keep(i, e) {
            out.push(e);
        }
    }
    Ok(if out.len() == es.len() { a } else { lib.assoc(h, &out) })
}

/// `key_sort(a)`: the entries ordered by key in the canonical order.
pub(crate) fn key_sort<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a] = *args else {
        return Ok(Reply::Decline);
    };
    let es = take!(lib.assoc_operand(h, t, a, 1));
    let mut ks: Vec<T<'h>> = es.iter().map(|&e| lib.key(h, e)).collect();
    sort_canonical(cx, &mut ks)?;
    // keys are distinct, so the sorted keys name the entries
    let at: FxHashMap<T<'h>, usize> = es.iter().enumerate().map(|(i, &e)| (lib.key(h, e), i)).collect();
    let out: Vec<T<'h>> = ks.iter().map(|k| es[at[k]]).collect();
    Ok(Reply::Done(lib.assoc(h, &out)))
}

/// `insert(a, k -> v)` or `insert(a, k, v)`: `a` with key `k` set to `v` (in its place if `k` is a key, at the
/// end otherwise).
pub(crate) fn insert<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `insert(xs, x, n)`: `x` inserted into a list (any compound term) at position n, a path `[i, j, ...]` or several
    // paths; `insert(a, k -> v, n)`: the entry at position n (or before the key n) of an association (decision io-core.16)
    if let [x, e, pos] = *args {
        if lib.as_assoc(h, x).is_none() && matches!(h.view(x), View::App { .. }) {
            return insert_into(lib, cx, t, x, e, pos);
        }
        if let (Some(es), Some(_)) = (lib.as_assoc(h, x), lib.entry(h, e)) {
            let at = match h.view(pos) {
                View::Int(i) => {
                    let n = es.len() as i64;
                    let i = if i < 0 { n + 2 + i } else { i };
                    if !(1..=n + 1).contains(&i) {
                        return Ok(Reply::Done(lib.bad_arg(h, t, 3)));
                    }
                    (i - 1) as usize
                }
                _ => match position(lib, cx, x, es, pos, false)? {
                    Some(p) => p,
                    None => return Ok(Reply::Done(lib.bad_arg(h, t, 3))),
                },
            };
            charge_copy(cx, es.len() + 1)?;
            let mut out = es.to_vec();
            out.insert(at, e);
            return Ok(Reply::Again(h.call(lib.s.assoc, &out)));
        }
    }
    let (a, k, e) = match *args {
        [a, e] => match lib.entry(h, e) {
            Some((k, _)) => (a, k, e),
            None => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
        },
        [a, k, v] => (a, k, lib.rule(h, k, v)),
        _ => return Ok(Reply::Decline),
    };
    let es = take!(lib.assoc_operand(h, t, a, 1));
    let p = position(lib, cx, a, es, k, false)?;
    charge_copy(cx, es.len())?;
    let mut out = es.to_vec();
    match p {
        Some(p) => out[p] = e,
        None => out.push(e),
    }
    Ok(Reply::Done(lib.assoc(h, &out)))
}

/// `x` with `e` inserted at the positions `pos` (an integer, a path, or a list of paths); a position outside is
/// `failure(argument, insert, 3)`.
fn insert_into<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, x: T<'h>, e: T<'h>, pos: T<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let ints = |p: T<'h>| -> Option<Vec<i64>> {
        match h.view(p) {
            View::Int(i) => Some(vec![i]),
            _ => cx.as_list(p)?.iter().map(|&q| if let View::Int(i) = h.view(q) { Some(i) } else { None }).collect(),
        }
    };
    let paths: Vec<Vec<i64>> = match cx.as_list(pos) {
        Some(ps) if !ps.is_empty() && ps.iter().all(|&p| cx.as_list(p).is_some()) => {
            match ps.iter().map(|&p| ints(p)).collect::<Option<Vec<_>>>() {
                Some(v) => v,
                None => return Ok(Reply::Decline),
            }
        }
        _ => match ints(pos) {
            Some(p) if !p.is_empty() => vec![p],
            _ => return Ok(Reply::Decline),
        },
    };
    // the last positions first, so the earlier ones stay valid
    let mut paths = paths;
    paths.sort();
    let mut cur = x;
    for p in paths.iter().rev() {
        cx.tick(1)?;
        match insert_path(h, cur, p, e) {
            Some(u) => cur = u,
            None => return Ok(Reply::Done(lib.bad_arg(h, t, 3))),
        }
    }
    Ok(Reply::Again(cur))
}

/// `x` with `e` inserted at the path `p` (the last index the insertion point, negative from the end).
fn insert_path<'h>(h: &'h mc_core::Heap, x: T<'h>, p: &[i64], e: T<'h>) -> Option<T<'h>> {
    let View::App { head, args } = h.view(x) else { return None };
    let n = args.len() as i64;
    let (&i, rest) = p.split_first()?;
    let mut out = args.to_vec();
    if rest.is_empty() {
        let i = if i < 0 { n + 2 + i } else { i };
        if !(1..=n + 1).contains(&i) {
            return None;
        }
        out.insert((i - 1) as usize, e);
    } else {
        let i = if i < 0 { n + 1 + i } else { i };
        if !(1..=n).contains(&i) {
            return None;
        }
        out[(i - 1) as usize] = insert_path(h, args[(i - 1) as usize], rest, e)?;
    }
    Some(h.app(head, &out))
}

/// `delete_key(a, k)`: `a` without key `k` (unchanged if it has no such key).
pub(crate) fn delete_key<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, k] = *args else {
        return Ok(Reply::Decline);
    };
    let es = take!(lib.assoc_operand(h, t, a, 1));
    Ok(Reply::Done(match position(lib, cx, a, es, k, false)? {
        Some(p) => {
            charge_copy(cx, es.len())?;
            let mut out = es.to_vec();
            out.remove(p);
            lib.assoc(h, &out)
        }
        None => a,
    }))
}

/// `merge([a1, a2, ...])` (or `merge(a1, a2, ...)`): the entries of all, later values replacing earlier ones;
/// `merge([a1, a2, ...], f)`: for every key, `f` applied to the list of its values in order (also for a key with
/// one value), keys in order of first occurrence (decision 1B.10).
pub(crate) fn merge<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (items, f): (Vec<T<'h>>, Option<T<'h>>) = match *args {
        [] => return Ok(Reply::Decline),
        [x] if lib.as_assoc(h, x).is_none() => (take!(list_of_assocs(lib, h, t, x, 1)), None),
        [x, f] if lib.as_assoc(h, x).is_none() => (take!(list_of_assocs(lib, h, t, x, 1)), Some(f)),
        _ => {
            for (i, &x) in args.iter().enumerate() {
                if lib.as_assoc(h, x).is_none() {
                    return Ok(match h.view(x) {
                        View::Sym(_) => Reply::Decline,
                        _ => Reply::Done(lib.bad_arg(h, t, i + 1)),
                    });
                }
            }
            (args.to_vec(), None)
        }
    };
    let mut meter = Meter::default();
    let Some(f) = f else {
        let mut b = Builder::default();
        for &a in &items {
            for &e in lib.as_assoc(h, a).unwrap_or(&[]) {
                meter.step(cx)?;
                b.push(lib.key(h, e), e);
            }
        }
        return Ok(Reply::Done(b.finish(lib, h)));
    };
    // the values of every key, keys in order of first occurrence
    let mut keys: Vec<T<'h>> = Vec::new();
    let mut vals: Vec<Vec<T<'h>>> = Vec::new();
    let mut at: FxHashMap<T<'h>, usize> = FxHashMap::default();
    for &a in &items {
        for &e in lib.as_assoc(h, a).unwrap_or(&[]) {
            meter.step(cx)?;
            let k = lib.key(h, e);
            let p = *at.entry(k).or_insert_with(|| {
                keys.push(k);
                vals.push(Vec::new());
                keys.len() - 1
            });
            vals[p].push(lib.value(h, e));
        }
    }
    let groups: Vec<T<'h>> = vals.iter().map(|vs| cx.list(vs)).collect();
    let combined = map_calls(cx, f, &groups)?;
    entries_of(lib, cx, &keys, &combined)
}

/// The association `k_i -> v_i` (distinct keys), or the first failure among the values.
fn entries_of<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, keys: &[T<'h>], vals: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    charge_copy(cx, keys.len())?;
    let mut out = Vec::with_capacity(keys.len());
    for (&k, &v) in keys.iter().zip(vals) {
        if lib.is_failure(h, v) {
            return Ok(Reply::Done(v));
        }
        out.push(lib.rule(h, k, v));
    }
    Ok(Reply::Done(from_distinct(lib, h, keys, &out)))
}

/// The association of `entries`, whose keys are the distinct `keys`, with its index cached when it is large
/// enough to be indexed (the callers have charged for the loop over the entries).
pub(crate) fn from_distinct<'h>(lib: &Lib, h: &'h Heap, keys: &[T<'h>], entries: &[T<'h>]) -> T<'h> {
    let a = lib.assoc(h, entries);
    if entries.len() > SMALL {
        let index: KeyIndex = keys.iter().enumerate().map(|(i, k)| (k.raw(), i as u32)).collect();
        lib.indexes.put(a, Arc::new(index));
    }
    a
}

/// Argument `k` of `t` as a list of associations.
fn list_of_assocs<'h>(lib: &Lib, h: &'h Heap, t: T<'h>, x: T<'h>, k: usize) -> Result<Vec<T<'h>>, Reply<'h>> {
    match h.view(x) {
        View::App { head, args } if head == h.sym_term(lib.s.list) => {
            if args.iter().all(|&a| lib.as_assoc(h, a).is_some()) {
                Ok(args.to_vec())
            } else {
                Err(Reply::Done(lib.bad_arg(h, t, k)))
            }
        }
        View::Sym(_) => Err(Reply::Decline),
        _ => Err(Reply::Done(lib.bad_arg(h, t, k))),
    }
}

/// `map_values(f, a)`: `a` with every value `v` replaced by `f(v)`; a failure from `f` is the result.
pub(crate) fn map_values<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, a] = *args else {
        return Ok(Reply::Decline);
    };
    let es = take!(lib.assoc_operand(h, t, a, 2));
    let keys: Vec<T<'h>> = es.iter().map(|&e| lib.key(h, e)).collect();
    let vals: Vec<T<'h>> = es.iter().map(|&e| lib.value(h, e)).collect();
    let mapped = map_calls(cx, f, &vals)?;
    entries_of(lib, cx, &keys, &mapped)
}

/// `select_keys(a, f)`: the entries whose key `k` has `f(k)` true, in order; a failure from `f` is the result.
pub(crate) fn select_keys<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, f] = *args else {
        return Ok(Reply::Decline);
    };
    let es = take!(lib.assoc_operand(h, t, a, 1));
    let keys: Vec<T<'h>> = es.iter().map(|&e| lib.key(h, e)).collect();
    let tests = map_calls(cx, f, &keys)?;
    if let Some(&fail) = tests.iter().find(|&&v| lib.is_failure(h, v)) {
        return Ok(Reply::Done(fail));
    }
    let keep: Vec<bool> = tests.iter().map(|&v| cx.is_true(v)).collect();
    Ok(Reply::Done(filtered(lib, cx, a, es, |i, _| keep[i])?))
}
