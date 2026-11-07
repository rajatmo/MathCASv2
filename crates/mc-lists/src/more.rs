#![allow(clippy::needless_borrow, clippy::redundant_guards)]
//! Further list and functional operations (round 14, package cov-core): `map_apply`, `comap`, `comap_apply`,
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

/// `map_apply(f, x)`: each element `g(a, b, ...)` of `x` becomes `f(a, b, ...)` (atoms stay); an association's
/// values likewise.
pub(crate) fn map_apply<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, x] = *args else { return Ok(Reply::Decline) };
    let r = rebuild_each(lib, cx, x, |cx, a| match cx.as_list(a) {
        Some(ys) => h.app(f, ys),
        None => match compound(h, a) {
            Some((_, ys)) => h.app(f, ys),
            None => a,
        },
    })?;
    Ok(match r {
        Some(v) => Reply::Again(v),
        None => Reply::Done(x),
    })
}

/// `comap(fs, x)`: each element `g` of `fs` becomes `g(x)`, keeping the head of `fs` (`comap([f, g], x)` is
/// `[f(x), g(x)]`, `comap(f + g, x)` is `f(x) + g(x)`); an association of functions gives an association.
pub(crate) fn comap<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [fs, x] = *args else { return Ok(Reply::Decline) };
    Ok(match rebuild_each(lib, cx, fs, |_, g| h.app(g, &[x]))? {
        Some(v) => Reply::Again(v),
        None => Reply::Decline,
    })
}

/// `comap_apply(fs, xs)`: each element `g` of `fs` becomes `g(a, b, ...)` with the arguments of `xs`.
pub(crate) fn comap_apply<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [fs, xs] = *args else { return Ok(Reply::Decline) };
    let Some(ys) = elements(lib, cx, xs) else { return Ok(Reply::Decline) };
    Ok(match rebuild_each(lib, cx, fs, |_, g| h.app(g, &ys))? {
        Some(v) => Reply::Again(v),
        None => Reply::Decline,
    })
}

/// `map_all(f, x)`: `f` applied to every part of `x`, innermost first (atoms included, heads not).
pub(crate) fn map_all<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let [f, x] = *args else { return Ok(Reply::Decline) };
    let mut m = Meter::default();
    let v = map_all_in(cx, &mut m, f, x, 0)?;
    Ok(Reply::Again(v))
}

