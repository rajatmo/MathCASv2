//! Combinatorial constructions: `subsets`, `tuples`, `permutations`, `outer`, `inner`.
//!
//! Every function computes the size of its result first and refuses one over [`MAX_CELLS`] argument slots with
//! `failure(limit, memory, call)` before allocating (decision 1A.4); building then charges fuel per slot.
use crate::ctx::{cells, take, Lib, Meter, MAX_CELLS};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};
use rustc_hash::FxHashMap;

/// `n` choose `k`, saturating.
fn binomial(n: usize, k: usize) -> u128 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut c: u128 = 1;
    for i in 0..k {
        // exact at every step: c * (n - i) is divisible by i + 1
        c = match c.checked_mul((n - i) as u128) {
            Some(v) => v / (i as u128 + 1),
            None => return u128::MAX,
        };
    }
    c
}

/// The size argument (argument 2 of `subsets` and `permutations`): `n` for the sizes 0 to `n`, `[n]` for exactly
/// `n`, `[a, b]` for `a` to `b`. The range is clamped to `0..=len`; an empty range gives no sizes.
fn sizes<'h>(lib: &Lib, h: &'h Heap, t: T<'h>, a: Option<T<'h>>, len: usize) -> Result<(usize, usize), Reply<'h>> {
    let Some(a) = a else { return Ok((0, len)) };
    if matches!(h.view(a), View::Sym(s) if h.sym_name(s) == "all") {
        return Ok((0, len));
    }
    let int = |x: T<'h>| match h.view(x) {
        View::Int(v) if v >= 0 => Ok(v as u64),
        View::Sym(_) | View::App { .. } => Err(Reply::Decline),
        _ => Err(Reply::Done(lib.bad_arg(h, t, 2))),
    };
    let (lo, hi) = match h.view(a) {
        View::App { head, args } if head == h.sym_term(lib.s.list) => match *args {
            [n] => (int(n)?, int(n)?),
            [lo, hi] => (int(lo)?, int(hi)?),
            _ => return Err(Reply::Done(lib.bad_arg(h, t, 2))),
        },
        _ => (0, int(a)?),
    };
    let hi = hi.min(len as u64) as usize;
    Ok((lo.min(hi as u64 + 1) as usize, hi))
}

/// `subsets(xs)`: every sublist of `xs`, by size and then lexicographically by position; `subsets(xs, n)`: those
/// with at most `n` elements; `subsets(xs, [n])`: exactly `n`; `subsets(xs, [a, b])`: `a` to `b`. Elements are taken
/// by position (repeated elements give repeated subsets).
pub(crate) fn subsets<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `subsets(xs, spec, m)`: the first m subsets, `[k]` the k-th, `[a, b]` the a-th to the b-th (decision io-core.14)
    let (xs, spec, pick) = match *args {
        [xs] => (xs, None, None),
        [xs, s] => (xs, Some(s), None),
        [xs, s, m] => {
            let int = |x: T<'h>| match h.view(x) {
                View::Int(v @ 1..) => Some(v as usize),
                _ => None,
            };
            let pick = match (h.view(m), cx.as_list(m)) {
                (View::Int(v @ 0..), _) => (1, v as usize),
                (_, Some(&[k])) => match int(k) {
                    Some(k) => (k, k),
                    None => return Ok(Reply::Decline),
                },
                (_, Some(&[a, b])) => match (int(a), int(b)) {
                    (Some(a), Some(b)) => (a, b),
                    _ => return Ok(Reply::Decline),
                },
                _ => return Ok(Reply::Decline),
            };
            (xs, Some(s), Some(pick))
        }
        _ => return Ok(Reply::Decline),
    };
    // the subsets of a product, a sum, ... are products, sums, ... (decision io-core.14)
    let (head, xs) = take!(lib.operand(h, t, xs, 1));
    let n = xs.len();
    let (lo, hi) = take!(sizes(lib, h, t, spec, n));
    let limit = pick.map_or(usize::MAX, |(_, b)| b);
    let total = (lo..=hi).fold(0u128, |s, k| s.saturating_add(binomial(n, k).saturating_mul(k as u128 + 1)));
    if total.min(limit as u128 * (hi as u128 + 1)) > MAX_CELLS {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let list = h.sym_term(lib.s.list);
    let mut out = vec![];
    let mut m = Meter::default();
    let mut part = vec![];
    for k in lo..=hi {
        // the positions of the current subset, advanced lexicographically
        let mut ix: Vec<usize> = (0..k).collect();
        loop {
            for _ in 0..=k {
                m.step(cx)?;
            }
            if out.len() >= limit {
                break;
            }
            part.clear();
            part.extend(ix.iter().map(|&i| xs[i]));
            out.push(h.app(head, &part));
            let Some(i) = (0..k).rev().find(|&i| ix[i] < n - k + i) else { break };
            ix[i] += 1;
            for j in i + 1..k {
                ix[j] = ix[j - 1] + 1;
            }
        }
    }
    if let Some((a, _)) = pick {
        out.drain(..(a - 1).min(out.len()));
    }
    Ok(if head == list { Reply::Done(cx.list(&out)) } else { Reply::Again(cx.list(&out)) })
}

