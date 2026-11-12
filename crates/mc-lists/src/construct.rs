//! Constructors and iteration: `range`, `array`, `table`, `nest`, `nest_list`, `fold`, `fold_list`, and the maps
//! `map_indexed`, `map_thread`, `scan`.
use crate::ctx::{apply, count_operand, dims_operand, group_cost, nest_flat, take, Lib, Meter, MAX_CELLS, MAX_LEN};
use mc_arith::{Kind, Num, Real};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// `range(n)`, `range(a, b)`, `range(a, b, step)`: `a, a + step, ...` up to `b` (`a` defaults to 1, `step` to 1).
/// Machine integers are built directly; other exact bounds exactly; with an inexact bound or step the elements are
/// `a + k*step` in the inexact kind, and the count tolerates rounding (decision I.7). A symbolic bound declines.
/// `range(a, b, s)` with symbolic bounds whose number of steps `floor((b - a)/s)` evaluates to an integer:
/// `range(0, pi, pi/4)` is `[0, pi/4, pi/2, 3*pi/4, pi]` (docs bug 1). Otherwise the call stays unevaluated.
fn symbolic_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: T<'h>, b: T<'h>, s: T<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let sym = |n: &str| h.sym_term(h.sym(n));
    let (add, mul, pow) = (sym("add"), sym("mul"), sym("pow"));
    let width = h.app(add, &[b, h.app(mul, &[h.int(-1), a])]);
    let steps = h.app(sym("floor"), &[h.app(mul, &[width, h.app(pow, &[s, h.int(-1)])])]);
    let View::Int(n) = h.view(cx.eval(steps)?) else { return Ok(Reply::Decline) };
    if !(-1..=MAX_LEN as i64).contains(&n) {
        return Ok(Reply::Decline);
    }
    cx.tick(n.max(0) as u64)?;
    let out: Vec<T<'h>> = (0..=n).map(|k| h.app(add, &[a, h.app(mul, &[h.int(k), s])])).collect();
    Ok(Reply::Again(cx.list(&out)))
}

/// `range` of quantities (G56): `range(quantity(10, "Feet"))` lists `quantity(k, "Feet")` for `k` in `range(10)`;
/// with two or three bounds every bound is read in the unit of the first quantity among them
/// (`quantity_magnitude(q, unit)`), so `range(quantity(3, "Ounces"), quantity(1, "Pounds"))` counts ounces.
/// `None` when no bound is a quantity.
fn quantity_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let quantity = h.sym_term(h.sym("quantity"));
    let unit = args.iter().find_map(|&x| match h.view(x) {
        View::App { head, args: [_, u] } if head == quantity => Some(*u),
        _ => None,
    });
    let Some(unit) = unit else { return Ok(None) };
    let magnitude = h.sym_term(h.sym("quantity_magnitude"));
    let mut bounds = Vec::with_capacity(args.len());
    for &x in args {
        bounds.push(match h.view(x) {
            View::App { head, .. } if head == quantity => cx.eval(h.app(magnitude, &[x, unit]))?,
            _ => x,
        });
    }
    let ks = cx.eval(h.app(h.sym_term(h.sym("range")), &bounds))?;
    let Some(ks) = cx.as_list(ks) else { return Ok(None) };
    cx.tick(ks.len() as u64)?;
    let qs: Vec<T<'h>> = ks.iter().map(|&k| h.app(quantity, &[k, unit])).collect();
    Ok(Some(Reply::Again(cx.list(&qs))))
}

pub(crate) fn range<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let one = h.int(1);
    // range over a list of bounds: the list of the ranges (range is listable)
    if let [ns] = *args {
        if let Some(ns) = cx.as_list(ns) {
            let r = h.sym("range");
            return Ok(Reply::Again(cx.list(&ns.iter().map(|&n| h.call(r, &[n])).collect::<Vec<_>>())));
        }
    }
    let (a, b, s) = match *args {
        [b] => (one, b, one),
        [a, b] => (a, b, one),
        [a, b, s] => (a, b, s),
        _ => return Ok(Reply::Decline),
    };
    if let (View::Int(a), View::Int(b), View::Int(s)) = (h.view(a), h.view(b), h.view(s)) {
        return int_range(lib, cx, t, a, b, s);
    }
    if let Some(r) = quantity_range(cx, args)? {
        return Ok(r);
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
                return match h.view(x) {
                    View::Sym(_) | View::App { .. } => symbolic_range(cx, a, b, s),
                    _ => Ok(Reply::Done(lib.bad_arg(h, t, pos(k)))),
                }
            }
        }
    }
    // invariant: one real was pushed for each of the three bounds (anything else returned above)
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
    // invariant: an array of three
    let kind = [&a, &b, &s].iter().map(|r| r.kind()).max().expect("three");
    let prec = [&a, &b, &s].iter().filter_map(|r| r.prec()).max().unwrap_or(mc_arith::number::MIN_PREC);
    let (a, s) = (a.to_kind(kind, prec), s.to_kind(kind, prec));
    let mut m = Meter::default();
    if kind == Kind::Machine {
        // machine reals straight into a packed payload (decision N4a.9): the same elements as below
        // `a + k*step` in machine arithmetic, as `Real::add` and `Real::mul` compute it for machine reals (k below
        // 2^28 converts exactly)
        let (a, s) = (a.to_f64(), s.to_f64());
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            m.step(cx)?;
            out.push(if k == 0 { a } else { a + k as f64 * s });
        }
        return Ok(Reply::Done(h.reals(&out)));
    }
    let mut out = Vec::with_capacity(n);
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
        // invariant: the caller checked that the step is nonzero
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
        out.push(x);
        if k + 1 < n {
            // stays between a and b, so no overflow
            x += s;
        }
    }
    // machine integers straight into a packed payload when long enough (decision N4a.9)
    Ok(Reply::Done(h.ints(&out)))
}

