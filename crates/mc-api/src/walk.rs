//! Walks over a term as the graph it is.
//!
//! Terms are hash-consed, so a term is a directed acyclic graph: `nest(x => [x, x], 1, 60)` is 61 distinct terms
//! but a tree of 2^60 leaves. A walker that recurses into every argument without remembering what it has seen
//! takes time exponential in the depth of such a term. The tools here visit each distinct subterm once:
//!
//! - [`Walk`]: a depth-first stack that hands out each item once (a drop-in for the `vec![t]` + `pop` loop of a
//!   predicate or a collector; the order of first visits is the order of the plain stack).
//! - [`Memo`] and [`fold`]: a bottom-up fold with a memo keyed by the term's identity (sizes, depths, counts of the
//!   tree the term stands for, rebuilt terms).
//! - [`any`], [`free_of`], [`symbols`], [`leaf_count`], [`depth`], [`map`]: the common walks built on them.
//!
//! A count over the tree (leaves, nodes) is the same number as before; it saturates at `u64::MAX` instead of
//! taking forever to reach.
use crate::{Store, Sym, View, T};
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hash, Hasher};

/// A fast hasher for term handles and small keys (a multiplicative mix, as in the Fx hash of the Rust compiler).
#[derive(Default, Clone, Copy)]
pub struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.add(b as u64);
        }
    }
    fn write_u8(&mut self, v: u8) {
        self.add(v as u64);
    }
    fn write_u32(&mut self, v: u32) {
        self.add(v as u64);
    }
    fn write_u64(&mut self, v: u64) {
        self.add(v);
    }
    fn write_usize(&mut self, v: usize) {
        self.add(v as u64);
    }
}

impl IdHasher {
    #[inline]
    fn add(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

/// The hasher builder of [`Seen`] and [`Memo`].
pub type IdBuild = BuildHasherDefault<IdHasher>;
/// A set of items already visited.
pub type Seen<K> = HashSet<K, IdBuild>;
/// A memo keyed by a term's identity (or a term with its context).
pub type Memo<K, V> = HashMap<K, V, IdBuild>;

/// A new empty [`Seen`].
pub fn seen<K>() -> Seen<K> {
    Seen::default()
}

/// A new empty [`Memo`].
pub fn memo<K, V>() -> Memo<K, V> {
    Memo::default()
}

/// A depth-first stack that hands out each item once. Use it where a walker kept a `Vec` of pending items:
/// `push`, `extend`, `extend_from_slice` and `pop` behave as on the vector, except that `pop` skips an item it has
/// handed out before. The item is a term or a term with its context (a flag, a depth): two visits of one term in
/// different contexts are different items.
///
/// Only for walks whose result does not depend on how often a subterm occurs (predicates, searches, collections of
/// distinct things); counts over the tree use [`fold`].
pub struct Walk<K = T<'static>> {
    stack: Vec<K>,
    seen: Seen<K>,
    /// items handed out before the set was started (small terms never start it)
    early: usize,
}

/// Items handed out before [`Walk`] starts remembering: a small term costs no set, and an item handed out early is
/// handed out at most twice.
const EARLY: usize = 48;

impl<K: Copy + Eq + Hash> Walk<K> {
    /// A walk from one item.
    pub fn new(start: K) -> Walk<K> {
        Walk { stack: vec![start], seen: Seen::default(), early: 0 }
    }
    /// A walk from several items (popped last first, as from a vector).
    pub fn from_vec(stack: Vec<K>) -> Walk<K> {
        Walk { stack, seen: Seen::default(), early: 0 }
    }
    /// The next item not handed out before.
    #[inline]
    pub fn pop(&mut self) -> Option<K> {
        while let Some(k) = self.stack.pop() {
            if self.early < EARLY {
                self.early += 1;
                return Some(k);
            }
            if self.seen.insert(k) {
                return Some(k);
            }
        }
        None
    }
    #[inline]
    pub fn push(&mut self, k: K) {
        self.stack.push(k);
    }
    pub fn extend_from_slice(&mut self, items: &[K]) {
        self.stack.extend_from_slice(items);
    }
    /// Whether nothing is pending (items already handed out may still be on the stack).
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

/// `extend` takes items or references to them, as on a vector.
impl<K: Copy + Eq + Hash> Extend<K> for Walk<K> {
    fn extend<I: IntoIterator<Item = K>>(&mut self, items: I) {
        self.stack.extend(items);
    }
}

impl<'a, K: Copy + Eq + Hash + 'a> Extend<&'a K> for Walk<K> {
    fn extend<I: IntoIterator<Item = &'a K>>(&mut self, items: I) {
        self.stack.extend(items);
    }
}