/// Every choice of one element from each of `lists`, lexicographically by position, as lists of the elements.
/// The caller checked the size.
fn product<'h>(cx: &mut NativeCx<'_, '_, 'h>, lists: &[&'h [T<'h>]], each: &mut dyn FnMut(&[T<'h>])) -> Res<()> {
    if lists.iter().any(|l| l.is_empty()) {
        return Ok(());
    }
    let mut ix = vec![0usize; lists.len()];
    let mut pick: Vec<T<'h>> = lists.iter().map(|l| l[0]).collect();
    let mut m = Meter::default();
    loop {
        for _ in 0..=lists.len() {
            m.step(cx)?;
        }
        each(&pick);
        // the odometer: the last position runs fastest
        let mut d = lists.len();
        loop {
            if d == 0 {
                return Ok(());
            }
            d -= 1;
            ix[d] += 1;
            if ix[d] < lists[d].len() {
                pick[d] = lists[d][ix[d]];
                break;
            }
            ix[d] = 0;
            pick[d] = lists[d][0];
        }
    }
}

/// The number of choices of [`product`], saturating.
fn product_len(lists: &[&[T<'_>]]) -> u128 {
    lists.iter().fold(1u128, |p, l| p.saturating_mul(l.len() as u128))
}

/// `tuples(xs, n)`: every list of `n` elements of `xs`, lexicographically by position; `tuples([xs1, ..., xsk])`:
/// every list with its `i`-th element from `xsi`.
pub(crate) fn tuples<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let lists: Vec<&'h [T<'h>]> = match *args {
        [xs, n] => {
            let xs = take!(lib.list_operand(h, t, xs, 1));
            let n = take!(crate::ctx::count_operand(lib, h, t, n, 2));
            // the size before the n copies of the list are made
            let count = match xs.len() {
                0 | 1 => Some(1),
                l => (l as u128).checked_pow(n.min(128) as u32),
            };
            if !xs.is_empty() && count.is_none_or(|c| c.saturating_mul(n as u128 + 1) > MAX_CELLS) {
                return Ok(Reply::Done(lib.too_large(h, t)));
            }
            if xs.is_empty() && n > 0 {
                return Ok(Reply::Done(cx.list(&[])));
            }
            vec![xs; n]
        }
        [xss] => {
            let xss = take!(lib.list_operand(h, t, xss, 1));
            let mut v = Vec::with_capacity(xss.len());
            for &xs in xss {
                let Some(xs) = cx.as_list(xs) else { return Ok(Reply::Done(lib.bad_arg(h, t, 1))) };
                v.push(xs);
            }
            v
        }
        _ => return Ok(Reply::Decline),
    };
    let count = product_len(&lists);
    if count.saturating_mul(lists.len() as u128 + 1) > MAX_CELLS {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let mut out = Vec::with_capacity(count as usize);
    product(cx, &lists, &mut |p| out.push(cx_list(h, lib, p)))?;
    Ok(Reply::Done(cx.list(&out)))
}

/// `list(items...)` without a `NativeCx` borrow (inside callbacks that hold it).
fn cx_list<'h>(h: &'h Heap, lib: &Lib, items: &[T<'h>]) -> T<'h> {
    h.call(lib.s.list, items)
}

/// The number of distinct arrangements of `k` elements drawn from a multiset with these multiplicities, for every
/// `k` (an estimate in floating point, enough to refuse a result far over the limit): `k!` times the coefficient
/// of `x^k` in the product of the truncated exponentials `sum_j x^j / j!`.
fn arrangements(mult: &[usize], n: usize) -> Vec<f64> {
    let mut c = vec![0.0f64; n + 1];
    c[0] = 1.0;
    let mut deg = 0;
    for &m in mult {
        let mut next = vec![0.0f64; n + 1];
        for (i, &ci) in c.iter().enumerate().take(deg + 1) {
            let mut term = 1.0;
            for j in 0..=m.min(n - i) {
                if j > 0 {
                    term /= j as f64;
                }
                next[i + j] += ci * term;
            }
        }
        deg += m;
        c = next;
    }
    let mut fact = 1.0;
    for (k, ck) in c.iter_mut().enumerate() {
        if k > 0 {
            fact *= k as f64;
        }
        *ck *= fact;
    }
    c
}

/// `permutations(xs)`: every distinct ordering of the elements; `permutations(xs, n)`, `(xs, [n])`, `(xs, [a, b])`:
/// the distinct arrangements of at most `n`, exactly `n`, or `a` to `b` of them. Identical elements are not told
/// apart; arrangements come lexicographically by position, each where its first choice of positions occurs
/// (decision 1A.12): `permutations([a, b, a])` is `[[a, b, a], [a, a, b], [b, a, a]]`.
pub(crate) fn permutations<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, spec) = match *args {
        [xs] => (xs, None),
        [xs, s] => (xs, Some(s)),
        _ => return Ok(Reply::Decline),
    };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    let n = xs.len();
    let (lo, hi) = match spec {
        None => (n, n),
        s => take!(sizes(lib, h, t, s, n)),
    };
    // the class of each position (identical elements share one) and each class's positions in order
    let mut index: FxHashMap<T<'h>, usize> = FxHashMap::default();
    let mut pos: Vec<Vec<usize>> = vec![];
    let mut class = Vec::with_capacity(n);
    for (i, &x) in xs.iter().enumerate() {
        let c = *index.entry(x).or_insert_with(|| {
            pos.push(vec![]);
            pos.len() - 1
        });
        pos[c].push(i);
        class.push(c);
    }
    let mult: Vec<usize> = pos.iter().map(Vec::len).collect();
    let est = arrangements(&mult, n);
    let total: f64 = (lo..=hi).map(|k| est[k] * (k as f64 + 1.0)).sum();
    if total > MAX_CELLS as f64 * 1.01 {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let mut out = vec![];
    let mut used = vec![0usize; pos.len()];
    let mut path: Vec<usize> = Vec::with_capacity(n);
    let mut part = Vec::with_capacity(n);
    let mut slots: u128 = 0;
    let mut m = Meter::default();
    for k in lo..=hi {
        let mut from = 0;
        loop {
            if path.len() == k {
                slots += k as u128 + 1;
                if slots > MAX_CELLS {
                    return Ok(Reply::Done(lib.too_large(h, t)));
                }
                part.clear();
                part.extend(path.iter().map(|&i| xs[i]));
                out.push(cx.list(&part));
            } else {
                // the next position not yet used that is its class's first unused one
                let mut found = None;
                for (i, &c) in class.iter().enumerate().skip(from) {
                    m.step(cx)?;
                    if pos[c].get(used[c]) == Some(&i) {
                        found = Some(i);
                        break;
                    }
                }
                if let Some(i) = found {
                    used[class[i]] += 1;
                    path.push(i);
                    from = 0;
                    continue;
                }
            }
            match path.pop() {
                Some(i) => {
                    used[class[i]] -= 1;
                    from = i + 1;
                }
                None => break,
            }
        }
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// `outer(f, xs1, ..., xsk)`: `f(x1, ..., xk)` for every choice of a leaf `xi` of `xsi`, nested like the
/// arguments (the structure of `xs1` outermost); `outer(f, xs1, ..., xsk, n)`: the parts at level `n` are the
/// leaves. The arguments may have any one head, which the result keeps (decision cov-core.12, replacing 1A.13's
/// first level only). Evaluated.
pub(crate) fn outer<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, ref rest @ ..] = *args else { return Ok(Reply::Decline) };
    if rest.is_empty() {
        return Ok(Reply::Again(h.app(f, &[])));
    }
    // a final integer: the depth of the leaves
    let (xss, depth) = match h.view(rest[rest.len() - 1]) {
        View::Int(n) if rest.len() >= 2 => {
            if n < 1 {
                return Ok(Reply::Done(lib.bad_arg(h, t, rest.len() + 1)));
            }
            (&rest[..rest.len() - 1], n as usize)
        }
        _ => (rest, usize::MAX),
    };
    let head = match h.view(xss[0]) {
        View::App { head, .. } if !crate::measure::is_complex_number(h, xss[0]) => head,
        View::Sym(_) => return Ok(Reply::Decline),
        _ => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
    };
    for (k, &xs) in xss.iter().enumerate() {
        match h.view(xs) {
            View::App { head: g, .. } if g == head => {}
            View::Sym(_) => return Ok(Reply::Decline),
            _ => return Ok(Reply::Done(lib.bad_arg(h, t, k + 2))),
        }
    }
    // the number of leaves of each argument bounds the size
    let mut total: u128 = 1;
    for &xs in xss {
        total = total.saturating_mul(leaves(h, head, xs, depth) as u128);
    }
    if total.saturating_mul(xss.len() as u128 + 1) > MAX_CELLS {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let mut chosen = Vec::with_capacity(xss.len());
    let v = outer_at(cx, f, head, xss, depth, 0, &mut chosen)?;
    Ok(Reply::Again(v))
}

/// The leaves of `x` for `outer` (parts with head `head` are opened down to `depth` levels).
fn leaves<'h>(h: &'h Heap, head: T<'h>, x: T<'h>, depth: usize) -> usize {
    match h.view(x) {
        View::App { head: g, args } if g == head && depth > 0 => args.iter().map(|&a| leaves(h, head, a, depth - 1)).sum(),
        _ => 1,
    }
}

/// The outer product from argument `k` on, with the leaves chosen so far.
fn outer_at<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    head: T<'h>,
    xss: &[T<'h>],
    depth: usize,
    k: usize,
    chosen: &mut Vec<T<'h>>,
) -> Res<T<'h>> {
    if k == xss.len() {
        cx.tick(1)?;
        return Ok(cx.heap().app(f, chosen));
    }
    open(cx, f, head, xss, depth, k, xss[k], depth, chosen)
}

#[allow(clippy::too_many_arguments)]
fn open<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    head: T<'h>,
    xss: &[T<'h>],
    depth: usize,
    k: usize,
    x: T<'h>,
    left: usize,
    chosen: &mut Vec<T<'h>>,
) -> Res<T<'h>> {
    let h = cx.heap();
    match h.view(x) {
        View::App { head: g, args } if g == head && left > 0 => {
            let mut out = Vec::with_capacity(args.len());
            for &a in args {
                out.push(open(cx, f, head, xss, depth, k, a, left - 1, chosen)?);
            }
            Ok(h.app(head, &out))
        }
        _ => {
            chosen.push(x);
            let v = outer_at(cx, f, head, xss, depth, k + 1, chosen)?;
            chosen.pop();
            Ok(v)
        }
    }
}