/// `array(f, n)`: `[f(1), ..., f(n)]`; `array(f, [n1, ..., nd])`: the nested lists of `f(i1, ..., id)`. The calls
/// are evaluated as a group (like `map`).
pub(crate) fn array<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (f, n, origin, head) = match *args {
        [f, n] => (f, n, None, None),
        [f, n, r] => (f, n, Some(r), None),
        // array(f, dims, origin, g): the head g instead of list at every level
        [f, n, r, g] => (f, n, Some(r), Some(g)),
        _ => return Ok(Reply::Decline),
    };
    let ds = take!(dims_operand(lib, h, t, n, 2));
    // each dimension's indices: from 1, from the origin given (`array(f, 3, 0)` is [f(0), f(1), f(2)]), or d
    // equally spaced points of a range [a, b] (`array(f, 5, [2, 14])`)
    let int = |r: T<'h>| match h.view(r) {
        View::Int(r) => Some(r),
        _ => None,
    };
    let range = |r: T<'h>| match cx.as_list(r) {
        Some(&[a, b]) if cx.as_list(a).is_none() && cx.as_list(b).is_none() => Some((a, b)),
        _ => None,
    };
    let dims: Vec<Dim<'h>> = match origin {
        None => vec![Dim::Start(1); ds.len()],
        Some(r) if int(r).is_some() => vec![Dim::Start(int(r).unwrap_or(1)); ds.len()],
        Some(r) if ds.len() == 1 && range(r).is_some() => match range(r) {
            Some((a, b)) => vec![Dim::Range(a, b)],
            None => return Ok(Reply::Decline),
        },
        Some(r) => match cx.as_list(r) {
            Some(rs) if rs.len() == ds.len() => {
                let mut v = vec![];
                for &r in rs {
                    match (int(r), range(r)) {
                        (Some(k), _) => v.push(Dim::Start(k)),
                        (None, Some((a, b))) => v.push(Dim::Range(a, b)),
                        _ => return Ok(Reply::Decline),
                    }
                }
                v
            }
            _ => return Ok(Reply::Decline),
        },
    };
    let dims_ = &dims;
    let total: usize = ds.iter().product();
    let cost = group_cost(cx, f);
    let ds_ = &ds;
    let out = cx.group(total, cost, &|cx: &mut NativeCx<'_, '_, 'h>, i| {
        cx.tick(1)?;
        let h = cx.heap();
        // the multi-index of flat position i, row-major
        let mut ix = vec![h.int(0); ds_.len()];
        let mut r = i;
        for (k, &d) in ds_.iter().enumerate().rev() {
            let j = (r % d) as i64;
            ix[k] = match dims_[k] {
                Dim::Start(s) => h.int(j + s),
                // a + (b - a) j/(d - 1)
                Dim::Range(a, b) if d > 1 => {
                    let step = h.call(h.sym("mul"), &[h.call(h.sym("add"), &[b, h.call(h.sym("mul"), &[h.int(-1), a])]), h.int(j), h.call(h.sym("pow"), &[h.int(d as i64 - 1), h.int(-1)])]);
                    cx.eval(h.call(h.sym("add"), &[a, step]))?
                }
                Dim::Range(a, _) => a,
            };
            r /= d;
        }
        apply(cx, f, &ix)
    })?;
    let nested = nest_flat(cx, out, &ds);
    let Some(g) = head else { return Ok(Reply::Done(nested)) };
    Ok(Reply::Again(with_head(cx, nested, g, ds.len())))
}

/// One dimension of `array`: indices from a start, or equally spaced points of a range.
#[derive(Clone, Copy)]
enum Dim<'h> {
    Start(i64),
    Range(T<'h>, T<'h>),
}

