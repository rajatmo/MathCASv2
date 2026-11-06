//! Rearranging lists: `riffle`, `catenate`, `partition`, `rotate_left`, `rotate_right`, `pad_left`, `pad_right`,
//! `constant_array`, `thread`.
use crate::ctx::{cells, charge_copy, count_operand, dims_operand, take, Lib, Meter, MAX_CELLS, MAX_LEN};
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// `riffle(xs, s)`: `s` between consecutive elements; `riffle(xs, ys)` with a list `ys` of `n` or `n - 1`
/// elements (`n` the length of `xs`): the elements of `xs` and `ys` alternating. A list `ys` of another length is
/// `failure(shape_mismatch, riffle)` (decision 1A.10).
pub(crate) fn riffle<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, s] = *args else { return Ok(Reply::Decline) };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    let n = xs.len();
    let seps: Option<&[T<'h>]> = cx.as_list(s);
    if let Some(ys) = seps {
        if ys.len() != n && ys.len() + 1 != n {
            return Ok(Reply::Done(lib.shape(h, t)));
        }
    }
    let total = match seps {
        Some(ys) => n + ys.len(),
        None => (2 * n).saturating_sub(1),
    };
    charge_copy(cx, total)?;
    let mut out = Vec::with_capacity(total);
    for (i, &x) in xs.iter().enumerate() {
        out.push(x);
        match seps {
            Some(ys) => out.extend(ys.get(i)),
            None if i + 1 < n => out.push(s),
            None => {}
        }
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// `catenate(xss)`: the elements of a list of lists, joined into one list.
pub(crate) fn catenate<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xss] = *args else { return Ok(Reply::Decline) };
    let xss = take!(lib.list_operand(h, t, xss, 1));
    let mut parts = Vec::with_capacity(xss.len());
    let mut total = 0usize;
    for &xs in xss {
        let Some(xs) = cx.as_list(xs) else { return Ok(Reply::Done(lib.bad_arg(h, t, 1))) };
        total += xs.len();
        parts.push(xs);
    }
    if total > MAX_LEN {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    charge_copy(cx, total)?;
    Ok(Reply::Done(cx.list(&parts.concat())))
}

/// `partition(xs, n)`: the consecutive runs of `n` elements (a shorter run at the end is dropped);
/// `partition(xs, n, d)`: runs starting every `d` elements (overlapping for `d < n`). `n` and `d` are positive.
pub(crate) fn partition<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, n, d) = match *args {
        [xs, n] => (xs, n, n),
        [xs, n, d] => (xs, n, d),
        _ => return Ok(Reply::Decline),
    };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    let n = take!(count_operand(lib, h, t, n, 2));
    let d = take!(count_operand(lib, h, t, d, args.len()));
    if n == 0 {
        return Ok(Reply::Done(lib.bad_arg(h, t, 2)));
    }
    if d == 0 {
        return Ok(Reply::Done(lib.bad_arg(h, t, 3)));
    }
    let parts = if xs.len() < n { 0 } else { (xs.len() - n) / d + 1 };
    if cells(&[parts, n]) > MAX_CELLS {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    charge_copy(cx, parts * n)?;
    let out: Vec<T<'h>> = (0..parts).map(|k| cx.list(&xs[k * d..k * d + n])).collect();
    Ok(Reply::Done(cx.list(&out)))
}

/// The rotation of `rotate_left` (`left`) and `rotate_right` by the optional count (default 1, any integer).
fn rotate<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], left: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, k) = match *args {
        [xs] => (xs, 1),
        [xs, k] => (xs, take!(lib.int_operand(h, t, k, 2))),
        _ => return Ok(Reply::Decline),
    };
    let (head, xs) = take!(lib.operand(h, t, xs, 1));
    if xs.is_empty() {
        return Ok(lib.rebuilt(h, head, xs));
    }
    let n = xs.len() as i64;
    let k = if left { k } else { -k };
    let s = k.rem_euclid(n) as usize;
    charge_copy(cx, xs.len())?;
    let v = [&xs[s..], &xs[..s]].concat();
    Ok(lib.rebuilt(h, head, &v))
}

