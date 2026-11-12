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
    // quantities and uncertain numbers: their arithmetic is the evaluator's (decision io-core.5)
    if other.iter().any(|&x| cx.is_value(x)) {
        let f = match op {
            Op::Add => mc_api::names::ADD,
            Op::Mul => mc_api::names::MUL,
        };
        // without the exact identity, which a quantity does not add to
        if exact[0] == int128(h, op.unit()) {
            other.pop();
        }
        return cx.eval(h.call(h.sym(f), &other));
    }
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
            // a plain sum in the canonical order, the same as the leaves' sum and compiled code (compensated summation
            // is `total(xs, method: "CompensatedSummation")`)
            Ok(Some(h.real(v.iter().fold(0.0, |acc, &x| acc + x))))
        }
        // the canonical order of complex numbers is the general path's
        Packed::Complexes { .. } => Ok(None),
    }
}

/// `nl_total_compensated(xs)`: Neumaier's compensated sum (Neumaier 1974) of a list of machine numbers, in the order
/// given: a running compensation collects the low bits each addition loses, so a million `0.1` add to `100000.0`.
/// The helper of `total(xs, method: "CompensatedSummation")`; a list with anything but machine reals and integers goes
/// to the library's `b29_total_fold`.
pub(crate) fn total_compensated<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs] = *args else { return Ok(Reply::Decline) };
    let fold = || Ok(Reply::Again(h.call(h.sym("b29_total_fold"), &[xs])));
    let values: Vec<f64> = match h.packed(xs) {
        Some(Packed::Reals { data, .. }) if h.packed(xs).is_some_and(|p| p.rank() == 1) => data.to_vec(),
        Some(_) => return fold(),
        None => {
            let View::App { head, args: items } = h.view(xs) else { return Ok(Reply::Decline) };
            if head != h.sym_term(lib.s.list) {
                return Ok(Reply::Decline);
            }
            let mut v = Vec::with_capacity(items.len());
            for &it in items {
                match h.view(it) {
                    View::Real(x) => v.push(x),
                    View::Int(k) => v.push(k as f64),
                    _ => return fold(),
                }
            }
            v
        }
    };
    cx.tick(values.len() as u64 / 4 + 1)?;
    let (mut sum, mut comp) = (0.0f64, 0.0f64);
    for &x in &values {
        let t = sum + x;
        comp += if sum.abs() >= x.abs() { (sum - t) + x } else { (x - t) + sum };
        sum = t;
    }
    let total = sum + comp;
    // infinities and NaN: the compensation is NaN there, the plain sum is the answer
    Ok(Reply::Done(h.real(if total.is_finite() { total } else { values.iter().sum() })))
}

/// `sum(xs)`: the canonical sum of the elements of the list `xs` (`0` for `[]`).
pub(crate) fn sum<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if let Some(r) = nested(cx, "sum", args) {
        return Ok(r);
    }
    if args.len() == 2 {
        return crate::sums::iterated(lib, cx, t, args);
    }
    let [xs] = *args else {
        return Ok(Reply::Decline);
    };
    // the first argument is held (the body of an iterated sum): a list to add is evaluated here
    let xs = cx.eval(xs)?;
    if lib.is_failure(h, xs) {
        return Ok(Reply::Done(xs));
    }
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

