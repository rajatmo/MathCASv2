//! Access to parts: `length`, `part` (what `xs[i]` lowers to), `first`, `last`, `rest`, `most`, `take`, `drop`.
//!
//! Indices are 1-based; a negative index counts from the end (`-1` is the last argument); index 0 is the head. An
//! index outside the term gives `failure(index, i, n)` with `n` the number of arguments (decision I.5).
use crate::ctx::{charge_copy, compound, take, Lib};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// `length(t)`: the number of arguments of a compound term, 0 for an atom.
pub(crate) fn length<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    Ok(Reply::Done(h.int(compound(h, x).map_or(0, |(_, a)| a.len()) as i64)))
}

/// The position of index `i` among `n` arguments: `Some(0)` for the head, `Some(k)` for argument `k` (1-based).
fn position(i: i64, n: usize) -> Option<usize> {
    let n = n as i64;
    match i {
        0..=i64::MAX if i <= n => Some(i as usize),
        i64::MIN..=-1 if i.checked_neg().is_some_and(|m| m <= n) => Some((n + 1 + i) as usize),
        _ => None,
    }
}

/// Why `part` stops without a value.
enum Stop<'h> {
    /// an index that is not (yet) a number: the call stays unevaluated
    Symbolic,
    /// the failure to return
    Fail(T<'h>),
}

/// How deep `part` with spans or index lists recurses: one level per index argument.
const MAX_INDICES: usize = 1024;

/// `part(t, i, j, ...)`: successive parts; an index `span(i, j)` or a list of indices selects several arguments
/// (keeping the head), and the remaining indices apply to each of them.
pub(crate) fn part<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let Some((&x, idx)) = args.split_first() else {
        return Ok(Reply::Decline);
    };
    if idx.len() > MAX_INDICES {
        return Ok(Reply::Done(lib.bad_arg(h, t, MAX_INDICES + 2)));
    }
    let mut rebuilt = false;
    match part_at(lib, cx, x, idx, &mut rebuilt)? {
        Ok(v) if rebuilt => Ok(Reply::Again(v)),
        Ok(v) => Ok(Reply::Done(v)),
        Err(Stop::Symbolic) => Ok(Reply::Decline),
        Err(Stop::Fail(f)) => Ok(Reply::Done(f)),
    }
}

/// The part of `x` at the indices `idx`. `rebuilt` is set when a selected term other than a list was built
/// (it must be evaluated again, decision I.3).
fn part_at<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    x: T<'h>,
    idx: &[T<'h>],
    rebuilt: &mut bool,
) -> Res<Result<T<'h>, Stop<'h>>> {
    let h = cx.heap();
    let mut x = x;
    for (k, &i) in idx.iter().enumerate() {
        let (head, args) = match h.view(x) {
            View::App { head, args } => (head, args),
            // `s[1]` for a symbol `s` without a value stays as it is
            View::Sym(_) => return Ok(Err(Stop::Symbolic)),
            _ => return Ok(Err(Stop::Fail(lib.bad_index(h, i, 0)))),
        };
        let selected: Vec<T<'h>> = match h.view(i) {
            View::Int(i) => match position(i, args.len()) {
                Some(0) => {
                    x = head;
                    continue;
                }
                Some(p) => {
                    x = args[p - 1];
                    continue;
                }
                None => return Ok(Err(Stop::Fail(lib.bad_index(h, h.int(i), args.len())))),
            },
            View::App { head: g, args: ab } if g == h.sym_term(lib.s.span) && ab.len() == 2 => {
                match span(lib, h, ab[0], ab[1], args.len()) {
                    Ok((lo, hi)) => args[lo..hi].to_vec(),
                    Err(s) => return Ok(Err(s)),
                }
            }
            View::App { head: g, args: is } if g == h.sym_term(lib.s.list) => {
                charge_copy(cx, is.len())?;
                let mut out = Vec::with_capacity(is.len());
                for &j in is {
                    match h.view(j) {
                        View::Int(j) => match position(j, args.len()) {
                            Some(p @ 1..) => out.push(args[p - 1]),
                            _ => return Ok(Err(Stop::Fail(lib.bad_index(h, h.int(j), args.len())))),
                        },
                        View::Sym(_) | View::App { .. } => return Ok(Err(Stop::Symbolic)),
                        _ => return Ok(Err(Stop::Fail(lib.bad_index(h, j, args.len())))),
                    }
                }
                out
            }
            View::Sym(_) | View::App { .. } => return Ok(Err(Stop::Symbolic)),
            _ => return Ok(Err(Stop::Fail(lib.bad_index(h, i, args.len())))),
        };
        // several arguments: the remaining indices apply to each
        let rest = &idx[k + 1..];
        let mut out = Vec::with_capacity(selected.len());
        if rest.is_empty() {
            out = selected;
        } else {
            for s in selected {
                cx.tick(1)?;
                match part_at(lib, cx, s, rest, rebuilt)? {
                    Ok(v) => out.push(v),
                    e => return Ok(e),
                }
            }
        }
        charge_copy(cx, out.len())?;
        if head != h.sym_term(lib.s.list) {
            *rebuilt = true;
        }
        return Ok(Ok(h.app(head, &out)));
    }
    Ok(Ok(x))
}

