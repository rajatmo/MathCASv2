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
    if args.len() >= 2 && (args.len() > 3 || cx.as_list(args[1]).is_some()) {
        return partition_general(lib, cx, t, args);
    }
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

/// Padding of the runs of [`runs`] beyond the ends of the list.
enum Pad<'p, 'h> {
    /// the list itself, cyclically
    Cyclic,
    /// these elements, cyclically, aligned with the positions
    List(&'p [T<'h>]),
    /// no padding: runs are shorter
    Nothing,
}

/// The runs of `n` elements of `xs` starting every `d`, with element 1 at place `kl` of the first run and the last
/// element at place `kr` of the last (`None`: no overhang, only complete runs); places count from 1, negative from
/// the end of a run. Positions outside the list are taken from `pad` by position.
fn runs<'h>(xs: &[T<'h>], n: usize, d: usize, over: Option<(i64, i64)>, pad: &Pad<'_, 'h>) -> Option<Vec<Vec<T<'h>>>> {
    let len = xs.len() as i64;
    let (n, d) = (n as i64, d as i64);
    let place = |k: i64| if k > 0 { k } else { n + k + 1 };
    let (first, last) = match over {
        None => (1, len - n + 1),
        Some((kl, kr)) => {
            let (kl, kr) = (place(kl), place(kr));
            if !(1..=n).contains(&kl) || !(1..=n).contains(&kr) {
                return None;
            }
            (2 - kl, len - kr + 1)
        }
    };
    let mut out = vec![];
    let mut start = first;
    while start <= last {
        let mut run = Vec::with_capacity(n as usize);
        for p in start..start + n {
            if (1..=len).contains(&p) {
                run.push(xs[(p - 1) as usize]);
            } else {
                match pad {
                    Pad::Cyclic if len > 0 => run.push(xs[(p - 1).rem_euclid(len) as usize]),
                    Pad::List(ps) if !ps.is_empty() => run.push(ps[(p - 1).rem_euclid(ps.len() as i64) as usize]),
                    _ => {}
                }
            }
        }
        out.push(run);
        if out.len() > MAX_LEN {
            return None;
        }
        start += d;
    }
    Some(out)
}

