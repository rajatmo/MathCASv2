#![allow(clippy::needless_borrow, clippy::redundant_guards)]
//! Further list and functional operations (round 14): `map_apply`, `comap`, `comap_apply`,
//! `map_all`, `is_array`, `fold_pair_list`, `fold_pair`, `subsequences`, the `contains_*` tests, `nearest`, `pick`,
//! `min_max`, `take_drop`, `power_range`, `first_position`, `count_distinct`, `delete_elements`,
//! `position_largest`, `position_smallest`, `symmetric_difference`, `xor`, `implies`, `true_q`, `construct`,
//! `identity`, `hold_complete`, `check`, `value_q`, `to_character_code`, `character_range`, `up_to`.
use crate::assocs::entries;
use crate::ctx::{apply, take, Lib, Meter};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The head and arguments of a compound term (not a complex number), or `None` for an atom.
fn compound<'h>(h: &'h Heap, t: T<'h>) -> Option<(T<'h>, &'h [T<'h>])> {
    crate::ctx::compound(h, t)
}

/// The elements a "list" argument stands for: a list's (or any compound term's) arguments, an association's values.
fn elements<'h>(lib: &Lib, cx: &NativeCx<'_, '_, 'h>, x: T<'h>) -> Option<Vec<T<'h>>> {
    let h = cx.heap();
    if let Some(es) = entries(lib, h, x) {
        return Some(es.into_iter().map(|(_, v)| v).collect());
    }
    if let Some(xs) = cx.as_list(x) {
        return Some(xs.to_vec());
    }
    compound(h, x).map(|(_, a)| a.to_vec())
}

/// `x` rebuilt with each argument (an association: each value) replaced by `f(argument)`.
fn rebuild_each<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    x: T<'h>,
    mut f: impl FnMut(&mut NativeCx<'_, '_, 'h>, T<'h>) -> T<'h>,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if let Some(es) = entries(lib, h, x) {
        cx.tick(es.len() as u64)?;
        let rule = h.sym_term(lib.s.rule);
        let out: Vec<T<'h>> = es.iter().map(|&(k, v)| h.app(rule, &[k, f(cx, v)])).collect();
        return Ok(Some(h.app(h.sym_term(lib.s.assoc), &out)));
    }
    let (head, args) = match cx.as_list(x) {
        Some(xs) => (h.sym_term(lib.s.list), xs),
        None => match compound(h, x) {
            Some(c) => c,
            None => return Ok(None),
        },
    };
    cx.tick(args.len() as u64)?;
    let out: Vec<T<'h>> = args.iter().map(|&a| f(cx, a)).collect();
    Ok(Some(h.app(head, &out)))
}

/// `x` rebuilt with its head and each argument replaced by `f(part)` (`heads: true`: the head is a part at level 1);
/// `None` for an atom or an association (whose head is not a part of its tree).
fn rebuild_with_head<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    x: T<'h>,
    mut f: impl FnMut(&mut NativeCx<'_, '_, 'h>, T<'h>) -> T<'h>,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if entries(lib, h, x).is_some() {
        return Ok(None);
    }
    let (head, args) = match cx.as_list(x) {
        Some(xs) => (h.sym_term(lib.s.list), xs),
        None => match compound(h, x) {
            Some(c) => c,
            None => return Ok(None),
        },
    };
    cx.tick(args.len() as u64 + 1)?;
    let new_head = f(cx, head);
    let out: Vec<T<'h>> = args.iter().map(|&a| f(cx, a)).collect();
    Ok(Some(h.app(new_head, &out)))
}

/// `map_apply(f, x)`: each element `g(a, b, ...)` of `x` becomes `f(a, b, ...)` (atoms stay); an association's
/// values likewise.
pub(crate) fn map_apply<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let Some((args, heads)) = crate::ctx::heads_flag(h, args, 2) else { return Ok(Reply::Decline) };
    let [f, x] = *args else { return Ok(Reply::Decline) };
    let each = |cx: &mut NativeCx<'_, '_, 'h>, a: T<'h>| match cx.as_list(a) {
        Some(ys) => h.app(f, ys),
        None => match compound(h, a) {
            Some((_, ys)) => h.app(f, ys),
            None => a,
        },
    };
    let r = if heads { rebuild_with_head(lib, cx, x, each)? } else { rebuild_each(lib, cx, x, each)? };
    Ok(match r {
        Some(v) => Reply::Again(v),
        None if heads => Reply::Decline,
        None => Reply::Done(x),
    })
}

/// `comap(fs, x)`: each element `g` of `fs` becomes `g(x)`, keeping the head of `fs` (`comap([f, g], x)` is
/// `[f(x), g(x)]`, `comap(f + g, x)` is `f(x) + g(x)`); an association of functions gives an association.
pub(crate) fn comap<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // heads: true: the head of fs is a part at level 1 too (comap(h(a), x, heads: true) is h(x)(a(x)))
    let Some((args, heads)) = crate::ctx::heads_flag(h, args, 2) else { return Ok(Reply::Decline) };
    if heads {
        let [fs, x] = *args else { return Ok(Reply::Decline) };
        return Ok(match rebuild_with_head(lib, cx, fs, |_, g| h.app(g, &[x]))? {
            Some(v) => Reply::Again(v),
            None => Reply::Decline,
        });
    }
    // comap(fs, x, levels): every part g of fs at the levels becomes g(x)
    if let [fs, x, spec] = *args {
        let Some(levels) = crate::levels::level_spec(h, spec) else { return Ok(Reply::Decline) };
        let mut visit = |_: &mut NativeCx<'_, '_, 'h>, u: T<'h>, _: &[i64]| -> Res<Option<T<'h>>> { Ok(Some(h.app(u, &[x]))) };
        return Ok(match crate::levels::walk(cx, fs, levels, &mut visit)? {
            Some(v) => Reply::Again(v),
            None => Reply::Decline,
        });
    }
    let [fs, x] = *args else { return Ok(Reply::Decline) };
    Ok(match rebuild_each(lib, cx, fs, |_, g| h.app(g, &[x]))? {
        Some(v) => Reply::Again(v),
        None => Reply::Decline,
    })
}

/// `comap_apply(fs, xs)`: each element `g` of `fs` becomes `g(a, b, ...)` with the arguments of `xs`.
pub(crate) fn comap_apply<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let Some((args, heads)) = crate::ctx::heads_flag(h, args, 2) else { return Ok(Reply::Decline) };
    let [fs, xs] = *args else { return Ok(Reply::Decline) };
    let Some(ys) = elements(lib, cx, xs) else { return Ok(Reply::Decline) };
    let r = if heads {
        rebuild_with_head(lib, cx, fs, |_, g| h.app(g, &ys))?
    } else {
        rebuild_each(lib, cx, fs, |_, g| h.app(g, &ys))?
    };
    Ok(match r {
        Some(v) => Reply::Again(v),
        None => Reply::Decline,
    })
}

/// `map_all(f, x)`: `f` applied to every part of `x`, innermost first (atoms included; heads too with
/// `heads: true`).
pub(crate) fn map_all<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let Some((args, heads)) = crate::ctx::heads_flag(cx.heap(), args, 2) else { return Ok(Reply::Decline) };
    let [f, x] = *args else { return Ok(Reply::Decline) };
    let mut m = Meter::default();
    let v = map_all_in(cx, &mut m, f, x, 0, heads)?;
    Ok(Reply::Again(v))
}

fn map_all_in<'h>(cx: &mut NativeCx<'_, '_, 'h>, m: &mut Meter, f: T<'h>, x: T<'h>, depth: usize, heads: bool) -> Res<T<'h>> {
    let h = cx.heap();
    m.step(cx)?;
    let inner = match compound(h, x) {
        Some((head, args)) if depth < 10_000 => {
            let mut out = Vec::with_capacity(args.len());
            for &a in args {
                out.push(map_all_in(cx, m, f, a, depth + 1, heads)?);
            }
            let head = if heads { map_all_in(cx, m, f, head, depth + 1, heads)? } else { head };
            h.app(head, &out)
        }
        _ => x,
    };
    Ok(h.app(f, &[inner]))
}

