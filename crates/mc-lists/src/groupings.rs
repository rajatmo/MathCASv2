//! `groupings(xs, spec)` (round 17): every way of grouping the elements of `xs`, in order, into a tree of
//! calls whose heads and arities `spec` gives.
//!
//! - `spec`: an arity `k` (head `list`), a list of arities, `h -> k`, `h -> [k1, ...]`, `[h1, h2] -> k`, a list of
//!   such rules; `[k, orderless]` or `h -> [k, orderless]` takes the parts of `h` in one order only (larger parts
//!   first), so trees that differ only in the order of the parts of `h` are counted once. An arity 1 wraps: each tree is followed by itself wrapped in each
//!   one-argument head, and a one-argument head is not applied to its own result.
//! - `xs`: a list, `n` for `range(n)`, or a list of lists of one length (each grouped, every shape applied to each in
//!   turn).
//! - **Order**: trees with fewer calls first; among equal counts,
//!   by the root's head in the order given, then by the sizes of the root's parts (partitions of the number of
//!   leaves into arity parts, larger parts first), then by the shapes of the parts (each in this same order), then by
//!   where the larger parts stand (from the left). Each shape is filled with the elements left to right.
//! - The trees are counted by the super-Catalan numbers (Schroeder's problems, OEIS A001003 for arities 2 and up),
//!   so the number of shapes is capped ([`MAX_SHAPES`]); past it the call gives `failure(too_large, groupings)`.
use crate::ctx::Lib;
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

/// The most shapes (trees without their elements) one call builds.
pub const MAX_SHAPES: usize = 200_000;

/// A tree shape: a leaf, or a head (index into the heads) applied to parts.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Shape {
    Leaf,
    Node(usize, Vec<Rc<Shape>>),
}

impl Shape {
    fn leaves(&self) -> usize {
        match self {
            Shape::Leaf => 1,
            Shape::Node(_, cs) => cs.iter().map(|c| c.leaves()).sum(),
        }
    }
    fn calls(&self) -> usize {
        match self {
            Shape::Leaf => 0,
            Shape::Node(_, cs) => 1 + cs.iter().map(|c| c.calls()).sum::<usize>(),
        }
    }
}

/// A head of the specification.
struct Head<'h> {
    head: T<'h>,
    arity: usize,
    orderless: bool,
}

/// The partitions of `n` into `k` positive parts, each non-increasing, in decreasing lexicographic order.
fn partitions(n: usize, k: usize) -> Vec<Vec<usize>> {
    fn go(n: usize, k: usize, max: usize, cur: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if k == 0 {
            if n == 0 {
                out.push(cur.clone());
            }
            return;
        }
        // at least one per remaining part
        let hi = max.min(n.saturating_sub(k - 1));
        for p in (1..=hi).rev() {
            cur.push(p);
            go(n - p, k - 1, p, cur, out);
            cur.pop();
        }
    }
    let mut out = vec![];
    go(n, k, n, &mut vec![], &mut out);
    out
}

/// The distinct orders of `cs`: permutations in lexicographic order of positions, without repeats, then stably by
/// the sizes of the parts, larger first from the left.
fn arrangements(cs: &[Rc<Shape>]) -> Vec<Vec<Rc<Shape>>> {
    let k = cs.len();
    let mut idx: Vec<usize> = (0..k).collect();
    let mut seen = FxHashSet::default();
    let mut out = vec![];
    loop {
        let v: Vec<Rc<Shape>> = idx.iter().map(|&i| cs[i].clone()).collect();
        if seen.insert(v.clone()) {
            out.push(v);
        }
        // next permutation
        let Some(i) = (0..k.saturating_sub(1)).rev().find(|&i| idx[i] < idx[i + 1]) else { break };
        let j = (i + 1..k).rev().find(|&j| idx[j] > idx[i]).unwrap_or(i + 1);
        idx.swap(i, j);
        idx[i + 1..].reverse();
    }
    out.sort_by(|a, b| {
        let sa: Vec<usize> = a.iter().map(|c| c.leaves()).collect();
        let sb: Vec<usize> = b.iter().map(|c| c.leaves()).collect();
        sb.cmp(&sa)
    });
    out
}

