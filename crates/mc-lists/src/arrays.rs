//! Functions of blocks and slices of full arrays: `array_filter` (a function of the neighbourhood of every element)
//! and `array_reduce` (a function of the vectors or sub-arrays along given levels).
//!
//! Both read their array as a full (rectangular) nested list: the levels taken are those where every element is a
//! list of one length. The blocks and slices are built as nested lists of the array's own elements, the calls of the
//! function are evaluated as a group (like `map`), and a failure among the values is the result (decision I.11).
//! Every element copied charges fuel; a result of more than [`MAX_CELLS`] elements is refused before it is built.
use crate::ctx::{Lib, Meter, MAX_CELLS};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The dimensions of the full array `a`, at most `max` levels: the lengths of the levels whose elements are all
/// lists of one length.
fn full_dims<'h>(cx: &NativeCx<'_, '_, 'h>, a: T<'h>, max: usize) -> Vec<usize> {
    let mut dims = vec![];
    let mut level: Vec<T<'h>> = vec![a];
    while dims.len() < max {
        let mut len = None;
        let mut next = vec![];
        for &x in &level {
            match cx.as_list(x) {
                Some(xs) if len.is_none_or(|n| n == xs.len()) => {
                    len = Some(xs.len());
                    next.extend_from_slice(xs);
                }
                _ => return dims,
            }
        }
        match len {
            Some(0) => {
                dims.push(0);
                return dims;
            }
            Some(n) => dims.push(n),
            None => return dims,
        }
        level = next;
    }
    dims
}

/// The elements of the full array `a` at level `k` (its first `k` dimensions are `dims`), in row-major order.
fn flat_at<'h>(cx: &NativeCx<'_, '_, 'h>, a: T<'h>, k: usize) -> Vec<T<'h>> {
    let mut level = vec![a];
    for _ in 0..k {
        let mut next = Vec::new();
        for &x in &level {
            // invariant: full_dims checked that every element down to level k is a list
            next.extend_from_slice(cx.as_list(x).expect("a level of a full array"));
        }
        level = next;
    }
    level
}

/// The flat elements `xs` (row-major) as nested lists of dimensions `ds`.
fn nest<'h>(cx: &NativeCx<'_, '_, 'h>, xs: Vec<T<'h>>, ds: &[usize]) -> T<'h> {
    crate::ctx::nest_flat(cx, xs, ds)
}

/// The values of `f` at each of `xs`, evaluated as a group; the first failure among them as `Err`.
fn values<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, xs: &[T<'h>]) -> Res<Result<Vec<T<'h>>, T<'h>>> {
    crate::ctx::keys(lib, cx, f, xs)
}

/// The radius of a block in one dimension: elements before and after the centre.
type Radius = (usize, usize);

/// A radius `r` or `[before, after]`.
fn radius(h: &Heap, lib: &Lib, t: T<'_>) -> Option<Radius> {
    match h.view(t) {
        View::Int(r) if r >= 0 => Some((r as usize, r as usize)),
        View::App { head, args: &[a, b] } if head == h.sym_term(lib.s.list) => match (h.view(a), h.view(b)) {
            (View::Int(a), View::Int(b)) if a >= 0 && b >= 0 => Some((a as usize, b as usize)),
            _ => None,
        },
        _ => None,
    }
}

/// How `array_filter` reads outside the array.
enum Padding<'h> {
    /// the nearest element (the edge repeated)
    Fixed,
    /// cyclically
    Periodic,
    /// mirrored at the edge element, which is not repeated
    Reflected,
    /// this value
    Value(T<'h>),
}

/// Index `i` (possibly outside `0..n`) of a dimension of length `n` under the padding; `None` for a padding value.
fn pad_index(p: &Padding<'_>, i: i64, n: usize) -> Option<usize> {
    let n = n as i64;
    if (0..n).contains(&i) {
        return Some(i as usize);
    }
    match p {
        Padding::Fixed => Some(i.clamp(0, n - 1) as usize),
        Padding::Periodic => Some(i.rem_euclid(n) as usize),
        Padding::Reflected => {
            if n == 1 {
                return Some(0);
            }
            let period = 2 * (n - 1);
            let j = i.rem_euclid(period);
            Some(if j < n { j } else { period - j } as usize)
        }
        Padding::Value(_) => None,
    }
}