/// `is_array(x)`: whether `x` is a full array (lists of equal length to some depth, no list among the elements at
/// that depth); `is_array(x, d)`: of depth `d` (an integer, or a pattern the depth matches); `is_array(x, d, test)`:
/// with `test(e)` true for every element.
pub(crate) fn is_array<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (x, depth, test) = match *args {
        [x] => (x, None, None),
        [x, d] => (x, Some(d), None),
        [x, d, p] => (x, Some(d), Some(p)),
        _ => return Ok(Reply::Decline),
    };
    let no = Reply::Done(cx.bool(false));
    if cx.as_list(x).is_none() && h.packed(x).is_none() {
        return Ok(no);
    }
    let dims = crate::structure::dims_of(cx, x)?;
    // the elements at the array's depth
    let mut level = vec![x];
    for _ in 0..dims.len() {
        let mut next = Vec::new();
        for &u in &level {
            cx.tick(1)?;
            match cx.as_list(u) {
                Some(us) => next.extend_from_slice(us),
                None => return Ok(no),
            }
        }
        level = next;
    }
    if level.iter().any(|&e| cx.as_list(e).is_some()) {
        return Ok(no);
    }
    if let Some(d) = depth {
        let n = h.int(dims.len() as i64);
        let ok = match h.view(d) {
            View::Int(k) => k == dims.len() as i64,
            _ => {
                let v = cx.eval(h.call(h.sym("match_q"), &[n, d]))?;
                cx.is_true(v)
            }
        };
        if !ok {
            return Ok(no);
        }
    }
    if let Some(p) = test {
        for &e in &level {
            let v = apply(cx, p, &[e])?;
            if !cx.is_true(v) {
                return Ok(no);
            }
        }
    }
    let _ = lib;
    Ok(Reply::Done(cx.bool(true)))
}

/// The pair `[out, next]` `f` gives, or `None`.
fn pair<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, y: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let p = apply(cx, f, &[y, x])?;
    Ok(match cx.as_list(p) {
        Some(&[a, b]) => Some((a, b)),
        _ => None,
    })
}

/// `fold_pair_list(f, y, xs)`: with `f(y, x)` giving `[out, y']`, the outputs for each element of `xs` in turn
/// (`fold_pair_list(f, xs)` starts from the first element); `fold_pair_list(f, y, xs, g)`: `g([out, y'])` instead
/// of `out`. The result keeps the head of `xs`.
pub(crate) fn fold_pair_list<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    fold_pairs(lib, cx, args, true)
}

/// `fold_pair(f, y, xs)`: the last output of `fold_pair_list` (`g` of the last pair with a fourth argument).
pub(crate) fn fold_pair<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    fold_pairs(lib, cx, args, false)
}

fn fold_pairs<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], all: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (f, y, xs, g) = match *args {
        [f, xs] => {
            let Some((head, a)) = compound(h, xs) else { return Ok(Reply::Decline) };
            let Some((&first, rest)) = a.split_first() else { return Ok(Reply::Decline) };
            (f, first, h.app(head, rest), None)
        }
        [f, y, xs] => (f, y, xs, None),
        [f, y, xs, g] => (f, y, xs, Some(g)),
        _ => return Ok(Reply::Decline),
    };
    let Some((head, elems)) = compound(h, xs) else { return Ok(Reply::Decline) };
    if elems.is_empty() && !all {
        return Ok(Reply::Decline);
    }
    let mut state = y;
    let mut outs = Vec::with_capacity(elems.len());
    let mut last = None;
    for &x in elems {
        cx.tick(1)?;
        let Some((out, next)) = pair(cx, f, state, x)? else { return Ok(Reply::Decline) };
        // `fold_pair` keeps only the last output: a failure (a `throw`) held in an earlier pair ends the fold there,
        // as `fold` does, instead of being forgotten by the later pairs
        if !all {
            for v in [out, next] {
                if lib.is_failure(h, v) {
                    return Ok(Reply::Done(v));
                }
            }
        }
        let shown = match g {
            Some(g) => h.app(g, &[cx.list(&[out, next])]),
            None => out,
        };
        if all {
            outs.push(shown);
        }
        last = Some(shown);
        state = next;
    }
    Ok(match (all, last) {
        (true, _) => Reply::Again(h.app(head, &outs)),
        (false, Some(v)) => Reply::Again(v),
        (false, None) => Reply::Decline,
    })
}

/// `subsequences(xs)`: every run of consecutive elements, by length then position (the empty one first);
/// `subsequences(xs, n)`: of length at most `n`; `subsequences(xs, [n])`: exactly `n`; `subsequences(xs, [a, b])`:
/// `a` to `b`; `[a, b, s]`: lengths `a, a + s, ...`. The runs keep the head of `xs`.
pub(crate) fn subsequences<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (x, spec, pick) = match *args {
        [x] => (x, None, None),
        [x, s] => (x, Some(s), None),
        [x, s, p] => (x, Some(s), Some(p)),
        _ => return Ok(Reply::Decline),
    };
    let (head, xs) = take!(lib.operand(h, t, x, 1));
    let n = xs.len() as i64;
    let lengths: Vec<i64> = match spec {
        None => (0..=n).collect(),
        Some(s) => match h.view(s) {
            View::Int(k) if k >= 0 => (0..=k.min(n)).collect(),
            View::Sym(a) if h.sym_name(a) == "all" => (0..=n).collect(),
            _ => {
                let Some(ks) = cx.as_list(s) else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
                let int = |k: T<'h>| if let View::Int(v) = h.view(k) { Some(v) } else { None };
                let ks: Option<Vec<i64>> = ks.iter().map(|&k| int(k)).collect();
                match ks.as_deref() {
                    Some(&[k]) => vec![k],
                    Some(&[a, b]) => (a..=b).collect(),
                    Some(&[a, b, s]) if s > 0 => (a..=b).step_by(s as usize).collect(),
                    _ => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
                }
            }
        },
    };
    let total: i64 = lengths.iter().filter(|&&k| (0..=n).contains(&k)).map(|&k| n - k + 1).sum();
    if total > 1_000_000 {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let mut out = Vec::with_capacity(total as usize);
    for &k in &lengths {
        if !(0..=n).contains(&k) {
            continue;
        }
        // one empty run, not one per position
        let starts = if k == 0 { 1 } else { (n - k) as usize + 1 };
        for i in 0..starts {
            cx.tick(k as u64 + 1)?;
            out.push(h.app(head, &xs[i..i + k as usize]));
        }
    }
    // subsequences(xs, spec, m): the first m of them, up_to(m) at most m, [k] the k-th, [a, b] the a-th to the
    // b-th, [a, b, s] every s-th from a to b (s may be negative) (round 37); a position past the end declines
    if let Some(p) = pick {
        let len = out.len() as i64;
        let int = |k: T<'h>| if let View::Int(v) = h.view(k) { Some(v) } else { None };
        let idx: Vec<i64> = if let Some(m) = int(p) {
            if m < 0 || m > len {
                return Ok(Reply::Decline);
            }
            (1..=m).collect()
        } else if let Some(m) = up_to_count(h, p, usize::MAX) {
            (1..=m.min(len)).collect()
        } else {
            let Some(ks) = cx.as_list(p) else { return Ok(Reply::Decline) };
            let ks: Option<Vec<i64>> = ks.iter().map(|&k| int(k)).collect();
            let (a, b, s) = match ks.as_deref() {
                Some(&[k]) => (k, k, 1),
                Some(&[a, b]) => (a, b, 1),
                Some(&[a, b, s]) if s != 0 => (a, b, s),
                _ => return Ok(Reply::Decline),
            };
            let mut v = vec![];
            let mut k = a;
            while (s > 0 && k <= b) || (s < 0 && k >= b) {
                if k < 1 || k > len {
                    return Ok(Reply::Decline);
                }
                v.push(k);
                k += s;
            }
            v
        };
        if idx.iter().any(|&k| k < 1 || k > len) {
            return Ok(Reply::Decline);
        }
        let chosen: Vec<T<'h>> = idx.iter().map(|&k| out[k as usize - 1]).collect();
        return Ok(Reply::Again(cx.list(&chosen)));
    }
    Ok(Reply::Again(cx.list(&out)))
}

/// Which of the `contains_*` tests.
#[derive(Clone, Copy)]
enum Contains {
    Any,
    All,
    None,
    Only,
    Exactly,
}

