//! Constructors and iteration: `range`, `array`, `table`, `nest`, `nest_list`, `fold`, `fold_list`, and the maps
//! `map_indexed`, `map_thread`, `scan`.
use crate::ctx::{apply, count_operand, dims_operand, group_cost, nest_flat, take, Lib, Meter, MAX_CELLS, MAX_LEN};
use mc_arith::{Kind, Num, Real};
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// `range(n)`, `range(a, b)`, `range(a, b, step)`: `a, a + step, ...` up to `b` (`a` defaults to 1, `step` to 1).
/// Machine integers are built directly; other exact bounds exactly; with an inexact bound or step the elements are
/// `a + k*step` in the inexact kind, and the count tolerates rounding (decision I.7). A symbolic bound declines.
pub(crate) fn range<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let one = h.int(1);
    let (a, b, s) = match *args {
        [b] => (one, b, one),
        [a, b] => (a, b, one),
        [a, b, s] => (a, b, s),
        _ => return Ok(Reply::Decline),
    };
    if let (View::Int(a), View::Int(b), View::Int(s)) = (h.view(a), h.view(b), h.view(s)) {
        return int_range(lib, cx, t, a, b, s);
    }
    // general bounds: real numbers only
    let ar = lib.arith(h);
    let mut reals = Vec::with_capacity(3);
    // the position of bound k among the arguments (the defaults are numbers, so never reported)
    let pos = |k: usize| if args.len() == 1 { k } else { k + 1 };
    for (k, &x) in [a, b, s].iter().enumerate() {
        match ar.num(x) {
            Some(Num::Real(r)) => reals.push(r),
            Some(Num::Complex(..)) => return Ok(Reply::Done(lib.bad_arg(h, t, pos(k)))),
            None => {
                return Ok(match h.view(x) {
                    View::Sym(_) | View::App { .. } => Reply::Decline,
                    _ => Reply::Done(lib.bad_arg(h, t, pos(k))),
                })
            }
        }
    }
    let [a, b, s] = <[Real; 3]>::try_from(reals).expect("three reals");
    if s.is_zero() {
        return Ok(Reply::Done(lib.bad_arg(h, t, args.len())));
    }
    let Some(n) = count(&a, &b, &s) else {
        return Ok(Reply::Done(lib.too_large(h, t)));
    };
    if n > MAX_LEN {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    // the elements in the most inexact kind of the three (a ball at the largest precision)
    let kind = [&a, &b, &s].iter().map(|r| r.kind()).max().expect("three");
    let prec = [&a, &b, &s].iter().filter_map(|r| r.prec()).max().unwrap_or(mc_arith::number::MIN_PREC);
    let (a, s) = (a.to_kind(kind, prec), s.to_kind(kind, prec));
    let mut out = Vec::with_capacity(n);
    let mut m = Meter::default();
    let mut x = a.clone();
    for k in 0..n {
        m.step(cx)?;
        // inexact elements are a + k*step, not a running sum, so that rounding does not accumulate
        if kind != Kind::Exact && k > 0 {
            x = a.add(&Real::int(k as i64).mul(&s));
        }
        out.push(ar.number(&Num::Real(x.clone())));
        if kind == Kind::Exact {
            x = x.add(&s);
        }
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// The number of elements of `range(a, b, s)`, `s` nonzero; `None` beyond `usize`.
fn count(a: &Real, b: &Real, s: &Real) -> Option<usize> {
    if let (Real::Exact(a), Real::Exact(b), Real::Exact(s)) = (a, b, s) {
        let q = b.sub(a).mul(&s.recip().expect("nonzero step"));
        let (fl, _) = q.floor_fract();
        if fl.neg {
            return Some(0);
        }
        return fl.to_i64().and_then(|v| usize::try_from(v).ok()).and_then(|v| v.checked_add(1));
    }
    let q = (b.to_f64() - a.to_f64()) / s.to_f64();
    if q.is_nan() || q < 0.0 {
        return Some(0);
    }
    // a quotient a few ulps below an integer (0.3/0.1) still reaches it
    let q = (q + q.abs() * 8.0 * f64::EPSILON).floor();
    (q < MAX_LEN as f64).then_some(q as usize + 1)
}

fn int_range<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, a: i64, b: i64, s: i64) -> Res<Reply<'h>> {
    let h = cx.heap();
    if s == 0 {
        return Ok(Reply::Done(lib.bad_arg(h, t, 3)));
    }
    let span = b as i128 - a as i128;
    let n: i128 = if (span < 0) != (s < 0) && span != 0 { 0 } else { span / s as i128 + 1 };
    if n > MAX_LEN as i128 {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let n = n as usize;
    crate::ctx::charge_copy(cx, n)?;
    let mut out = Vec::with_capacity(n);
    let mut m = Meter::default();
    let mut x = a;
    for k in 0..n {
        m.step(cx)?;
        out.push(h.int(x));
        if k + 1 < n {
            // stays between a and b, so no overflow
            x += s;
        }
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// `array(f, n)`: `[f(1), ..., f(n)]`; `array(f, [n1, ..., nd])`: the nested lists of `f(i1, ..., id)`. The calls
/// are evaluated as a group (like `map`).
pub(crate) fn array<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, n] = *args else {
        return Ok(Reply::Decline);
    };
    let ds = take!(dims_operand(lib, h, t, n, 2));
    let total: usize = ds.iter().product();
    let cost = group_cost(cx, f);
    let ds_ = &ds;
    let out = cx.group(total, cost, &|cx: &mut NativeCx<'_, '_, 'h>, i| {
        cx.tick(1)?;
        let h = cx.heap();
        // the multi-index of flat position i, row-major, 1-based
        let mut ix = vec![h.int(0); ds_.len()];
        let mut r = i;
        for (k, &d) in ds_.iter().enumerate().rev() {
            ix[k] = h.int((r % d) as i64 + 1);
            r /= d;
        }
        apply(cx, f, &ix)
    })?;
    Ok(Reply::Done(nest_flat(cx, out, &ds)))
}

