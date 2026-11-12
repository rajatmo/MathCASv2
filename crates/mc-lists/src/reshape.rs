//! Rebuilding: `append`, `prepend`, `join`, `reverse`, `unique`, `flatten`, `transpose`, `apply`.
//!
//! These keep the head of their operand; a result with a head other than `list` is evaluated again (decision I.3).
use crate::ctx::{charge_copy, take, Lib, Meter, MAX_LEN};
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};
use rustc_hash::FxHashSet;

/// `append(xs, x)`: `x` added at the end.
pub(crate) fn append<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, x] = *args else {
        return Ok(Reply::Decline);
    };
    let (head, xs) = take!(lib.operand(h, t, xs, 1));
    charge_copy(cx, xs.len())?;
    let mut v = Vec::with_capacity(xs.len() + 1);
    v.extend_from_slice(xs);
    v.push(x);
    Ok(lib.rebuilt(h, head, &v))
}

/// `prepend(xs, x)`: `x` added at the start.
pub(crate) fn prepend<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, x] = *args else {
        return Ok(Reply::Decline);
    };
    let (head, xs) = take!(lib.operand(h, t, xs, 1));
    charge_copy(cx, xs.len())?;
    let mut v = Vec::with_capacity(xs.len() + 1);
    v.push(x);
    // an association: the new entry replaces one with its key, at the front
    let key = |r: T<'h>| match h.view(r) {
        View::App { head: g, args: &[k, _] } if matches!(h.view(g), View::Sym(s) if h.sym_name(s) == "rule" || h.sym_name(s) == "rule_delayed") => Some(k),
        _ => None,
    };
    let is_assoc = matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "assoc");
    match key(x).filter(|_| is_assoc) {
        Some(k) => v.extend(xs.iter().copied().filter(|&r| key(r) != Some(k))),
        None => v.extend_from_slice(xs),
    }
    Ok(lib.rebuilt(h, head, &v))
}

/// `join(xs, ys, ...)`: the arguments of terms with one head, in order; `join()` is `[]`.
pub(crate) fn join<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // join(xs, ys, ..., n) with an integer n >= 2 after at least two operands: the joins of corresponding parts at
    // level n (decision r37.2)
    if let [ops @ .., n] = args {
        if let (View::Int(n @ 2..), 2..) = (h.view(*n), ops.len()) {
            return Ok(match join_at(lib, cx, ops, n as usize)? {
                Some(v) => Reply::Done(v),
                None => Reply::Done(lib.bad_arg(h, t, 1)),
            });
        }
    }
    let mut head = h.sym_term(lib.s.list);
    let mut parts = Vec::with_capacity(args.len());
    for (k, &a) in args.iter().enumerate() {
        let (g, xs) = take!(lib.operand(h, t, a, k + 1));
        if k == 0 {
            head = g;
        } else if g != head {
            return Ok(Reply::Done(lib.bad_arg(h, t, k + 1)));
        }
        parts.push(xs);
    }
    let n: usize = parts.iter().map(|p| p.len()).sum();
    if n > MAX_LEN {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    charge_copy(cx, n)?;
    Ok(lib.rebuilt(h, head, &parts.concat()))
}

/// The join of `ops` at level `n`: at level 1 their arguments in order (one head), at a deeper level the list
/// whose k-th part is the join at level n - 1 of the k-th parts of the operands that have one (ragged operands
/// contribute where they reach). `None` when an operand is not a list down to level n - 1 or the heads differ.
fn join_at<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, ops: &[T<'h>], n: usize) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let mut parts: Vec<&[T<'h>]> = Vec::with_capacity(ops.len());
    let mut head = None;
    for &o in ops {
        let View::App { head: g, args } = h.view(o) else { return Ok(None) };
        if head.is_some_and(|x| x != g) || (n > 1 && g != h.sym_term(lib.s.list)) {
            return Ok(None);
        }
        head = Some(g);
        parts.push(args);
    }
    let Some(head) = head else { return Ok(None) };
    let total: usize = parts.iter().map(|p| p.len()).sum();
    if total > MAX_LEN {
        return Ok(None);
    }
    charge_copy(cx, total)?;
    if n == 1 {
        return Ok(Some(h.app(head, &parts.concat())));
    }
    let width = parts.iter().map(|p| p.len()).max().unwrap_or(0);
    let mut out = Vec::with_capacity(width);
    for k in 0..width {
        let col: Vec<T<'h>> = parts.iter().filter_map(|p| p.get(k).copied()).collect();
        match join_at(lib, cx, &col, n - 1)? {
            Some(v) => out.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some(h.app(head, &out)))
}