/// The row-major index tuples of dimensions `ds`, as an odometer.
fn advance(idx: &mut [usize], ds: &[usize]) -> bool {
    for k in (0..ds.len()).rev() {
        idx[k] += 1;
        if idx[k] < ds[k] {
            return true;
        }
        idx[k] = 0;
    }
    false
}

/// `array_filter(f, a, r)`: `f` applied to the block of radius `r` around every element of the full array `a`; `r` a
/// count (every level), `[r1, r2, ...]` (level i, the levels after the last one's elements taken whole) or
/// `[[b1, a1], ...]` (before and after); `padding:` "Fixed" (default: the edge repeated), "Periodic", "Reflected" or
/// a value.
pub(crate) fn array_filter<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, pad) = crate::ctx::option(h, args, 3, "padding");
    let &[f, a, r] = args else { return Ok(Reply::Decline) };
    let padding = match pad.map(|p| (p, h.view(p))) {
        None => Padding::Fixed,
        Some((_, View::Str("Fixed"))) => Padding::Fixed,
        Some((_, View::Str("Periodic"))) => Padding::Periodic,
        Some((_, View::Str("Reflected"))) => Padding::Reflected,
        Some((p, _)) => Padding::Value(p),
    };
    let all = full_dims(cx, a, usize::MAX);
    if all.is_empty() {
        return Ok(match h.view(a) {
            View::Sym(_) => Reply::Decline,
            _ => Reply::Done(lib.bad_arg(h, t, 2)),
        });
    }
    let radii: Vec<Radius> = match radius(h, lib, r) {
        Some(rr) if matches!(h.view(r), View::Int(_)) => vec![rr; all.len()],
        _ => match cx.as_list(r) {
            Some(rs) if !rs.is_empty() && rs.len() <= all.len() => {
                let mut out = Vec::with_capacity(rs.len());
                for &x in rs {
                    match radius(h, lib, x) {
                        Some(rr) => out.push(rr),
                        None => return Ok(Reply::Done(lib.bad_arg(h, t, 3))),
                    }
                }
                out
            }
            // a radius list longer than the array's depth (a kernel array) is not read
            Some(_) => return Ok(Reply::Decline),
            None => {
                return Ok(match h.view(r) {
                    View::Sym(_) => Reply::Decline,
                    _ => Reply::Done(lib.bad_arg(h, t, 3)),
                })
            }
        },
    };
    let k = radii.len();
    let dims = &all[..k];
    let bdims: Vec<usize> = radii.iter().map(|&(b, a)| b + a + 1).collect();
    let n: u128 = dims.iter().map(|&d| d as u128).product();
    let per: u128 = bdims.iter().map(|&d| d as u128).product();
    if n.saturating_mul(per) > MAX_CELLS {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    if n == 0 {
        return Ok(Reply::Done(nest(cx, vec![], dims)));
    }
    let flat = flat_at(cx, a, k);
    let mut strides = vec![1usize; k];
    for i in (0..k.saturating_sub(1)).rev() {
        strides[i] = strides[i + 1] * dims[i + 1];
    }
    let mut blocks = Vec::with_capacity(n as usize);
    let mut meter = Meter::default();
    let mut p = vec![0usize; k];
    loop {
        let mut cells = Vec::with_capacity(per as usize);
        let mut o = vec![0usize; k];
        loop {
            let mut at = Some(0usize);
            for i in 0..k {
                let q = p[i] as i64 - radii[i].0 as i64 + o[i] as i64;
                at = match (at, pad_index(&padding, q, dims[i])) {
                    (Some(s), Some(j)) => Some(s + j * strides[i]),
                    _ => None,
                };
            }
            cells.push(match (at, &padding) {
                (Some(j), _) => flat[j],
                (None, Padding::Value(v)) => *v,
                // invariant: only a padding value leaves an index unresolved
                (None, _) => unreachable!("an index outside the array without a padding value"),
            });
            meter.step(cx)?;
            if !advance(&mut o, &bdims) {
                break;
            }
        }
        blocks.push(nest(cx, cells, &bdims));
        if !advance(&mut p, dims) {
            break;
        }
    }
    let vs = match values(lib, cx, f, &blocks)? {
        Ok(vs) => vs,
        Err(e) => return Ok(Reply::Done(e)),
    };
    Ok(Reply::Done(nest(cx, vs, dims)))
}