/// A vector (its elements) or a matrix (its rows, of one length) for `inner`.
enum Tensor<'h> {
    Vector(&'h [T<'h>]),
    Matrix(Vec<&'h [T<'h>]>, usize),
}

impl<'h> Tensor<'h> {
    /// A list whose elements are all lists of one length is a matrix; any other list a vector (elements that are
    /// lists among others are entries). Rows of different lengths are `None`.
    fn of(cx: &NativeCx<'_, '_, 'h>, xs: &'h [T<'h>]) -> Option<Tensor<'h>> {
        let rows: Option<Vec<&'h [T<'h>]>> = xs.iter().map(|&x| cx.as_list(x)).collect();
        match rows {
            Some(rows) if !rows.is_empty() => {
                let w = rows[0].len();
                rows.iter().all(|r| r.len() == w).then_some(Tensor::Matrix(rows, w))
            }
            _ => Some(Tensor::Vector(xs)),
        }
    }
}

/// `inner(f, a, b)`, `inner(f, a, b, g)`: the generalised product of a vector or matrix `a` with a vector or matrix
/// `b`, `g` (default `add`) of the `f`s of matching entries along the last index of `a` and the first of `b`:
/// `inner(f, [a1, a2], [b1, b2], g)` is `g(f(a1, b1), f(a2, b2))`, and `inner(mul, m1, m2)` the matrix product.
/// Lengths that do not match are `failure(shape_mismatch, inner)`. The result is evaluated.
pub(crate) fn inner<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (f, a, b, g) = match *args {
        [f, a, b] => (f, a, b, h.sym_term(lib.s.add)),
        [f, a, b, g] => (f, a, b, g),
        _ => return Ok(Reply::Decline),
    };
    let a = take!(lib.list_operand(h, t, a, 2));
    let b = take!(lib.list_operand(h, t, b, 3));
    let (Some(ta), Some(tb)) = (Tensor::of(cx, a), Tensor::of(cx, b)) else {
        return Ok(Reply::Done(lib.shape(h, t)));
    };
    // rows of a (one row for a vector), the contracted length, columns of b (one for a vector)
    let (rows, n): (Vec<&'h [T<'h>]>, usize) = match &ta {
        Tensor::Vector(v) => (vec![*v], v.len()),
        Tensor::Matrix(r, w) => (r.clone(), *w),
    };
    let (bn, cols) = match &tb {
        Tensor::Vector(v) => (v.len(), 1),
        Tensor::Matrix(r, w) => (r.len(), *w),
    };
    if n != bn {
        return Ok(Reply::Done(lib.shape(h, t)));
    }
    if cells(&[rows.len(), cols, n]) > MAX_CELLS {
        return Ok(Reply::Done(lib.too_large(h, t)));
    }
    let entry = |k: usize, j: usize| match &tb {
        Tensor::Vector(v) => v[k],
        Tensor::Matrix(r, _) => r[k][j],
    };
    let mut m = Meter::default();
    let mut out_rows = Vec::with_capacity(rows.len());
    let mut terms = Vec::with_capacity(n);
    for row in &rows {
        let mut out = Vec::with_capacity(cols);
        for j in 0..cols {
            terms.clear();
            for (k, &x) in row.iter().enumerate() {
                m.step(cx)?;
                terms.push(h.app(f, &[x, entry(k, j)]));
            }
            out.push(h.app(g, &terms));
        }
        out_rows.push(match tb {
            Tensor::Vector(_) => out[0],
            Tensor::Matrix(..) => cx.list(&out),
        });
    }
    Ok(Reply::Again(match ta {
        Tensor::Vector(_) => out_rows[0],
        Tensor::Matrix(..) => cx.list(&out_rows),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binomials_are_exact_and_saturate() {
        assert_eq!(binomial(5, 2), 10);
        assert_eq!(binomial(5, 0), 1);
        assert_eq!(binomial(5, 6), 0);
        assert_eq!(binomial(60, 30), 118264581564861424);
        assert_eq!(binomial(1000, 500), u128::MAX);
    }

    #[test]
    fn arrangement_counts() {
        // [a, b, a]: lengths 0..3 give 1, 2, 3, 3
        let c = arrangements(&[2, 1], 3);
        let r: Vec<i64> = c.iter().map(|x| x.round() as i64).collect();
        assert_eq!(r, vec![1, 2, 3, 3]);
        // distinct elements: n! / (n - k)!
        let c = arrangements(&[1; 6], 6);
        assert_eq!(c[6].round() as i64, 720);
        assert_eq!(c[2].round() as i64, 30);
    }
}