/// The nested lists `t` (`depth` levels) with head `g` instead of `list` at every level.
fn with_head<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>, g: T<'h>, depth: usize) -> T<'h> {
    let h = cx.heap();
    match (depth, cx.as_list(t)) {
        (1.., Some(xs)) => {
            let ys: Vec<T<'h>> = xs.iter().map(|&x| with_head(cx, x, g, depth - 1)).collect();
            h.app(g, &ys)
        }
        _ => t,
    }
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
/// like `map` (decision 1A.14). An iterator is a symbol and its range, or a count alone: `table(e, 4, 7)` is four
/// lists of seven values of `e`, `table(e, 3, k, r)` three copies of `table(e, k, r)` (decision io-core.3).
pub(crate) fn table<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    // `parallel: true` as the last argument splits the values over the worker threads (decision sp-core.4)
    match crate::parallel::option(cx, args)? {
        Some((rest, parallel)) => table_with(lib, cx, t, rest, parallel),
        None => table_with(lib, cx, t, args, false),
    }
}

/// `table` with the values evaluated as a group split by the granularity rule, or (`parallel`) over the worker
/// threads at once.
pub(crate) fn table_with<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], parallel: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    // the iterator lists `[k, a, b]` and `[k, a, b, step]` are `k, range(a, b)` and `k, range(a, b, step)`: bounds
    // and steps of any numeric kind (R24-04)
    let iter_list = |t: T<'h>| match h.view(t) {
        View::App { head, args: it @ ([_, _] | [_, _, _] | [_, _, _, _]) } if head == h.sym_term(h.sym(mc_api::names::LIST)) && matches!(h.view(it[0]), View::Sym(_)) => Some(it),
        _ => None,
    };
    // only in an iterator slot: a list right after a variable is that variable's values (`table(k, k, [x, y, z])`,
    // X26-9: round 25 read `[x, y, z]` there as an iterator and broke every table over a list of symbols)
    let mut lowered = false;
    let mut out = vec![args[0]];
    let mut i = 1;
    while i < args.len() {
        let a = args[i];
        if matches!(h.view(a), View::Sym(_)) && i + 1 < args.len() {
            // a variable and its values or range, taken as they are
            out.push(a);
            out.push(args[i + 1]);
            i += 2;
            continue;
        }
        match iter_list(a) {
            // `[k, n]` and `[k, [x1, ...]]`: the variable and its count or values as they are
            Some([k, n]) => {
                out.push(*k);
                out.push(*n);
                lowered = true;
            }
            Some(it) => {
                out.push(it[0]);
                out.push(h.call(h.sym("range"), &it[1..]));
                lowered = true;
            }
            None => out.push(a),
        }
        i += 1;
    }
    if lowered {
        return table_with(lib, cx, t, &out, parallel);
    }
    // a count iterator before more iterators, `table(e, n, ...)`: n copies of `table(e, ...)` (decision io-core.3)
    if args.len() >= 3 && !matches!(h.view(args[1]), View::Sym(_)) {
        let mut inner = vec![args[0]];
        inner.extend_from_slice(&args[2..]);
        let inner = h.call(lib.s.table, &inner);
        return table_with(lib, cx, t, &[inner, args[1]], parallel);
    }
    // `table(e, k, 1, 3)`: the range `1` and then a count `3`, which is rarely what was meant (R21-G3): say how to
    // write k from 1 to 3 (the value is still the documented one)
    if let [_, k, lo, n] = *args {
        if matches!(h.view(k), View::Sym(_)) && matches!(h.view(lo), View::Int(_)) && matches!(h.view(n), View::Int(_)) {
            cx.effect()?;
            let text = "the iterator `k, a, b` is a variable and its range, then a count: for k from a to b write `k, range(a, b)`";
            cx.emit(h.call(h.sym("message"), &[h.sym_term(h.sym("table")), h.string(text)]))?;
        }
    }
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
            let elem = |cx: &mut NativeCx<'_, '_, 'h>, _| {
                cx.tick(1)?;
                cx.eval(e)
            };
            let out = if parallel { cx.group_parallel(n, &elem)? } else { cx.group(n, 0, &elem)? };
            return Ok(Reply::Done(cx.list(&out)));
        }
        // a symbol alone at the end is neither an iterator nor a count
        [.., last] if args.len().is_multiple_of(2) && matches!(h.view(last), View::Sym(_)) => return Ok(Reply::Decline),
        [e, k, r, ..] => (e, k, r),
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
    let elem = |cx: &mut NativeCx<'_, '_, 'h>, i: usize| {
        cx.tick(1)?;
        apply(cx, f, &[vs_[i]])
    };
    let out = if parallel { cx.group_parallel(vs.len(), &elem)? } else { cx.group(vs.len(), cost, &elem)? };
    Ok(Reply::Done(cx.list(&out)))
}