impl<K: Copy + Eq + Hash> From<Vec<K>> for Walk<K> {
    fn from(v: Vec<K>) -> Walk<K> {
        Walk::from_vec(v)
    }
}

/// Fold `t` bottom-up: `node(u, values of u's arguments)` for every distinct subterm `u` (a leaf gets no values;
/// the head of a compound term is not visited). Each distinct subterm is computed once and its value reused, so the
/// fold is linear in the number of distinct subterms. Iterative: no recursion limit on deep terms.
pub fn fold<'h, S, V>(h: &'h S, t: T<'h>, memo: &mut Memo<T<'h>, V>, mut node: impl FnMut(T<'h>, &[V]) -> V) -> V
where
    S: Store + ?Sized,
    V: Clone,
{
    if let Some(v) = memo.get(&t) {
        return v.clone();
    }
    // (term, whether its arguments are done)
    let mut stack = vec![(t, false)];
    let mut vals: Vec<V> = Vec::new();
    while let Some((u, ready)) = stack.pop() {
        if memo.contains_key(&u) && !ready {
            continue;
        }
        let args: &[T<'h>] = match h.view(u) {
            View::App { args, .. } => args,
            _ => &[],
        };
        if !ready {
            stack.push((u, true));
            for &a in args.iter().rev() {
                if !memo.contains_key(&a) {
                    stack.push((a, false));
                }
            }
            continue;
        }
        if memo.contains_key(&u) {
            continue;
        }
        vals.clear();
        vals.extend(args.iter().map(|a| memo[a].clone()));
        let v = node(u, &vals);
        memo.insert(u, v);
    }
    memo[&t].clone()
}

/// The value of `k` from `memo`, or `f(memo)` computed and remembered: the step of a recursive walker that decides
/// each distinct subterm once (its recursive calls pass `memo` on). Only for a function of `k` alone (the other
/// parameters fixed across the walk).
pub fn cached<K: Copy + Eq + Hash, V: Clone>(memo: &mut Memo<K, V>, k: K, f: impl FnOnce(&mut Memo<K, V>) -> V) -> V {
    if let Some(v) = memo.get(&k) {
        return v.clone();
    }
    let v = f(memo);
    memo.insert(k, v.clone());
    v
}

/// Whether `pred` holds for some subterm of `t` (heads included), each distinct subterm tested once.
pub fn any<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>, mut pred: impl FnMut(T<'h>) -> bool) -> bool {
    let mut w = Walk::new(t);
    while let Some(u) = w.pop() {
        if pred(u) {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            w.push(head);
            w.extend_from_slice(args);
        }
    }
    false
}

/// Whether `x` occurs nowhere in `t` (heads included).
pub fn free_of<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>, x: T<'h>) -> bool {
    !any(h, t, |u| u == x)
}

/// The distinct symbols of `t` (heads included), in the order of a depth-first walk that takes arguments left to
/// right.
pub fn symbols<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>) -> Vec<Sym> {
    let mut out = vec![];
    let mut w = Walk::new(t);
    while let Some(u) = w.pop() {
        match h.view(u) {
            View::Sym(s) => {
                if !out.contains(&s) {
                    out.push(s);
                }
            }
            View::App { head, args } => {
                w.extend(args.iter().rev().copied());
                w.push(head);
            }
            _ => {}
        }
    }
    out
}