/// The shapes with `n` leaves, in the order of the module documentation; `None` past [`MAX_SHAPES`].
fn shapes(n: usize, heads: &[Head<'_>], memo: &mut FxHashMap<usize, Rc<Vec<Rc<Shape>>>>, budget: &mut usize) -> Option<Rc<Vec<Rc<Shape>>>> {
    if let Some(v) = memo.get(&n) {
        return Some(v.clone());
    }
    let mut base: Vec<Rc<Shape>> = vec![];
    if n == 1 {
        base.push(Rc::new(Shape::Leaf));
    } else {
        for (hi, h) in heads.iter().enumerate() {
            if h.arity < 2 || h.arity > n || h.arity > 8 {
                continue;
            }
            for p in partitions(n, h.arity) {
                let mut seen = FxHashSet::default();
                let parts: Vec<Rc<Vec<Rc<Shape>>>> = p.iter().map(|&s| shapes(s, heads, memo, budget)).collect::<Option<_>>()?;
                // a part size no tree has (2 leaves with arity 3 only)
                if parts.iter().any(|p| p.is_empty()) {
                    continue;
                }
                // every choice of a shape for each part, the last part varying fastest
                let mut choice = vec![0usize; parts.len()];
                loop {
                    let cs: Vec<Rc<Shape>> = choice.iter().zip(&parts).map(|(&c, ps)| ps[c].clone()).collect();
                    // an orderless head takes its parts in one order only: larger parts first
                    let arr = arrangements(&cs);
                    let take = if h.orderless { 1 } else { arr.len() };
                    for a in arr.into_iter().take(take) {
                        let t = Rc::new(Shape::Node(hi, a));
                        if seen.insert(t.clone()) {
                            *budget = budget.checked_sub(1)?;
                            base.push(t);
                        }
                    }
                    let mut i = parts.len();
                    loop {
                        if i == 0 {
                            break;
                        }
                        i -= 1;
                        choice[i] += 1;
                        if choice[i] < parts[i].len() {
                            break;
                        }
                        choice[i] = 0;
                        if i == 0 {
                            i = usize::MAX;
                            break;
                        }
                    }
                    if i == usize::MAX || parts.is_empty() {
                        break;
                    }
                }
            }
        }
        base.sort_by_key(|t| t.calls());
    }
    // one-argument heads wrap each tree, right after it (not their own results)
    let mut out = Vec::with_capacity(base.len());
    for t in base {
        out.push(t.clone());
        for (ui, u) in heads.iter().enumerate() {
            if u.arity == 1 {
                *budget = budget.checked_sub(1)?;
                out.push(Rc::new(Shape::Node(ui, vec![t.clone()])));
            }
        }
    }
    let out = Rc::new(out);
    memo.insert(n, out.clone());
    Some(out)
}

/// The tree of `s` with the elements `xs` from `*i` on.
fn fill<'h>(h: &'h Heap, heads: &[Head<'h>], s: &Shape, xs: &[T<'h>], i: &mut usize) -> T<'h> {
    match s {
        Shape::Leaf => {
            *i += 1;
            xs[*i - 1]
        }
        Shape::Node(k, cs) => {
            let args: Vec<T<'h>> = cs.iter().map(|c| fill(h, heads, c, xs, i)).collect();
            h.app(heads[*k].head, &args)
        }
    }
}