/// `map_indexed(f, xs)`: `[f(x1, 1), f(x2, 2), ...]`, each element with its position (an integer, decision
/// 1A.15), evaluated as a group.
pub(crate) fn map_indexed<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // heads: true: the heads are parts too, positions as index lists (round 37); heads: false is the plain form
    let (args, heads) = crate::ctx::option(h, args, 2, "heads");
    if let Some(v) = heads {
        match h.view(v) {
            View::Sym(s) if h.sym_name(s) == "true" => {
                let levels = match *args {
                    [_, _] => crate::levels::Levels::only(1),
                    [_, _, spec] => match crate::levels::level_spec(h, spec) {
                        Some(levels) => levels,
                        None => return Ok(Reply::Decline),
                    },
                    _ => return Ok(Reply::Decline),
                };
                return crate::levels::map_indexed_with_heads(cx, args[0], args[1], levels);
            }
            View::Sym(s) if h.sym_name(s) == "false" => {}
            _ => return Ok(Reply::Decline),
        }
    }
    // with levels, the positions are index lists (decision p6core.9)
    if let [f, x, spec] = *args {
        return match crate::levels::level_spec(h, spec) {
            Some(levels) => crate::levels::map_indexed_at(cx, f, x, levels),
            None => Ok(Reply::Decline),
        };
    }
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
    // map_thread(f, xss, n): threaded at level n, map_thread(g, xss) with g threading the next level
    if let [f, xss, n] = *args {
        let View::Int(n) = h.view(n) else { return Ok(Reply::Decline) };
        if n == 1 {
            return Ok(Reply::Again(h.call(h.sym("map_thread"), &[f, xss])));
        }
        if n < 1 {
            return Ok(Reply::Decline);
        }
        let xss = take!(lib.list_operand(h, t, xss, 2));
        let Some(first) = xss.first().and_then(|&x| cx.as_list(x)) else { return Ok(Reply::Decline) };
        let len = first.len();
        let mut rows = Vec::with_capacity(len);
        for i in 0..len {
            let mut col = Vec::with_capacity(xss.len());
            for &x in xss {
                match cx.as_list(x) {
                    Some(v) if v.len() == len => col.push(v[i]),
                    _ => return Ok(Reply::Done(lib.shape(h, t))),
                }
            }
            rows.push(h.call(h.sym("map_thread"), &[f, cx.list(&col), h.int(n - 1)]));
        }
        return Ok(Reply::Again(cx.list(&rows)));
    }
    let [f, xss] = *args else { return Ok(Reply::Decline) };
    let xss = take!(lib.list_operand(h, t, xss, 2));
    // associations with the same keys: the association of `f` of each key's values (decision io-core.4)
    if !xss.is_empty() && xss.iter().all(|&a| crate::assocs::entries(lib, h, a).is_some()) {
        let mut vals = Vec::with_capacity(xss.len());
        for &a in xss {
            let es = crate::assocs::entries(lib, h, a).unwrap_or_default();
            vals.push(cx.list(&es.iter().map(|&(_, v)| v).collect::<Vec<_>>()));
        }
        let keys: Vec<T<'h>> = crate::assocs::entries(lib, h, xss[0]).unwrap_or_default().iter().map(|&(k, _)| k).collect();
        // the same keys in the same order: the values thread; anything else is left as it is
        if xss.iter().any(|&a| crate::assocs::entries(lib, h, a).unwrap_or_default().iter().map(|&(k, _)| k).ne(keys.iter().copied())) {
            return Ok(Reply::Done(lib.shape(h, t)));
        }
        let inner = h.app(h.sym_term(h.sym("map_thread")), &[f, cx.list(&vals)]);
        let mapped = cx.eval(inner)?;
        let Some(ys) = cx.as_list(mapped) else { return Ok(Reply::Done(mapped)) };
        let rule = h.sym_term(lib.s.rule);
        let rules: Vec<T<'h>> = keys.iter().zip(ys).map(|(&k, &y)| h.app(rule, &[k, y])).collect();
        return Ok(Reply::Done(h.app(h.sym_term(lib.s.assoc), &rules)));
    }
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
    // an undefined symbol `f`: f(x) of a normal form is one, built without evaluating it again (so a deep nest of
    // an undefined function costs no evaluation depth)
    let inert = matches!(h.view(f), View::Sym(s) if cx.is_inert(s));
    each(x);
    for _ in 0..n {
        cx.tick(1)?;
        x = if inert { h.app(f, &[x]) } else { apply(cx, f, &[x])? };
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
    if let Some(v) = packed_fold(cx, args, false)? {
        return Ok(Reply::Done(v));
    }
    let r = take!(fold_with(lib, cx, t, args, &mut None, |_| {})?);
    Ok(Reply::Done(r))
}

