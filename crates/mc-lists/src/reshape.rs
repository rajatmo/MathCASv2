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
    v.extend_from_slice(xs);
    Ok(lib.rebuilt(h, head, &v))
}

/// `join(xs, ys, ...)`: the arguments of terms with one head, in order; `join()` is `[]`.
pub(crate) fn join<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
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
    let [xs] = *args else {
        return Ok(Reply::Decline);
    };
    let (head, xs) = take!(lib.operand(h, t, xs, 1));
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
    let (xs, depth) = match *args {
        [xs] => (xs, u64::MAX),
        [xs, d] => match take!(lib.int_operand(h, t, d, 2)) {
            d @ 0.. => (xs, d as u64),
            _ => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
        },
        _ => return Ok(Reply::Decline),
    };
    let (head, top) = take!(lib.operand(h, t, xs, 1));
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
            View::App { head: g, args } if g == head && d < depth => stack.push((args, 0, d + 1)),
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
    let [m] = *args else {
        return Ok(Reply::Decline);
    };
    // a symbolic argument (a call of another head, `transpose(r(z))`) stays unevaluated
    if matches!(h.view(m), View::App { head, .. } if head != h.sym_term(lib.s.list)) {
        return Ok(Reply::Decline);
    }
    let rows = take!(lib.list_operand(h, t, m, 1));
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

/// `apply(f, xs)`: the head of `xs` replaced by `f` (semantics 1: the explicit form of variadic calls).
pub(crate) fn apply<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if let [f, x, spec] = *args {
        return match crate::levels::level_spec(h, spec) {
            Some(levels) => crate::levels::apply_at(cx, f, x, levels),
            None => Ok(Reply::Decline),
        };
    }
    let [f, xs] = *args else {
        return Ok(Reply::Decline);
    };
    let (_, xs) = take!(lib.operand(h, t, xs, 2));
    charge_copy(cx, xs.len())?;
    Ok(Reply::Again(h.app(f, xs)))
}