/// `sum(f, it1, it2, ...)` (and `product`) with several iterators: the first iterator outermost,
/// `sum(sum(f, it2, ...), it1)`, so an inner bound may depend on an outer variable. `None` unless every iterator is
/// a list (or a symbol: an indefinite sum).
fn nested<'h>(cx: &mut NativeCx<'_, '_, 'h>, op: &str, args: &[T<'h>]) -> Option<Reply<'h>> {
    let h = cx.heap();
    if args.len() < 3 || !args[1..].iter().all(|&a| cx.as_list(a).is_some() || matches!(h.view(a), View::Sym(_))) {
        return None;
    }
    let f = h.sym_term(h.sym(op));
    let mut inner = vec![args[0]];
    inner.extend_from_slice(&args[2..]);
    Some(Reply::Again(h.app(f, &[h.app(f, &inner), args[1]])))
}

/// `product(xs)`: the canonical product of the elements (`1` for `[]`).
pub(crate) fn product<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if let Some(r) = nested(cx, "product", args) {
        return Ok(r);
    }
    if args.len() == 2 {
        return crate::sums::iterated_product(lib, cx, t, args);
    }
    let [xs] = *args else {
        return Ok(Reply::Decline);
    };
    let xs = cx.eval(xs)?;
    if lib.is_failure(h, xs) {
        return Ok(Reply::Done(xs));
    }
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
            // negative levels count from the leaves of the full array (-1 the deepest level, decision r37.6):
            // total(xs, -k) adds levels 1 to d + 1 - k of an array of depth d, total(xs, [a, b]) levels a to b
            let depth = |cx: &mut NativeCx<'_, '_, 'h>| -> Res<i64> { Ok(crate::structure::list_dims(cx, xs)?.len() as i64) };
            let int = |x: T<'h>| match h.view(x) {
                View::Int(k) => Some(k),
                _ => None,
            };
            if let Some(k @ ..=-1) = int(n) {
                let lv = depth(cx)? + 1 + k;
                return Ok(if lv >= 1 { Reply::Again(h.call(h.sym("total"), &[xs, h.int(lv)])) } else { Reply::Decline });
            }
            if let Some(&[a, b]) = cx.as_list(n) {
                let (Some(a), Some(b)) = (int(a), int(b)) else { return Ok(Reply::Decline) };
                let d = depth(cx)?;
                let at = |k: i64| if k < 0 { d + 1 + k } else { k };
                let (a, b) = (at(a), at(b));
                if a < 1 || b < a || b > d {
                    return Ok(Reply::Decline);
                }
                if a == 1 {
                    return Ok(Reply::Again(h.call(h.sym("total"), &[xs, h.int(b)])));
                }
                // levels a to b of xs: levels a - 1 to b - 1 of each element
                let elems = take!(lib.list_operand(h, t, xs, 1));
                cx.tick(elems.len() as u64)?;
                let inner = cx.list(&[h.int(a - 1), h.int(b - 1)]);
                let parts: Vec<T<'h>> = elems.iter().map(|&x| h.call(h.sym("total"), &[x, inner])).collect();
                return Ok(Reply::Again(cx.list(&parts)));
            }
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
        Packed::Complexes { .. } => Ok(None),
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
    if xs.iter().any(|&x| cx.is_value(x)) {
        return value_extreme(cx, &xs, max);
    }
    let ar = lib.arith(h);
    let mut best = first;
    if ar.compare(first, first).is_none() {
        return symbolic_extreme(cx, t, &xs, max);
    }
    let want = if max { Cmp3::Greater } else { Cmp3::Less };
    for &x in rest {
        m.step(cx)?;
        match ar.compare(x, best) {
            Some(c) if c == want => best = x,
            Some(Cmp3::Less | Cmp3::Equal | Cmp3::Greater) => {}
            // a symbolic element, a NaN or overlapping balls: decided by numeric values if they all have one
            _ => return symbolic_extreme(cx, t, &xs, max),
        }
    }
    Ok(Reply::Done(best))
}

/// `max`/`min` when an element is a number-like value (G54): an interval stands for its largest (smallest) end, the
/// other elements are compared by the evaluator's `greater`/`less` (quantities in a common unit, `around` by its
/// spread). The call stays unevaluated when a comparison is not decided.
fn value_extreme<'h>(cx: &mut NativeCx<'_, '_, 'h>, xs: &[T<'h>], max: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    if xs.len() > 10_000 {
        return Ok(Reply::Decline);
    }
    let interval = h.sym_term(h.sym("interval"));
    let list = h.sym_term(h.sym(mc_api::names::LIST));
    let mut items = Vec::with_capacity(xs.len());
    for &x in xs {
        match h.view(x) {
            // interval([a, b], [c, d], ...): every end is a candidate
            View::App { head, args } if head == interval && !args.is_empty() => {
                for &r in args {
                    match h.view(r) {
                        View::App { head, args: [a, b] } if head == list => items.push(if max { *b } else { *a }),
                        _ => return Ok(Reply::Decline),
                    }
                }
            }
            _ => items.push(x),
        }
    }
    let cmp = h.sym_term(h.sym(if max { "greater" } else { "less" }));
    let mut best = items[0];
    for &x in &items[1..] {
        cx.tick(1)?;
        let v = cx.eval(h.app(cmp, &[x, best]))?;
        if cx.is_true(v) {
            best = x;
        } else if v != cx.bool(false) {
            return Ok(Reply::Decline);
        }
    }
    Ok(Reply::Done(best))
}

/// `max`/`min` with a symbolic element: [`numeric_extreme`] when every element has a value; otherwise the numeric
/// elements are replaced by their extreme and repeated elements dropped, so `min(x, 2, 1, x)` is `min(1, x)`. The
/// call stays as it is when nothing can be dropped.
fn symbolic_extreme<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, xs: &[T<'h>], max: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    if let r @ Reply::Done(_) = numeric_extreme(cx, xs, max)? {
        return Ok(r);
    }
    if xs.len() > 1000 {
        return Ok(Reply::Decline);
    }
    let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
    let n = h.sym_term(h.sym("n"));
    let (mut numeric, mut other) = (Vec::new(), Vec::new());
    for &x in xs {
        cx.tick(1)?;
        match h.view(cx.eval(h.app(n, &[x]))?) {
            View::Real(v) if v.is_finite() => numeric.push(x),
            View::Int(_) => numeric.push(x),
            _ if !other.contains(&x) => other.push(x),
            _ => {}
        }
    }
    let mut kept = Vec::with_capacity(other.len() + 1);
    match numeric.as_slice() {
        [] => {}
        [x] => kept.push(*x),
        _ => match numeric_extreme(cx, &numeric, max)? {
            Reply::Done(x) => kept.push(x),
            _ => kept.extend_from_slice(&numeric),
        },
    }
    kept.extend_from_slice(&other);
    Ok(match kept.as_slice() {
        [x] => Reply::Done(*x),
        _ if kept.len() < xs.len() => Reply::Again(h.app(head, &kept)),
        _ => Reply::Decline,
    })
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