/// The values an iterator of `table` runs over: a list's elements, or `1, ..., n` for an integer `n` (none for
/// `n <= 0`). A symbolic range declines.
fn iterator_values<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    r: T<'h>,
    k: usize,
) -> Res<Result<Vec<T<'h>>, Reply<'h>>> {
    let h = cx.heap();
    let r = cx.eval(r)?;
    if let Some(xs) = cx.as_list(r) {
        return Ok(Ok(xs.to_vec()));
    }
    Ok(match h.view(r) {
        View::Int(n) if n as i128 > MAX_CELLS as i128 => Err(Reply::Done(lib.too_large(h, t))),
        View::Int(n) => {
            crate::ctx::charge_copy(cx, n.max(0) as usize)?;
            Ok((1..=n).map(|i| h.int(i)).collect())
        }
        _ if lib.is_failure(h, r) => Err(Reply::Done(r)),
        View::Sym(_) | View::App { .. } => Err(Reply::Decline),
        _ => Err(Reply::Done(lib.bad_arg(h, t, k))),
    })
}

/// `table(e, n)`: `e` evaluated `n` times; `table(e, k, r)`: the list of the values of `e` with `k` bound to each
/// value of the range `r` (a list, or an integer `n` for `1, ..., n`); `table(e, k1, r1, k2, r2, ...)`: nested
/// lists, the first iterator outermost, each range evaluated with the iterators before it bound. Every argument is
/// held: `e` is the body of a closure `k => e` (lexical, hygienic: semantics 6.7), called on each value as a group
/// like `map` (decision 1A.14).
pub(crate) fn table<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (e, k, r) = match *args {
        [e, n] => {
            let n = cx.eval(n)?;
            if lib.is_failure(h, n) {
                return Ok(Reply::Done(n));
            }
            let n = take!(count_operand(lib, h, t, n, 2));
            if n as u128 > MAX_CELLS {
                return Ok(Reply::Done(lib.too_large(h, t)));
            }
            let out = cx.group(n, 0, &|cx: &mut NativeCx<'_, '_, 'h>, _| {
                cx.tick(1)?;
                cx.eval(e)
            })?;
            return Ok(Reply::Done(cx.list(&out)));
        }
        [e, k, r, ..] if args.len() % 2 == 1 => (e, k, r),
        _ => return Ok(Reply::Decline),
    };
    if !matches!(h.view(k), View::Sym(_)) {
        return Ok(Reply::Done(lib.bad_arg(h, t, 2)));
    }
    let vs = take!(iterator_values(lib, cx, t, r, 3)?);
    // the remaining iterators run inside the body
    let body = if args.len() > 3 {
        let mut inner = vec![e];
        inner.extend_from_slice(&args[3..]);
        h.call(lib.s.table, &inner)
    } else {
        e
    };
    let f = cx.eval(h.call(lib.s.fn_, &[cx.list(&[k]), body]))?;
    let cost = group_cost(cx, f);
    let vs_ = &vs;
    let out = cx.group(vs.len(), cost, &|cx: &mut NativeCx<'_, '_, 'h>, i| {
        cx.tick(1)?;
        apply(cx, f, &[vs_[i]])
    })?;
    Ok(Reply::Done(cx.list(&out)))
}