fn contains<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], which: Contains) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, test) = crate::ctx::option(h, args, 2, "same_test");
    let [a, b] = *args else { return Ok(Reply::Decline) };
    let (Some(xs), Some(ys)) = (elements(lib, cx, a), elements(lib, cx, b)) else { return Ok(Reply::Decline) };
    // `same_test: f`: elements are the same when f(x, y) is true (every pair compared; P9B.8)
    if let Some(f) = test {
        let same = |cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, y: T<'h>| -> Res<bool> {
            cx.tick(1)?;
            let v = apply(cx, f, &[x, y])?;
            Ok(cx.is_true(v))
        };
        let in_list = |cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, zs: &[T<'h>], x_first: bool| -> Res<bool> {
            for &z in zs {
                let hit = if x_first { same(cx, x, z)? } else { same(cx, z, x)? };
                if hit {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        let any_of = |cx: &mut NativeCx<'_, '_, 'h>, from: &[T<'h>], into: &[T<'h>], x_first: bool, want: bool| -> Res<bool> {
            for &y in from {
                if in_list(cx, y, into, x_first)? == want {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        let v = match which {
            Contains::Any => any_of(cx, &ys, &xs, false, true)?,
            Contains::All => !any_of(cx, &ys, &xs, false, false)?,
            Contains::None => !any_of(cx, &ys, &xs, false, true)?,
            Contains::Only => !any_of(cx, &xs, &ys, true, false)?,
            Contains::Exactly => !any_of(cx, &ys, &xs, false, false)? && !any_of(cx, &xs, &ys, true, false)?,
        };
        return Ok(Reply::Done(cx.bool(v)));
    }
    cx.tick((xs.len() + ys.len()) as u64)?;
    let xset: rustc_hash::FxHashSet<T<'h>> = xs.iter().copied().collect();
    let yset: rustc_hash::FxHashSet<T<'h>> = ys.iter().copied().collect();
    let v = match which {
        Contains::Any => ys.iter().any(|y| xset.contains(y)),
        Contains::All => ys.iter().all(|y| xset.contains(y)),
        Contains::None => !ys.iter().any(|y| xset.contains(y)),
        Contains::Only => xs.iter().all(|x| yset.contains(x)),
        Contains::Exactly => xset == yset,
    };
    Ok(Reply::Done(cx.bool(v)))
}

/// `contains_any(xs, ys)`: whether `xs` has some element of `ys` (elements compared by identity).
pub(crate) fn contains_any<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    contains(lib, cx, args, Contains::Any)
}
/// `contains_all(xs, ys)`: whether `xs` has every element of `ys`.
pub(crate) fn contains_all<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    contains(lib, cx, args, Contains::All)
}
/// `contains_none(xs, ys)`: whether `xs` has no element of `ys`.
pub(crate) fn contains_none<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    contains(lib, cx, args, Contains::None)
}
/// `contains_only(xs, ys)`: whether every element of `xs` is in `ys`.
pub(crate) fn contains_only<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    contains(lib, cx, args, Contains::Only)
}
/// `contains_exactly(xs, ys)`: whether `xs` and `ys` have the same distinct elements.
pub(crate) fn contains_exactly<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    contains(lib, cx, args, Contains::Exactly)
}

/// The Levenshtein distance of two strings, by characters.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let sub = prev[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// A machine real for a numeric element (`n(x)`), if it has one.
fn real<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    Ok(match h.view(x) {
        View::Int(v) => Some(v as f64),
        View::Real(v) => Some(v),
        View::Str(_) => None,
        // numeric constants (`pi`) are symbols
        _ => match h.view(cx.eval(h.call(h.sym("n"), &[x]))?) {
            View::Real(v) => Some(v),
            View::Int(v) => Some(v as f64),
            _ => None,
        },
    })
}

/// The distance of two elements for `nearest`: numbers by their difference, numeric vectors by the Euclidean norm,
/// strings by edit distance.
fn distance<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: T<'h>, b: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    if let (View::Str(s), View::Str(t)) = (h.view(a), h.view(b)) {
        return Ok(Some(edit_distance(s, t) as f64));
    }
    if let (Some(us), Some(vs)) = (cx.as_list(a), cx.as_list(b)) {
        if us.len() != vs.len() {
            return Ok(None);
        }
        let (us, vs) = (us.to_vec(), vs.to_vec());
        let mut s = 0.0;
        for (u, v) in us.into_iter().zip(vs) {
            match distance(cx, u, v)? {
                Some(d) => s += d * d,
                None => return Ok(None),
            }
        }
        return Ok(Some(s.sqrt()));
    }
    match (real(cx, a)?, real(cx, b)?) {
        (Some(x), Some(y)) => Ok(Some((x - y).abs())),
        _ => Ok(None),
    }
}

/// The distance of two elements under a user's `distance_function` (a name or a function of two arguments).
fn custom_distance<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, a: T<'h>, b: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let v = cx.eval(h.app(f, &[a, b]))?;
    real(cx, v)
}

/// `nearest(xs, x)`: the elements of `xs` nearest to `x` (every one at the least distance); `nearest(xs, x, n)`:
/// the `n` nearest, nearest first (equal distances in order); `nearest(xs, x, [n, r])`: at most `n` (`all`) of
/// those within the distance `r`. Numbers, numeric vectors (Euclidean) and strings (edit distance), or the
/// option `distance_function` (a function of two elements). `xs` may be a list of rules `v -> label`, an
/// association `label -> v`, or `list -> labels`, `list -> "Index"`, `list -> "Element"`, `list -> "Distance"`,
/// `list -> all` (an association per element) or a list of those names (a list per element): the labels are given.
pub(crate) fn nearest<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let sym = |s: &str| h.sym_term(h.sym(s));
    // the options: distance_function
    let (args, dist_fn) = match args.split_last() {
        Some((&last, rest)) if matches!(h.view(last), View::App { head, .. } if head == sym("options")) => {
            let View::App { args: rules, .. } = h.view(last) else { return Ok(Reply::Decline) };
            let mut f = None;
            for &r in rules {
                match h.view(r) {
                    View::App { args: &[k, v], .. } if matches!(h.view(k), View::Sym(s) if h.sym_name(s) == "distance_function") => {
                        // `automatic` is the default distance
                        if !matches!(h.view(v), View::Sym(s) if h.sym_name(s) == "automatic") {
                            f = Some(v);
                        }
                    }
                    _ => return Ok(Reply::Decline),
                }
            }
            (rest, f)
        }
        _ => (args, None),
    };
    let (data, x, n, radius) = match *args {
        [d, x] => (d, x, None, None),
        [d, x, n] => match h.view(n) {
            View::Int(k) if k >= 0 => (d, x, Some(k as usize), None),
            _ => match cx.as_list(n) {
                Some(&[c, r]) => {
                    let c = match h.view(c) {
                        View::Int(k) if k >= 0 => Some(k as usize),
                        View::Sym(s) if h.sym_name(s) == "all" => None,
                        _ => return Ok(Reply::Decline),
                    };
                    let Some(r) = real(cx, r)? else { return Ok(Reply::Decline) };
                    (d, x, c, Some(r))
                }
                _ => return Ok(Reply::Decline),
            },
        },
        _ => return Ok(Reply::Decline),
    };
    let rule = h.sym_term(lib.s.rule);
    // `list -> spec` with a result spec instead of labels
    let mut data = data;
    let mut shape: Option<T<'h>> = None;
    if let View::App { head, args: &[l, r] } = h.view(data) {
        if head == rule && cx.as_list(l).is_some() {
            let is_name = |u: T<'h>| match h.view(u) {
                View::Str(s) => matches!(s, "Index" | "Element" | "Distance"),
                View::Sym(s) => h.sym_name(s) == "all",
                _ => false,
            };
            let spec = is_name(r) || cx.as_list(r).is_some_and(|es| !es.is_empty() && es.iter().all(|&e| is_name(e)));
            if spec {
                data = l;
                shape = Some(r);
            }
        }
    }
    // `list -> labels` (a list of as many labels) or `list -> automatic`
    let mut explicit_labels: Option<Vec<T<'h>>> = None;
    if shape.is_none() {
        if let View::App { head, args: &[l, r] } = h.view(data) {
            if head == rule {
                if let (Some(vs), Some(ls)) = (cx.as_list(l), cx.as_list(r)) {
                    if vs.len() == ls.len() {
                        explicit_labels = Some(ls.to_vec());
                        data = l;
                    }
                } else if cx.as_list(l).is_some() && matches!(h.view(r), View::Sym(s) if h.sym_name(s) == "automatic") {
                    let n = cx.as_list(l).map_or(0, <[T<'h>]>::len);
                    explicit_labels = Some((1..=n as i64).map(|i| h.int(i)).collect());
                    data = l;
                }
            }
        }
    }
    let (values, labels): (Vec<T<'h>>, Vec<T<'h>>) = if let Some(es) = entries(lib, h, data) {
        es.iter().map(|&(k, v)| (v, k)).unzip()
    } else {
        let xs = take!(lib.list_operand(h, t, data, 1));
        match explicit_labels {
            Some(ls) => (xs.to_vec(), ls),
            None => xs
                .iter()
                .map(|&e| match h.view(e) {
                    View::App { head, args: &[v, l] } if head == rule => (v, l),
                    _ => (e, e),
                })
                .unzip(),
        }
    };
    let mut ds = Vec::with_capacity(values.len());
    for &v in &values {
        cx.tick(1)?;
        let d = match dist_fn {
            Some(f) => custom_distance(cx, f, v, x)?,
            None => distance(cx, v, x)?,
        };
        match d {
            Some(d) if !d.is_nan() => ds.push(d),
            _ => return Ok(Reply::Decline),
        }
    }
    let mut ix: Vec<usize> = (0..values.len()).collect();
    ix.sort_by(|&a, &b| ds[a].total_cmp(&ds[b]));
    if let Some(r) = radius {
        ix.retain(|&i| ds[i] <= r);
    }
    let chosen: Vec<usize> = match (n, radius) {
        (Some(k), _) => ix.into_iter().take(k).collect(),
        (None, Some(_)) => ix,
        (None, None) => {
            let best = ix.first().map(|&i| ds[i]);
            ix.into_iter().take_while(|&i| Some(ds[i]) == best).collect()
        }
    };
    let Some(shape) = shape else {
        let out: Vec<T<'h>> = chosen.iter().map(|&i| labels[i]).collect();
        return Ok(Reply::Done(cx.list(&out)));
    };
    let field = |i: usize, name: &str| -> T<'h> {
        match name {
            "Index" => h.int(i as i64 + 1),
            "Element" => values[i],
            _ => h.real(ds[i]),
        }
    };
    let out: Vec<T<'h>> = chosen
        .iter()
        .map(|&i| match h.view(shape) {
            View::Str(name) => field(i, name),
            View::Sym(_) => {
                let rules: Vec<T<'h>> = ["Element", "Index", "Distance"].iter().map(|&nm| h.app(rule, &[h.string(nm), field(i, nm)])).collect();
                h.app(sym("assoc"), &rules)
            }
            _ => {
                let names = cx.as_list(shape).unwrap_or(&[]).to_vec();
                let items: Vec<T<'h>> = names
                    .iter()
                    .map(|&nm| match h.view(nm) {
                        View::Str(name) => field(i, name),
                        _ => h.int(0),
                    })
                    .collect();
                cx.list(&items)
            }
        })
        .collect();
    Ok(Reply::Done(cx.list(&out)))
}

