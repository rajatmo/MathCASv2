//! Reductions: `sum`, `product`, `total`, `max`, `min`.
//!
//! `sum` and `product` build one canonical `add` or `mul` through `mc-arith`, however many terms there are
//! (decision I.8): machine integers are folded in 128 bits, the other exact numbers are combined by a balanced tree
//! (so a sum of `n` rationals costs about `M(size) log n`, not `n` growing additions), and the remaining terms go to
//! a single canonicaliser call with the exact total.
use crate::ctx::{int128, take, Lib, Meter};
use mc_arith::{Arith, Cmp3};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// `add` or `mul`.
#[derive(Clone, Copy)]
enum Op {
    Add,
    Mul,
}

impl Op {
    fn of<'h>(self, ar: &Arith<'h, Heap>, args: &[T<'h>]) -> T<'h> {
        match self {
            Op::Add => ar.add(args),
            Op::Mul => ar.mul(args),
        }
    }
    fn small(self, a: i128, b: i64) -> Option<i128> {
        match self {
            Op::Add => a.checked_add(b as i128),
            // products stay within 64 bits, so that the 128-bit product never overflows
            Op::Mul => a.checked_mul(b as i128).filter(|v| i64::try_from(*v).is_ok()),
        }
    }
    fn unit(self) -> i128 {
        match self {
            Op::Add => 0,
            Op::Mul => 1,
        }
    }
}

/// The canonical `add` or `mul` of `xs`.
fn reduce<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, op: Op, xs: &[T<'h>]) -> Res<T<'h>> {
    let h = cx.heap();
    let mut m = Meter::default();
    let mut acc = op.unit();
    let mut exact = vec![];
    let mut other = vec![];
    for &x in xs {
        m.step(cx)?;
        match h.view(x) {
            View::Int(v) => match op.small(acc, v) {
                Some(a) => acc = a,
                None => {
                    exact.push(int128(h, acc));
                    acc = v as i128;
                }
            },
            View::BigInt { .. } | View::Rational { .. } => exact.push(x),
            _ => other.push(x),
        }
    }
    if acc != op.unit() || exact.is_empty() {
        exact.push(int128(h, acc));
    }
    let ar = lib.arith(h);
    // the balanced tree of exact numbers, level by level
    while exact.len() > 1 {
        let mut next = Vec::with_capacity(exact.len().div_ceil(2));
        for pair in exact.chunks(2) {
            cx.tick(1)?;
            next.push(if pair.len() == 2 { op.of(&ar, pair) } else { pair[0] });
        }
        exact = next;
    }
    if other.is_empty() {
        return Ok(exact[0]);
    }
    // the canonicaliser's own pass is linear (collection by hashing) plus a sort
    cx.tick(other.len() as u64 / 4)?;
    other.push(exact[0]);
    Ok(op.of(&ar, &other))
}

/// `sum(xs)`: the canonical sum of the elements of the list `xs` (`0` for `[]`).
pub(crate) fn sum<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs] = *args else {
        return Ok(Reply::Decline);
    };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    Ok(Reply::Done(reduce(lib, cx, Op::Add, xs)?))
}

/// `product(xs)`: the canonical product of the elements (`1` for `[]`).
pub(crate) fn product<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs] = *args else {
        return Ok(Reply::Decline);
    };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    Ok(Reply::Done(reduce(lib, cx, Op::Mul, xs)?))
}

/// The elements of nested lists, depth first (the arguments themselves when they are not lists).
fn leaves<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let list = h.sym_term(lib.s.list);
    let mut out = Vec::with_capacity(args.len());
    let mut stack: Vec<(&[T<'h>], usize)> = vec![(args, 0)];
    let mut m = Meter::default();
    while let Some(top) = stack.last_mut() {
        let (xs, i) = *top;
        if i == xs.len() {
            stack.pop();
            continue;
        }
        top.1 += 1;
        m.step(cx)?;
        match h.view(xs[i]) {
            View::App { head, args } if head == list => stack.push((args, 0)),
            _ => out.push(xs[i]),
        }
    }
    Ok(out)
}

/// `total(xs)`: the sum of every element of the nested lists `xs` (decision I.9).
pub(crate) fn total<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs] = *args else {
        return Ok(Reply::Decline);
    };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    let all = leaves(lib, cx, xs)?;
    Ok(Reply::Done(reduce(lib, cx, Op::Add, &all)?))
}

/// `max(xs)` or `max(a, b, ...)`, lists flattened: the largest element when all are real numbers and every
/// comparison is decided; otherwise the call stays unevaluated. No elements give `failure(argument, max, 1)`.
pub(crate) fn max<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    extreme(lib, cx, t, args, true)
}

/// `min(xs)` or `min(a, b, ...)`, as `max`.
pub(crate) fn min<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    extreme(lib, cx, t, args, false)
}

fn extreme<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], max: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let xs = leaves(lib, cx, args)?;
    let Some((&first, rest)) = xs.split_first() else {
        return Ok(Reply::Done(lib.bad_arg(h, t, 1)));
    };
    let mut m = Meter::default();
    // machine integers without setting up the canonicaliser
    if let View::Int(mut best) = h.view(first) {
        let mut all = true;
        for &x in rest {
            m.step(cx)?;
            let View::Int(v) = h.view(x) else {
                all = false;
                break;
            };
            if (v > best) == max && v != best {
                best = v;
            }
        }
        if all {
            return Ok(Reply::Done(h.int(best)));
        }
    }
    let ar = lib.arith(h);
    let mut best = first;
    if ar.compare(first, first).is_none() {
        return Ok(Reply::Decline);
    }
    let want = if max { Cmp3::Greater } else { Cmp3::Less };
    for &x in rest {
        m.step(cx)?;
        match ar.compare(x, best) {
            Some(c) if c == want => best = x,
            Some(Cmp3::Less | Cmp3::Equal | Cmp3::Greater) => {}
            // a symbolic element, a NaN or overlapping balls: not decided
            _ => return Ok(Reply::Decline),
        }
    }
    Ok(Reply::Done(best))
}
