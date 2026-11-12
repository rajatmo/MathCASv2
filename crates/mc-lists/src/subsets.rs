//! Matching list patterns against sub-multisets of a list: `subset_cases`,
//! `subset_position`, `subset_replace`.
//!
//! A list pattern `[p1, ..., pk]` matches a *subset* of the elements of `xs` taken in any order: positions
//! `i1, ..., ik` (distinct) such that `[xs[i1], ..., xs[ik]]` matches the pattern. Position tuples are enumerated
//! depth first, each slot taking the free positions in increasing order, so the first tuple found for a subset is the
//! least one in the lexicographic order of tuples, and a subset already found (as a set of positions) is not found
//! again in another order. Patterns with runs (`__`, `repeated`) take subsets of every size they allow, the largest
//! first. Without overlaps the positions of a match are used up (a greedy cover, scanning in the same order); with
//! alternatives `[a] | [__]` each alternative is tried in turn on what is left.
//!
//! Each slot whose pattern stands for exactly one element is tested on its own first (a necessary condition of the
//! whole match), which prunes the search; the whole list is then matched by the evaluator's matcher, so conditions
//! and variables shared between slots mean what they mean in rules. Every candidate charges fuel; at most
//! [`MAX_TESTS`] candidates are tried per call, after which the call stays unevaluated.
use crate::ctx::{take, Lib};
use crate::matching::Matcher;
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Pat, Reply, Res};
use rustc_hash::FxHashSet;

/// The most candidate tuples one call tests.
const MAX_TESTS: u64 = 2_000_000;

/// One alternative of the pattern: the whole-list matcher, the per-slot matchers of single-element slots, the
/// least and greatest number of elements it takes, and the right side of its rule.
struct Alt<'l, 'h> {
    whole: Matcher<'l, 'h>,
    slots: Vec<Option<Matcher<'l, 'h>>>,
    min: usize,
    max: usize,
    rhs: Option<T<'h>>,
}

/// The number of elements an argument pattern takes, at least and at most.
fn arg_len(p: &Pat) -> (usize, usize) {
    match p {
        Pat::Seq { min, .. } => (*min as usize, usize::MAX),
        Pat::Repeat { min, max, .. } => (*min as usize, if *max == u32::MAX { usize::MAX } else { *max as usize }),
        Pat::Opt { .. } => (0, 1),
        Pat::Named(_, q) | Pat::Cond(q, _) => arg_len(q),
        Pat::Alt(qs) => qs.iter().map(arg_len).fold((usize::MAX, 0), |(a, b), (c, d)| (a.min(c), b.max(d))),
        _ => (1, 1),
    }
}

/// The argument patterns of a list pattern (under names and conditions).
fn list_args(p: &Pat) -> Option<&[Pat]> {
    match p {
        Pat::App { args, .. } => Some(args),
        Pat::Named(_, q) | Pat::Cond(q, _) => list_args(q),
        _ => None,
    }
}

/// The element terms of a list pattern term (under conditions and names), for per-slot pruning.
fn list_terms<'h>(h: &'h Heap, lib: &Lib, t: T<'h>) -> Option<&'h [T<'h>]> {
    match h.view(t) {
        View::App { head, args } if head == h.sym_term(lib.s.list) => Some(args),
        View::App { head, args: &[q, _] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "condition") => {
            list_terms(h, lib, q)
        }
        _ => None,
    }
}

/// The alternatives of the pattern argument `p` (a list pattern, `p1 | p2`, a rule `p -> r` or a list of rules), or
/// `None` when one is not a list pattern.
fn alternatives<'l, 'h>(lib: &'l Lib, h: &'h Heap, cx: &NativeCx<'_, '_, 'h>, p: T<'h>) -> Option<Vec<Alt<'l, 'h>>> {
    let split = |q: T<'h>| match h.view(q) {
        View::App { head, args: &[l, r] } if head == h.sym_term(lib.s.rule) || head == h.sym_term(lib.s.rule_delayed) => (l, Some(r)),
        _ => (q, None),
    };
    let rules: Vec<(T<'h>, Option<T<'h>>)> = match cx.as_list(p) {
        Some(qs) if !qs.is_empty() && qs.iter().all(|&q| split(q).1.is_some()) => qs.iter().map(|&q| split(q)).collect(),
        _ => vec![split(p)],
    };
    let mut out = vec![];
    for (pat, rhs) in rules {
        // `p1 | p2 | ...` of list patterns: each alternative in turn
        let pieces: Vec<T<'h>> = match h.view(pat) {
            View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "alternatives") => args.to_vec(),
            _ => vec![pat],
        };
        for q in pieces {
            let low = mc_eval::syntax::pattern(h, q).ok()?;
            let terms = list_terms(h, lib, q);
            let (mut min, mut max) = (0usize, 0usize);
            let n_args = match list_args(&low.pat) {
                Some(args) => {
                    for a in args {
                        let (lo, hi) = arg_len(a);
                        min = min.saturating_add(lo);
                        max = max.saturating_add(hi);
                    }
                    args.len()
                }
                // a list without pattern variables (`[2, 4]`) lowers to a literal: one element per entry
                None => {
                    let ts = terms?;
                    min = ts.len();
                    max = ts.len();
                    ts.len()
                }
            };
            let slots = match terms {
                Some(ts) if ts.len() == n_args && min == max => {
                    ts.iter().map(|&e| Matcher::new(lib, h, e)).collect()
                }
                _ => vec![],
            };
            out.push(Alt { whole: Matcher::new(lib, h, q)?, slots, min, max, rhs });
        }
    }
    Some(out)
}