/// `pick(xs, sel)`: the elements of `xs` where `sel` (of the same shape) is `true`; `pick(xs, sel, p)`: where the
/// element of `sel` matches the pattern `p`. Nested lists are picked from element by element.
pub(crate) fn pick<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, sel, p) = match *args {
        [x, s] => (x, s, None),
        [x, s, p] => (x, s, Some(p)),
        _ => return Ok(Reply::Decline),
    };
    let mut m = match p {
        Some(p) => match crate::matching::Matcher::new(lib, h, p) {
            Some(m) => Some(m),
            None => return Ok(Reply::Decline),
        },
        None => None,
    };
    Ok(match pick_in(cx, &mut m, xs, sel, 0)? {
        Some(v) => Reply::Done(v),
        None => Reply::Decline,
    })
}

fn pick_in<'l, 'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    m: &mut Option<crate::matching::Matcher<'l, 'h>>,
    xs: T<'h>,
    sel: T<'h>,
    depth: usize,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let (Some((head, a)), Some((_, s))) = (compound(h, xs), compound(h, sel)) else { return Ok(None) };
    if a.len() != s.len() || depth > 1000 {
        return Ok(None);
    }
    let mut out = Vec::with_capacity(a.len());
    for (&x, &y) in a.iter().zip(s) {
        cx.tick(1)?;
        let hit = match m {
            Some(m) => m.matches(cx, y)?,
            None => cx.is_true(y),
        };
        if hit {
            out.push(x);
        } else if compound(h, y).is_some() && compound(h, x).is_some() {
            if let Some(v) = pick_in(cx, m, x, y, depth + 1)? {
                out.push(v);
            }
        }
    }
    Ok(Some(h.app(head, &out)))
}

/// `min_max(xs)`: `[min(xs), max(xs)]`; `min_max(xs, d)`: widened by `d` on each side (`[d1, d2]`: by `d1` below,
/// `d2` above).
pub(crate) fn min_max<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let sym = |s: &str| h.sym_term(h.sym(s));
    let (xs, d) = match *args {
        [xs] => (xs, None),
        [xs, d] => (xs, Some(d)),
        _ => return Ok(Reply::Decline),
    };
    let lo = h.app(sym("min"), &[xs]);
    let hi = h.app(sym("max"), &[xs]);
    let (lo, hi) = match d {
        None => (lo, hi),
        Some(d) => {
            let (d1, d2) = match cx.as_list(d) {
                Some(&[a, b]) => (a, b),
                _ => (d, d),
            };
            let neg = h.app(sym("mul"), &[h.int(-1), d1]);
            (h.app(sym("add"), &[lo, neg]), h.app(sym("add"), &[hi, d2]))
        }
    };
    Ok(Reply::Again(cx.list(&[lo, hi])))
}

/// `take_drop(xs, n)`: `[take(xs, n), drop(xs, n)]`.
pub(crate) fn take_drop<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, n] = *args else { return Ok(Reply::Decline) };
    let sym = |s: &str| h.sym_term(h.sym(s));
    let a = h.app(sym("take"), &[xs, n]);
    let b = h.app(sym("drop"), &[xs, n]);
    Ok(Reply::Again(cx.list(&[a, b])))
}

/// The most terms of a symbolic `power_range`.
const SYMBOLIC_STEPS: usize = 10_000;

