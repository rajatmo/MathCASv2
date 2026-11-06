//! Measures of terms: `depth` and `leaf_count`. Both walk the term with an explicit stack (any depth) and visit
//! each shared subterm once, so a hash-consed term with exponentially many paths costs time linear in its size.
use crate::ctx::{int128, Lib, Meter};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};
use rustc_hash::FxHashMap;

/// A bottom-up fold over the distinct subterms of a term: atoms give `atom`; a compound term combines the values
/// of its children (its head first when `with_head`) with `combine`, starting from `start`, then `finish`. `None`
/// from `combine` or `finish` (overflow) ends the walk.
struct Fold {
    with_head: bool,
    atom: u128,
    start: u128,
    combine: fn(u128, u128) -> Option<u128>,
    finish: fn(u128) -> Option<u128>,
}

fn fold_up<'h>(cx: &mut NativeCx<'_, '_, 'h>, h: &'h Heap, t: T<'h>, f: &Fold) -> Res<Option<u128>> {
    let Fold { with_head, atom, start, combine, finish } = *f;
    let children = |u: T<'h>| -> Option<(Option<T<'h>>, &'h [T<'h>])> {
        match h.view(u) {
            View::App { head, args } => Some((with_head.then_some(head), args)),
            _ => None,
        }
    };
    if children(t).is_none() {
        return Ok(Some(atom));
    }
    let mut done: FxHashMap<T<'h>, u128> = FxHashMap::default();
    // (term, children visited so far (the head counts as child 0 when included), accumulated value)
    let mut stack: Vec<(T<'h>, usize, u128)> = vec![(t, 0, start)];
    let mut m = Meter::default();
    while let Some(&mut (u, i, acc)) = stack.last_mut() {
        m.step(cx)?;
        // invariant: only compound terms are pushed
        let (head, args) = children(u).expect("only compound terms are pushed");
        let n = args.len() + usize::from(head.is_some());
        if i == n {
            stack.pop();
            let Some(v) = finish(acc) else {
                return Ok(None);
            };
            done.insert(u, v);
            match stack.last_mut() {
                Some(parent) => {
                    let Some(c) = combine(parent.2, v) else {
                        return Ok(None);
                    };
                    parent.2 = c;
                }
                None => return Ok(Some(v)),
            }
            continue;
        }
        let child = match head {
            Some(hd) if i == 0 => hd,
            Some(_) => args[i - 1],
            None => args[i],
        };
        // invariant: inside the loop over a nonempty stack
        stack.last_mut().expect("nonempty").1 += 1;
        let known = if children(child).is_none() { Some(atom) } else { done.get(&child).copied() };
        match known {
            Some(v) => {
                // invariant: inside the loop over a nonempty stack
                let top = stack.last_mut().expect("nonempty");
                let Some(c) = combine(top.2, v) else {
                    return Ok(None);
                };
                top.2 = c;
            }
            None => stack.push((child, 0, start)),
        }
    }
    // invariant: the loop returns when the root is finished, before the stack empties
    unreachable!("the root returns when it is finished")
}

/// `depth(t)`: 1 for an atom, one more than the deepest argument for a compound term (heads not counted).
pub(crate) fn depth<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    // a compound term without arguments is one level, like an atom
    let f = Fold {
        with_head: false,
        atom: 1,
        start: 0,
        combine: |a, b| Some(a.max(b)),
        finish: |a| Some(if a == 0 { 1 } else { a + 1 }),
    };
    let d = fold_up(cx, h, x, &f)?;
    // invariant: a depth is at most the number of nodes, far below the integer limit
    Ok(Reply::Done(int128(h, d.expect("depth cannot overflow") as i128)))
}

/// `leaf_count(t)`: the number of atoms in the tree of `t`, heads included (`f(x, y)` has 3). Counts beyond 2^127
/// give `failure(limit, memory, leaf_count)`.
pub(crate) fn leaf_count<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    let f = Fold {
        with_head: true,
        atom: 1,
        start: 0,
        combine: |a, b| a.checked_add(b).filter(|v| *v <= i128::MAX as u128),
        finish: Some,
    };
    let n = fold_up(cx, h, x, &f)?;
    Ok(Reply::Done(match n {
        Some(n) => int128(h, n as i128),
        None => lib.too_large(h, t),
    }))
}
