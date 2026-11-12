//! `find_shortest_tour`: the shortest closed tour through a list of points (or the shortest path between two of
//! them through all the others), found exactly by dynamic programming over subsets (Bellman 1962; Held and Karp
//! 1962): the shortest path from the start through a set S of points ending at j is the minimum over i in S of the
//! one through S \ {j} ending at i, plus d(i, j). Time O(2^n n^2), memory O(2^n n): at most [`MAX_POINTS`] points;
//! a larger list stays unevaluated (no heuristic tour is given as the shortest one).
//!
//! Distances: Euclidean between points (lists of numbers of one length), |x - y| between numbers, the edit
//! distance (Levenshtein 1966) between strings, or `distance_function: f`. The minimum is decided on the distances
//! as machine numbers; the length returned is the exact sum of the distances along the tour found (exact for exact
//! points). Tours whose lengths agree to the rounding of machine numbers are treated as equally short.
use crate::ctx::{Lib, Meter};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The most points of a tour: 2^19 * 19 subproblems of 9 bytes at 20 points.
pub(crate) const MAX_POINTS: usize = 20;

/// The edit distance between two strings, by characters.
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

fn is_number(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. })
}

/// The machine value of a real term, or `None`.
fn machine<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    if let View::Real(x) = h.view(t) {
        return Ok(Some(x));
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(match h.view(v) {
        View::Real(x) => Some(x),
        View::Int(k) => Some(k as f64),
        _ => None,
    })
}

/// The distance between items `a` and `b` as a term (evaluated), or `None` when the items are of no kind the
/// function knows.
fn distance<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, f: Option<T<'h>>, a: T<'h>, b: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if let Some(f) = f {
        return cx.eval(h.app(f, &[a, b])).map(Some);
    }
    if let (View::Str(x), View::Str(y)) = (h.view(a), h.view(b)) {
        return Ok(Some(h.int(edit_distance(x, y) as i64)));
    }
    if is_number(h, a) && is_number(h, b) {
        let d = h.call(h.sym("add"), &[a, h.call(h.sym("mul"), &[h.int(-1), b])]);
        return cx.eval(h.call(h.sym("abs"), &[d])).map(Some);
    }
    match (cx.as_list(a), cx.as_list(b)) {
        (Some(xs), Some(ys)) if xs.len() == ys.len() && !xs.is_empty() && xs.iter().chain(ys).all(|&x| is_number(h, x)) => {
            let sq: Vec<T<'h>> = xs
                .iter()
                .zip(ys)
                .map(|(&x, &y)| h.call(h.sym("pow"), &[h.call(h.sym("add"), &[x, h.call(h.sym("mul"), &[h.int(-1), y])]), h.int(2)]))
                .collect();
            let s = h.call(lib.s.add, &sq);
            cx.eval(h.call(h.sym("sqrt"), &[s])).map(Some)
        }
        _ => Ok(None),
    }
}

/// `find_shortest_tour(xs)`: `[length, tour]`, the shortest closed tour through the elements of `xs` as positions
/// from 1 back to 1; `find_shortest_tour(xs, j, k)`: the shortest path from element `j` to element `k` through all
/// the others. Option `distance_function: f`.
pub(crate) fn find_shortest_tour<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, df) = crate::ctx::option(h, args, 1, "distance_function");
    let (xs, ends) = match *args {
        [xs] => (xs, None),
        [xs, j, k] => match (h.view(j), h.view(k)) {
            (View::Int(j), View::Int(k)) => (xs, Some((j, k))),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    let items = match cx.as_list(xs) {
        Some(items) => items,
        None => return Ok(Reply::Decline),
    };
    let n = items.len();
    if n == 0 || n > MAX_POINTS {
        return Ok(Reply::Decline);
    }
    let ends = match ends {
        None => None,
        Some((j, k)) if j >= 1 && k >= 1 && j as usize <= n && k as usize <= n && (j != k || n == 1) => {
            Some((j as usize - 1, k as usize - 1))
        }
        Some(_) => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
    };
    // the distances, exact and as machine numbers
    let mut exact = vec![h.int(0); n * n];
    let mut w = vec![0.0f64; n * n];
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            cx.tick(1)?;
            let Some(d) = distance(lib, cx, df, items[i], items[j])? else { return Ok(Reply::Decline) };
            // an infinite distance: no step between the two
            if matches!(h.view(d), View::Sym(s) if h.sym_name(s) == "inf") {
                w[i * n + j] = f64::INFINITY;
                continue;
            }
            match machine(cx, d)? {
                Some(x) if x.is_finite() && x >= 0.0 => {
                    exact[i * n + j] = d;
                    w[i * n + j] = x;
                }
                _ => return Ok(Reply::Decline),
            }
        }
    }
    let order = match ends {
        None => cycle(cx, &w, n)?,
        Some((s, e)) => path(cx, &w, n, s, e)?,
    };
    // no tour of finite length (infinite distances): failure(not_found, ...)
    let Some(order) = order else { return Ok(Reply::Done(lib.fail(h, lib.s.not_found, &[t]))) };
    let edges: Vec<T<'h>> = order.windows(2).map(|p| exact[p[0] * n + p[1]]).collect();
    let length = if edges.is_empty() { h.int(0) } else { cx.eval(h.call(lib.s.add, &edges))? };
    let tour: Vec<T<'h>> = order.iter().map(|&i| h.int(i as i64 + 1)).collect();
    let tour = cx.list(&tour);
    Ok(Reply::Done(cx.list(&[length, tour])))
}

