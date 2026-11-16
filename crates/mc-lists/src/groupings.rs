//! `groupings(xs, spec)`: every way of grouping the elements of `xs`, in order, into a tree of
//! calls whose heads and arities `spec` gives.
//!
//! - `spec`: an arity `k` (head `list`), a list of arities, `h -> k`, `h -> [k1, ...]`, `[h1, h2] -> k`, a list of
//!   such rules; `[k, orderless]` or `h -> [k, orderless]` takes the parts of `h` in one order only (larger parts
//!   first), so trees that differ only in the order of the parts of `h` are counted once. An arity 1 wraps: each tree is followed by itself wrapped in each
//!   one-argument head, and a one-argument head is not applied to its own result.
//! - `xs`: a list, `n` for `range(n)`, or a list of lists of one length (each grouped, every shape applied to each in
//!   turn).
//!   `h: k` written as an option is the rule `h -> k` (a head that is a function name, such as `construct`, whose
//!   calls `construct(f, x)` are `f(x)`); `groupings(xs, spec, w)` gives each tree inside `w` (`quote` keeps them
//!   unevaluated).
//! - `[h -> k, m]` as an element of a list of rules: the head `h` is used at most `m` times in a tree.
//! - **Order** (heads of two or more arguments): trees with fewer calls first; among equal counts, by the heads used
//!   (the indices of the heads of all the calls, ascending, compared as lists: trees of one head first, then those
//!   using the first two heads, ...). Within one multiset of heads the trees are taken by their *canonical form*:
//!   the tree with the parts of every call sorted (more calls first, then by the canonical word), and the canonical
//!   word of a tree is its heads and leaves read in prefix order (head indices; a leaf after every head). Canonical
//!   forms come in the order of their words; each is followed at once by all the trees it stands for: for each
//!   choice of the parts' own trees (the parts in their sorted order, the last part varying fastest, equal parts
//!   taken as a multiset), the distinct orders of the parts (permutations of their positions in lexicographic order).
//!   An orderless head keeps the sorted order only. Each tree is filled with the elements left to right.
//! - With one-argument heads the trees of the heads of two or more arguments come by the number of calls, the root's
//!   head, the sizes of the root's parts (larger first), the shapes of the parts and where the larger parts stand,
//!   each tree followed by its wrappings.
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
    /// The heads of the calls (indices into the specification), ascending: the multiset of heads used.
    fn heads(&self) -> Vec<usize> {
        fn go(s: &Shape, out: &mut Vec<usize>) {
            if let Shape::Node(k, cs) = s {
                out.push(*k);
                for c in cs {
                    go(c, out);
                }
            }
        }
        let mut out = vec![];
        go(self, &mut out);
        out.sort_unstable();
        out
    }
}

/// A head of the specification.
struct Head<'h> {
    head: T<'h>,
    arity: usize,
    orderless: bool,
    /// the most calls of this head in one tree (`[h -> k, m]`)
    limit: Option<usize>,
}

/// The canonical word of a shape: head indices in prefix order, a leaf as `usize::MAX` (after every head).
fn word(s: &Shape, out: &mut Vec<usize>) {
    match s {
        Shape::Leaf => out.push(usize::MAX),
        Shape::Node(k, cs) => {
            out.push(*k);
            for c in cs {
                word(c, out);
            }
        }
    }
}

/// The order of the parts of a canonical tree: more calls first, then by the canonical word.
fn part_key(s: &Shape) -> (std::cmp::Reverse<usize>, Vec<usize>) {
    let mut w = vec![];
    word(s, &mut w);
    (std::cmp::Reverse(s.calls()), w)
}

/// The calls of each head group in `s` (`group[i]` is the group of head `i`: heads with one head term share it).
fn head_counts(s: &Shape, group: &[usize], counts: &mut [usize]) {
    if let Shape::Node(k, cs) = s {
        counts[group[*k]] += 1;
        for c in cs {
            head_counts(c, group, counts);
        }
    }
}

/// Generator of the trees in canonical order (heads of two or more arguments only).
struct Canonical<'a, 'h> {
    heads: &'a [Head<'h>],
    /// the head group of each head (heads with equal head terms share a group, and its limit)
    group: Vec<usize>,
    /// the limit of each group
    limits: Vec<Option<usize>>,
    memo: FxHashMap<usize, Rc<Vec<Rc<Shape>>>>,
    variants: FxHashMap<Rc<Shape>, Rc<Vec<Rc<Shape>>>>,
    budget: usize,
}