/// `map_indexed(f, xs)`: `[f(x1, 1), f(x2, 2), ...]`, each element with its position (an integer, decision
/// 1A.15), evaluated as a group.
pub(crate) fn map_indexed<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, xs] = *args else { return Ok(Reply::Decline) };
    let xs = take!(lib.list_operand(h, t, xs, 2));
    let cost = group_cost(cx, f);
    let out = cx.group(xs.len(), cost, &|cx: &mut NativeCx<'_, '_, 'h>, i| {
        cx.tick(1)?;
        let h = cx.heap();
        apply(cx, f, &[xs[i], h.int(i as i64 + 1)])
    })?;
    Ok(Reply::Done(cx.list(&out)))
}

/// `map_thread(f, [xs, ys, ...])`: `[f(xs[1], ys[1], ...), f(xs[2], ys[2], ...), ...]`, evaluated as a group;
/// lists of different lengths are `failure(shape_mismatch, map_thread)`.
pub(crate) fn map_thread<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, xss] = *args else { return Ok(Reply::Decline) };
    let xss = take!(lib.list_operand(h, t, xss, 2));
    let mut lists = Vec::with_capacity(xss.len());
    for &xs in xss {
        let Some(xs) = cx.as_list(xs) else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
        if lists.first().is_some_and(|l: &&[T<'h>]| l.len() != xs.len()) {
            return Ok(Reply::Done(lib.shape(h, t)));
        }
        lists.push(xs);
    }
    let n = lists.first().map_or(0, |l| l.len());
    let cost = group_cost(cx, f);
    let lists_ = &lists;
    let out = cx.group(n, cost, &|cx: &mut NativeCx<'_, '_, 'h>, i| {
        cx.tick(1)?;
        let call: Vec<T<'h>> = lists_.iter().map(|l| l[i]).collect();
        apply(cx, f, &call)
    })?;
    Ok(Reply::Done(cx.list(&out)))
}

/// `scan(f, xs)`: `f(x)` evaluated for each element in order, for its effects (`print`, definitions); the value
/// is `null`, or the first failure `f` gives (which ends the scan).
pub(crate) fn scan<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, xs] = *args else { return Ok(Reply::Decline) };
    let (_, xs) = take!(lib.operand(h, t, xs, 2));
    for &x in xs {
        cx.tick(1)?;
        let v = apply(cx, f, &[x])?;
        if lib.is_failure(h, v) {
            return Ok(Reply::Done(v));
        }
    }
    Ok(Reply::Done(cx.null()))
}

/// The count argument of `nest` and `nest_list`: a nonnegative machine integer.
fn times<'h>(lib: &Lib, cx: &NativeCx<'_, '_, 'h>, t: T<'h>, n: T<'h>) -> Result<u64, Reply<'h>> {
    let h = cx.heap();
    match lib.int_operand(h, t, n, 3)? {
        n @ 0.. => Ok(n as u64),
        _ => Err(Reply::Done(lib.bad_arg(h, t, 3))),
    }
}

