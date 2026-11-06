//! Reductions: `sum`, `product`, `total`, `max`, `min`.
//!
//! `sum` and `product` build one canonical `add` or `mul` through `mc-arith`, however many terms there are
//! (decision I.8): machine integers are folded in 128 bits, the other exact numbers are combined by a balanced tree
//! (so a sum of `n` rationals costs about `M(size) log n`, not `n` growing additions), and the remaining terms go to
//! a single canonicaliser call with the exact total.
use crate::ctx::{int128, take, Lib, Meter};
use mc_arith::{Arith, Cmp3};
use mc_core::{Heap, Packed, Store, View, T};
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

/// The sum of every leaf of a packed array, computed on its payload exactly as [`reduce`] computes it on the
/// leaves (decision N4a.10): machine integers summed exactly (in 128 bits, which `2^28` leaves cannot overflow);
/// machine reals added in the canonical order, ascending by value, from an exact zero (semantics 2.2, `mc-arith`
/// decision B.5). Equal values are interchangeable in that order, and the running sum starts at `+0.0`, so it never
/// depends on how ties are broken. `None` (the general path) when a real is a NaN.
fn packed_sum<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: Packed<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    crate::ctx::charge_copy(cx, p.count())?;
    match p {
        Packed::Ints { data, .. } => Ok(Some(int128(h, data.iter().map(|&v| v as i128).sum()))),
        Packed::Reals { data, .. } => {
            if data.iter().any(|x| x.is_nan()) {
                return Ok(None);
            }
            let mut v = data.to_vec();
            v.sort_unstable_by(f64::total_cmp);
            // the sort is n log n: charged like the canonicaliser's own pass
            cx.tick(v.len() as u64 / 4)?;
            Ok(Some(h.real(v.iter().fold(0.0, |acc, &x| acc + x))))
        }
    }
}

/// `sum(xs)`: the canonical sum of the elements of the list `xs` (`0` for `[]`).
pub(crate) fn sum<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if args.len() == 2 {
        return crate::sums::iterated(lib, cx, t, args);
    }
    let [xs] = *args else {
        return Ok(Reply::Decline);
    };
    if let Some(p) = h.packed(xs).filter(|p| p.rank() == 1) {
        if let Some(v) = packed_sum(cx, p)? {
            return Ok(Reply::Done(v));
        }
    }
    let xs = take!(lib.list_operand(h, t, xs, 1));
    // lists among the elements are added element-wise (`sum([[1, 2], [3, 4]])` is `[4, 6]`)
    let list = h.sym_term(lib.s.list);
    if xs.iter().any(|&x| matches!(h.view(x), View::App { head, .. } if head == list)) {
        cx.tick(xs.len() as u64)?;
        return Ok(Reply::Again(h.app(h.sym_term(h.sym("add")), xs)));
    }
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

/// Which levels `total` adds up.
#[derive(Clone, Copy)]
enum Levels {
    /// levels 1 to n (`usize::MAX`: every level)
    UpTo(usize),
    /// level n only (n >= 2), or the innermost lists (-1)
    Only(i64),
}

/// `total(xs)`: the sum of the elements of `xs` (lists among them added element-wise, so `total(m)` of a matrix is
/// its column sums); `total(xs, n)`: levels 1 to n added (`inf`: every level, the sum of all leaves); `total(xs,
/// [n])`: the totals at level n only (`[-1]`: of the innermost lists). Decision p6core.6, replacing I.9's sum of all
/// leaves for one argument.
pub(crate) fn total<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, levels) = match *args {
        [xs] => (xs, Levels::UpTo(1)),
        [xs, n] => {
            let levels = match h.view(n) {
                View::Int(k @ 1..) => Levels::UpTo(k as usize),
                View::Sym(s) if h.sym_name(s) == "inf" => Levels::UpTo(usize::MAX),
                View::App { head, args: [k] } if head == h.sym_term(lib.s.list) => match h.view(*k) {
                    View::Int(1) => Levels::UpTo(1),
                    View::Int(k @ 2..) => Levels::Only(k),
                    View::Int(-1) => Levels::Only(-1),
                    _ => return Ok(Reply::Decline),
                },
                _ => return Ok(Reply::Decline),
            };
            (xs, levels)
        }
        _ => return Ok(Reply::Decline),
    };
    let list = h.sym_term(lib.s.list);
    if let Some(p) = h.packed(xs) {
        let all = match levels {
            Levels::UpTo(n) => n >= p.rank(),
            Levels::Only(k) => k == -1 && p.rank() == 1,
        };
        if all {
            if let Some(v) = packed_sum(cx, p)? {
                return Ok(Reply::Done(v));
            }
        } else if matches!(levels, Levels::UpTo(1)) {
            if let Some(v) = packed_columns(cx, p)? {
                return Ok(Reply::Done(v));
            }
        }
    }
    let elems = take!(lib.list_operand(h, t, xs, 1));
    let is_list = |x: T<'h>| matches!(h.view(x), View::App { head, .. } if head == list);
    match levels {
        Levels::UpTo(n) => {
            let flat = if n == usize::MAX { leaves(lib, cx, elems)? } else { flattened(lib, cx, elems, n - 1)? };
            if flat.iter().any(|&x| is_list(x)) {
                // lists left at level n: added element-wise by `add`
                cx.tick(flat.len() as u64)?;
                return Ok(Reply::Again(h.app(h.sym_term(h.sym("add")), &flat)));
            }
            Ok(Reply::Done(reduce(lib, cx, Op::Add, &flat)?))
        }
        Levels::Only(-1) if !elems.iter().any(|&x| is_list(x)) => Ok(Reply::Done(reduce(lib, cx, Op::Add, elems)?)),
        Levels::Only(k) => {
            let inner = cx.list(&[h.int(if k == -1 { -1 } else { k - 1 })]);
            let tot = h.sym_term(h.sym("total"));
            cx.tick(elems.len() as u64)?;
            let parts: Vec<T<'h>> = elems
                .iter()
                .map(|&x| if k == -1 && !is_list(x) { x } else { h.app(tot, &[x, inner]) })
                .collect();
            Ok(Reply::Again(cx.list(&parts)))
        }
    }
}

