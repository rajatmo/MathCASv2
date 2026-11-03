//! Substitution of pattern-variable bindings into term right sides and term conditions (semantics 6.4, 6.6).
//!
//! A variable is written in the term as its symbol; substitution replaces every occurrence. It is iterative (terms
//! may be arbitrarily deep), shares work across repeated subterms (terms are DAGs), and skips every subterm whose
//! symbol mask shows it contains none of the variables. Binding forms inside the term are not renamed yet (the
//! hygiene of 6.6 comes with closures, a known gap).
use crate::matcher::{Binds, Val};
use mc_core::{Heap, Store, Sym, View, T};
use rustc_hash::FxHashMap;

/// The term variable `v` stands for: its term, or `list(...)` of a sequence variable's terms (semantics 6.1).
pub(crate) fn value<'h>(h: &'h Heap, b: &Binds<'h>, v: u16, list: Sym) -> T<'h> {
    match b.get(v) {
        Val::One(t) => t,
        Val::Seq(..) => h.call(list, b.seq(v)),
        Val::Unbound => h.call(list, &[]),
    }
}

/// `t` with each symbol `pairs[i].0` replaced by `pairs[i].1`.
pub(crate) fn substitute<'h>(h: &'h Heap, t: T<'h>, pairs: &[(Sym, T<'h>)]) -> T<'h> {
    let mask = pairs.iter().fold(0u64, |m, (s, _)| m | h.mask(h.sym_term(*s)));
    if pairs.is_empty() || h.mask(t) & mask == 0 {
        return t;
    }
    let mut done: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    let mut stack = vec![(t, false)];
    let mut parts: Vec<T<'h>> = vec![];
    while let Some((u, expanded)) = stack.pop() {
        if done.contains_key(&u) {
            continue;
        }
        let r = if h.mask(u) & mask == 0 {
            u
        } else {
            match h.view(u) {
                View::Sym(s) => pairs.iter().find(|(p, _)| *p == s).map_or(u, |(_, v)| *v),
                View::App { head, args } => {
                    if !expanded {
                        stack.push((u, true));
                        stack.push((head, false));
                        stack.extend(args.iter().map(|&a| (a, false)));
                        continue;
                    }
                    parts.clear();
                    parts.extend(args.iter().map(|a| done[a]));
                    h.app(done[&head], &parts)
                }
                _ => u,
            }
        };
        done.insert(u, r);
    }
    done[&t]
}