/// Apply `f` to `x` `n` times, calling `each` with every value (the start included); a failure value ends the
/// iteration and is returned.
fn iterate<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    x: T<'h>,
    n: u64,
    mut each: impl FnMut(T<'h>),
) -> Res<Result<T<'h>, T<'h>>> {
    let h = cx.heap();
    let mut x = x;
    each(x);
    for _ in 0..n {
        cx.tick(1)?;
        x = apply(cx, f, &[x])?;
        if lib.is_failure(h, x) {
            return Ok(Err(x));
        }
        each(x);
    }
    Ok(Ok(x))
}

/// `nest(f, x, n)`: `f(f(...f(x)))` with `n` applications.
pub(crate) fn nest<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let [f, x, n] = *args else {
        return Ok(Reply::Decline);
    };
    let n = take!(times(lib, cx, t, n));
    let r = iterate(lib, cx, f, x, n, |_| {})?;
    Ok(Reply::Done(r.unwrap_or_else(|e| e)))
}

/// `nest_list(f, x, n)`: `[x, f(x), ..., f^n(x)]`.
pub(crate) fn nest_list<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let [f, x, n] = *args else {
        return Ok(Reply::Decline);
    };
    let n = take!(times(lib, cx, t, n));
    if n >= MAX_LEN as u64 {
        return Ok(Reply::Done(lib.too_large(cx.heap(), t)));
    }
    let mut out = Vec::with_capacity(n as usize + 1);
    Ok(Reply::Done(match iterate(lib, cx, f, x, n, |v| out.push(v))? {
        Ok(_) => cx.list(&out),
        Err(e) => e,
    }))
}

/// `fold(f, x, xs)`: `f(...f(f(x, xs[1]), xs[2])..., xs[n])`; `fold(f, xs)` starts from the first element.
pub(crate) fn fold<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let r = take!(fold_with(lib, cx, t, args, |_| {})?);
    Ok(Reply::Done(r))
}

/// `fold_list(f, x, xs)`: `[x, f(x, xs[1]), ...]`, every intermediate value.
pub(crate) fn fold_list<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let mut out = vec![];
    take!(fold_with(lib, cx, t, args, |v| out.push(v))?);
    Ok(Reply::Done(cx.list(&out)))
}

/// The fold of `fold` and `fold_list`; `each` sees every value. An argument error, or a failure value (which ends
/// the fold), is the reply.
fn fold_with<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    args: &[T<'h>],
    mut each: impl FnMut(T<'h>),
) -> Res<Result<T<'h>, Reply<'h>>> {
    let h = cx.heap();
    let (f, x, xs) = match *args {
        [f, x, xs] => match lib.list_operand(h, t, xs, 3) {
            Ok(xs) => (f, x, xs),
            Err(r) => return Ok(Err(r)),
        },
        [f, xs] => match lib.list_operand(h, t, xs, 2) {
            Ok([x, rest @ ..]) => (f, *x, rest),
            Ok([]) => return Ok(Err(Reply::Done(lib.bad_index(h, h.int(1), 0)))),
            Err(r) => return Ok(Err(r)),
        },
        _ => return Ok(Err(Reply::Decline)),
    };
    let mut acc = x;
    each(acc);
    for &a in xs {
        cx.tick(1)?;
        acc = apply(cx, f, &[acc, a])?;
        if lib.is_failure(h, acc) {
            return Ok(Err(Reply::Done(acc)));
        }
        each(acc);
    }
    Ok(Ok(acc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_arith::num::{Int, Rat};

    fn exact(n: i64, d: i64) -> Real {
        Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)))
    }

    #[test]
    fn counts() {
        assert_eq!(count(&exact(1, 1), &exact(10, 1), &exact(1, 1)), Some(10));
        assert_eq!(count(&exact(0, 1), &exact(1, 1), &exact(1, 3)), Some(4));
        assert_eq!(count(&exact(1, 1), &exact(0, 1), &exact(1, 1)), Some(0));
        assert_eq!(count(&exact(5, 1), &exact(1, 1), &exact(-2, 1)), Some(3));
        assert_eq!(count(&Real::Machine(0.0), &Real::Machine(0.3), &Real::Machine(0.1)), Some(4));
        assert_eq!(count(&Real::Machine(0.0), &Real::Machine(1.0), &Real::Machine(0.1)), Some(11));
        assert_eq!(count(&Real::Machine(0.0), &Real::Machine(-1.0), &Real::Machine(0.1)), Some(0));
    }
}