impl<'a, 'h> Canonical<'a, 'h> {
    fn new(heads: &'a [Head<'h>]) -> Canonical<'a, 'h> {
        let mut group = Vec::with_capacity(heads.len());
        let mut limits: Vec<Option<usize>> = vec![];
        for (i, h) in heads.iter().enumerate() {
            let g = heads[..i].iter().position(|o| o.head == h.head).map_or(i, |j| group[j]);
            group.push(g);
            if limits.len() <= g {
                limits.resize(g + 1, None);
            }
            if let Some(m) = h.limit {
                limits[g] = Some(limits[g].map_or(m, |o: usize| o.min(m)));
            }
        }
        Canonical { heads, group, limits, memo: FxHashMap::default(), variants: FxHashMap::default(), budget: MAX_SHAPES }
    }

    /// Within the limits of the heads?
    fn allowed(&self, s: &Shape) -> bool {
        if self.limits.iter().all(|l| l.is_none()) {
            return true;
        }
        let mut counts = vec![0usize; self.limits.len()];
        head_counts(s, &self.group, &mut counts);
        counts.iter().zip(&self.limits).all(|(&c, l)| l.is_none_or(|m| c <= m))
    }

    fn spend(&mut self, n: usize) -> Option<()> {
        self.budget = self.budget.checked_sub(n)?;
        Some(())
    }

    /// The canonical trees with `n` leaves (parts sorted), in no particular order; `None` past the budget.
    fn canon(&mut self, n: usize) -> Option<Rc<Vec<Rc<Shape>>>> {
        if let Some(v) = self.memo.get(&n) {
            return Some(v.clone());
        }
        let mut out: Vec<Rc<Shape>> = vec![];
        if n == 1 {
            out.push(Rc::new(Shape::Leaf));
        } else {
            let mut seen = FxHashSet::default();
            for hi in 0..self.heads.len() {
                let k = self.heads[hi].arity;
                if k < 2 || k > n || k > 8 {
                    continue;
                }
                for p in partitions(n, k) {
                    let parts: Vec<Rc<Vec<Rc<Shape>>>> = p.iter().map(|&s| self.canon(s)).collect::<Option<_>>()?;
                    if parts.iter().any(|q| q.is_empty()) {
                        continue;
                    }
                    // a choice per part; parts of one size take non-decreasing indices (a multiset)
                    let mut choice = vec![0usize; k];
                    'all: loop {
                        let valid = (1..k).all(|i| p[i] != p[i - 1] || choice[i] >= choice[i - 1]);
                        if valid {
                            let mut cs: Vec<Rc<Shape>> = choice.iter().zip(&parts).map(|(&c, ps)| ps[c].clone()).collect();
                            cs.sort_by_cached_key(|c| part_key(c));
                            let t = Rc::new(Shape::Node(hi, cs));
                            if self.allowed(&t) && seen.insert(t.clone()) {
                                self.spend(1)?;
                                out.push(t);
                            }
                        }
                        let mut i = k;
                        loop {
                            if i == 0 {
                                break 'all;
                            }
                            i -= 1;
                            choice[i] += 1;
                            if choice[i] < parts[i].len() {
                                break;
                            }
                            choice[i] = 0;
                        }
                    }
                }
            }
        }
        let out = Rc::new(out);
        self.memo.insert(n, out.clone());
        Some(out)
    }

    /// The trees a canonical tree stands for, in order (see the module documentation).
    fn variants(&mut self, t: &Rc<Shape>) -> Option<Rc<Vec<Rc<Shape>>>> {
        if let Some(v) = self.variants.get(t) {
            return Some(v.clone());
        }
        let out = match &**t {
            Shape::Leaf => vec![t.clone()],
            Shape::Node(hi, cs) => {
                // groups of equal parts (adjacent: equal parts sort together)
                let mut groups: Vec<(Rc<Shape>, usize)> = vec![];
                for c in cs {
                    match groups.last_mut() {
                        Some((g, m)) if g == c => *m += 1,
                        _ => groups.push((c.clone(), 1)),
                    }
                }
                // per group: the multisets of its parts' trees, in order
                let mut per: Vec<Vec<Vec<Rc<Shape>>>> = vec![];
                for (c, m) in &groups {
                    let vs = self.variants(c)?;
                    per.push(multisets(&vs, *m));
                }
                let orderless = self.heads[*hi].orderless;
                let mut out = vec![];
                let mut choice = vec![0usize; per.len()];
                'all: loop {
                    let seq: Vec<Rc<Shape>> = choice.iter().zip(&per).flat_map(|(&c, g)| g[c].iter().cloned()).collect();
                    let perms = if orderless { vec![seq] } else { distinct_orders(&seq) };
                    self.spend(perms.len())?;
                    out.extend(perms.into_iter().map(|p| Rc::new(Shape::Node(*hi, p))));
                    let mut i = per.len();
                    loop {
                        if i == 0 {
                            break 'all;
                        }
                        i -= 1;
                        choice[i] += 1;
                        if choice[i] < per[i].len() {
                            break;
                        }
                        choice[i] = 0;
                    }
                }
                out
            }
        };
        let out = Rc::new(out);
        self.variants.insert(t.clone(), out.clone());
        Some(out)
    }

    /// Every tree with `n` leaves, in order; `None` past the budget.
    fn all(&mut self, n: usize) -> Option<Vec<Rc<Shape>>> {
        let mut cs: Vec<Rc<Shape>> = self.canon(n)?.to_vec();
        cs.sort_by_cached_key(|t| {
            let mut w = vec![];
            word(t, &mut w);
            (t.calls(), t.heads(), w)
        });
        let mut out = vec![];
        for t in &cs {
            out.extend(self.variants(t)?.iter().cloned());
        }
        Some(out)
    }
}