/// The argument range `lo..hi` (0-based) of `span(i, j)` over `n` arguments: from argument `i` to argument `j`,
/// both included, negative indices from the end; `j = i - 1` is empty.
fn span<'h>(lib: &Lib, h: &'h Heap, i: T<'h>, j: T<'h>, n: usize) -> Result<(usize, usize), Stop<'h>> {
    let end = |t: T<'h>| match h.view(t) {
        View::Int(v) => Ok(v),
        View::Sym(_) | View::App { .. } => Err(Stop::Symbolic),
        _ => Err(Stop::Fail(lib.bad_index(h, t, n))),
    };
    let (a, b) = (end(i)?, end(j)?);
    let lo = match position(a, n) {
        Some(p @ 1..) => p,
        // one past the end starts an empty span
        _ if a == n as i64 + 1 => n + 1,
        _ => return Err(Stop::Fail(lib.bad_index(h, i, n))),
    };
    let hi = match position(b, n) {
        Some(p) => p,
        None => return Err(Stop::Fail(lib.bad_index(h, j, n))),
    };
    if hi + 1 < lo {
        return Err(Stop::Fail(lib.bad_index(h, j, n)));
    }
    Ok((lo - 1, hi.max(lo - 1)))
}

/// `first(xs)`, `last(xs)`: the first or last argument.
pub(crate) fn first<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    end(lib, cx, t, args, 1)
}

/// `last(xs)`
pub(crate) fn last<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    end(lib, cx, t, args, -1)
}

fn end<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], i: i64) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    let (_, xs) = take!(lib.operand(h, t, x, 1));
    Ok(Reply::Done(match position(i, xs.len()) {
        Some(p @ 1..) => xs[p - 1],
        _ => lib.bad_index(h, h.int(i), xs.len()),
    }))
}

/// `rest(xs)`: all arguments but the first; `most(xs)`: all but the last.
pub(crate) fn rest<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    let (head, xs) = take!(lib.operand(h, t, x, 1));
    if xs.is_empty() {
        return Ok(Reply::Done(lib.bad_index(h, h.int(1), 0)));
    }
    charge_copy(cx, xs.len())?;
    Ok(lib.rebuilt(h, head, &xs[1..]))
}

/// `most(xs)`
pub(crate) fn most<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    let (head, xs) = take!(lib.operand(h, t, x, 1));
    if xs.is_empty() {
        return Ok(Reply::Done(lib.bad_index(h, h.int(-1), 0)));
    }
    charge_copy(cx, xs.len())?;
    Ok(lib.rebuilt(h, head, &xs[..xs.len() - 1]))
}

/// The arguments `take(xs, n)` keeps: the first `n`, or the last `-n` for negative `n`.
fn taken(n: i64, len: usize) -> Option<(usize, usize)> {
    let k = usize::try_from(n.unsigned_abs()).ok().filter(|&k| k <= len)?;
    Some(if n >= 0 { (0, k) } else { (len - k, len) })
}

/// `take(xs, n)`: the first `n` arguments (the last `-n` for negative `n`).
pub(crate) fn take_<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x, n] = *args else {
        return Ok(Reply::Decline);
    };
    let (head, xs) = take!(lib.operand(h, t, x, 1));
    let n = take!(lib.int_operand(h, t, n, 2));
    let Some((lo, hi)) = taken(n, xs.len()) else {
        return Ok(Reply::Done(lib.bad_index(h, h.int(n), xs.len())));
    };
    charge_copy(cx, hi - lo)?;
    Ok(lib.rebuilt(h, head, &xs[lo..hi]))
}

/// `drop(xs, n)`: all but the first `n` arguments (the last `-n` for negative `n`).
pub(crate) fn drop<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x, n] = *args else {
        return Ok(Reply::Decline);
    };
    let (head, xs) = take!(lib.operand(h, t, x, 1));
    let n = take!(lib.int_operand(h, t, n, 2));
    let Some((lo, hi)) = taken(n, xs.len()) else {
        return Ok(Reply::Done(lib.bad_index(h, h.int(n), xs.len())));
    };
    charge_copy(cx, xs.len())?;
    let kept: Vec<T<'h>> = xs[..lo].iter().chain(&xs[hi..]).copied().collect();
    Ok(lib.rebuilt(h, head, &kept))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions() {
        assert_eq!(position(1, 3), Some(1));
        assert_eq!(position(3, 3), Some(3));
        assert_eq!(position(4, 3), None);
        assert_eq!(position(0, 3), Some(0));
        assert_eq!(position(-1, 3), Some(3));
        assert_eq!(position(-3, 3), Some(1));
        assert_eq!(position(-4, 3), None);
        assert_eq!(position(i64::MIN, 3), None);
        assert_eq!(taken(2, 5), Some((0, 2)));
        assert_eq!(taken(-2, 5), Some((3, 5)));
        assert_eq!(taken(6, 5), None);
        assert_eq!(taken(i64::MIN, 5), None);
    }
}