/// The table of shortest paths from `start` through the subsets of the other points (bit `b` of a subset is
/// `others[b]`), ending at each of them: cost and the index (into `others`) of the point before the last.
fn table(cx: &mut NativeCx<'_, '_, '_>, w: &[f64], n: usize, start: usize, others: &[usize]) -> Res<(Vec<f64>, Vec<u8>)> {
    let m = others.len();
    let size = 1usize << m;
    let mut cost = vec![f64::INFINITY; size * m];
    let mut prev = vec![u8::MAX; size * m];
    for (b, &j) in others.iter().enumerate() {
        cost[(1 << b) * m + b] = w[start * n + j];
    }
    let mut meter = Meter::default();
    for set in 1..size {
        for last in 0..m {
            meter.step(cx)?;
            if set & (1 << last) == 0 {
                continue;
            }
            let c = cost[set * m + last];
            if !c.is_finite() {
                continue;
            }
            let from = others[last];
            for next in 0..m {
                if set & (1 << next) != 0 {
                    continue;
                }
                let s2 = set | (1 << next);
                let v = c + w[from * n + others[next]];
                // the first predecessor in index order wins a tie
                if v < cost[s2 * m + next] {
                    cost[s2 * m + next] = v;
                    prev[s2 * m + next] = last as u8;
                }
            }
        }
    }
    Ok((cost, prev))
}

/// The points of the path the table ends with at `last` over the full set, from the start.
fn walk(prev: &[u8], m: usize, others: &[usize], mut last: usize) -> Vec<usize> {
    let mut set = (1usize << m) - 1;
    let mut out = vec![];
    loop {
        out.push(others[last]);
        let p = prev[set * m + last];
        set &= !(1 << last);
        if p == u8::MAX {
            break;
        }
        last = p as usize;
    }
    out.reverse();
    out
}

/// The shortest closed tour from point 0, as positions 0, ..., 0; of the two directions the one whose second point
/// has the smaller index. `None` when every tour has an infinite step.
fn cycle(cx: &mut NativeCx<'_, '_, '_>, w: &[f64], n: usize) -> Res<Option<Vec<usize>>> {
    if n == 1 {
        return Ok(Some(vec![0, 0]));
    }
    let others: Vec<usize> = (1..n).collect();
    let m = others.len();
    let (cost, prev) = table(cx, w, n, 0, &others)?;
    let full = (1usize << m) - 1;
    let mut best = (f64::INFINITY, 0);
    for last in 0..m {
        let v = cost[full * m + last] + w[others[last] * n];
        if v < best.0 {
            best = (v, last);
        }
    }
    if !best.0.is_finite() {
        return Ok(None);
    }
    let mut order = vec![0];
    order.extend(walk(&prev, m, &others, best.1));
    order.push(0);
    // the reverse direction is as long only for symmetric distances
    let symmetric = (0..n).all(|i| (0..n).all(|j| w[i * n + j] == w[j * n + i]));
    if symmetric && order.len() > 3 && order[1] > order[order.len() - 2] {
        order.reverse();
    }
    Ok(Some(order))
}

/// The shortest path from `s` to `e` through every other point; `None` when every such path has an infinite step.
fn path(cx: &mut NativeCx<'_, '_, '_>, w: &[f64], n: usize, s: usize, e: usize) -> Res<Option<Vec<usize>>> {
    if n == 1 {
        return Ok(Some(vec![s]));
    }
    let others: Vec<usize> = (0..n).filter(|&i| i != s).collect();
    let m = others.len();
    let (cost, prev) = table(cx, w, n, s, &others)?;
    // invariant: e is one of the others (e != s for n > 1)
    let last = others.iter().position(|&i| i == e).expect("the end among the other points");
    if !cost[((1usize << m) - 1) * m + last].is_finite() {
        return Ok(None);
    }
    let mut order = vec![s];
    order.extend(walk(&prev, m, &others, last));
    Ok(Some(order))
}

#[cfg(test)]
mod tests {
    use super::edit_distance;

    #[test]
    fn edit_distances() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("dog", "dog"), 0);
    }
}