/// `reverse(xs)`: the arguments in reverse order.
pub(crate) fn reverse<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // reverse(x, n), reverse(x, [n1, n2, ...]): the parts at those levels reversed (decision p6core.9)
    if let [x, spec] = *args {
        let ns: Vec<i64> = match h.view(spec) {
            View::Int(n) => vec![n],
            _ => match cx.as_list(spec) {
                Some(xs) => {
                    let mut v = vec![];
                    for &e in xs {
                        let View::Int(n) = h.view(e) else { return Ok(Reply::Decline) };
                        v.push(n);
                    }
                    v
                }
                None => return Ok(Reply::Decline),
            },
        };
        if ns.iter().any(|&n| n < 1) {
            return Ok(Reply::Decline);
        }
        let mut visit = |_: &mut NativeCx<'_, '_, 'h>, u: T<'h>, pos: &[i64]| -> Res<Option<T<'h>>> {
            if !ns.contains(&(pos.len() as i64 + 1)) || crate::measure::is_complex_number(h, u) {
                return Ok(None);
            }
            Ok(match h.view(u) {
                View::App { head, args } => {
                    let v: Vec<T<'h>> = args.iter().rev().copied().collect();
                    Some(h.app(head, &v))
                }
                _ => None,
            })
        };
        return Ok(match crate::levels::walk(cx, x, crate::levels::Levels::every(), &mut visit)? {
            Some(v) => Reply::Again(v),
            None => Reply::Decline,
        });
    }
    let [xs] = *args else {
        return Ok(Reply::Decline);
    };
    let (head, xs) = take!(lib.operand(h, t, xs, 1));
    charge_copy(cx, xs.len())?;
    let v: Vec<T<'h>> = xs.iter().rev().copied().collect();
    Ok(lib.rebuilt(h, head, &v))
}

/// `unique(xs)`: the arguments without repetitions, in the order of their first occurrence, compared by identity
/// (`===`: one hash-consed term each); linear time with a hash set (it replaced the prelude's quadratic definition).
pub(crate) fn unique<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, test) = match *args {
        [xs] => (xs, None),
        // a test is a function: a list or a number there leaves the call
        [_, f] if cx.as_list(f).is_some() || !matches!(h.view(f), View::Sym(_) | View::App { .. }) => {
            return Ok(Reply::Decline)
        }
        [xs, f] => (xs, Some(f)),
        _ => return Ok(Reply::Decline),
    };
    let (head, xs) = take!(lib.operand(h, t, xs, 1));
    // `unique(xs, f)`: an element is dropped when `f(y, x)` is true for an element y kept before it (decision io-core.7)
    if let Some(f) = test {
        return Ok(match crate::group::classes(lib, cx, xs, f)? {
            Ok(gs) => lib.rebuilt(h, head, &gs.iter().map(|g| g[0]).collect::<Vec<_>>()),
            Err(v) => Reply::Done(v),
        });
    }
    let mut seen = FxHashSet::default();
    let mut out = Vec::new();
    let mut meter = Meter::default();
    for &x in xs {
        meter.step(cx)?;
        if seen.insert(x) {
            out.push(x);
        }
    }
    Ok(lib.rebuilt(h, head, &out))
}