/// `power_range(a, b)`: `a, 10 a, 100 a, ...` while not past `b`; `power_range(a, b, r)`: ratio `r`;
/// `power_range(b)`: from 1. Symbolic bounds: the terms up to the one equal to `b`.
pub(crate) fn power_range<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (a, b, r) = match *args {
        // power_range(b): 1, 10, 100, ... up to b
        [b] => (h.int(1), b, h.int(10)),
        [a, b] => (a, b, h.int(10)),
        [a, b, r] => (a, b, r),
        _ => return Ok(Reply::Decline),
    };
    let mul = h.sym_term(h.sym("mul"));
    let (Some(fa), Some(fb), Some(fr)) = (real(cx, a)?, real(cx, b)?, real(cx, r)?) else {
        // symbolic: a, a r, a r^2, ... until a term equal to b (at most SYMBOLIC_STEPS terms), else unevaluated
        //
        let mut out = vec![a];
        let mut cur = a;
        while cur != b {
            if out.len() >= SYMBOLIC_STEPS {
                return Ok(Reply::Decline);
            }
            cx.tick(1)?;
            cur = cx.eval(h.app(mul, &[cur, r]))?;
            out.push(cur);
        }
        return Ok(Reply::Done(cx.list(&out)));
    };
    if fr == 0.0 || fr == 1.0 || fa == 0.0 {
        return Ok(Reply::Done(lib.bad_arg(h, t, if fa == 0.0 { 1 } else { 3 })));
    }
    // a negative ratio: the terms alternate in sign; they run while their size has not passed the size of b
    if fr < 0.0 {
        if fr == -1.0 || fb == 0.0 {
            return Ok(Reply::Done(lib.bad_arg(h, t, if fb == 0.0 { 2 } else { 3 })));
        }
        let growing = fr.abs() > 1.0;
        let (mut out, mut cur, mut fc) = (vec![], a, fa);
        while out.len() < 1_000_000 && fc.is_finite() && fc != 0.0 && if growing { fc.abs() <= fb.abs() * (1.0 + 1e-12) } else { fc.abs() >= fb.abs() * (1.0 - 1e-12) } {
            cx.tick(1)?;
            out.push(cur);
            cur = cx.eval(h.app(mul, &[cur, r]))?;
            match real(cx, cur)? {
                Some(v) => fc = v,
                None => break,
            }
        }
        return Ok(Reply::Done(cx.list(&out)));
    }
    let growing = (fr > 1.0) == (fa > 0.0);
    let past = |v: f64| if growing { v > fb * (1.0 + 1e-12) } else { v < fb * (1.0 - 1e-12) };
    let mut out = vec![];
    let mut cur = a;
    let mut fc = fa;
    while !past(fc) && out.len() < 1_000_000 {
        cx.tick(1)?;
        out.push(cur);
        cur = cx.eval(h.app(mul, &[cur, r]))?;
        match real(cx, cur)? {
            Some(v) => fc = v,
            None => break,
        }
        if (fr < 1.0 && fa > 0.0 && fc < fb * (1.0 - 1e-12)) || !fc.is_finite() {
            break;
        }
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// `first_position(x, p)`: the position of the first part of `x` matching `p` (as `position` orders them);
/// `first_position(x, p, d)`: `d` when none does (`missing("NotFound")` without it).
pub(crate) fn first_position<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `heads: false` is passed on to `position`
    let (args, heads) = crate::ctx::option(h, args, 2, "heads");
    let (x, p, d, lev) = match *args {
        [x, p] => (x, p, None, None),
        [x, p, d] => (x, p, Some(d), None),
        // first_position(x, p, d, levels): the first position at those levels
        [x, p, d, l] => (x, p, Some(d), Some(l)),
        _ => return Ok(Reply::Decline),
    };
    let mut pargs = vec![x, p];
    pargs.extend(lev);
    if let Some(v) = heads {
        pargs.push(h.call(h.sym("options"), &[h.call(h.sym("rule"), &[h.sym_term(h.sym("heads")), v])]));
    }
    let call = h.call(h.sym("position"), &pargs);
    let ps = cx.eval(call)?;
    Ok(match cx.as_list(ps) {
        Some(&[first, ..]) => Reply::Done(first),
        Some(&[]) => match d {
            Some(d) => Reply::Done(d),
            None => Reply::Again(h.call(h.sym("missing"), &[h.string("NotFound")])),
        },
        _ => Reply::Decline,
    })
}

/// `count_distinct(xs)`: the number of distinct elements.
pub(crate) fn count_distinct<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // count_distinct(xs, f): the classes `unique(xs, f)` keeps
    if let [xs, f] = *args {
        if cx.as_list(f).is_some() || !matches!(h.view(f), View::Sym(_) | View::App { .. }) {
            return Ok(Reply::Decline);
        }
        let xs = take!(lib.list_operand(h, t, xs, 1));
        return Ok(match crate::group::classes(lib, cx, xs, f)? {
            Ok(gs) => Reply::Done(h.int(gs.len() as i64)),
            Err(v) => Reply::Done(v),
        });
    }
    let [xs] = *args else { return Ok(Reply::Decline) };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    cx.tick(xs.len() as u64)?;
    let set: rustc_hash::FxHashSet<T<'h>> = xs.iter().copied().collect();
    Ok(Reply::Done(h.int(set.len() as i64)))
}

/// `delete_elements(xs, es)`: `xs` without any element of the list `es`.
pub(crate) fn delete_elements<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x, es] = *args else { return Ok(Reply::Decline) };
    let (head, xs) = take!(lib.operand(h, t, x, 1));
    // delete_elements(xs, n -> es): at most n occurrences of each element of es, the first ones; n -> es with a
    // list n: at most n_k of es_k (counts of a repeated element add up); n may be inf (decision r37.3)
    if let View::App { head: r, args: &[n, es] } = h.view(es) {
        if r == h.sym_term(h.sym("rule")) {
            let es = take!(lib.list_operand(h, t, es, 2));
            let inf = h.sym_term(h.sym("inf"));
            let count = |c: T<'h>| match h.view(c) {
                View::Int(k @ 0..) => Some(k as u64),
                _ if c == inf => Some(u64::MAX),
                _ => None,
            };
            let counts: Vec<u64> = match (count(n), cx.as_list(n)) {
                (Some(k), _) => vec![k; es.len()],
                (None, Some(ns)) if ns.len() == es.len() => match ns.iter().map(|&c| count(c)).collect::<Option<Vec<_>>>() {
                    Some(v) => v,
                    None => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
                },
                _ => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
            };
            cx.tick((xs.len() + es.len()) as u64)?;
            let mut left: rustc_hash::FxHashMap<T<'h>, u64> = rustc_hash::FxHashMap::default();
            for (&e, &k) in es.iter().zip(&counts) {
                let c = left.entry(e).or_insert(0);
                *c = c.saturating_add(k);
            }
            let mut kept = Vec::with_capacity(xs.len());
            for &e in xs {
                match left.get_mut(&e) {
                    Some(c) if *c > 0 => *c -= 1,
                    _ => kept.push(e),
                }
            }
            return Ok(lib.rebuilt(h, head, &kept));
        }
    }
    let es = take!(lib.list_operand(h, t, es, 2));
    cx.tick((xs.len() + es.len()) as u64)?;
    let gone: rustc_hash::FxHashSet<T<'h>> = es.iter().copied().collect();
    let kept: Vec<T<'h>> = xs.iter().copied().filter(|e| !gone.contains(e)).collect();
    Ok(lib.rebuilt(h, head, &kept))
}

/// `position_largest(xs)`: the positions of the largest elements (numerical order); `position_smallest(xs)`.
pub(crate) fn position_largest<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    position_extreme(lib, cx, t, args, true)
}
/// `position_smallest(xs)`: the positions of the smallest elements.
pub(crate) fn position_smallest<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    position_extreme(lib, cx, t, args, false)
}

fn position_extreme<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], largest: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    // position_largest(xs, n, p): ordered by the ordering function p (as `ordering(xs, all, p)` orders), the n
    // extreme groups of elements p does not separate
    if let [xs, n, p] = *args {
        let View::Int(n @ 0..) = h.view(n) else { return Ok(Reply::Decline) };
        let Some(items) = cx.as_list(xs).map(|v| v.to_vec()) else { return Ok(Reply::Decline) };
        let order = cx.eval(h.call(h.sym("ordering"), &[xs, h.sym_term(h.sym("all")), p]))?;
        let Some(mut ix) = cx.as_list(order).and_then(|o| {
            o.iter().map(|&k| match h.view(k) { View::Int(k) if k >= 1 && k as usize <= items.len() => Some(k as usize - 1), _ => None }).collect::<Option<Vec<usize>>>()
        }) else {
            return Ok(Reply::Decline);
        };
        if largest {
            ix.reverse();
        }
        let (zero, falsity) = (h.int(0), cx.bool(false));
        let mut groups: Vec<Vec<T<'h>>> = vec![];
        let mut prev: Option<usize> = None;
        for i in ix {
            cx.tick(1)?;
            let same = match prev {
                None => false,
                Some(j) => {
                    let a = cx.eval(h.app(p, &[items[j], items[i]]))?;
                    let b = cx.eval(h.app(p, &[items[i], items[j]]))?;
                    a == zero || (a == falsity && b == falsity)
                }
            };
            if !same {
                if groups.len() as i64 == n {
                    break;
                }
                groups.push(vec![]);
            }
            prev = Some(i);
            if let Some(g) = groups.last_mut() {
                g.push(h.int(i as i64 + 1));
            }
        }
        let out: Vec<T<'h>> = groups.iter().map(|g| cx.list(g)).collect();
        return Ok(Reply::Done(cx.list(&out)));
    }
    let (xs, n) = match *args {
        [xs] => (xs, None),
        [xs, n] => match h.view(n) {
            View::Int(n) if n >= 0 => (xs, Some(n as usize)),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    // an association: the positions of its values
    let xs: Vec<T<'h>> = match crate::levels::assoc_entries(h, xs) {
        Some(es) => es.into_iter().map(|(_, v)| v).collect(),
        None => take!(lib.list_operand(h, t, xs, 1)).to_vec(),
    };
    let inf = h.sym_term(h.sym("inf"));
    let mut vals = Vec::with_capacity(xs.len());
    for &x in &xs {
        cx.tick(1)?;
        let v = if x == inf { Some(f64::INFINITY) } else { real(cx, x)? };
        match v {
            Some(v) if !v.is_nan() => vals.push(v),
            _ => return Ok(Reply::Decline),
        }
    }
    let Some(n) = n else {
        // the positions of the extreme elements, as integers
        let Some(best) = vals.iter().copied().reduce(|a, b| if (b > a) == largest && b != a { b } else { a }) else {
            return Ok(Reply::Done(cx.list(&[])));
        };
        let out: Vec<T<'h>> = vals.iter().enumerate().filter(|(_, &v)| v == best).map(|(i, _)| h.int(i as i64 + 1)).collect();
        return Ok(Reply::Done(cx.list(&out)));
    };
    // the positions of the n smallest (largest) distinct values, grouped by value: positions in increasing order
    // within a group for the smallest; the largest is that order reversed
    let mut idx: Vec<usize> = (0..vals.len()).collect();
    idx.sort_by(|&i, &j| vals[i].total_cmp(&vals[j]));
    if largest {
        idx.reverse();
    }
    let mut groups: Vec<Vec<T<'h>>> = vec![];
    let mut last: Option<f64> = None;
    for i in idx {
        if last != Some(vals[i]) {
            if groups.len() == n {
                break;
            }
            groups.push(vec![]);
            last = Some(vals[i]);
        }
        if let Some(g) = groups.last_mut() {
            g.push(h.int(i as i64 + 1));
        }
    }
    let out: Vec<T<'h>> = groups.iter().map(|g| cx.list(g)).collect();
    Ok(Reply::Done(cx.list(&out)))
}