/// The elements of `xs` with nested lists spliced in to depth `d` (0: `xs` itself).
fn flattened<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, xs: &[T<'h>], d: usize) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let list = h.sym_term(lib.s.list);
    let mut out = xs.to_vec();
    for _ in 0..d {
        if !out.iter().any(|&x| matches!(h.view(x), View::App { head, .. } if head == list)) {
            break;
        }
        let mut next = Vec::with_capacity(out.len());
        let mut m = Meter::default();
        for &x in &out {
            m.step(cx)?;
            match h.view(x) {
                View::App { head, args } if head == list => next.extend_from_slice(args),
                _ => next.push(x),
            }
        }
        out = next;
    }
    Ok(out)
}

/// The sums over the first index of a packed array of rank 2 or more (each sum as [`packed_sum`] computes it).
fn packed_columns<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: Packed<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let dims = p.dims();
    if dims.len() < 2 {
        return Ok(None);
    }
    crate::ctx::charge_copy(cx, p.count())?;
    let rows = dims[0] as usize;
    let width = p.count() / rows;
    let sub: Vec<usize> = dims[1..].iter().map(|&d| d as usize).collect();
    match p {
        Packed::Ints { data, .. } => {
            let mut sums = vec![0i128; width];
            for row in data.chunks(width) {
                for (s, &v) in sums.iter_mut().zip(row) {
                    *s += v as i128;
                }
            }
            let Ok(v) = sums.iter().map(|&s| i64::try_from(s)).collect::<Result<Vec<i64>, _>>() else { return Ok(None) };
            Ok(Some(h.int_array(&sub, &v)))
        }
        Packed::Reals { data, .. } => {
            if data.iter().any(|x| x.is_nan()) {
                return Ok(None);
            }
            cx.tick(p.count() as u64 / 4)?;
            let mut col = Vec::with_capacity(rows);
            let mut sums = Vec::with_capacity(width);
            for j in 0..width {
                col.clear();
                col.extend((0..rows).map(|i| data[i * width + j]));
                col.sort_unstable_by(f64::total_cmp);
                sums.push(col.iter().fold(0.0, |acc, &x| acc + x));
            }
            Ok(Some(h.real_array(&sub, &sums)))
        }
    }
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
        return numeric_extreme(cx, &xs, max);
    }
    let want = if max { Cmp3::Greater } else { Cmp3::Less };
    for &x in rest {
        m.step(cx)?;
        match ar.compare(x, best) {
            Some(c) if c == want => best = x,
            Some(Cmp3::Less | Cmp3::Equal | Cmp3::Greater) => {}
            // a symbolic element, a NaN or overlapping balls: decided by numeric values if they all have one
            _ => return numeric_extreme(cx, &xs, max),
        }
    }
    Ok(Reply::Done(best))
}

/// `max`/`min` of numeric expressions (`max(pi, 3.2, sqrt(10))`): the element with the extreme value of `n(x)`,
/// when every element has a machine-real value and the extreme is clear of rounding (docs bug 8). Otherwise the call
/// stays unevaluated.
fn numeric_extreme<'h>(cx: &mut NativeCx<'_, '_, 'h>, xs: &[T<'h>], max: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    if xs.len() > 1000 {
        return Ok(Reply::Decline);
    }
    let n = h.sym_term(h.sym("n"));
    let mut vals = Vec::with_capacity(xs.len());
    for &x in xs {
        cx.tick(1)?;
        match h.view(cx.eval(h.app(n, &[x]))?) {
            View::Real(v) if v.is_finite() => vals.push(v),
            View::Int(v) => vals.push(v as f64),
            _ => return Ok(Reply::Decline),
        }
    }
    let key = |v: f64| if max { v } else { -v };
    let (i, &v) = vals.iter().enumerate().fold((0, &vals[0]), |b, (i, v)| if key(*v) > key(*b.1) { (i, v) } else { b });
    // a close second could be a rounding artefact: then the call stays
    let tol = 1e-12 * v.abs().max(1e-300);
    if vals.iter().enumerate().any(|(j, &w)| j != i && (w - v).abs() <= tol && xs[j] != xs[i]) {
        return Ok(Reply::Decline);
    }
    Ok(Reply::Done(xs[i]))
}