fn map_all_in<'h>(cx: &mut NativeCx<'_, '_, 'h>, m: &mut Meter, f: T<'h>, x: T<'h>, depth: usize) -> Res<T<'h>> {
    let h = cx.heap();
    m.step(cx)?;
    let inner = match compound(h, x) {
        Some((head, args)) if depth < 10_000 => {
            let mut out = Vec::with_capacity(args.len());
            for &a in args {
                out.push(map_all_in(cx, m, f, a, depth + 1)?);
            }
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
    let _ = lib;
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
    let (x, spec) = match *args {
        [x] => (x, None),
        [x, s] => (x, Some(s)),
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
    let [a, b] = *args else { return Ok(Reply::Decline) };
    let (Some(xs), Some(ys)) = (elements(lib, cx, a), elements(lib, cx, b)) else { return Ok(Reply::Decline) };
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
        View::Str(_) | View::Sym(_) => None,
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

/// `nearest(xs, x)`: the elements of `xs` nearest to `x` (every one at the least distance); `nearest(xs, x, n)`:
/// the `n` nearest, nearest first (equal distances in order). Numbers, numeric vectors (Euclidean) and strings
/// (edit distance). `xs` may be a list of rules `v -> label` or an association `label -> v`: the labels are given.
pub(crate) fn nearest<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (data, x, n) = match *args {
        [d, x] => (d, x, None),
        [d, x, n] => match h.view(n) {
            View::Int(k) if k >= 0 => (d, x, Some(k as usize)),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    let rule = h.sym_term(lib.s.rule);
    let (values, labels): (Vec<T<'h>>, Vec<T<'h>>) = if let Some(es) = entries(lib, h, data) {
        es.iter().map(|&(k, v)| (v, k)).unzip()
    } else {
        let xs = take!(lib.list_operand(h, t, data, 1));
        xs.iter()
            .map(|&e| match h.view(e) {
                View::App { head, args: &[v, l] } if head == rule => (v, l),
                _ => (e, e),
            })
            .unzip()
    };
    let mut ds = Vec::with_capacity(values.len());
    for &v in &values {
        cx.tick(1)?;
        match distance(cx, v, x)? {
            Some(d) if !d.is_nan() => ds.push(d),
            _ => return Ok(Reply::Decline),
        }
    }
    let mut ix: Vec<usize> = (0..values.len()).collect();
    ix.sort_by(|&a, &b| ds[a].total_cmp(&ds[b]));
    let chosen: Vec<usize> = match n {
        Some(k) => ix.into_iter().take(k).collect(),
        None => {
            let best = ix.first().map(|&i| ds[i]);
            ix.into_iter().take_while(|&i| Some(ds[i]) == best).collect()
        }
    };
    let out: Vec<T<'h>> = chosen.iter().map(|&i| labels[i]).collect();
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

/// `power_range(a, b)`: `a, 10 a, 100 a, ...` while not past `b`; `power_range(a, b, r)`: ratio `r`.
pub(crate) fn power_range<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (a, b, r) = match *args {
        [a, b] => (a, b, h.int(10)),
        [a, b, r] => (a, b, r),
        _ => return Ok(Reply::Decline),
    };
    let (Some(fa), Some(fb), Some(fr)) = (real(cx, a)?, real(cx, b)?, real(cx, r)?) else {
        return Ok(Reply::Decline);
    };
    if fr <= 0.0 || fr == 1.0 || fa == 0.0 {
        return Ok(Reply::Done(lib.bad_arg(h, t, if fa == 0.0 { 1 } else { 3 })));
    }
    let growing = (fr > 1.0) == (fa > 0.0);
    let past = |v: f64| if growing { v > fb * (1.0 + 1e-12) } else { v < fb * (1.0 - 1e-12) };
    let mut out = vec![];
    let mut cur = a;
    let mut fc = fa;
    let mul = h.sym_term(h.sym("mul"));
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
    let (x, p, d) = match *args {
        [x, p] => (x, p, None),
        [x, p, d] => (x, p, Some(d)),
        _ => return Ok(Reply::Decline),
    };
    let ps = cx.eval(h.call(h.sym("position"), &[x, p]))?;
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
    let [xs] = *args else { return Ok(Reply::Decline) };
    let xs = take!(lib.list_operand(h, t, xs, 1)).to_vec();
    let mut vals = Vec::with_capacity(xs.len());
    for &x in &xs {
        cx.tick(1)?;
        match real(cx, x)? {
            Some(v) if !v.is_nan() => vals.push(v),
            _ => return Ok(Reply::Decline),
        }
    }
    let Some(best) = vals.iter().copied().reduce(|a, b| if (b > a) == largest && b != a { b } else { a }) else {
        return Ok(Reply::Done(cx.list(&[])));
    };
    let out: Vec<T<'h>> =
        vals.iter().enumerate().filter(|(_, &v)| v == best).map(|(i, _)| cx.list(&[h.int(i as i64 + 1)])).collect();
    Ok(Reply::Done(cx.list(&out)))
}

/// `symmetric_difference(xs, ys, ...)`: the sorted distinct elements that occur in an odd number of the lists.
pub(crate) fn symmetric_difference<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if args.is_empty() {
        return Ok(Reply::Decline);
    }
    let mut count: rustc_hash::FxHashMap<T<'h>, usize> = Default::default();
    for (k, &a) in args.iter().enumerate() {
        let xs = take!(lib.list_operand(h, t, a, k + 1));
        cx.tick(xs.len() as u64)?;
        let set: rustc_hash::FxHashSet<T<'h>> = xs.iter().copied().collect();
        for x in set {
            *count.entry(x).or_default() += 1;
        }
    }
    let odd: Vec<T<'h>> = count.into_iter().filter(|(_, c)| c % 2 == 1).map(|(x, _)| x).collect();
    Ok(Reply::Again(h.call(h.sym("sort"), &[cx.list(&odd)])))
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

/// `value_q(x)` (held): whether `x` has a value (evaluates to something other than itself).
pub(crate) fn value_q<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let [x] = *args else { return Ok(Reply::Decline) };
    let v = cx.eval(x)?;
    Ok(Reply::Done(cx.bool(v != x)))
}

/// `to_character_code(s)`: the code points of the characters of `s` (a list of strings: one list each).
pub(crate) fn to_character_code<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [s] = *args else { return Ok(Reply::Decline) };
    let codes = |s: &str| -> Vec<T<'h>> { s.chars().map(|c| h.int(c as i64)).collect() };
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