/// `symmetric_difference(xs, ys, ...)`: the sorted distinct elements that occur in an odd number of the lists.
pub(crate) fn symmetric_difference<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // no sets: the empty list
    if args.is_empty() {
        return Ok(Reply::Done(cx.list(&[])));
    }
    // same_test: f: the elements x for which an odd number of the sets hold an element y with f(x, y) true, one of
    // each class of such elements, sorted
    let (sets, test) = crate::ctx::option(h, args, 1, "same_test");
    if let Some(f) = test {
        let mut lists: Vec<&'h [T<'h>]> = Vec::with_capacity(sets.len());
        for (k, &a) in sets.iter().enumerate() {
            let Some(xs) = cx.as_list(a) else { return Ok(Reply::Done(lib.bad_arg(h, t, k + 1))) };
            lists.push(xs);
        }
        let same = |cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, y: T<'h>| -> Res<bool> {
            cx.tick(1)?;
            let v = cx.eval(h.app(f, &[x, y]))?;
            Ok(cx.is_true(v))
        };
        let mut kept: Vec<T<'h>> = vec![];
        for xs in &lists {
            for &x in *xs {
                let mut seen = false;
                for &y in &kept {
                    if same(cx, y, x)? {
                        seen = true;
                        break;
                    }
                }
                if seen {
                    continue;
                }
                let mut holding = 0usize;
                for ys in &lists {
                    let mut any = false;
                    for &y in *ys {
                        if same(cx, x, y)? {
                            any = true;
                            break;
                        }
                    }
                    holding += usize::from(any);
                }
                if holding % 2 == 1 {
                    kept.push(x);
                }
            }
        }
        return Ok(Reply::Again(h.call(h.sym("sort"), &[cx.list(&kept)])));
    }
    let mut count: rustc_hash::FxHashMap<T<'h>, usize> = Default::default();
    // the operands may share any head, which the result keeps (decision r37.4)
    let mut head = None;
    for (k, &a) in args.iter().enumerate() {
        let (g, xs) = take!(lib.elements_operand(h, t, a, k + 1));
        if head.is_some_and(|x| x != g) {
            return Ok(Reply::Done(lib.bad_arg(h, t, k + 1)));
        }
        head = Some(g);
        cx.tick(xs.len() as u64)?;
        let set: rustc_hash::FxHashSet<T<'h>> = xs.iter().copied().collect();
        for x in set {
            *count.entry(x).or_default() += 1;
        }
    }
    let odd: Vec<T<'h>> = count.into_iter().filter(|(_, c)| c % 2 == 1).map(|(x, _)| x).collect();
    let sorted = h.call(h.sym("sort"), &[cx.list(&odd)]);
    Ok(Reply::Again(match head {
        Some(g) if g != h.sym_term(lib.s.list) => h.call(h.sym("apply"), &[g, sorted]),
        _ => sorted,
    }))
}

/// `xor(a, b, ...)`: `true` when an odd number of the arguments are `true` (all `true` or `false`); unevaluated
/// otherwise.
pub(crate) fn xor<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let fals = cx.bool(false);
    let mut odd = false;
    for &a in args {
        if cx.is_true(a) {
            odd = !odd;
        } else if a != fals {
            return Ok(Reply::Decline);
        }
    }
    Ok(Reply::Done(cx.bool(odd)))
}

/// `implies(p, q)`: `true` when `p` is `false` or `q` is `true`, `q` when `p` is `true`.
pub(crate) fn implies<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let [p, q] = *args else { return Ok(Reply::Decline) };
    let fals = cx.bool(false);
    if p == fals || cx.is_true(q) {
        return Ok(Reply::Done(cx.bool(true)));
    }
    if cx.is_true(p) {
        return Ok(Reply::Done(q));
    }
    Ok(Reply::Decline)
}

/// `true_q(x)`: whether `x` is `true` (anything else is `false`).
pub(crate) fn true_q<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let [x] = *args else { return Ok(Reply::Decline) };
    Ok(Reply::Done(cx.bool(cx.is_true(x))))
}

/// `construct(f, a, b, ...)`: `f(a, b, ...)`.
pub(crate) fn construct<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let Some((&f, rest)) = args.split_first() else { return Ok(Reply::Decline) };
    Ok(Reply::Again(h.app(f, rest)))
}

/// `identity(x)`: `x`.
pub(crate) fn identity<'h>(_lib: &Lib, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(match *args {
        [x] => Reply::Done(x),
        _ => Reply::Decline,
    })
}

/// `hold_complete(x, ...)`: kept as it is, its arguments unevaluated (held).
pub(crate) fn hold_complete<'h>(_lib: &Lib, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(Reply::Decline)
}

/// `quote(x)` (held): `x` kept unevaluated, wrapped so that it stays so when passed on (semantics 5);
/// `unquote(q)` releases it. `eval(e)` written as the argument is evaluated first (an argument in a held position).
pub(crate) fn quote<'h>(_lib: &Lib, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(Reply::Decline)
}

/// `unquote(e)`: `e` with every outermost `quote(x)` and `hold_complete(x)` in it (at any depth, heads included)
/// replaced by `x`, then evaluated: `unquote(quote(1 + 2))` is 3, `unquote(f(quote(1 + 1)))` is `f(2)`. A quote of
/// several arguments inside a call gives them all to the call (`f(quote(a, b))` is `f(a, b)`); at the top it stays.
/// Iterative, each shared subterm once (P9B.3).
pub(crate) fn unquote<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [e] = *args else { return Ok(Reply::Decline) };
    let q = h.sym_term(h.sym("quote"));
    let hc = h.sym_term(h.sym("hold_complete"));
    let holder = |u: T<'h>| match h.view(u) {
        View::App { head, args } if head == q || head == hc => Some(args),
        _ => None,
    };
    if let Some(inner) = holder(e) {
        return Ok(match inner {
            [x] => Reply::Again(*x),
            _ => Reply::Decline,
        });
    }
    let mask = h.mask(q) | h.mask(hc);
    let mut done: rustc_hash::FxHashMap<T<'h>, T<'h>> = rustc_hash::FxHashMap::default();
    let mut stack = vec![(e, false)];
    while let Some((u, expanded)) = stack.pop() {
        cx.tick(1)?;
        if done.contains_key(&u) {
            continue;
        }
        if h.mask(u) & mask == 0 || h.packed(u).is_some() {
            done.insert(u, u);
            continue;
        }
        let View::App { head, args } = h.view(u) else {
            done.insert(u, u);
            continue;
        };
        if !expanded {
            stack.push((u, true));
            stack.push((head, false));
            for &a in args.iter().filter(|&&a| holder(a).is_none()) {
                stack.push((a, false));
            }
            continue;
        }
        let nh = match holder(head) {
            Some(&[x]) => x,
            _ => done[&head],
        };
        let mut na = Vec::with_capacity(args.len());
        for &a in args {
            match holder(a) {
                Some(xs) => na.extend_from_slice(xs),
                None => na.push(done[&a]),
            }
        }
        done.insert(u, h.app(nh, &na));
    }
    Ok(Reply::Again(done[&e]))
}