/// `fold_list(f, x, xs)`: `[x, f(x, xs[1]), ...]`, every intermediate value.
pub(crate) fn fold_list<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    if let Some(v) = packed_fold(cx, args, true)? {
        return Ok(Reply::Done(v));
    }
    let h = cx.heap();
    // fold_list(f, []) has no intermediate values: [] (decision r37.4)
    if let [_, xs] = *args {
        if cx.as_list(xs).is_some_and(|v| v.is_empty()) {
            return Ok(Reply::Done(xs));
        }
    }
    let mut out = vec![];
    let mut head = None;
    take!(fold_with(lib, cx, t, args, &mut head, |v| out.push(v))?);
    // the intermediate values keep the head of the elements' term (fold_list(f, x, p(a, b)) is p(x, f(x, a), ...))
    let list = h.sym_term(lib.s.list);
    Ok(match head {
        Some(g) if g != list => Reply::Again(h.app(g, &out)),
        _ => Reply::Done(cx.list(&out)),
    })
}

/// `differences(xs)`: `[x2 - x1, x3 - x2, ...]`; `differences(xs, n)`: the n-th differences. On a packed array of
/// machine reals or integers the payload is subtracted directly (the same values as `x2 + (-1)*x1`).
pub(crate) fn differences<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, n) = match *args {
        [xs] => (xs, 1),
        [xs, n] => match h.view(n) {
            View::Int(n @ 0..) => (xs, n),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    if n == 0 {
        return Ok(Reply::Done(xs));
    }
    if let Some(p) = h.packed(xs).filter(|p| p.rank() == 1) {
        crate::ctx::charge_copy(cx, p.count())?;
        let once = |x: T<'h>| -> Option<T<'h>> {
            match h.packed(x)? {
                mc_core::Packed::Reals { data, .. } => {
                    let v: Vec<f64> = data.windows(2).map(|w| w[1] - w[0]).collect();
                    Some(h.reals(&v))
                }
                mc_core::Packed::Ints { data, .. } => {
                    let v: Option<Vec<i64>> = data.windows(2).map(|w| w[1].checked_sub(w[0])).collect();
                    Some(h.ints(&v?))
                }
                mc_core::Packed::Complexes { data, .. } => {
                    // x2 + (-1)*x1 part by part: the product negates exactly, the sum is commutative
                    if data.iter().any(|x| !x.is_finite()) {
                        return None;
                    }
                    let v: Vec<f64> = data.windows(4).step_by(2).flat_map(|w| [w[2] + -w[0], w[3] + -w[1]]).collect();
                    Some(h.complexes(&v))
                }
            }
        };
        if let Some(d) = once(xs) {
            return Ok(Reply::Again(if n == 1 { d } else { h.app(h.sym_term(h.sym("differences")), &[d, h.int(n - 1)]) }));
        }
    }
    let ys = take!(lib.list_operand(h, t, xs, 1));
    let (add, mul) = (h.sym_term(h.sym("add")), h.sym_term(h.sym("mul")));
    cx.tick(ys.len() as u64)?;
    let d: Vec<T<'h>> = ys.windows(2).map(|w| h.app(add, &[w[1], h.app(mul, &[h.int(-1), w[0]])])).collect();
    let d = cx.list(&d);
    Ok(Reply::Again(if n == 1 { d } else { h.app(h.sym_term(h.sym("differences")), &[d, h.int(n - 1)]) }))
}

/// Whether `f` is addition or multiplication: `add`, `mul`, or `(a, b) => a + b` (`a*b`, either order). `Some(true)`
/// for a product.
fn arithmetic_op(h: &Heap, f: T<'_>) -> Option<bool> {
    let op = |t: T<'_>| match h.view(t) {
        View::Sym(s) => match h.sym_name(s).as_str() {
            "add" => Some(false),
            "mul" => Some(true),
            _ => None,
        },
        _ => None,
    };
    if let Some(p) = op(f) {
        return Some(p);
    }
    let View::App { head, args: [params, body] } = h.view(f) else { return None };
    if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "fn") {
        return None;
    }
    let View::App { args: [a, b], .. } = h.view(*params) else { return None };
    let View::App { head: g, args: [x, y] } = h.view(*body) else { return None };
    let (View::Sym(sa), View::Sym(sb)) = (h.view(*a), h.view(*b)) else { return None };
    let same = (*x == *a && *y == *b) || (*x == *b && *y == *a);
    (sa != sb && same).then(|| op(g)).flatten()
}