/// A match: the positions (0-based, in slot order) and the right side's value.
type Found<'h> = (Vec<usize>, Option<T<'h>>);

/// The search state of one call.
struct Search<'a, 'h> {
    xs: &'a [T<'h>],
    used: Vec<bool>,
    seen: FxHashSet<Vec<usize>>,
    tests: u64,
}

impl<'h> Search<'_, 'h> {
    /// The first tuple of `k` free positions (none of them in a subset already found) that matches `alt`.
    fn first(&mut self, cx: &mut NativeCx<'_, '_, 'h>, alt: &mut Alt<'_, 'h>, k: usize) -> Res<Option<Vec<usize>>> {
        let mut tuple = Vec::with_capacity(k);
        self.dfs(cx, alt, k, &mut tuple)
    }

    fn dfs(&mut self, cx: &mut NativeCx<'_, '_, 'h>, alt: &mut Alt<'_, 'h>, k: usize, tuple: &mut Vec<usize>) -> Res<Option<Vec<usize>>> {
        if tuple.len() == k {
            let mut key = tuple.clone();
            key.sort_unstable();
            if self.seen.contains(&key) {
                return Ok(None);
            }
            self.tests += 1;
            if self.tests > MAX_TESTS {
                return Ok(None);
            }
            cx.tick(1)?;
            let elems: Vec<T<'h>> = tuple.iter().map(|&i| self.xs[i]).collect();
            let run = cx.list(&elems);
            if alt.whole.matches(cx, run)? {
                return Ok(Some(tuple.clone()));
            }
            return Ok(None);
        }
        let j = tuple.len();
        for i in 0..self.xs.len() {
            if self.used[i] || tuple.contains(&i) {
                continue;
            }
            if let Some(Some(m)) = alt.slots.get_mut(j) {
                cx.tick(1)?;
                if !m.matches(cx, self.xs[i])? {
                    continue;
                }
            }
            tuple.push(i);
            let r = self.dfs(cx, alt, k, tuple)?;
            tuple.pop();
            if r.is_some() {
                return Ok(r);
            }
            if self.tests > MAX_TESTS {
                return Ok(None);
            }
        }
        Ok(None)
    }

    /// Every match of the alternatives, in order (at most `limit`); without `overlaps` the positions of a match are
    /// used up. `None` when the search was cut off.
    fn all(&mut self, cx: &mut NativeCx<'_, '_, 'h>, alts: &mut [Alt<'_, 'h>], overlaps: bool, limit: usize) -> Res<Option<Vec<Found<'h>>>> {
        let mut out = vec![];
        for alt in alts.iter_mut() {
            let free = |s: &Search<'_, 'h>| s.used.iter().filter(|&&u| !u).count();
            let top = alt.max.min(free(self));
            let mut k = top + 1;
            while k > alt.min && out.len() < limit {
                k -= 1;
                loop {
                    if out.len() >= limit || k > free(self) {
                        break;
                    }
                    let Some(tuple) = self.first(cx, alt, k)? else { break };
                    let rhs = match alt.rhs {
                        Some(r) => Some(alt.whole.substitute(cx, r)?),
                        None => None,
                    };
                    let mut key = tuple.clone();
                    key.sort_unstable();
                    self.seen.insert(key);
                    if !overlaps {
                        for &i in &tuple {
                            self.used[i] = true;
                        }
                    }
                    out.push((tuple, rhs));
                }
                if self.tests > MAX_TESTS {
                    return Ok(None);
                }
            }
        }
        Ok(Some(out))
    }
}

/// The arguments `(xs, p)`, `(xs, p, n)` with the `overlaps:` option (`default` when absent).
struct Args<'h> {
    xs: &'h [T<'h>],
    head: T<'h>,
    p: T<'h>,
    limit: usize,
    overlaps: bool,
}