/// The number of leaves of the tree `t` stands for (atoms, heads not counted), saturating.
pub fn leaf_count<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>) -> u64 {
    fold(h, t, &mut memo(), |_, vs| if vs.is_empty() { 1 } else { vs.iter().fold(0u64, |a, &v| a.saturating_add(v)) })
}

/// The number of nodes of the tree `t` stands for (each term counts 1, heads not counted), saturating.
pub fn tree_size<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>) -> u64 {
    fold(h, t, &mut memo(), |_, vs| vs.iter().fold(1u64, |a, &v| a.saturating_add(v)))
}

/// The depth of `t`: 1 for an atom, one more than the deepest argument for a compound term (heads not counted).
pub fn depth<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>) -> u64 {
    fold(h, t, &mut memo(), |_, vs| 1 + vs.iter().copied().max().unwrap_or(0))
}

/// The number of distinct subterms of `t` (heads not counted).
pub fn distinct_count<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>) -> usize {
    let mut m = memo();
    fold(h, t, &mut m, |_, _| ());
    m.len()
}

/// Rebuild `t` with `f` applied top-down: where `f(u)` is `Some(v)`, `u` is replaced by `v` (and not entered);
/// elsewhere a compound term is rebuilt with the same head from its mapped arguments (the same handle when nothing
/// changed). Each distinct subterm is mapped once.
pub fn map<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>, memo: &mut Memo<T<'h>, T<'h>>, f: &mut impl FnMut(T<'h>) -> Option<T<'h>>) -> T<'h> {
    if let Some(&v) = memo.get(&t) {
        return v;
    }
    let v = match f(t) {
        Some(v) => v,
        None => match h.view(t) {
            View::App { head, args } => {
                let new: Vec<T<'h>> = args.iter().map(|&a| map(h, a, memo, f)).collect();
                if new.as_slice() == args {
                    t
                } else {
                    h.app(head, &new)
                }
            }
            _ => t,
        },
    };
    memo.insert(t, v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RefStore;

    /// `[x, x]` nested `n` times around `x0`
    fn doubled<'h>(h: &'h RefStore, x0: T<'h>, n: usize) -> T<'h> {
        let list = h.sym("list");
        (0..n).fold(x0, |x, _| h.call(list, &[x, x]))
    }

    #[test]
    fn shared_terms_are_walked_once() {
        let h = RefStore::new();
        let one = h.int(1);
        let y = h.sym_term(h.sym("y"));
        let d = doubled(&h, one, 60);
        assert!(free_of(&h, d, y));
        assert!(!free_of(&h, d, one));
        assert_eq!(leaf_count(&h, d), 1u64 << 60);
        assert_eq!(tree_size(&h, d), (1u64 << 61) - 1);
        assert_eq!(depth(&h, d), 61);
        assert_eq!(distinct_count(&h, d), 61);
        assert_eq!(symbols(&h, d).len(), 1);
        let d70 = doubled(&h, one, 70);
        assert_eq!(leaf_count(&h, d70), u64::MAX);
        let two = h.int(2);
        let mut m = memo();
        let e = map(&h, d, &mut m, &mut |u| (u == one).then_some(two));
        assert_eq!(e, doubled(&h, two, 60));
    }

    #[test]
    fn a_walk_hands_out_each_item_once_in_stack_order() {
        let mut w = Walk::from_vec((0..200u32).chain(0..200).collect());
        let mut out = vec![];
        while let Some(k) = w.pop() {
            out.push(k);
        }
        // the first EARLY pops are not remembered: at most those come twice
        let mut sorted = out.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 200);
        assert!(out.len() <= 200 + EARLY);
        assert_eq!(out[0], 199);
    }
}
