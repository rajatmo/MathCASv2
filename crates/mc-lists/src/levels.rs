//! Level specifications and the walks over the parts of a term at given levels: `apply(f, x, levels)`,
//! `map_indexed(f, x, levels)`, `count(x, p, levels)` and `map(f, x, levels)` (decision p6core.9).
//!
//! A part at position `[i1, ..., ik]` is at level `k` (the term itself at level 0); its negative level is minus its
//! depth (atoms -1). A specification is `n` (levels 1 to n), `inf` (1 and below), `[n]` (level n only), `[m, n]`
//! (m to n); a negative bound compares with the negative level. Heads are not parts. Complex numbers are atoms.
use crate::ctx::Lib;
use crate::measure::is_complex_number;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::Arc;

/// The deepest term the walks recurse into (deeper terms leave the call unevaluated).
const MAX_DEPTH: usize = 4096;

/// A level range `lo..=hi`; `i64::MAX` for no upper bound.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Levels {
    lo: i64,
    hi: i64,
}

impl Levels {
    /// Level `n` only.
    pub(crate) fn only(n: i64) -> Levels {
        Levels { lo: n, hi: n }
    }

    /// Every level, the term itself included.
    pub(crate) fn every() -> Levels {
        Levels { lo: 0, hi: i64::MAX }
    }

    /// Is a part at level `level` inside, when both bounds are levels from the top (`None` for negative bounds)?
    pub(crate) fn contains_level(self, level: usize) -> Option<bool> {
        (self.lo >= 0 && self.hi >= 0).then(|| (self.lo..=self.hi).contains(&(level as i64)))
    }

    /// Is a part at level `level` with depth `depth` inside?
    fn contains(self, level: usize, depth: usize) -> bool {
        let (pos, neg) = (level as i64, -(depth as i64));
        let ok = |b: i64, lower: bool| {
            let v = if b >= 0 { pos } else { neg };
            if lower {
                v >= b
            } else {
                v <= b
            }
        };
        ok(self.lo, true) && ok(self.hi, false)
    }
}

/// The level specification `t`, if it is one.
pub(crate) fn level_spec(h: &Heap, t: T<'_>) -> Option<Levels> {
    let bound = |t: T<'_>| match h.view(t) {
        View::Int(n) => Some(n),
        View::Sym(s) if h.sym_name(s) == "inf" => Some(i64::MAX),
        _ => None,
    };
    match h.view(t) {
        View::Int(_) | View::Sym(_) => Some(Levels { lo: 1, hi: bound(t)? }),
        View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "list") => match *args {
            [n] => {
                let n = bound(n)?;
                Some(Levels { lo: n, hi: n })
            }
            [m, n] => Some(Levels { lo: bound(m)?, hi: bound(n)? }),
            _ => None,
        },
        _ => None,
    }
}

/// The parts of `t` at `levels` with heads counted as parts (`level(x, spec, heads: true)`): each head is a part
/// one level below its call, visited before the arguments; the walk is depth first, a part after its own parts.
/// The values of an association are its parts (its keys are not). Returns the depth of `t` (heads do not add
/// depth), or `None` past [`MAX_DEPTH`].
pub(crate) fn with_heads<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    level: usize,
    levels: Levels,
    out: &mut Vec<T<'h>>,
) -> Res<Option<usize>> {
    let h = cx.heap();
    if level > MAX_DEPTH {
        return Ok(None);
    }
    cx.tick(1)?;
    let depth = match parts(h, t) {
        None => 1,
        Some((head, args)) => {
            let values: Vec<T<'h>> = match assoc_entries(h, t) {
                Some(es) => es.into_iter().map(|(_, v)| v).collect(),
                None => args.to_vec(),
            };
            if with_heads(cx, head, level + 1, levels, out)?.is_none() {
                return Ok(None);
            }
            let mut d = 0;
            for a in values {
                match with_heads(cx, a, level + 1, levels, out)? {
                    Some(k) => d = d.max(k),
                    None => return Ok(None),
                }
            }
            d + 1
        }
    };
    if levels.contains(level, depth) {
        out.push(t);
    }
    Ok(Some(depth))
}