/// The heads of a specification (see the module documentation).
fn spec_heads<'h>(cx: &NativeCx<'_, '_, 'h>, spec: T<'h>) -> Option<Vec<Head<'h>>> {
    let h = cx.heap();
    let list = h.sym_term(h.sym("list"));
    let rule = h.sym_term(h.sym("rule"));
    let orderless = h.sym_term(h.sym("orderless"));
    let int = |t: T<'h>| match h.view(t) {
        View::Int(k) if (1..=64).contains(&k) => Some(k as usize),
        _ => None,
    };
    // arities: k, [k1, k2, ...], [k, orderless]
    let arities = |t: T<'h>| -> Option<Vec<(usize, bool)>> {
        if let Some(k) = int(t) {
            return Some(vec![(k, false)]);
        }
        let xs = cx.as_list(t)?;
        match xs {
            &[k, o] if o == orderless => Some(vec![(int(k)?, true)]),
            _ => xs.iter().map(|&k| int(k).map(|k| (k, false))).collect(),
        }
    };
    let rule_heads = |t: T<'h>| -> Option<Vec<Head<'h>>> {
        let View::App { head, args: &[l, r] } = h.view(t) else { return None };
        if head != rule {
            return None;
        }
        let hs: Vec<T<'h>> = match cx.as_list(l) {
            Some(hs) => hs.to_vec(),
            None => vec![l],
        };
        let ars = arities(r)?;
        Some(hs.iter().flat_map(|&hd| ars.iter().map(move |&(arity, orderless)| Head { head: hd, arity, orderless })).collect())
    };
    if let Some(hs) = rule_heads(spec) {
        return Some(hs);
    }
    if let Some(ars) = arities(spec) {
        return Some(ars.into_iter().map(|(arity, orderless)| Head { head: list, arity, orderless }).collect());
    }
    let xs = cx.as_list(spec)?;
    let mut out = vec![];
    for &x in xs {
        out.extend(rule_heads(x)?);
    }
    Some(out)
}

/// `groupings(xs, spec)`.
pub(crate) fn groupings<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, spec] = *args else { return Ok(Reply::Decline) };
    let Some(heads) = spec_heads(cx, spec) else { return Ok(Reply::Decline) };
    // the inputs: range(n), a list, or a list of lists of one length
    let inputs: Vec<Vec<T<'h>>> = match (h.view(xs), cx.as_list(xs)) {
        (View::Int(n), _) if (0..=64).contains(&n) => vec![(1..=n).map(|i| h.int(i)).collect()],
        (_, Some(items)) => {
            let rows: Option<Vec<&[T<'h>]>> = items.iter().map(|&r| cx.as_list(r)).collect();
            match rows {
                Some(rows) if !rows.is_empty() && rows.iter().all(|r| r.len() == rows[0].len()) => rows.iter().map(|r| r.to_vec()).collect(),
                _ => vec![items.to_vec()],
            }
        }
        _ => return Ok(Reply::Decline),
    };
    let n = inputs[0].len();
    if n == 0 {
        return Ok(Reply::Done(cx.list(&[])));
    }
    let mut budget = MAX_SHAPES;
    let mut memo = FxHashMap::default();
    let Some(all) = shapes(n, &heads, &mut memo, &mut budget) else { return Ok(Reply::Done(lib.too_large(h, t))) };
    if all.len().saturating_mul(inputs.len()) > MAX_SHAPES {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    cx.tick((MAX_SHAPES - budget) as u64 + all.len() as u64)?;
    let mut out = Vec::with_capacity(all.len() * inputs.len());
    for s in all.iter() {
        for xs in &inputs {
            let mut i = 0;
            out.push(fill(h, &heads, s, xs, &mut i));
        }
    }
    Ok(Reply::Again(cx.list(&out)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partitions_in_decreasing_order() {
        assert_eq!(partitions(4, 2), vec![vec![3, 1], vec![2, 2]]);
        assert_eq!(partitions(5, 3), vec![vec![3, 1, 1], vec![2, 2, 1]]);
        assert_eq!(partitions(2, 3), Vec::<Vec<usize>>::new());
    }
}