/// `inactivate(e)`, `inactivate(e, p)` (held): `e` with every symbol `f` used as a head (except `list`), or every
/// one matching the pattern `p` (`g`, `g | h`, a condition), replaced by the inert head `inactive(f)`, then
/// evaluated: the inert calls stay, their arguments are evaluated where nothing in them is inert. A head that is
/// itself a call (`f(x)(y)`) is treated the same way, unless `heads: false` (then it is left as it is). `activate`
/// undoes it (P9B.7).
pub(crate) fn inactivate<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, heads) = crate::ctx::option(h, args, 1, "heads");
    let heads = !matches!(heads.map(|v| h.view(v)), Some(View::Sym(s)) if h.sym_name(s) == "false");
    let (e, p) = match *args {
        [e] => (e, None),
        [e, p] => (e, Some(p)),
        _ => return Ok(Reply::Decline),
    };
    let mut m = match p {
        Some(p) => match crate::matching::Matcher::new(lib, h, p) {
            Some(m) => Some(m),
            None => return Ok(Reply::Decline),
        },
        None => None,
    };
    let inactive = h.sym_term(h.sym("inactive"));
    let list = h.sym_term(lib.s.list);
    let mut done: rustc_hash::FxHashMap<T<'h>, T<'h>> = rustc_hash::FxHashMap::default();
    let mut stack = vec![(e, false)];
    while let Some((u, expanded)) = stack.pop() {
        cx.tick(1)?;
        if done.contains_key(&u) {
            continue;
        }
        let View::App { head, args } = (if h.packed(u).is_some() { View::Sym(lib.s.list) } else { h.view(u) }) else {
            done.insert(u, u);
            continue;
        };
        if !expanded {
            stack.push((u, true));
            if heads || matches!(h.view(head), View::Sym(_)) {
                stack.push((head, false));
            }
            for &a in args {
                stack.push((a, false));
            }
            continue;
        }
        let nh = match h.view(head) {
            View::Sym(_) if head == list || head == inactive => head,
            View::Sym(_) => {
                let hit = match m.as_mut() {
                    Some(m) => m.matches(cx, head)?,
                    None => true,
                };
                if hit {
                    h.app(inactive, &[head])
                } else {
                    head
                }
            }
            _ if heads => done[&head],
            _ => head,
        };
        let na: Vec<T<'h>> = args.iter().map(|a| done[a]).collect();
        done.insert(u, h.app(nh, &na));
    }
    Ok(Reply::Again(done[&e]))
}

/// `sequence_alignment(a, b)`: a global alignment of the strings (or lists) `a` and `b` (Needleman and Wunsch 1970:
/// a match scores 1, a mismatch -1, a gap -1 per element), as the list of the runs they share and, between them,
/// the pairs `[piece of a, piece of b]` where they differ (adjacent differences merged). The traceback runs from the
/// ends preferring a match, so among equal-scoring alignments the gaps come as early as possible. At most 10^7 cells
/// (P9B.9).
///
/// Options: `ignore_case: true` (strings compared and given in lower case); `method: "Global"` (the default) or
/// `"Local"` (Smith and Waterman 1981: the best-scoring pair of segments with the same scores, cells floored at 0; the
/// parts before and after it are differences, the first maximal cell in row order taken); `merge_differences: false`
/// (adjacent differences of different kinds, elements of `a` alone, of `b` alone, or of both, kept apart). Other
/// options leave the call unevaluated.
pub(crate) fn sequence_alignment<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, opts) = match args.split_last() {
        Some((&o, rest)) if matches!(h.view(o), View::App { head, .. } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "options")) => (rest, Some(o)),
        _ => (args, None),
    };
    let [a, b] = *args else { return Ok(Reply::Decline) };
    let (mut fold_case, mut local, mut merge) = (false, false, true);
    if let Some(o) = opts {
        let View::App { args: rules, .. } = h.view(o) else { return Ok(Reply::Decline) };
        for &r in rules {
            let View::App { args: &[k, v], .. } = h.view(r) else { return Ok(Reply::Decline) };
            let View::Sym(ks) = h.view(k) else { return Ok(Reply::Decline) };
            let truth = |v: T<'h>| match h.view(v) {
                View::Sym(s) if h.sym_name(s) == "true" => Some(true),
                View::Sym(s) if h.sym_name(s) == "false" => Some(false),
                _ => None,
            };
            match (h.sym_name(ks).as_str(), h.view(v)) {
                ("ignore_case", _) => match truth(v) {
                    Some(x) => fold_case = x,
                    None => return Ok(Reply::Decline),
                },
                ("merge_differences", _) => match truth(v) {
                    Some(x) => merge = x,
                    None => return Ok(Reply::Decline),
                },
                ("method", View::Str(m)) if m == "Global" => local = false,
                ("method", View::Str(m)) if m == "Local" => local = true,
                _ => return Ok(Reply::Decline),
            }
        }
    }
    let strings = matches!((h.view(a), h.view(b)), (View::Str(_), View::Str(_)));
    let elems = |x: T<'h>| -> Option<Vec<T<'h>>> {
        match h.view(x) {
            View::Str(s) if fold_case => Some(s.to_lowercase().chars().map(|c| h.string(&c.to_string())).collect()),
            View::Str(s) => Some(s.chars().map(|c| h.string(&c.to_string())).collect()),
            _ => cx.as_list(x).map(|v| v.to_vec()),
        }
    };
    let (Some(xs), Some(ys)) = (elems(a), elems(b)) else { return Ok(Reply::Done(lib.bad_arg(h, t, 1))) };
    let (n, m) = (xs.len(), ys.len());
    if (n + 1).saturating_mul(m + 1) > 10_000_000 {
        return Ok(Reply::Decline);
    }
    cx.tick(((n + 1) * (m + 1)) as u64)?;
    let w = m + 1;
    let mut sc = vec![0i64; (n + 1) * w];
    if !local {
        for i in 0..=n {
            sc[i * w] = -(i as i64);
        }
        for (j, v) in sc.iter_mut().take(m + 1).enumerate() {
            *v = -(j as i64);
        }
    }
    let floor = if local { 0 } else { i64::MIN };
    for i in 1..=n {
        for j in 1..=m {
            let d = sc[(i - 1) * w + j - 1] + if xs[i - 1] == ys[j - 1] { 1 } else { -1 };
            let u = sc[(i - 1) * w + j] - 1;
            let l = sc[i * w + j - 1] - 1;
            sc[i * w + j] = d.max(u).max(l).max(floor);
        }
    }
    // where the traceback starts and stops: the whole tables, or the best local segments
    let (mut i, mut j) = (n, m);
    if local {
        let mut best = (0i64, 0usize, 0usize);
        for ii in 1..=n {
            for jj in 1..=m {
                if sc[ii * w + jj] > best.0 {
                    best = (sc[ii * w + jj], ii, jj);
                }
            }
        }
        (i, j) = (best.1, best.2);
    }
    let (end_i, end_j) = (i, j);
    // columns from the end: Some((x, y)) a pair (either may be absent: a gap)
    let mut cols: Vec<(Option<T<'h>>, Option<T<'h>>)> = vec![];
    while (i > 0 || j > 0) && !(local && (i == 0 || j == 0 || sc[i * w + j] == 0)) {
        let here = sc[i * w + j];
        if i > 0 && j > 0 && here == sc[(i - 1) * w + j - 1] + if xs[i - 1] == ys[j - 1] { 1 } else { -1 } {
            cols.push((Some(xs[i - 1]), Some(ys[j - 1])));
            i -= 1;
            j -= 1;
        } else if i > 0 && here == sc[(i - 1) * w + j] - 1 {
            cols.push((Some(xs[i - 1]), None));
            i -= 1;
        } else {
            cols.push((None, Some(ys[j - 1])));
            j -= 1;
        }
    }
    cols.reverse();
    let piece = |v: &[T<'h>]| -> T<'h> {
        if strings {
            let s: String = v.iter().map(|&c| match h.view(c) {
                View::Str(s) => s.to_string(),
                _ => String::new(),
            }).collect();
            h.string(&s)
        } else {
            cx.list(v)
        }
    };
    let mut out = vec![];
    // a local alignment: the parts before the segments, as one difference
    if local && (i > 0 || j > 0) {
        out.push(cx.list(&[piece(&xs[..i]), piece(&ys[..j])]));
    }
    // the kind of a column: 0 shared, 1 of a alone, 2 of b alone, 3 of both, differing
    let kind = |c: &(Option<T<'h>>, Option<T<'h>>)| match c {
        (Some(x), Some(y)) if x == y => 0,
        (Some(_), None) => 1,
        (None, Some(_)) => 2,
        _ => 3,
    };
    let mut k = 0;
    while k < cols.len() {
        let start = k;
        let k0 = kind(&cols[k]);
        if k0 == 0 {
            while k < cols.len() && kind(&cols[k]) == 0 {
                k += 1;
            }
            let run: Vec<T<'h>> = cols[start..k].iter().filter_map(|c| c.0).collect();
            out.push(piece(&run));
        } else {
            while k < cols.len() && kind(&cols[k]) != 0 && (merge || kind(&cols[k]) == k0) {
                k += 1;
            }
            let pa: Vec<T<'h>> = cols[start..k].iter().filter_map(|c| c.0).collect();
            let pb: Vec<T<'h>> = cols[start..k].iter().filter_map(|c| c.1).collect();
            out.push(cx.list(&[piece(&pa), piece(&pb)]));
        }
    }
    // and the parts after them
    if local && (end_i < n || end_j < m) {
        out.push(cx.list(&[piece(&xs[end_i..]), piece(&ys[end_j..])]));
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// `check(x, y)` (held): the value of `x`, or the value of `y` when `x` gives a failure.
pub(crate) fn check<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (x, y) = match *args {
        [x, y] => (x, y),
        [x, y, _] => (x, y),
        _ => return Ok(Reply::Decline),
    };
    let v = cx.eval(x)?;
    if lib.is_failure(h, v) {
        return Ok(Reply::Again(y));
    }
    Ok(Reply::Done(v))
}

/// `value_q(x)` (held): whether `x` has a value (evaluates to something other than itself). `method:`
/// `"TrialEvaluation"` is that; `"OwnValuesPresent"` whether the symbol `x` has an own value;
/// `"SymbolDefinitionsPresent"` whether the symbol `x`, or the head of the call `x`, has any definition (for a
/// list, any element); `automatic` (the default) the own value for a symbol, the trial evaluation otherwise.
pub(crate) fn value_q<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, method) = crate::ctx::option(h, args, 1, "method");
    let [x] = *args else { return Ok(Reply::Decline) };
    let method = match method.map(|m| h.view(m)) {
        None => "Automatic".to_string(),
        Some(View::Sym(s)) if h.sym_name(s) == "automatic" => "Automatic".to_string(),
        Some(View::Str(m)) => m.to_string(),
        Some(_) => return Ok(Reply::Decline),
    };
    let own = |cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>| match h.view(x) {
        View::Sym(s) => cx.value(s).is_some(),
        _ => false,
    };
    fn defined<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>) -> bool {
        let h = cx.heap();
        if let Some(xs) = cx.as_list(x) {
            return xs.iter().any(|&e| defined(cx, e));
        }
        let mut u = x;
        loop {
            match h.view(u) {
                View::Sym(s) => return cx.is_defined(s),
                View::App { head, .. } => u = head,
                _ => return false,
            }
        }
    }
    let r = match method.as_str() {
        "OwnValuesPresent" => own(cx, x),
        "SymbolDefinitionsPresent" => defined(cx, x),
        "TrialEvaluation" => cx.eval(x)? != x,
        "Automatic" if matches!(h.view(x), View::Sym(_)) => own(cx, x) || cx.eval(x)? != x,
        "Automatic" => cx.eval(x)? != x,
        _ => return Ok(Reply::Decline),
    };
    Ok(Reply::Done(cx.bool(r)))
}