/// The marker a visit returns to delete a part: the walk drops it from the arguments of its parent.
pub(crate) fn gone(h: &Heap) -> T<'_> {
    h.sym_term(h.sym("levels·gone"))
}

/// The parts of a term for a walk: the arguments of a compound term other than a complex number.
fn parts<'h>(h: &'h Heap, t: T<'h>) -> Option<(T<'h>, &'h [T<'h>])> {
    match h.view(t) {
        View::App { .. } if is_complex_number(h, t) => None,
        View::App { head, args } => Some((head, args)),
        _ => None,
    }
}

/// The entries `(k, v)` of an association in normal form.
pub(crate) fn assoc_entries<'h>(h: &'h Heap, t: T<'h>) -> Option<Vec<(T<'h>, T<'h>)>> {
    if h.packed(t).is_some() {
        return None;
    }
    let View::App { head, args } = h.view(t) else { return None };
    if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "assoc") {
        return None;
    }
    args.iter()
        .map(|&r| match h.view(r) {
            View::App { head, args: &[k, v] }
                if matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "rule" | "rule_delayed")) =>
            {
                Some((k, v))
            }
            _ => None,
        })
        .collect()
}

/// The entries of `t` when it is an association in normal form (the heads given).
fn entries_of<'h>(h: &'h Heap, t: T<'h>, heads: AssocHeads<'h>) -> Option<Vec<(T<'h>, T<'h>)>> {
    match h.view(t) {
        View::App { head, args } if head == heads.assoc && h.packed(t).is_none() => args
            .iter()
            .map(|&r| match h.view(r) {
                View::App { head, args: &[k, v] } if head == heads.rule || head == heads.rule_delayed => Some((k, v)),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// Entry `i` of the association `t` (in normal form).
fn es_rule<'h>(h: &'h Heap, t: T<'h>, i: usize) -> T<'h> {
    match h.view(t) {
        View::App { args, .. } => args[i],
        _ => t,
    }
}

/// A visit of a part at a level: its (rebuilt) term and position; `Some` replaces it.
pub(crate) type Visit<'v, 'a, 'b, 'h> = dyn FnMut(&mut NativeCx<'a, 'b, 'h>, T<'h>, &[i64]) -> Res<Option<T<'h>>> + 'v;

/// Rebuild `t` bottom-up, visiting every part inside `levels` (after its own parts); returns the new term and its
/// depth, or `None` when the term is too deep.
pub(crate) fn walk<'a, 'b, 'h>(
    cx: &mut NativeCx<'a, 'b, 'h>,
    t: T<'h>,
    levels: Levels,
    visit: &mut Visit<'_, 'a, 'b, 'h>,
) -> Res<Option<T<'h>>> {
    let mut pos = vec![];
    let h = cx.heap();
    let heads = AssocHeads {
        assoc: h.sym_term(h.sym("assoc")),
        rule: h.sym_term(h.sym("rule")),
        rule_delayed: h.sym_term(h.sym("rule_delayed")),
    };
    Ok(walk_at(cx, t, levels, visit, &mut pos, heads)?.map(|(v, _)| v))
}

/// The heads of associations and their entries, for a walk.
#[derive(Clone, Copy)]
struct AssocHeads<'h> {
    assoc: T<'h>,
    rule: T<'h>,
    rule_delayed: T<'h>,
}

fn walk_at<'a, 'b, 'h>(
    cx: &mut NativeCx<'a, 'b, 'h>,
    t: T<'h>,
    levels: Levels,
    visit: &mut Visit<'_, 'a, 'b, 'h>,
    pos: &mut Vec<i64>,
    heads: AssocHeads<'h>,
) -> Res<Option<(T<'h>, usize)>> {
    let h = cx.heap();
    if pos.len() > MAX_DEPTH {
        return Ok(None);
    }
    cx.tick(1)?;
    // an association's parts are its values: the keys are not walked (decision io-core.4)
    if let Some(es) = entries_of(h, t, heads) {
        let mut out = Vec::with_capacity(es.len());
        let mut depth = 0;
        let g = gone(h);
        for (i, &(k, v)) in es.iter().enumerate() {
            pos.push(i as i64 + 1);
            let r = walk_at(cx, v, levels, visit, pos, heads)?;
            pos.pop();
            let Some((v2, d)) = r else { return Ok(None) };
            depth = depth.max(d);
            if v2 != g {
                out.push(if v2 == v {
                    es_rule(h, t, i)
                } else {
                    // the entry keeps its kind of rule (`k :> v` stays delayed)
                    let head = match h.view(es_rule(h, t, i)) {
                        View::App { head, .. } => head,
                        _ => heads.rule,
                    };
                    h.app(head, &[k, v2])
                });
            }
        }
        let u = h.app(heads.assoc, &out);
        let u = if u == t { t } else { u };
        let depth = depth + 1;
        let u = if levels.contains(pos.len(), depth) { visit(cx, u, pos)?.unwrap_or(u) } else { u };
        return Ok(Some((u, depth)));
    }
    let (u, depth) = match parts(h, t) {
        None => (t, 1),
        Some((head, args)) => {
            let mut out = Vec::with_capacity(args.len());
            let mut depth = 0;
            for (i, &a) in args.iter().enumerate() {
                pos.push(i as i64 + 1);
                let r = walk_at(cx, a, levels, visit, pos, heads)?;
                pos.pop();
                let Some((v, d)) = r else { return Ok(None) };
                depth = depth.max(d);
                out.push(v);
            }
            let g = gone(h);
            if out.contains(&g) {
                out.retain(|&a| a != g);
            }
            let u = if out.as_slice() == args { t } else { h.app(head, &out) };
            (u, depth + 1)
        }
    };
    let u = if levels.contains(pos.len(), depth) { visit(cx, u, pos)?.unwrap_or(u) } else { u };
    Ok(Some((u, depth)))
}