/// `fold`/`fold_list` (`list`) of addition or multiplication over a packed array of machine reals or integers,
/// on its payload (decision p6core.30): the values the scalar loop gives, bit for bit (each step adds or
/// multiplies two machine numbers, an exact start converted first, as the arithmetic folds them); a packed result.
/// `None` when the shortcut does not apply (other functions or elements, non-finite reals, integer overflow).
fn packed_fold<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], list: bool) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let (f, start, xs) = match *args {
        [f, x, xs] => (f, Some(x), xs),
        [f, xs] => (f, None, xs),
        _ => return Ok(None),
    };
    let Some(p) = h.packed(xs) else { return Ok(None) };
    if p.rank() != 1 {
        return Ok(None);
    }
    let Some(product) = arithmetic_op(h, f) else { return Ok(None) };
    crate::ctx::charge_copy(cx, p.count())?;
    match p {
        mc_core::Packed::Ints { data, .. } => {
            let mut acc: i64 = match start.map(|x| h.view(x)) {
                None if data.is_empty() => return Ok(None),
                None => data[0],
                Some(View::Int(k)) => k,
                Some(_) => return Ok(None),
            };
            let rest = if start.is_none() { &data[1..] } else { data };
            let mut out = Vec::with_capacity(if list { rest.len() + 1 } else { 0 });
            if list {
                out.push(acc);
            }
            for &x in rest {
                let Some(v) = (if product { acc.checked_mul(x) } else { acc.checked_add(x) }) else { return Ok(None) };
                acc = v;
                if list {
                    out.push(acc);
                }
            }
            Ok(Some(if list { h.ints(&out) } else { h.int(acc) }))
        }
        mc_core::Packed::Complexes { .. } => Ok(None),
        mc_core::Packed::Reals { data, .. } => {
            if data.iter().any(|x| !x.is_finite()) {
                return Ok(None);
            }
            // the start: a machine real, or an exact integer (kept as it is in the list, converted in the first step)
            let (mut acc, first) = match start.map(|x| h.view(x)) {
                None if data.is_empty() => return Ok(None),
                None => (data[0], None),
                Some(View::Real(x)) if x.is_finite() => (x, None),
                Some(View::Int(k)) if k.unsigned_abs() <= 1 << 53 => (k as f64, start),
                Some(_) => return Ok(None),
            };
            let rest = if start.is_none() { &data[1..] } else { data };
            let mut out = Vec::with_capacity(if list { rest.len() + 1 } else { 0 });
            if list && first.is_none() {
                out.push(acc);
            }
            for &x in rest {
                acc = if product { acc * x } else { acc + x };
                if list {
                    out.push(acc);
                }
            }
            if !list {
                // an exact start with no elements stays exact
                return Ok(Some(match (first, rest.is_empty()) {
                    (Some(x), true) => x,
                    _ => h.real(acc),
                }));
            }
            Ok(Some(match first {
                None => h.reals(&out),
                Some(x) => {
                    let mut terms = Vec::with_capacity(out.len() + 1);
                    terms.push(x);
                    terms.extend(out.iter().map(|&v| h.real(v)));
                    cx.list(&terms)
                }
            }))
        }
    }
}