/// A level `n` (1-based, negative from the deepest) of an array of depth `d`, 0-based.
fn level_index(h: &Heap, t: T<'_>, d: usize) -> Option<usize> {
    match h.view(t) {
        View::Int(n) if n >= 1 && n as usize <= d => Some(n as usize - 1),
        View::Int(n) if n < 0 && n.unsigned_abs() as usize <= d => Some(d - n.unsigned_abs() as usize),
        _ => None,
    }
}

/// `array_reduce(f, a, n)`: `f` applied to each vector of the full array `a` along level `n` (that level removed
/// from the result); `array_reduce(f, a, [n1, n2, ...])`: to each sub-array along those levels, in that order.
pub(crate) fn array_reduce<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[f, a, lv] = args else { return Ok(Reply::Decline) };
    let dims = full_dims(cx, a, usize::MAX);
    if dims.is_empty() {
        return Ok(match h.view(a) {
            View::Sym(_) => Reply::Decline,
            _ => Reply::Done(lib.bad_arg(h, t, 2)),
        });
    }
    let d = dims.len();
    let levels: Vec<usize> = match cx.as_list(lv) {
        Some(ls) if !ls.is_empty() => {
            let mut out = Vec::with_capacity(ls.len());
            for &l in ls {
                match level_index(h, l, d) {
                    Some(i) if !out.contains(&i) => out.push(i),
                    _ => return Ok(Reply::Done(lib.bad_arg(h, t, 3))),
                }
            }
            out
        }
        Some(_) => return Ok(Reply::Done(lib.bad_arg(h, t, 3))),
        None => match level_index(h, lv, d) {
            Some(i) => vec![i],
            None => {
                return Ok(match h.view(lv) {
                    View::Sym(_) | View::App { .. } => Reply::Decline,
                    _ => Reply::Done(lib.bad_arg(h, t, 3)),
                })
            }
        },
    };
    let kept: Vec<usize> = (0..d).filter(|i| !levels.contains(i)).collect();
    let kdims: Vec<usize> = kept.iter().map(|&i| dims[i]).collect();
    let rdims: Vec<usize> = levels.iter().map(|&i| dims[i]).collect();
    let total: u128 = dims.iter().map(|&x| x as u128).product();
    if total.saturating_mul(2) > MAX_CELLS {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    if total == 0 {
        return Ok(Reply::Decline);
    }
    let flat = flat_at(cx, a, d);
    let mut strides = vec![1usize; d];
    for i in (0..d - 1).rev() {
        strides[i] = strides[i + 1] * dims[i + 1];
    }
    let mut slices = vec![];
    let mut meter = Meter::default();
    let mut p = vec![0usize; kept.len()];
    loop {
        let base: usize = kept.iter().zip(&p).map(|(&i, &j)| j * strides[i]).sum();
        let mut cells = vec![];
        let mut o = vec![0usize; levels.len()];
        loop {
            let at: usize = base + levels.iter().zip(&o).map(|(&i, &j)| j * strides[i]).sum::<usize>();
            cells.push(flat[at]);
            meter.step(cx)?;
            if !advance(&mut o, &rdims) {
                break;
            }
        }
        slices.push(nest(cx, cells, &rdims));
        if kept.is_empty() || !advance(&mut p, &kdims) {
            break;
        }
    }
    let vs = match values(lib, cx, f, &slices)? {
        Ok(vs) => vs,
        Err(e) => return Ok(Reply::Done(e)),
    };
    if kept.is_empty() {
        return Ok(Reply::Done(vs[0]));
    }
    Ok(Reply::Done(nest(cx, vs, &kdims)))
}