/// The position list of `pos` in `x`, where a step into an association's value is its key, `key(k)` (the keys are
/// the positions of an association's values).
fn keyed_position<'h>(cx: &NativeCx<'_, '_, 'h>, x: T<'h>, pos: &[i64]) -> T<'h> {
    let h = cx.heap();
    let key = h.sym("key");
    let mut cur = x;
    let mut v: Vec<T<'h>> = Vec::with_capacity(pos.len());
    for &i in pos {
        let step = usize::try_from(i - 1).ok();
        if let (Some(es), Some(j)) = (assoc_entries(h, cur), step) {
            let Some(&(k, val)) = es.get(j) else { v.push(h.int(i)); continue };
            v.push(h.call(key, &[k]));
            cur = val;
        } else if let (View::App { args, .. }, Some(j)) = (h.view(cur), step) {
            v.push(h.int(i));
            if let Some(&a) = args.get(j) {
                cur = a;
            }
        } else {
            v.push(h.int(i));
        }
    }
    cx.list(&v)
}

/// `apply(f, x, levels)`: the heads of the parts of `x` at `levels` replaced by `f`.
pub(crate) fn apply_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, levels: Levels) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut visit = |_: &mut NativeCx<'_, '_, 'h>, u: T<'h>, _: &[i64]| -> Res<Option<T<'h>>> {
        // an association's arguments are its values
        if let Some(es) = assoc_entries(h, u) {
            let vs: Vec<T<'h>> = es.iter().map(|&(_, v)| v).collect();
            return Ok(Some(h.app(f, &vs)));
        }
        Ok(parts(h, u).map(|(_, args)| h.app(f, args)))
    };
    Ok(match walk(cx, x, levels, &mut visit)? {
        Some(v) => Reply::Again(v),
        None => Reply::Decline,
    })
}

/// `map_indexed(f, x, levels)`: each part `p` of `x` at `levels` replaced by `f(p, position)`.
pub(crate) fn map_indexed_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, levels: Levels) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut visit = |cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>, pos: &[i64]| -> Res<Option<T<'h>>> {
        Ok(Some(h.app(f, &[u, keyed_position(cx, x, pos)])))
    };
    Ok(match walk(cx, x, levels, &mut visit)? {
        Some(v) => Reply::Again(v),
        None => Reply::Decline,
    })
}