/// `rotate_left(xs)`, `rotate_left(xs, k)`: the elements cycled `k` places to the left (`xs[k + 1]` first).
pub(crate) fn rotate_left<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    rotate(lib, cx, t, args, true)
}

/// `rotate_right(xs)`, `rotate_right(xs, k)`: the elements cycled `k` places to the right.
pub(crate) fn rotate_right<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    rotate(lib, cx, t, args, false)
}

/// Padding of `pad_left` (`left`) and `pad_right` to length `n` with the optional element (default 0).
fn pad<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], left: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, n, x) = match *args {
        [xs, n] => (xs, n, h.int(0)),
        [xs, n, x] => (xs, n, x),
        _ => return Ok(Reply::Decline),
    };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    let n = take!(count_operand(lib, h, t, n, 2));
    if n > MAX_LEN {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    charge_copy(cx, n)?;
    let len = xs.len();
    let v = match (left, n >= len) {
        (true, true) => [&vec![x; n - len][..], xs].concat(),
        (true, false) => xs[len - n..].to_vec(),
        (false, true) => [xs, &vec![x; n - len][..]].concat(),
        (false, false) => xs[..n].to_vec(),
    };
    Ok(Reply::Done(cx.list(&v)))
}

/// `pad_left(xs, n)`, `pad_left(xs, n, x)`: `xs` made `n` long by adding `x` (default 0) at the start, or
/// keeping only its last `n` elements. The padding is the term `x` itself, a list included (decision 1A.11).
pub(crate) fn pad_left<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    pad(lib, cx, t, args, true)
}

/// `pad_right(xs, n)`, `pad_right(xs, n, x)`: `xs` made `n` long by adding `x` (default 0) at the end, or keeping
/// only its first `n` elements.
pub(crate) fn pad_right<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    pad(lib, cx, t, args, false)
}

/// `constant_array(c, n)`: the list of `n` copies of `c`; `constant_array(c, [n1, ..., nd])`: nested lists of
/// those lengths. Rows are one shared term (the store is hash-consed), so building costs the sum of the lengths.
pub(crate) fn constant_array<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [c, ds] = *args else { return Ok(Reply::Decline) };
    let ds = take!(dims_operand(lib, h, t, ds, 2));
    // the rows are shared, but rows of machine numbers are packed into one array of all the cells
    if cells(&ds) > MAX_CELLS {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let mut v = c;
    for &d in ds.iter().rev() {
        charge_copy(cx, d)?;
        v = cx.list(&vec![v; d]);
    }
    Ok(Reply::Done(v))
}

/// `thread(f(a, b, ...))`: `f` threaded over the list arguments: `[f(a1, b1, ...), f(a2, b2, ...), ...]`, other
/// arguments repeated in each call. Lists of different lengths are `failure(shape_mismatch, thread)`; a term
/// without list arguments (or an atom) is itself. The calls are evaluated.
pub(crate) fn thread<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else { return Ok(Reply::Decline) };
    let View::App { head, args: xs } = h.view(x) else { return Ok(Reply::Done(x)) };
    let mut n = None;
    for &a in xs {
        if let Some(l) = cx.as_list(a) {
            match n {
                None => n = Some(l.len()),
                Some(m) if m != l.len() => return Ok(Reply::Done(lib.shape(h, t))),
                _ => {}
            }
        }
    }
    let Some(n) = n else { return Ok(Reply::Done(x)) };
    if cells(&[n, xs.len()]) > MAX_CELLS {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let mut out = Vec::with_capacity(n);
    let mut m = Meter::default();
    let mut call = Vec::with_capacity(xs.len());
    for i in 0..n {
        call.clear();
        for &a in xs {
            m.step(cx)?;
            call.push(cx.as_list(a).map_or(a, |l| l[i]));
        }
        out.push(h.app(head, &call));
    }
    Ok(Reply::Again(cx.list(&out)))
}