/// `flatten(xs)`: arguments with the same head as `xs` spliced in, at every level; `flatten(xs, d)`: down to
/// depth `d` (0 leaves `xs` unchanged). Iterative: nesting depth costs no stack.
pub(crate) fn flatten<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `flatten(xs, inf)`; `flatten(x, d, g)`: the arguments with head g spliced in (decision io-core.14)
    let inf = |d: T<'h>| matches!(h.view(d), View::Sym(s) if h.sym_name(s) == "inf");
    let (xs, depth, splice) = match *args {
        [xs] => (xs, u64::MAX, None),
        [xs, d] | [xs, d, _] if inf(d) => (xs, u64::MAX, args.get(2).copied()),
        [xs, d] | [xs, d, _] => match take!(lib.int_operand(h, t, d, 2)) {
            d @ 0.. => (xs, d as u64, args.get(2).copied()),
            _ => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
        },
        _ => return Ok(Reply::Decline),
    };
    let (head, top) = take!(lib.operand(h, t, xs, 1));
    let splice = splice.unwrap_or(head);
    let mut out = Vec::with_capacity(top.len());
    // (arguments, next position, depth of these arguments)
    let mut stack: Vec<(&'h [T<'h>], usize, u64)> = vec![(top, 0, 0)];
    let mut m = Meter::default();
    while let Some(top) = stack.last_mut() {
        let (xs, i, d) = *top;
        if i == xs.len() {
            stack.pop();
            continue;
        }
        top.1 += 1;
        m.step(cx)?;
        let x = xs[i];
        match h.view(x) {
            View::App { head: g, args } if g == splice && d < depth => stack.push((args, 0, d + 1)),
            _ => {
                if out.len() == MAX_LEN {
                    return Ok(Reply::Done(lib.too_large(h, t)));
                }
                out.push(x);
            }
        }
    }
    Ok(lib.rebuilt(h, head, &out))
}

/// `transpose(m)`: rows and columns of a list of equal-length lists exchanged; unequal lengths give
/// `failure(shape_mismatch, transpose)`.
pub(crate) fn transpose<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if let [m, perm] = *args {
        return transpose_levels(lib, cx, t, m, perm);
    }
    let [m] = *args else {
        return Ok(Reply::Decline);
    };
    // a symbolic argument (a call of another head, `transpose(r(z))`) stays unevaluated
    if matches!(h.view(m), View::App { head, .. } if head != h.sym_term(lib.s.list)) {
        return Ok(Reply::Decline);
    }
    let rows = take!(lib.list_operand(h, t, m, 1));
    // a vector of numbers (a rank-1 array) is its own transpose (decision r37.4)
    if !rows.is_empty() && rows.iter().all(|&r| crate::measure::is_complex_number(h, r) || matches!(h.view(r), View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. })) {
        return Ok(Reply::Done(m));
    }
    let mut cols = None;
    let mut rs = Vec::with_capacity(rows.len());
    for &r in rows {
        let r = match h.view(r) {
            View::App { head, args } if head == h.sym_term(lib.s.list) => args,
            View::Sym(_) | View::App { .. } => return Ok(Reply::Decline),
            _ => return Ok(Reply::Done(lib.bad_arg(h, t, 1))),
        };
        if *cols.get_or_insert(r.len()) != r.len() {
            let f = match h.view(t) {
                View::App { head, .. } => head,
                _ => t,
            };
            return Ok(Reply::Done(lib.fail(h, lib.s.shape_mismatch, &[f])));
        }
        rs.push(r);
    }
    let cols = cols.unwrap_or(0);
    charge_copy(cx, rows.len() * cols)?;
    let mut out = Vec::with_capacity(cols);
    let mut col = Vec::with_capacity(rs.len());
    for j in 0..cols {
        cx.tick(1)?;
        col.clear();
        col.extend(rs.iter().map(|r| r[j]));
        out.push(cx.list(&col));
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// `transpose(a, [p1, ..., pk])`: level i of the full array `a` becomes level p_i of the result, so the part at
/// [j_1, ..., j_m] of the result is the part of `a` at [j_p1, ..., j_pk] (levels below k unchanged). The p_i are
/// 1..m, each at least once; a level named twice runs along a diagonal (`transpose(m, [1, 1])` is the diagonal),
/// of the smaller length.
fn transpose_levels<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, a: T<'h>, perm: T<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let Some(ps) = cx.as_list(perm) else { return Ok(Reply::Decline) };
    let mut p = Vec::with_capacity(ps.len());
    for &x in ps {
        match h.view(x) {
            View::Int(k) if k >= 1 && (k as usize) <= ps.len() => p.push(k as usize - 1),
            _ => return Ok(Reply::Decline),
        }
    }
    let m = p.iter().copied().max().map_or(0, |x| x + 1);
    if p.is_empty() || (0..m).any(|l| !p.contains(&l)) {
        return Ok(Reply::Decline);
    }
    // a packed array is taken as its nested lists
    let a = if h.packed(a).is_some() { cx.eval(h.call(h.sym("normal"), &[a]))? } else { a };
    if cx.as_list(a).is_none() {
        return Ok(Reply::Decline);
    }
    let dims = crate::structure::list_dims(cx, a)?;
    if dims.len() < p.len() {
        return Ok(Reply::Done(lib.bad_arg(h, t, 2)));
    }
    // the length of each result level: the smallest of the levels sent to it
    let mut out_dims = vec![usize::MAX; m];
    for (l, &q) in p.iter().enumerate() {
        out_dims[q] = out_dims[q].min(dims[l]);
    }
    let total: usize = out_dims.iter().try_fold(1usize, |acc, &d| acc.checked_mul(d)).unwrap_or(usize::MAX);
    if total > MAX_LEN {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    charge_copy(cx, total)?;
    let mut idx = vec![0usize; m];
    let v = transpose_build(cx, a, &p, &out_dims, &mut idx, 0)?;
    Ok(Reply::Done(v))
}

fn transpose_build<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    a: T<'h>,
    p: &[usize],
    out_dims: &[usize],
    idx: &mut Vec<usize>,
    level: usize,
) -> Res<T<'h>> {
    if level == out_dims.len() {
        // the part of a at [j_p1, ..., j_pk]
        let mut u = a;
        for &q in p {
            match cx.as_list(u) {
                Some(items) => u = items[idx[q]],
                None => return Ok(u),
            }
        }
        return Ok(u);
    }
    let mut items = Vec::with_capacity(out_dims[level]);
    for j in 0..out_dims[level] {
        cx.tick(1)?;
        idx[level] = j;
        items.push(transpose_build(cx, a, p, out_dims, idx, level + 1)?);
    }
    Ok(cx.list(&items))
}