/// `map(f, x, levels)`: each part `p` of `x` at `levels` replaced by `f(p)`.
pub(crate) fn map_at_levels<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, levels: Levels) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut visit = |_: &mut NativeCx<'_, '_, 'h>, u: T<'h>, _: &[i64]| -> Res<Option<T<'h>>> { Ok(Some(h.app(f, &[u]))) };
    Ok(match walk(cx, x, levels, &mut visit)? {
        Some(v) => Reply::Again(v),
        None => Reply::Decline,
    })
}

/// `map(f, x, levels, heads: true)`: as `map(f, x, levels)`, the heads of the compound parts included (each head is
/// the part at position 0 of its call, one level below the call).
fn map_with_heads<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, levels: Levels) -> Res<Reply<'h>> {
    fn go<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, t: T<'h>, levels: Levels, level: usize) -> Res<Option<(T<'h>, usize)>> {
        let h = cx.heap();
        if level > MAX_DEPTH {
            return Ok(None);
        }
        cx.tick(1)?;
        let split = parts(h, t);
        let (u, depth) = match split {
            None => (t, 1),
            Some((head, args)) => {
                let Some((nh, _)) = go(cx, f, head, levels, level + 1)? else { return Ok(None) };
                let mut out = Vec::with_capacity(args.len());
                let mut depth = 0;
                for &a in args {
                    let Some((v, d)) = go(cx, f, a, levels, level + 1)? else { return Ok(None) };
                    depth = depth.max(d);
                    out.push(v);
                }
                (h.app(nh, &out), depth + 1)
            }
        };
        let u = if levels.contains(level, depth) { h.app(f, &[u]) } else { u };
        Ok(Some((u, depth)))
    }
    Ok(match go(cx, f, x, levels, 0)? {
        Some((v, _)) => Reply::Again(v),
        None => Reply::Decline,
    })
}

/// `map_indexed(f, x, levels, heads: true)`: each part `p` at `levels`, the heads of compound parts included (a
/// head is at position 0 of its call, one level below it), replaced by `f(p, position)` (round 37).
pub(crate) fn map_indexed_with_heads<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, levels: Levels) -> Res<Reply<'h>> {
    fn go<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, t: T<'h>, levels: Levels, pos: &mut Vec<i64>) -> Res<Option<(T<'h>, usize)>> {
        let h = cx.heap();
        if pos.len() > MAX_DEPTH {
            return Ok(None);
        }
        cx.tick(1)?;
        let (u, depth) = match parts(h, t) {
            None => (t, 1),
            Some((head, args)) => {
                pos.push(0);
                let r = go(cx, f, head, levels, pos)?;
                pos.pop();
                let Some((nh, _)) = r else { return Ok(None) };
                let mut out = Vec::with_capacity(args.len());
                let mut depth = 0;
                for (i, &a) in args.iter().enumerate() {
                    pos.push(i as i64 + 1);
                    let r = go(cx, f, a, levels, pos)?;
                    pos.pop();
                    let Some((v, d)) = r else { return Ok(None) };
                    depth = depth.max(d);
                    out.push(v);
                }
                (h.app(nh, &out), depth + 1)
            }
        };
        let u = if levels.contains(pos.len(), depth) {
            let p: Vec<T<'h>> = pos.iter().map(|&k| h.int(k)).collect();
            h.app(f, &[u, h.call(h.sym("list"), &p)])
        } else {
            u
        };
        Ok(Some((u, depth)))
    }
    Ok(match go(cx, f, x, levels, &mut vec![])? {
        Some((v, _)) => Reply::Again(v),
        None => Reply::Decline,
    })
}

