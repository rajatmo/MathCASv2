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
    /// the value of an atom (complex numbers are atoms, decision p6core.3)
    atom: fn(&Heap, T<'_>) -> u128,
    start: u128,
    combine: fn(u128, u128) -> Option<u128>,
    finish: fn(u128) -> Option<u128>,
}

fn fold_up<'h>(cx: &mut NativeCx<'_, '_, 'h>, h: &'h Heap, t: T<'h>, f: &Fold) -> Res<Option<u128>> {
    let Fold { with_head, atom, start, combine, finish } = *f;
    let (assoc, rule) = (h.sym_term(h.sym("assoc")), h.sym_term(h.sym("rule")));
    let children = |u: T<'h>| -> Option<(Option<T<'h>>, &'h [T<'h>])> {
        match h.view(u) {
            View::App { .. } if is_complex_number(h, u) => None,
            // an association counts as its head and its values (keys are not parts of its tree)
            View::App { head, args } if head == assoc && args.iter().all(|&r| is_call_of(h, r, rule, 2)) => {
                let values: Vec<T<'h>> = args
                    .iter()
                    .map(|&r| match h.view(r) {
                        View::App { args: kv, .. } => kv[1],
                        _ => r,
                    })
                    .collect();
                match h.view(h.app(head, &values)) {
                    View::App { args, .. } => Some((with_head.then_some(head), args)),
                    _ => None,
                }
            }
            View::App { head, args } => Some((with_head.then_some(head), args)),
            _ => None,
        }
    };
    if children(t).is_none() {
        return Ok(Some(atom(h, t)));
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
        let known = if children(child).is_none() { Some(atom(h, child)) } else { done.get(&child).copied() };
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

/// Is `t` a complex number `complex(re, im)` with real number parts?
pub(crate) fn is_complex_number(h: &Heap, t: T<'_>) -> bool {
    let View::App { head, args } = h.view(t) else { return false };
    let number = |a: T<'_>| {
        matches!(h.view(a), View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. })
    };
    args.len() == 2 && args.iter().all(|&a| number(a)) && matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "complex")
}

fn is_call_of(h: &Heap, t: T<'_>, head: T<'_>, n: usize) -> bool {
    matches!(h.view(t), View::App { head: g, args } if g.raw() == head.raw() && args.len() == n)
}

/// The leaves of an atom in its tree: a fraction is `rational(p, q)` (3), a complex number `complex(re, im)` with
/// its parts' leaves, any other atom 1.
fn leaves(h: &Heap, t: T<'_>) -> u128 {
    match h.view(t) {
        View::Rational { .. } => 3,
        View::App { args, .. } => 1 + args.iter().map(|&a| leaves(h, a)).sum::<u128>(),
        _ => 1,
    }
}

/// `depth(t)`: 1 for an atom, one more than the deepest argument for a compound term (heads not counted).
pub(crate) fn depth<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `heads: true`: a head counts as a part one level down (h(a)(x) is one deeper than h(x))
    let (args, heads) = crate::ctx::option(h, args, 1, "heads");
    let with_head = match heads.map(|v| h.view(v)) {
        None => false,
        Some(View::Sym(s)) if h.sym_name(s) == "true" => true,
        Some(View::Sym(s)) if h.sym_name(s) == "false" => false,
        Some(_) => return Ok(Reply::Decline),
    };
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    // a compound term without arguments is one level, like an atom
    let f = Fold {
        with_head,
        atom: |_, _| 1,
        start: 0,
        combine: |a, b| Some(a.max(b)),
        finish: |a| Some(if a == 0 { 1 } else { a + 1 }),
    };
    let d = fold_up(cx, h, x, &f)?;
    // invariant: a depth is at most the number of nodes, far below the integer limit
    Ok(Reply::Done(int128(h, d.expect("depth cannot overflow") as i128)))
}

/// `leaf_count(t)`: the number of atoms in the tree of `t`, heads included (`f(x, y)` has 3; with `heads: false`
/// 2). Counts beyond 2^127
/// give `failure(limit, memory, leaf_count)`.
pub(crate) fn leaf_count<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `heads: false`: the heads are not counted
    let (args, heads) = crate::ctx::option(h, args, 1, "heads");
    let with_head = match heads.map(|v| h.view(v)) {
        None => true,
        Some(View::Sym(s)) if h.sym_name(s) == "true" => true,
        Some(View::Sym(s)) if h.sym_name(s) == "false" => false,
        Some(_) => return Ok(Reply::Decline),
    };
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    let f = Fold {
        with_head,
        atom: leaves,
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
