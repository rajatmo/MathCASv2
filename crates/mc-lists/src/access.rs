//! Access to parts: `length`, `part` (what `xs[i]` lowers to), `first`, `last`, `rest`, `most`, `take`, `drop`.
//!
//! Indices are 1-based; a negative index counts from the end (`-1` is the last argument); index 0 is the head. An
//! index outside the term gives `failure(index, i, n)` with `n` the number of arguments (decision I.5).
use crate::ctx::{charge_copy, compound, packed_arg, packed_slice, take, Lib};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// `length(t)`: the number of arguments of a compound term, 0 for an atom.
pub(crate) fn length<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    // a packed array's length is its first dimension (decision N4a.8: read without unpacking)
    if let Some(p) = h.packed(x) {
        return Ok(Reply::Done(h.int(p.len() as i64)));
    }
    // a complex number is an atom (decision p6core.3)
    if crate::measure::is_complex_number(h, x) {
        return Ok(Reply::Done(h.int(0)));
    }
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
    // an association is indexed by key, not by position: its argument-head rules decide (decision 1A.16)
    if h.packed(x).is_none() && matches!(h.view(x), View::App { head, .. } if head == h.sym_term(lib.s.assoc)) {
        return Ok(Reply::Decline);
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
        // an association reached by the earlier indices (`xs[1, "b"]`): its own rules index it by key, so the
        // remaining indices go back to it as `part(assoc, ...)` (decision 1A.16)
        if h.packed(x).is_none() && matches!(h.view(x), View::App { head, .. } if head == h.sym_term(lib.s.assoc)) {
            let mut call = Vec::with_capacity(idx.len() - k + 1);
            call.push(x);
            call.extend_from_slice(&idx[k..]);
            *rebuilt = true;
            return Ok(Ok(h.call(h.sym("part"), &call)));
        }
        // a packed array: an element, a row or a final span straight from the payload (decision N4a.8)
        if let Some(p) = h.packed(x) {
            let n = p.len();
            let i = span_ends(lib, h, i, n);
            match h.view(i) {
                View::Int(j) => {
                    match position(j, n) {
                        Some(0) => x = h.sym_term(lib.s.list),
                        Some(q) => x = packed_arg(h, p, q - 1),
                        None => return Ok(Err(Stop::Fail(lib.bad_index(h, i, n)))),
                    }
                    continue;
                }
                View::App { head: g, args: ab } if g == h.sym_term(lib.s.span) && ab.len() == 2 && k + 1 == idx.len() => {
                    return Ok(match span(lib, h, ab[0], ab[1], n) {
                        Ok((lo, hi)) => {
                            charge_copy(cx, (hi - lo) * (p.count() / n))?;
                            Ok(packed_slice(h, p, lo, hi))
                        }
                        Err(s) => Err(s),
                    });
                }
                _ => {}
            }
        }
        let (head, args) = match h.view(x) {
            View::App { head, args } => (head, args),
            // `s[1]` for a symbol `s` without a value stays as it is
            View::Sym(_) => return Ok(Err(Stop::Symbolic)),
            _ => return Ok(Err(Stop::Fail(lib.bad_index(h, i, 0)))),
        };
        let i = span_ends(lib, h, i, args.len());
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
            // `span(i, j, step)`: every step-th argument from i towards j (a negative step runs backwards;
            // C9B.6)
            View::App { head: g, args: &[a, b, st] } if g == h.sym_term(lib.s.span) => {
                match stepped(lib, h, a, b, st, args.len()) {
                    Ok(ps) => {
                        charge_copy(cx, ps.len())?;
                        ps.into_iter().map(|p| args[p]).collect()
                    }
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
            // `all`: every argument, the remaining indices applied to each (decision cov-core.10)
            View::Sym(a) if h.sym_name(a) == "all" => args.to_vec(),
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

/// A span with `all` and `up_to(k)` ends written as integers over `n` arguments: `all` is the first
/// argument as a start, the last as an end, and a step of 1 towards the end (`-1` when the start is after it);
/// `up_to(k)` is `k`, or `n` when there are fewer arguments. Other indices are returned as they are.
fn span_ends<'h>(lib: &Lib, h: &'h Heap, i: T<'h>, n: usize) -> T<'h> {
    let View::App { head, args } = h.view(i) else { return i };
    if head != h.sym_term(lib.s.span) || !(2..=3).contains(&args.len()) {
        return i;
    }
    let all = |t: T<'h>| matches!(h.view(t), View::Sym(s) if h.sym_name(s) == "all");
    let up_to = |t: T<'h>| match h.view(t) {
        View::App { head, args: &[k] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "up_to") => match h.view(k) {
            View::Int(k) if k >= 0 => Some(k.min(n as i64)),
            _ => None,
        },
        _ => None,
    };
    let end = |t: T<'h>, start: bool| {
        if all(t) {
            h.int(if start { 1 } else { -1 })
        } else if let Some(k) = up_to(t) {
            h.int(k)
        } else {
            t
        }
    };
    let (a, b) = (end(args[0], true), end(args[1], false));
    let mut out = vec![a, b];
    if let Some(&st) = args.get(2) {
        let at = |t: T<'h>| match h.view(t) {
            View::Int(v) if v < 0 => Some(n as i64 + 1 + v),
            View::Int(v) => Some(v),
            _ => None,
        };
        out.push(if all(st) {
            match (at(a), at(b)) {
                (Some(x), Some(y)) if x > y => h.int(-1),
                _ => h.int(1),
            }
        } else {
            st
        });
    }
    if out.as_slice() == args {
        i
    } else {
        h.app(head, &out)
    }
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

/// The 0-based positions of `span(i, j, step)` over `n` arguments: `i`, `i + step`, ... while not past `j`
/// (negative indices from the end; a negative step counts down from `i` to `j`).
fn stepped<'h>(lib: &Lib, h: &'h Heap, i: T<'h>, j: T<'h>, st: T<'h>, n: usize) -> Result<Vec<usize>, Stop<'h>> {
    let int = |t: T<'h>| match h.view(t) {
        View::Int(v) => Ok(v),
        View::Sym(_) | View::App { .. } => Err(Stop::Symbolic),
        _ => Err(Stop::Fail(lib.bad_index(h, t, n))),
    };
    let (a, b, d) = (int(i)?, int(j)?, int(st)?);
    let at = |v: i64, t: T<'h>| match position(v, n) {
        Some(p @ 1..) => Ok(p as i64),
        _ => Err(Stop::Fail(lib.bad_index(h, t, n))),
    };
    if d == 0 {
        return Err(Stop::Fail(lib.bad_index(h, st, n)));
    }
    let (lo, hi) = (at(a, i)?, at(b, j)?);
    let mut out = vec![];
    let mut p = lo;
    while (d > 0 && p <= hi) || (d < 0 && p >= hi) {
        out.push((p - 1) as usize);
        p += d;
    }
    Ok(out)
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
    // first(xs, d), last(xs, d): d when xs is empty (d is evaluated like every argument)
    let (x, default) = match *args {
        [x] => (x, None),
        [x, d] => (x, Some(d)),
        _ => return Ok(Reply::Decline),
    };
    let (_, xs) = take!(lib.operand(h, t, x, 1));
    Ok(Reply::Done(match (position(i, xs.len()), default) {
        (Some(p @ 1..), _) => xs[p - 1],
        (_, Some(d)) => d,
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

/// The 0-based positions a sequence specification selects among `len` arguments: `n` (the first `n`, the last
/// `-n`), `[i]` (argument `i`), `[i, j]` and `[i, j, step]` (from `i` to `j`), `all`, `none`. `Ok(None)`: not a
/// specification (the call stays unevaluated); `Err(i)`: an index outside the arguments.
fn selected<'h>(h: &'h Heap, spec: T<'h>, len: usize) -> Result<Option<Vec<usize>>, i64> {
    let at = |i: i64| position(i, len).filter(|&p| p >= 1).map(|p| p - 1).ok_or(i);
    match h.view(spec) {
        View::Int(n) => {
            let (lo, hi) = taken(n, len).ok_or(n)?;
            Ok(Some((lo..hi).collect()))
        }
        View::Sym(s) => Ok(match h.sym_name(s).as_str() {
            "all" => Some((0..len).collect()),
            "none" => Some(vec![]),
            _ => None,
        }),
        View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "list") => {
            let mut v = vec![];
            for &a in args {
                match h.view(a) {
                    View::Int(i) => v.push(i),
                    _ => return Ok(None),
                }
            }
            match *v.as_slice() {
                [i] => Ok(Some(vec![at(i)?])),
                [i, j] | [i, j, _] => {
                    let step = v.get(2).copied().unwrap_or(1);
                    if step < 1 {
                        return Ok(None);
                    }
                    let (a, b) = (at(i)?, at(j)?);
                    Ok(Some(if a > b { vec![] } else { (a..=b).step_by(step as usize).collect() }))
                }
                _ => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

/// `take` (`drop` false) or `drop` of `x` by the specifications `specs`, the first for the arguments of `x`, the
/// next for the arguments of each kept argument, and so on. `Err` with the reply when it does not apply.
fn take_drop<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, x: T<'h>, specs: &[T<'h>], drop: bool) -> Res<Result<T<'h>, Reply<'h>>> {
    let h = cx.heap();
    let Some((&spec, rest)) = specs.split_first() else { return Ok(Ok(x)) };
    // one specification of a contiguous run of a packed array: a slice of the payload, not unpacked (gap G48)
    if let (Some(p), true) = (h.packed(x), rest.is_empty()) {
        if let Ok(Some(picked)) = selected(h, spec, p.len()) {
            let (lo, hi) = if drop {
                // dropping a run at either end keeps a run
                match (picked.first(), picked.last()) {
                    (None, _) | (_, None) => (0, p.len()),
                    (Some(&a), Some(&b)) if a == 0 && b + 1 == picked.len() => (picked.len(), p.len()),
                    (Some(&a), Some(&b)) if b + 1 == p.len() && b + 1 - a == picked.len() => (0, a),
                    _ => (1, 0),
                }
            } else {
                match (picked.first(), picked.last()) {
                    (Some(&a), Some(&b)) if b >= a && b + 1 - a == picked.len() => (a, b + 1),
                    _ => (1, 0),
                }
            };
            let contiguous = picked.windows(2).all(|w| w[1] == w[0] + 1);
            if lo < hi && contiguous {
                charge_copy(cx, hi - lo)?;
                return Ok(Ok(crate::ctx::packed_slice(h, p, lo, hi)));
            }
        }
    }
    let (head, xs) = match lib.operand(h, t, x, 1) {
        Ok(v) => v,
        Err(r) => return Ok(Err(r)),
    };
    let picked = match selected(h, spec, xs.len()) {
        Ok(Some(p)) => p,
        // a number that is not an integer (or a string) is no specification: a failure, as for any wrong kind
        Ok(None) if !matches!(h.view(spec), View::Sym(_) | View::App { .. }) => {
            return Ok(Err(Reply::Done(lib.bad_arg(h, t, 2))));
        }
        Ok(None) => return Ok(Err(Reply::Decline)),
        Err(i) => return Ok(Err(Reply::Done(lib.bad_index(h, h.int(i), xs.len())))),
    };
    charge_copy(cx, xs.len())?;
    let kept: Vec<T<'h>> = if drop {
        let mut keep = vec![true; xs.len()];
        for p in picked {
            keep[p] = false;
        }
        xs.iter().zip(&keep).filter(|(_, &k)| k).map(|(&x, _)| x).collect()
    } else {
        picked.into_iter().map(|p| xs[p]).collect()
    };
    let mut out = Vec::with_capacity(kept.len());
    for k in kept {
        match take_drop(lib, cx, t, k, rest, drop)? {
            Ok(v) => out.push(v),
            Err(r) => return Ok(Err(r)),
        }
    }
    // a power with one operand left is that operand (as in `rest`)
    Ok(Ok(match lib.rebuilt(h, head, &out) {
        Reply::Done(v) | Reply::Again(v) => v,
        Reply::Decline => h.app(head, &out),
    }))
}

/// A list result is final; any other term is evaluated (a call of its head).
fn settled<'h>(lib: &Lib, h: &'h Heap, v: T<'h>) -> Reply<'h> {
    if h.packed(v).is_some() {
        return Reply::Done(v);
    }
    match h.view(v) {
        View::App { head, .. } if head == h.sym_term(lib.s.list) => Reply::Done(v),
        _ => Reply::Again(v),
    }
}

/// `take(xs, spec, ...)`: the arguments the specification selects (`n`: the first `n`, the last `-n`; `[i]`,
/// `[i, j]`, `[i, j, step]`, `all`, `none`); further specifications apply to the arguments of each kept one.
pub(crate) fn take_<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let Some((&x, specs)) = args.split_first() else { return Ok(Reply::Decline) };
    if specs.is_empty() {
        return Ok(Reply::Decline);
    }
    let capped = up_to_specs(cx, x, specs);
    let specs = capped.as_deref().unwrap_or(specs);
    Ok(match take_drop(lib, cx, t, x, specs, false)? {
        Ok(v) => settled(lib, cx.heap(), v),
        Err(r) => r,
    })
}

/// The specifications with `up_to(n)` as the first replaced by `n` capped at the length of `x` (decision
/// cov-core.8), or `None` when there is none.
fn up_to_specs<'h>(cx: &NativeCx<'_, '_, 'h>, x: T<'h>, specs: &[T<'h>]) -> Option<Vec<T<'h>>> {
    let h = cx.heap();
    let len = match h.packed(x) {
        Some(p) => p.dims().first().map_or(0, |&d| d as usize),
        None => crate::ctx::compound(h, x)?.1.len(),
    };
    let n = crate::more::up_to_count(h, specs[0], len)?;
    let mut v = specs.to_vec();
    v[0] = h.int(n);
    Some(v)
}

/// `drop(xs, spec, ...)`: all but the arguments the specification selects (as `take`'s).
pub(crate) fn drop<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let Some((&x, specs)) = args.split_first() else { return Ok(Reply::Decline) };
    if specs.is_empty() {
        return Ok(Reply::Decline);
    }
    let capped = up_to_specs(cx, x, specs);
    let specs = capped.as_deref().unwrap_or(specs);
    Ok(match take_drop(lib, cx, t, x, specs, true)? {
        Ok(v) => settled(lib, cx.heap(), v),
        Err(r) => r,
    })
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