/// `apply(f, x, levels, heads: true)`: as `apply(f, x, levels)`, the heads of the compound parts included (each
/// head is the part at position 0 of its call, one level below the call); a compound part at `levels` gets the head
/// `f`, atoms stay.
pub(crate) fn apply_with_heads<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, levels: Levels) -> Res<Reply<'h>> {
    fn go<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, t: T<'h>, levels: Levels, level: usize) -> Res<Option<(T<'h>, usize)>> {
        let h = cx.heap();
        if level > MAX_DEPTH {
            return Ok(None);
        }
        cx.tick(1)?;
        Ok(Some(match parts(h, t) {
            None => (t, 1),
            Some((head, args)) => {
                let Some((nh, _)) = go(cx, f, head, levels, level + 1)? else { return Ok(None) };
                let mut out = Vec::with_capacity(args.len());
                let mut depth = 0;
                for &a in args {
                    let Some((v, d)) = go(cx, f, a, levels, level + 1)? else { return Ok(None) };
                    depth = depth.max(d);
                    out.push(v);
                }
                let depth = depth + 1;
                (h.app(if levels.contains(level, depth) { f } else { nh }, &out), depth)
            }
        }))
    }
    Ok(match go(cx, f, x, levels, 0)? {
        Some((v, _)) => Reply::Again(v),
        None => Reply::Decline,
    })
}

/// `map` beyond lists (decision p6core.9): `map(f, x, levels)`; `map(f, assoc(k -> v, ...))` maps the values;
/// `map(f, g(a, b))` is `g(f(a), f(b))` for any compound term. Lists go to the evaluator's own `map`.
pub(crate) struct MapMore {
    pub inner: Box<dyn Native>,
    pub lib: Arc<Lib>,
}

impl Native for MapMore {
    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        // `map(f, xs, parallel: true)` (decision sp-core.4)
        if let Some((rest, parallel)) = crate::parallel::option(cx, args)? {
            return match (rest, parallel) {
                (&[f, x], true) => crate::parallel::map_parallel(cx, f, x),
                _ => Ok(Reply::Again(h.app(h.sym_term(h.sym("map")), rest))),
            };
        }
        // `map(f, x, heads: true)` and `map(f, x, levels, heads: true)`
        let (rest, heads) = crate::ctx::option(h, args, 2, "heads");
        if let Some(v) = heads {
            let on = matches!(h.view(v), View::Sym(s) if h.sym_name(s) == "true");
            let off = matches!(h.view(v), View::Sym(s) if h.sym_name(s) == "false");
            return match (rest, on) {
                (&[f, x], true) => map_with_heads(cx, f, x, Levels::only(1)),
                (&[f, x, spec], true) => match level_spec(h, spec) {
                    Some(levels) => map_with_heads(cx, f, x, levels),
                    None => Ok(Reply::Decline),
                },
                (_, false) if off && !rest.is_empty() => Ok(Reply::Again(h.app(h.sym_term(h.sym("map")), rest))),
                _ => Ok(Reply::Decline),
            };
        }
        match *args {
            [f, x, spec] => match level_spec(h, spec) {
                Some(levels) => map_at_levels(cx, f, x, levels),
                None => Ok(Reply::Decline),
            },
            // a dataset maps over its rows (decision io-core.17)
            [f, x] if self.lib.is_dataset(h, x) => {
                let rows = h.app(h.sym_term(h.sym("rows")), &[x]);
                Ok(Reply::Again(h.app(h.sym_term(h.sym("map")), &[f, rows])))
            }
            [f, x] if cx.as_list(x).is_none() && h.packed(x).is_none() => {
                let Some((head, xs)) = parts(h, x) else { return self.inner.call(cx, t, args) };
                let (assoc, rule) = (h.sym_term(h.sym("assoc")), h.sym_term(self.lib.s.rule));
                let out: Vec<T<'h>> = if head == assoc {
                    xs.iter()
                        .map(|&r| match h.view(r) {
                            View::App { head: g, args: [k, v] } if g == rule => h.app(rule, &[*k, h.app(f, &[*v])]),
                            _ => h.app(f, &[r]),
                        })
                        .collect()
                } else {
                    xs.iter().map(|&a| h.app(f, &[a])).collect()
                };
                cx.tick(out.len() as u64)?;
                Ok(Reply::Again(h.app(head, &out)))
            }
            _ => self.inner.call(cx, t, args),
        }
    }
}