/// The multisets of `m` elements of `vs` (indices non-decreasing), the last varying fastest.
fn multisets(vs: &[Rc<Shape>], m: usize) -> Vec<Vec<Rc<Shape>>> {
    let mut out = vec![];
    let mut idx = vec![0usize; m];
    if vs.is_empty() {
        return out;
    }
    loop {
        out.push(idx.iter().map(|&i| vs[i].clone()).collect());
        // next non-decreasing index tuple
        let Some(i) = (0..m).rev().find(|&i| idx[i] + 1 < vs.len()) else { break };
        idx[i] += 1;
        let v = idx[i];
        for j in idx.iter_mut().skip(i + 1) {
            *j = v;
        }
    }
    out
}

/// The distinct orders of `cs`: permutations of the positions in lexicographic order, repeats dropped.
fn distinct_orders(cs: &[Rc<Shape>]) -> Vec<Vec<Rc<Shape>>> {
    let k = cs.len();
    let mut idx: Vec<usize> = (0..k).collect();
    let mut seen = FxHashSet::default();
    let mut out = vec![];
    loop {
        let v: Vec<Rc<Shape>> = idx.iter().map(|&i| cs[i].clone()).collect();
        if seen.insert(v.clone()) {
            out.push(v);
        }
        let Some(i) = (0..k.saturating_sub(1)).rev().find(|&i| idx[i] < idx[i + 1]) else { break };
        let j = (i + 1..k).rev().find(|&j| idx[j] > idx[i]).unwrap_or(i + 1);
        idx.swap(i, j);
        idx[i + 1..].reverse();
    }
    out
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
        // stable: fewer calls first, then by the heads used (as an ascending list of their indices, compared
        // lexicographically), the order of generation within equal keys
        base.sort_by_cached_key(|t| (t.calls(), t.heads()));
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
        Some(hs.iter().flat_map(|&hd| ars.iter().map(move |&(arity, orderless)| Head { head: hd, arity, orderless, limit: None })).collect())
    };
    if let Some(hs) = rule_heads(spec) {
        return Some(hs);
    }
    if let Some(ars) = arities(spec) {
        return Some(ars.into_iter().map(|(arity, orderless)| Head { head: list, arity, orderless, limit: None }).collect());
    }
    let xs = cx.as_list(spec)?;
    let mut out = vec![];
    for &x in xs {
        if let Some(hs) = rule_heads(x) {
            out.extend(hs);
            continue;
        }
        // [h -> k, m]: at most m calls of h
        let &[r, m] = cx.as_list(x)? else { return None };
        let View::Int(m) = h.view(m) else { return None };
        let m = usize::try_from(m).ok()?;
        out.extend(rule_heads(r)?.into_iter().map(|hd| Head { limit: Some(m), ..hd }));
    }
    Some(out)
}

/// `groupings(xs, spec)`.
pub(crate) fn groupings<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, spec, wrap) = match *args {
        [xs, spec] => (xs, spec, None),
        [xs, spec, w] => (xs, spec, Some(w)),
        _ => return Ok(Reply::Decline),
    };
    // `h: k` written as an option (a head that is a function name): the rule h -> k
    let spec = match h.view(spec) {
        View::App { head, args: &[r] } if head == h.sym_term(h.sym("options")) => r,
        _ => spec,
    };
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
    let (all, spent) = if heads.iter().any(|hd| hd.arity == 1) {
        let mut budget = MAX_SHAPES;
        let mut memo = FxHashMap::default();
        let Some(all) = shapes(n, &heads, &mut memo, &mut budget) else { return Ok(Reply::Done(lib.too_large(h, t))) };
        // the limits of `[h -> k, m]`
        let limits = Canonical::new(&heads);
        let all: Vec<Rc<Shape>> = all.iter().filter(|s| limits.allowed(s)).cloned().collect();
        (all, MAX_SHAPES - budget)
    } else {
        let mut gen = Canonical::new(&heads);
        let Some(all) = gen.all(n) else { return Ok(Reply::Done(lib.too_large(h, t))) };
        (all, MAX_SHAPES - gen.budget)
    };
    if all.len().saturating_mul(inputs.len()) > MAX_SHAPES {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    cx.tick(spent as u64 + all.len() as u64)?;
    let mut out = Vec::with_capacity(all.len() * inputs.len());
    for s in all.iter() {
        for xs in &inputs {
            let mut i = 0;
            let tree = fill(h, &heads, s, xs, &mut i);
            // groupings(xs, spec, w): each tree inside w (quote keeps it unevaluated)
            out.push(wrap.map_or(tree, |w| h.app(w, &[tree])));
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