fn args<'h>(lib: &Lib, h: &'h Heap, t: T<'h>, args: &[T<'h>], default: bool) -> Result<Args<'h>, Reply<'h>> {
    let (args, ov) = crate::ctx::option(h, args, 2, "overlaps");
    let overlaps = match ov.map(|v| h.view(v)) {
        None => default,
        Some(View::Sym(s)) if h.sym_name(s) == "true" || h.sym_name(s) == "all" => true,
        Some(View::Sym(s)) if h.sym_name(s) == "false" => false,
        Some(_) => return Err(Reply::Done(lib.bad_arg(h, t, args.len() + 1))),
    };
    let (xs, p, limit) = match *args {
        [xs, p] => (xs, p, usize::MAX),
        [xs, p, n] => match h.view(n) {
            View::Int(n) if n >= 0 => (xs, p, n as usize),
            View::Sym(s) if h.sym_name(s) == "inf" || h.sym_name(s) == "all" => (xs, p, usize::MAX),
            _ => return Err(Reply::Done(lib.bad_arg(h, t, 3))),
        },
        _ => return Err(Reply::Decline),
    };
    let (head, xs) = lib.operand(h, t, xs, 1)?;
    Ok(Args { xs, head, p, limit, overlaps })
}

/// The matches of a call, or the reply when there are none to give (a pattern that is not a list pattern, or a
/// search cut off: the call stays unevaluated).
fn search<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, a: &Args<'h>) -> Res<Result<Vec<Found<'h>>, Reply<'h>>> {
    let h = cx.heap();
    let Some(mut alts) = alternatives(lib, h, cx, a.p) else { return Ok(Err(Reply::Done(lib.bad_arg(h, t, 2)))) };
    let mut s = Search { xs: a.xs, used: vec![false; a.xs.len()], seen: FxHashSet::default(), tests: 0 };
    Ok(match s.all(cx, &mut alts, a.overlaps, a.limit)? {
        Some(found) => Ok(found),
        None => Err(Reply::Decline),
    })
}

/// `subset_cases(xs, p)`, `subset_cases(xs, p, n)`: the sublists of `xs` (elements in any order, each element used
/// once unless `overlaps: true`) that match the list pattern `p`, in the pattern's order; `p -> r` gives `r` for
/// each.
pub(crate) fn subset_cases<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, xs: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let a = take!(args(lib, h, t, xs, false));
    let found = take!(search(lib, cx, t, &a)?);
    let out: Vec<T<'h>> = found
        .into_iter()
        .map(|(ix, r)| r.unwrap_or_else(|| cx.list(&ix.iter().map(|&i| a.xs[i]).collect::<Vec<_>>())))
        .collect();
    Ok(Reply::Again(cx.list(&out)))
}

/// `subset_position(xs, p)`, `subset_position(xs, p, n)`: the positions of the elements of the subsets of `xs`
/// matching the list pattern `p`, in the pattern's order (every subset, unless `overlaps: false`).
pub(crate) fn subset_position<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, xs: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let a = take!(args(lib, h, t, xs, true));
    let found = take!(search(lib, cx, t, &a)?);
    let out: Vec<T<'h>> = found
        .into_iter()
        .map(|(ix, _)| cx.list(&ix.iter().map(|&i| h.int(i as i64 + 1)).collect::<Vec<_>>()))
        .collect();
    Ok(Reply::Done(cx.list(&out)))
}

/// `subset_count(xs, p)`, `subset_count(xs, p, n)`: the number of subsets `subset_cases` gives (disjoint unless
/// `overlaps: true`; at most `n`).
pub(crate) fn subset_count<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, xs: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let a = take!(args(lib, h, t, xs, false));
    let found = take!(search(lib, cx, t, &a)?);
    Ok(Reply::Done(h.int(found.len() as i64)))
}

/// `subset_replace(xs, p -> r)`, `subset_replace(xs, rules, n)`: each subset of `xs` matching a rule's list
/// pattern (not overlapping) replaced by its `r`, which takes the place of the element matching the pattern's
/// first slot; the subset's other elements are removed.
pub(crate) fn subset_replace<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, xs: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let a = take!(args(lib, h, t, xs, false));
    let found = take!(search(lib, cx, t, &a)?);
    if found.iter().any(|(_, r)| r.is_none()) {
        return Ok(Reply::Done(lib.bad_arg(h, t, 2)));
    }
    let mut at: Vec<Option<T<'h>>> = vec![None; a.xs.len()];
    let mut gone = vec![false; a.xs.len()];
    for (ix, r) in found {
        // an empty subset (a pattern `[___]` matching nothing) replaces nothing
        if let Some(&first) = ix.first() {
            for &i in &ix {
                gone[i] = true;
            }
            at[first] = r;
        }
    }
    let mut out = Vec::with_capacity(a.xs.len());
    for (i, &x) in a.xs.iter().enumerate() {
        if let Some(r) = at[i] {
            out.push(r);
        } else if !gone[i] {
            out.push(x);
        }
    }
    Ok(lib.rebuilt(h, a.head, &out))
}