/// `to_character_code(s)`: the code points of the characters of `s` (a list of strings: one list each).
pub(crate) fn to_character_code<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // to_character_code(s, encoding): "UTF8" the bytes of the UTF-8 encoding, "Unicode" the code points, "ASCII"
    // and "ISO8859-1" the code points that fit (none for the others) (P9B.10)
    #[derive(Clone, Copy, PartialEq)]
    enum Enc {
        Unicode,
        Utf8,
        Max(u32),
    }
    let (s, enc) = match *args {
        [s] => (s, Enc::Unicode),
        [s, e] => match h.view(e) {
            View::Str(e) if e == "Unicode" => (s, Enc::Unicode),
            View::Str(e) if e == "UTF8" || e == "UTF-8" => (s, Enc::Utf8),
            View::Str(e) if e == "ASCII" => (s, Enc::Max(127)),
            View::Str(e) if e == "ISO8859-1" || e == "ISOLatin1" => (s, Enc::Max(255)),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    let none = h.sym_term(h.sym("none"));
    let codes = |s: &str| -> Vec<T<'h>> {
        match enc {
            Enc::Unicode => s.chars().map(|c| h.int(c as i64)).collect(),
            Enc::Utf8 => s.bytes().map(|b| h.int(b as i64)).collect(),
            Enc::Max(m) => s.chars().map(|c| if (c as u32) <= m { h.int(c as i64) } else { none }).collect(),
        }
    };
    if let View::Str(s) = h.view(s) {
        cx.tick(s.len() as u64)?;
        return Ok(Reply::Done(cx.list(&codes(&s))));
    }
    let Some(xs) = cx.as_list(s) else { return Ok(Reply::Done(lib.bad_arg(h, t, 1))) };
    let mut out = Vec::with_capacity(xs.len());
    for &x in xs {
        let View::Str(s) = h.view(x) else { return Ok(Reply::Done(lib.bad_arg(h, t, 1))) };
        cx.tick(s.len() as u64)?;
        out.push(cx.list(&codes(&s)));
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// `character_range(a, b)`: the characters from `a` to `b` (one-character strings or code points), as strings.
pub(crate) fn character_range<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [a, b] = *args else { return Ok(Reply::Decline) };
    let code = |x: T<'h>| -> Option<u32> {
        match h.view(x) {
            View::Int(v) => u32::try_from(v).ok(),
            View::Str(s) => {
                let mut cs = s.chars();
                let c = cs.next()?;
                cs.next().is_none().then_some(c as u32)
            }
            _ => None,
        }
    };
    let (Some(lo), Some(hi)) = (code(a), code(b)) else { return Ok(Reply::Done(lib.bad_arg(h, t, 1))) };
    if hi >= lo && hi - lo > 1_000_000 {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let mut out = vec![];
    for c in lo..=hi {
        cx.tick(1)?;
        if let Some(ch) = char::from_u32(c) {
            out.push(h.string(&ch.to_string()));
        }
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// `up_to(n)`: kept as it is; `take`, `drop`, `take_drop`, `partition` and the `take_*` functions read it as "at
/// most `n`".
pub(crate) fn up_to<'h>(_lib: &Lib, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(Reply::Decline)
}

/// `n` of `up_to(n)` capped at `len`, when `x` is such a call.
pub(crate) fn up_to_count(h: &Heap, x: T<'_>, len: usize) -> Option<i64> {
    let len = len.min(i64::MAX as usize);
    match h.view(x) {
        View::App { head, args: &[n] } if head == h.sym_term(h.sym("up_to")) => match h.view(n) {
            View::Int(k) if k >= 0 => Some(k.min(len as i64)),
            View::Sym(s) if h.sym_name(s) == "inf" => Some(len as i64),
            _ => None,
        },
        _ => None,
    }
}