/// `partition` with dimension lists (`partition(m, [2, 2])`: blocks of a matrix), an overhang `k` or `[kl, kr]`
/// (runs reaching beyond the ends, padded cyclically) and a padding `x` or `[x1, ...]` (`[]`: none). Decision
/// p6core.8.
fn partition_general<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let ints = |x: T<'h>| -> Option<Vec<i64>> {
        match h.view(x) {
            View::Int(v) => Some(vec![v]),
            _ => cx.as_list(x)?.iter().map(|&e| match h.view(e) {
                View::Int(v) => Some(v),
                _ => None,
            }).collect(),
        }
    };
    let Some(ns) = ints(args[1]) else { return Ok(Reply::Decline) };
    let ds = match args.get(2) {
        Some(&d) => match ints(d) {
            Some(ds) => ds,
            None => return Ok(Reply::Decline),
        },
        None => ns.clone(),
    };
    let ds = if ds.len() == 1 && ns.len() > 1 { vec![ds[0]; ns.len()] } else { ds };
    if ns.is_empty() || ds.len() != ns.len() || ns.iter().chain(&ds).any(|&v| v < 1) {
        return Ok(Reply::Decline);
    }
    let over = match args.get(3) {
        None => None,
        Some(&k) => match ints(k).as_deref() {
            Some([k]) if h.packed(args[3]).is_none() && cx.as_list(args[3]).is_none() => Some((*k, *k)),
            Some([kl, kr]) => Some((*kl, *kr)),
            _ => return Ok(Reply::Decline),
        },
    };
    let pad = match args.get(4) {
        None => Pad::Cyclic,
        Some(&p) => match cx.as_list(p) {
            Some([]) => Pad::Nothing,
            Some(ps) => Pad::List(ps),
            None => Pad::List(std::slice::from_ref(&args[4])),
        },
    };
    let xs = take!(lib.list_operand(h, t, args[0], 1));
    if ns.len() == 1 {
        let Some(rs) = runs(xs, ns[0] as usize, ds[0] as usize, over, &pad) else { return Ok(Reply::Decline) };
        charge_copy(cx, rs.iter().map(Vec::len).sum())?;
        let out: Vec<T<'h>> = rs.iter().map(|r| cx.list(r)).collect();
        return Ok(Reply::Done(cx.list(&out)));
    }
    // blocks of a matrix: runs of rows, each row cut into runs of columns, the block index first
    if ns.len() != 2 || over.is_some() {
        return Ok(Reply::Decline);
    }
    let Some(row_runs) = runs(xs, ns[0] as usize, ds[0] as usize, None, &Pad::Nothing) else { return Ok(Reply::Decline) };
    let mut out = Vec::with_capacity(row_runs.len());
    for rows in row_runs {
        let mut cut = Vec::with_capacity(rows.len());
        for &r in &rows {
            let Some(r) = cx.as_list(r) else { return Ok(Reply::Done(lib.bad_arg(h, t, 1))) };
            let Some(pieces) = runs(r, ns[1] as usize, ds[1] as usize, None, &Pad::Nothing) else {
                return Ok(Reply::Decline);
            };
            charge_copy(cx, r.len())?;
            cut.push(pieces);
        }
        let blocks = cut.iter().map(Vec::len).min().unwrap_or(0);
        let row: Vec<T<'h>> = (0..blocks)
            .map(|j| {
                let block: Vec<T<'h>> = cut.iter().map(|pieces| cx.list(&pieces[j])).collect();
                cx.list(&block)
            })
            .collect();
        out.push(cx.list(&row));
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// The rotation of `rotate_left` (`left`) and `rotate_right` by the optional count (default 1, any integer).
fn rotate<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], left: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, k) = match *args {
        [xs] => (xs, 1),
        // a one-element list `[k]` (a level-1 specification) counts as `k`
        [xs, k] => match cx.as_list(k) {
            Some(&[k1]) => (xs, take!(lib.int_operand(h, t, k1, 2))),
            _ => (xs, take!(lib.int_operand(h, t, k, 2))),
        },
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
        [xs] => (xs, None, h.int(0)),
        [xs, n] => (xs, Some(n), h.int(0)),
        [xs, n, x] => (xs, Some(n), x),
        _ => return Ok(Reply::Decline),
    };
    let rows = take!(lib.list_operand(h, t, xs, 1));
    // the lengths: one, a list of lengths per level, or (none given) a ragged list of lists padded to a matrix
    let dims: Vec<usize> = match n {
        None => {
            let mut w = 0;
            for &r in rows {
                let Some(r) = cx.as_list(r) else { return Ok(Reply::Done(lib.bad_arg(h, t, 1))) };
                w = w.max(r.len());
            }
            vec![rows.len(), w]
        }
        Some(n) => match cx.as_list(n) {
            Some(ns) => {
                let mut v = vec![];
                for &d in ns {
                    v.push(take!(count_operand(lib, h, t, d, 2)));
                }
                v
            }
            None => vec![take!(count_operand(lib, h, t, n, 2))],
        },
    };
    if dims.is_empty() {
        return Ok(Reply::Decline);
    }
    if cells(&dims) > MAX_CELLS {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let fill = cx.as_list(x).map(<[T<'h>]>::to_vec);
    match padded(cx, xs, &dims, x, fill.as_deref(), left)? {
        Some(v) => Ok(Reply::Done(v)),
        None => Ok(Reply::Done(lib.bad_arg(h, t, 1))),
    }
}

/// `xs` padded or cut to `dims` (decision p6core.10: a list padding repeats cyclically, aligned with the result's
/// positions, `pad_left` from the result's end; a scalar `x` fills; deeper levels padded with copies of `x`).
/// `None` when a level to pad is not a list.
fn padded<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    xs: T<'h>,
    dims: &[usize],
    x: T<'h>,
    fill: Option<&[T<'h>]>,
    left: bool,
) -> Res<Option<T<'h>>> {
    let Some(elems) = cx.as_list(xs) else { return Ok(None) };
    let elems = elems.to_vec();
    let n = dims[0];
    charge_copy(cx, n)?;
    let len = elems.len();
    let filler = |cx: &NativeCx<'_, '_, 'h>, i: usize| -> T<'h> {
        // position i (0-based) of the result
        let base = match fill {
            Some(ps) if dims.len() == 1 && !ps.is_empty() => {
                let m = ps.len() as i64;
                let k = if left { (i as i64 - n as i64).rem_euclid(m) } else { (i as i64).rem_euclid(m) };
                return ps[k as usize];
            }
            _ => x,
        };
        let mut v = base;
        for &d in dims[1..].iter().rev() {
            v = cx.list(&vec![v; d]);
        }
        v
    };
    let mut out = Vec::with_capacity(n);
    if left {
        let cut = len.saturating_sub(n);
        for i in 0..n.saturating_sub(len) {
            out.push(filler(cx, i));
        }
        out.extend_from_slice(&elems[cut..]);
    } else {
        out.extend_from_slice(&elems[..len.min(n)]);
        for i in len..n {
            out.push(filler(cx, i));
        }
    }
    if dims.len() > 1 {
        let pads = n.saturating_sub(len);
        let range = if left { pads..n } else { 0..len.min(n) };
        for i in range {
            match padded(cx, out[i], &dims[1..], x, fill, left)? {
                Some(v) => out[i] = v,
                None => return Ok(None),
            }
        }
    }
    Ok(Some(cx.list(&out)))
}

/// `pad_left(xs, n)`, `pad_left(xs, n, x)`: `xs` made `n` long by adding `x` (default 0) at the start, or
/// keeping only its last `n` elements. A list `x` repeats cyclically (`[[u, v]]` pads with the list `[u, v]`);
/// `n` may be a list of lengths per level; without `n`, a ragged list of lists is padded to a matrix (p6core.10).
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
    // `thread(x, g)`: over the arguments with head `g` instead of lists (`thread(log(a == b), equal)`)
    let (x, over) = match *args {
        [x] => (x, None),
        [x, g] => (x, Some(g)),
        _ => return Ok(Reply::Decline),
    };
    let View::App { head, args: xs } = h.view(x) else { return Ok(Reply::Done(x)) };
    let parts: Vec<Option<&'h [T<'h>]>> = xs
        .iter()
        .map(|&a| match over {
            None => cx.as_list(a),
            Some(g) => match h.view(a) {
                View::App { head: ga, args } if ga == g => Some(args),
                _ => None,
            },
        })
        .collect();
    let mut n = None;
    for l in parts.iter().flatten() {
        match n {
            None => n = Some(l.len()),
            Some(m) if m != l.len() => return Ok(Reply::Done(lib.shape(h, t))),
            _ => {}
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
        for (&a, p) in xs.iter().zip(&parts) {
            m.step(cx)?;
            call.push(p.map_or(a, |l| l[i]));
        }
        out.push(h.app(head, &call));
    }
    Ok(Reply::Again(match over {
        None => cx.list(&out),
        Some(g) => h.app(g, &out),
    }))
}