/// `apply(f, xs)`: the head of `xs` replaced by `f` (semantics 1: the explicit form of variadic calls).
pub(crate) fn apply<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // apply(f, x, levels, heads: true): the heads of the compound parts are parts too, one level below their call
    // (as map's option, decision r37.7); heads: false is the plain apply
    let (args, heads) = crate::ctx::option(h, args, 2, "heads");
    if let Some(v) = heads {
        let on = matches!(h.view(v), View::Sym(s) if h.sym_name(s) == "true");
        let off = matches!(h.view(v), View::Sym(s) if h.sym_name(s) == "false");
        if !(on || off) {
            return Ok(Reply::Decline);
        }
        if on {
            let levels = match *args {
                [_, _] => crate::levels::Levels::only(0),
                [_, _, spec] => match crate::levels::level_spec(h, spec) {
                    Some(levels) => levels,
                    None => return Ok(Reply::Decline),
                },
                _ => return Ok(Reply::Decline),
            };
            return crate::levels::apply_with_heads(cx, args[0], args[1], levels);
        }
    }
    if let [f, x, spec] = *args {
        return match crate::levels::level_spec(h, spec) {
            Some(levels) => crate::levels::apply_at(cx, f, x, levels),
            None => Ok(Reply::Decline),
        };
    }
    let [f, xs] = *args else {
        return Ok(Reply::Decline);
    };
    // an association's arguments are its values (decision io-core.4)
    if let Some(es) = crate::assocs::entries(lib, h, xs) {
        let vs: Vec<T<'h>> = es.iter().map(|&(_, v)| v).collect();
        charge_copy(cx, vs.len())?;
        return Ok(Reply::Again(h.app(f, &vs)));
    }
    let (_, xs) = take!(lib.operand(h, t, xs, 2));
    charge_copy(cx, xs.len())?;
    Ok(Reply::Again(h.app(f, xs)))
}