/// The fold of `fold` and `fold_list`; `each` sees every value. An argument error, or a failure value (which ends
/// the fold), is the reply.
fn fold_with<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    args: &[T<'h>],
    head: &mut Option<T<'h>>,
    mut each: impl FnMut(T<'h>),
) -> Res<Result<T<'h>, Reply<'h>>> {
    let h = cx.heap();
    // the elements may come in a term with any head (fold(f, x, p(a, b)) is f(f(x, a), b), decision r37.4); the
    // head is reported for `fold_list`
    let (f, x, xs) = match *args {
        [f, x, xs] => match lib.elements_operand(h, t, xs, 3) {
            Ok((g, xs)) => {
                *head = Some(g);
                (f, x, xs)
            }
            Err(r) => return Ok(Err(r)),
        },
        [f, xs] => match lib.elements_operand(h, t, xs, 2) {
            Ok((g, [x, rest @ ..])) => {
                *head = Some(g);
                (f, *x, rest)
            }
            Ok((_, [])) => return Ok(Err(Reply::Done(lib.bad_index(h, h.int(1), 0)))),
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

/// `subdivide(n)`, `subdivide(b, n)`, `subdivide(a, b, n)`: the `n + 1` equally spaced values from `a` (default 0)
/// to `b` (default 1), `a + (b - a)*k/n` for `k = 0..n`, exact for exact ends.
pub(crate) fn subdivide<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (a, b, n) = match *args {
        [n] => (h.int(0), h.int(1), n),
        [b, n] => (h.int(0), b, n),
        [a, b, n] => (a, b, n),
        _ => return Ok(Reply::Decline),
    };
    let n = match h.view(n) {
        View::Int(n @ 1..) => n,
        View::Int(_) => return Ok(Reply::Done(lib.bad_arg(h, t, args.len()))),
        _ => return Ok(Reply::Decline),
    };
    if n as u64 > crate::ctx::MAX_LEN as u64 {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let (add, mul, pow) = (h.sym_term(h.sym("add")), h.sym_term(h.sym("mul")), h.sym_term(h.sym("pow")));
    let width = h.app(add, &[b, h.app(mul, &[h.int(-1), a])]);
    let step = h.app(mul, &[width, h.app(pow, &[h.int(n), h.int(-1)])]);
    crate::ctx::charge_copy(cx, n as usize + 1)?;
    let point = |k: i64| h.app(add, &[a, h.app(mul, &[h.int(k), step])]);
    if let Some(v) = machine_subdivide(cx, a, b, n, &point)? {
        return Ok(Reply::Done(v));
    }
    let out: Vec<T<'h>> = (0..=n).map(point).collect();
    Ok(Reply::Again(cx.list(&out)))
}

/// `subdivide(a, b, n)` with machine-real end points (or one an integer of magnitude at most 2^53), computed
/// directly (a hot spot of the profiles, decision sp-core.6): the general path's value of point k,
/// `a + k*((b - a)*n^(-1))`, is `a + k*(n^(-1)*(b + (-a)))` in machine arithmetic (the exact factors converted
/// first, the inexact ones in either order: one operation each). Point 0 is the general path's (it may be exact),
/// and points 1, 2 and n are checked against it: any difference leaves the general path in charge.
fn machine_subdivide<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    a: T<'h>,
    b: T<'h>,
    n: i64,
    point: &dyn Fn(i64) -> T<'h>,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let num = |t: T<'h>| match h.view(t) {
        View::Real(x) if x.is_finite() => Some((x, true)),
        View::Int(k) if k.unsigned_abs() <= 1 << 53 => Some((k as f64, false)),
        _ => None,
    };
    let (Some((fa, ra)), Some((fb, rb))) = (num(a), num(b)) else { return Ok(None) };
    if !(ra || rb) || n < 3 || n as u64 > 1 << 53 {
        return Ok(None);
    }
    let step = (1.0 / n as f64) * (fb + -fa);
    let value = |k: i64| fa + k as f64 * step;
    for k in [1, 2, n] {
        if !matches!(h.view(cx.eval(point(k))?), View::Real(x) if x.to_bits() == value(k).to_bits()) {
            return Ok(None);
        }
    }
    let first = cx.eval(point(0))?;
    Ok(Some(match h.view(first) {
        View::Real(x0) => {
            let vals: Vec<f64> = std::iter::once(x0).chain((1..=n).map(value)).collect();
            h.reals(&vals)
        }
        _ => {
            let items: Vec<T<'h>> = std::iter::once(first).chain((1..=n).map(|k| h.real(value(k)))).collect();
            cx.list(&items)
        }
    }))
}

/// The steps of `nest_while` (`list` false) and `nest_while_list`: `f` applied from `x` while `test` holds, at most
/// `max` times (decision io-core.8). The arguments after `test`: `m`, how many of the latest values the test sees
/// (an integer: the test waits until there are that many; `all`: every value so far; `[mmin, m]`: it waits for
/// `mmin`); `max`; and `n`, applications after the end (`n > 0`) or values dropped from the end (`n < 0`).
fn nest_while_steps<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    args: &[T<'h>],
    list: bool,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (&[f, x, test], rest) = args.split_at(args.len().min(3)) else { return Ok(Reply::Decline) };
    if rest.len() > 3 {
        return Ok(Reply::Decline);
    }
    let count = |v: T<'h>| match h.view(v) {
        View::Int(n @ 0..) => Some(n as u64),
        View::Sym(s) if h.sym_name(s) == "inf" => Some(u64::MAX),
        _ => None,
    };
    let (mmin, m) = match rest.first().map(|&v| h.view(v)) {
        None => (1, 1),
        Some(View::Int(k @ 1..)) => (k as usize, k as usize),
        Some(View::Sym(s)) if h.sym_name(s) == "all" => (1, usize::MAX),
        Some(View::App { .. }) => match cx.as_list(rest[0]) {
            Some(&[a, b]) => match (h.view(a), count(b)) {
                (View::Int(a @ 0..), Some(b)) if b >= 1 => ((a as usize).max(1), usize::try_from(b).unwrap_or(usize::MAX)),
                _ => return Ok(Reply::Decline),
            },
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    let max = match rest.get(1) {
        None => u64::MAX,
        Some(&v) => match count(v) {
            Some(n) => n,
            None => return Ok(Reply::Decline),
        },
    };
    let extra = match rest.get(2).map(|&v| h.view(v)) {
        None => 0,
        Some(View::Int(n)) => n,
        _ => return Ok(Reply::Decline),
    };
    let mut out = vec![x];
    let mut cur = x;
    let mut steps = 0u64;
    // the values kept: all of them for a list, else the window the test and `n` need
    let window = if list { usize::MAX } else { m.max(extra.unsigned_abs() as usize + 1) };
    while steps < max {
        cx.tick(1)?;
        if out.len() >= mmin {
            let k = out.len().min(m);
            let ok = apply(cx, test, &out[out.len() - k..])?;
            if lib.is_failure(h, ok) {
                return Ok(Reply::Done(ok));
            }
            if !cx.is_true(ok) {
                break;
            }
        }
        cur = apply(cx, f, &[cur])?;
        if lib.is_failure(h, cur) {
            return Ok(Reply::Done(cur));
        }
        steps += 1;
        // the list of an unbounded loop (a test that never fails) is cut like `fixed_point_list`'s
        if out.len() >= if list && max == u64::MAX { UNBOUNDED_LEN } else { MAX_LEN } {
            return Ok(Reply::Done(lib.too_large(h, t)));
        }
        out.push(cur);
        if out.len() > window.saturating_mul(2).max(64) {
            out.drain(..out.len() - window);
        }
    }
    for _ in 0..extra.max(0) {
        cx.tick(1)?;
        cur = apply(cx, f, &[cur])?;
        if lib.is_failure(h, cur) {
            return Ok(Reply::Done(cur));
        }
        out.push(cur);
    }
    if extra < 0 {
        let drop = (extra.unsigned_abs() as usize).min(out.len());
        out.truncate(out.len() - drop);
    }
    if list {
        return Ok(Reply::Done(cx.list(&out)));
    }
    Ok(match out.last() {
        Some(&v) => Reply::Done(v),
        None => Reply::Decline,
    })
}

/// `nest_while(f, x, test)`, `nest_while(f, x, test, m, max, n)`: `f` applied from `x` as long as `test` of the
/// latest value (the latest `m` values) is true, at most `max` times.
pub(crate) fn nest_while<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    nest_while_steps(lib, cx, t, args, false)
}

/// `nest_while_list(f, x, test)`: the list of the values `nest_while` goes through, `x` first.
pub(crate) fn nest_while_list<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    nest_while_steps(lib, cx, t, args, true)
}

/// The most values `fixed_point_list(f, x)` collects before it gives up (no count was asked for).
const UNBOUNDED_LEN: usize = 1 << 22;

/// Machine reals that differ in at most their last few bits (relative 2^-46): an iteration such as Newton's step for
/// a square root settles into a two-cycle of neighbouring doubles and never repeats exactly, so such values count
/// as the same when a fixed point is looked for.
fn machine_close<'h>(ar: &mc_arith::Arith<'h, Heap>, a: T<'h>, b: T<'h>) -> bool {
    use mc_arith::{Num, Real};
    match (ar.num(a), ar.num(b)) {
        (Some(Num::Real(Real::Machine(x))), Some(Num::Real(Real::Machine(y)))) => {
            x.is_finite() && y.is_finite() && (x - y).abs() <= x.abs().max(y.abs()) * (-46f64).exp2()
        }
        _ => false,
    }
}

/// `fixed_point_list(f, x)`, `fixed_point_list(f, x, n)`: `x, f(x), f(f(x)), ...` until a value repeats the one
/// before it (numbers that agree count as repeating), at most `n` applications.
pub(crate) fn fixed_point_list<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `same_test: g`: the iteration stops when `g(previous, next)` is true (decision io-core.8)
    let (args, test) = crate::ctx::option(h, args, 2, "same_test");
    let (f, x, max) = match *args {
        [f, x] => (f, x, u64::MAX),
        [f, x, n] => match h.view(n) {
            View::Int(n @ 0..) => (f, x, n as u64),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    let ar = lib.arith(h);
    let mut out = vec![x];
    let mut cur = x;
    let mut steps = 0u64;
    while steps < max {
        cx.tick(1)?;
        let next = apply(cx, f, &[cur])?;
        if lib.is_failure(h, next) {
            return Ok(Reply::Done(next));
        }
        steps += 1;
        // without an explicit count a sequence that never settles (an undefined f gives f(f(f(...))) forever) would
        // grow until memory ends: such a call is cut at UNBOUNDED_LEN values
        if out.len() >= if max == u64::MAX { UNBOUNDED_LEN } else { MAX_LEN } {
            return Ok(Reply::Done(lib.too_large(h, t)));
        }
        out.push(next);
        // the same term, or numbers that are equal or cannot be told apart (overlapping balls)
        let same = match test {
            Some(g) => {
                let v = apply(cx, g, &[cur, next])?;
                if lib.is_failure(h, v) {
                    return Ok(Reply::Done(v));
                }
                cx.is_true(v)
            }
            None => {
                next == cur
                    || (ar.num(next).is_some() && ar.num(cur).is_some() && ar.equal(next, cur) != Some(Some(false)))
                    || machine_close(&ar, next, cur)
            }
        };
        if same {
            break;
        }
        cur = next;
    }
    Ok(Reply::Done(cx.list(&out)))
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
