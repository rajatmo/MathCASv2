//! The value of a grid interpolation at a point of machine reals (`nl_li_machine(vals, grids, ks, xs)`), the engine of
//! the functions that `list_interpolation` and `function_interpolation` return (`lib/numerics.mc`, `nl_li_eval`).
//! Along each dimension in turn: the local Lagrange polynomial of order k through the k + 1 nodes nearest x
//! (Lagrange's formula, e.g. Press et al., *Numerical Recipes*, 3.2), the tensor product over the dimensions.
//!
//! Every operation is the one the language definition performs, on the same numbers and through the same
//! arithmetic (`Num::sum` and `Num::product`: exact operands first, inexact ones in the canonical order), so the
//! value is the one the language definition gives, bit for bit; only the nodes a later dimension reads are
//! computed. The call stays (and the language definition answers) for anything else: a point that is not all
//! machine reals, values that are not real numbers, a ragged array, orders that are not integers >= 0.
use mc_arith::{Arith, Num, Real};
use mc_core::{Heap, Packed, Store, T};

type Ar<'h> = Arith<'h, Heap>;
use mc_eval::{Native, NativeCx, Reply, Res};

/// The name of the native (private to the numerics library).
pub(crate) const NAME: &str = "nl_li_machine";

/// A rectangular array of real numbers, row-major.
struct Grid {
    dims: Vec<usize>,
    data: Vec<Num>,
}

fn real_num<'h>(ar: &Ar<'h>, t: T<'h>) -> Option<Num> {
    match ar.num(t)? {
        n @ Num::Real(Real::Exact(_) | Real::Machine(_)) => Some(n),
        _ => None,
    }
}

/// The array `t` of depth `depth` (packed or nested lists, real numbers at the bottom), `None` when it is ragged
/// or holds anything else.
fn read_array<'h>(cx: &NativeCx<'_, '_, 'h>, ar: &Ar<'h>, t: T<'h>, depth: usize) -> Option<Grid> {
    let h = cx.heap();
    if let Some(p) = h.packed(t) {
        if p.rank() != depth {
            return None;
        }
        let dims: Vec<usize> = p.dims().iter().map(|&d| d as usize).collect();
        let data = match p {
            Packed::Reals { data, .. } => data.iter().map(|&x| Num::Real(Real::Machine(x))).collect(),
            Packed::Ints { data, .. } => data.iter().map(|&k| Num::int(k)).collect(),
            _ => return None,
        };
        return Some(Grid { dims, data });
    }
    let mut dims = Vec::with_capacity(depth);
    let mut data = Vec::new();
    fn walk<'h>(
        cx: &NativeCx<'_, '_, 'h>,
        ar: &Ar<'h>,
        t: T<'h>,
        level: usize,
        depth: usize,
        dims: &mut Vec<usize>,
        data: &mut Vec<Num>,
    ) -> Option<()> {
        if level == depth {
            data.push(real_num(ar, t)?);
            return Some(());
        }
        let h = cx.heap();
        if let Some(p) = h.packed(t) {
            // a packed row inside a nested list
            if p.rank() != depth - level {
                return None;
            }
            for (i, &d) in p.dims().iter().enumerate() {
                if dims.len() <= level + i {
                    dims.push(d as usize);
                } else if dims[level + i] != d as usize {
                    return None;
                }
            }
            match p {
                Packed::Reals { data: v, .. } => data.extend(v.iter().map(|&x| Num::Real(Real::Machine(x)))),
                Packed::Ints { data: v, .. } => data.extend(v.iter().map(|&k| Num::int(k))),
                _ => return None,
            }
            return Some(());
        }
        let items = cx.as_list(t)?;
        if items.is_empty() {
            return None;
        }
        if dims.len() == level {
            dims.push(items.len());
        } else if dims[level] != items.len() {
            return None;
        }
        for &x in items {
            walk(cx, ar, x, level + 1, depth, dims, data)?;
        }
        Some(())
    }
    walk(cx, ar, t, 0, depth, &mut dims, &mut data)?;
    (dims.len() == depth && dims.iter().product::<usize>() == data.len()).then_some(Grid { dims, data })
}

/// The elements of a list of numbers, packed or not.
fn numbers<'h>(cx: &NativeCx<'_, '_, 'h>, ar: &Ar<'h>, t: T<'h>) -> Option<Vec<Num>> {
    if let Some(p) = cx.heap().packed(t) {
        if p.rank() != 1 {
            return None;
        }
        return match p {
            Packed::Reals { data, .. } => Some(data.iter().map(|&x| Num::Real(Real::Machine(x))).collect()),
            Packed::Ints { data, .. } => Some(data.iter().map(|&k| Num::int(k)).collect()),
            _ => None,
        };
    }
    cx.as_list(t)?.iter().map(|&x| real_num(ar, x)).collect()
}

fn neg(x: &Num) -> Num {
    x.neg()
}

/// The weights of the nodes along one dimension: the nodes used (0-based) and the factor of each, or the one node
/// whose value is taken as it is (order 0).
enum Weights {
    Nearest(usize),
    Lagrange(Vec<(usize, Num)>),
}

/// `nl_li_1d` of the numerics library at the machine real x on the coordinates `cs` with order k.
fn weights(cs: &[Num], k: i64, x: &Num) -> Option<Weights> {
    let xf = x.as_real()?.to_f64();
    let len = cs.len() as i64;
    let ord = k.min(len - 1);
    let c = |i: i64| &cs[(i - 1) as usize];
    let cf = |i: i64| c(i).as_real().map(Real::to_f64);
    let mut i = 1i64;
    while i < len - 1 && xf > cf(i + 1)? {
        i += 1;
    }
    if ord == 0 {
        let j = (i + 1).min(len);
        let mid = Num::product(vec![Num::sum(vec![c(i).clone(), c(j).clone()]), Num::int(2).recip().ok()?]);
        let take = if len == 1 || xf < mid.as_real()?.to_f64() { i } else { j };
        return Some(Weights::Nearest((take - 1) as usize));
    }
    let lo = 1.max((len - ord).min(i - (ord - 1).div_euclid(2)));
    let idx: Vec<i64> = (lo..=lo + ord).collect();
    let mut out = Vec::with_capacity(idx.len());
    for &j in &idx {
        let mut fs = Vec::with_capacity(idx.len());
        for &m in &idx {
            if m == j {
                fs.push(Num::int(1));
            } else {
                let num = Num::sum(vec![x.clone(), neg(c(m))]);
                let den = Num::sum(vec![c(j).clone(), neg(c(m))]);
                let inv = den.recip().ok()?;
                fs.push(Num::product(vec![num, inv]));
            }
        }
        out.push(((j - 1) as usize, Num::product(fs)));
    }
    Some(Weights::Lagrange(out))
}

/// The array after the first k dimensions are interpolated (the language definition's order), at the indices `tail`
/// of the dimensions k onwards; k = the rank gives the value.
fn value(g: &Grid, ws: &[Weights], k: usize, tail: &[usize]) -> Num {
    if k == 0 {
        let mut at = 0;
        for (i, &j) in tail.iter().enumerate() {
            at = at * g.dims[i] + j;
        }
        return g.data[at].clone();
    }
    let at = |j: usize| {
        let mut t = Vec::with_capacity(tail.len() + 1);
        t.push(j);
        t.extend_from_slice(tail);
        value(g, ws, k - 1, &t)
    };
    match &ws[k - 1] {
        Weights::Nearest(j) => at(*j),
        Weights::Lagrange(w) => Num::sum(w.iter().map(|(j, p)| Num::product(vec![at(*j), p.clone()])).collect()),
    }
}

// ---- the same operations on doubles: values and coordinates that are machine reals or integers of at most 2^53
// (exact as doubles), every order at least 1. `Num::sum` and `Num::product` of machine numbers add or multiply in
// ascending order from the first; an exact integer next to machine numbers is converted exactly first; the exact
// reciprocal 1/d of an integer d rounds to the double 1.0/d; so these give the `Num` path's doubles.

/// A double that is the exact value of a machine real or a small integer term.
fn small<'h>(h: &'h mc_core::Heap, t: T<'h>) -> Option<f64> {
    match h.view(t) {
        mc_core::View::Real(r) if r.is_finite() => Some(r),
        mc_core::View::Int(i) if i.unsigned_abs() <= 1 << 53 => Some(i as f64),
        _ => None,
    }
}

/// The doubles of a vector (packed or a list).
fn small_vector<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<f64>> {
    let h = cx.heap();
    if let Some(p) = h.packed(t) {
        return match p {
            Packed::Reals { data, .. } if p.rank() == 1 && data.iter().all(|x| x.is_finite()) => Some(data.to_vec()),
            Packed::Ints { data, .. } if p.rank() == 1 && data.iter().all(|i| i.unsigned_abs() <= 1 << 53) => {
                Some(data.iter().map(|&i| i as f64).collect())
            }
            _ => None,
        };
    }
    cx.as_list(t)?.iter().map(|&x| small(h, x)).collect()
}

/// The array `t` of the dimensions `dims` as doubles, row-major (`None` when its shape is another).
fn small_array<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>, dims: &[usize], out: &mut Vec<f64>) -> Option<()> {
    let h = cx.heap();
    if let Some(p) = h.packed(t) {
        if p.dims().len() != dims.len() || p.dims().iter().zip(dims).any(|(&a, &b)| a as usize != b) {
            return None;
        }
        out.extend(small_vector_flat(p)?);
        return Some(());
    }
    let Some((&n, rest)) = dims.split_first() else {
        out.push(small(h, t)?);
        return Some(());
    };
    let items = cx.as_list(t)?;
    if items.len() != n {
        return None;
    }
    for &x in items {
        small_array(cx, x, rest, out)?;
    }
    Some(())
}

/// The leaves of a packed array as doubles.
fn small_vector_flat(p: Packed<'_>) -> Option<Vec<f64>> {
    match p {
        Packed::Reals { data, .. } if data.iter().all(|x| x.is_finite()) => Some(data.to_vec()),
        Packed::Ints { data, .. } if data.iter().all(|i| i.unsigned_abs() <= 1 << 53) => Some(data.iter().map(|&i| i as f64).collect()),
        _ => None,
    }
}

/// `Num::sum` of machine numbers: ascending, from the first.
fn fsum(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.iter().skip(1).fold(v[0], |a, &x| a + x)
}

/// `Num::product` of machine numbers (after the exact 1 of the language's list): ascending, from the first.
fn fprod(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.iter().skip(1).fold(v[0], |a, &x| a * x)
}

/// [`weights`] on doubles, for an order of at least 1.
fn weights_f64(cs: &[f64], k: i64, x: f64) -> Option<Vec<(usize, f64)>> {
    let len = cs.len() as i64;
    let ord = k.min(len - 1);
    if ord < 1 {
        return None;
    }
    let c = |i: i64| cs[(i - 1) as usize];
    let mut i = 1i64;
    while i < len - 1 && x > c(i + 1) {
        i += 1;
    }
    let lo = 1.max((len - ord).min(i - (ord - 1).div_euclid(2)));
    let mut out = Vec::with_capacity(ord as usize + 1);
    for j in lo..=lo + ord {
        let fs: Vec<f64> = (lo..=lo + ord).filter(|&m| m != j).map(|m| (x - c(m)) * (1.0 / (c(j) - c(m)))).collect();
        out.push(((j - 1) as usize, fprod(fs)));
    }
    Some(out)
}

/// [`value`] on doubles.
fn value_f64(dims: &[usize], data: &[f64], ws: &[Vec<(usize, f64)>], k: usize, tail: &mut Vec<usize>) -> f64 {
    if k == 0 {
        let mut at = 0;
        for (i, &j) in tail.iter().rev().enumerate() {
            at = at * dims[i] + j;
        }
        return data[at];
    }
    let terms = ws[k - 1]
        .iter()
        .map(|&(j, p)| {
            tail.push(j);
            let v = value_f64(dims, data, ws, k - 1, tail);
            tail.pop();
            v * p
        })
        .collect();
    fsum(terms)
}

/// An interpolation order: an integer >= 0, or a machine real with such a value (a sampled function's body is made
/// numeric, orders included; the language definition takes floor(min(k, len - 1)), the same integer).
fn order<'h>(h: &'h mc_core::Heap, k: T<'h>) -> Option<i64> {
    match h.view(k) {
        mc_core::View::Int(k) if k >= 0 => Some(k),
        mc_core::View::Real(r) if r >= 0.0 && r.fract() == 0.0 && r < 1e9 => Some(r as i64),
        _ => None,
    }
}

/// The value on doubles, when every number allows it.
fn fast<'h>(cx: &NativeCx<'_, '_, 'h>, vals: T<'h>, grids: T<'h>, ks: T<'h>, xs: T<'h>) -> Option<f64> {
    let h = cx.heap();
    // the point: machine reals only (an exact coordinate keeps the language's exact arithmetic)
    let xs: Vec<f64> = match h.packed(xs) {
        Some(Packed::Reals { data, .. }) if data.iter().all(|x| x.is_finite()) => data.to_vec(),
        Some(_) => return None,
        None => cx
            .as_list(xs)?
            .iter()
            .map(|&x| match h.view(x) {
                mc_core::View::Real(r) if r.is_finite() => Some(r),
                _ => None,
            })
            .collect::<Option<Vec<f64>>>()?,
    };
    let d = xs.len();
    let grids = cx.as_list(grids)?;
    let ks = cx.as_list(ks)?;
    if d == 0 || grids.len() < d || ks.len() < d {
        return None;
    }
    let mut dims = Vec::with_capacity(d);
    let mut ws = Vec::with_capacity(d);
    for i in 0..d {
        let k = order(h, ks[i])?;
        let cs = small_vector(cx, grids[i])?;
        dims.push(cs.len());
        ws.push(weights_f64(&cs, k, xs[i])?);
    }
    let mut data = Vec::with_capacity(dims.iter().product());
    small_array(cx, vals, &dims, &mut data)?;
    let mut tail = Vec::with_capacity(d);
    Some(value_f64(&dims, &data, &ws, d, &mut tail))
}

/// `nl_li_machine(vals, grids, ks, xs)`: the interpolated value, or the call unevaluated.
pub(crate) struct LiMachine;

impl Native for LiMachine {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [vals, grids, ks, xs] = args else { return Ok(Reply::Decline) };
        // the common case first: no exact rationals and every order at least 1
        if let Some(v) = fast(cx, *vals, *grids, *ks, *xs) {
            cx.tick(1)?;
            return Ok(Reply::Done(h.real(v)));
        }
        let ar = Arith::new(h);
        let Some(points) = numbers(cx, &ar, *xs) else { return Ok(Reply::Decline) };
        let Some(grids) = cx.as_list(*grids).map(<[T<'h>]>::to_vec) else { return Ok(Reply::Decline) };
        let Some(ks) = cx.as_list(*ks).map(<[T<'h>]>::to_vec) else { return Ok(Reply::Decline) };
        let d = points.len();
        if d == 0 || grids.len() < d || ks.len() < d {
            return Ok(Reply::Decline);
        }
        if !points.iter().all(|p| matches!(p, Num::Real(Real::Machine(v)) if v.is_finite())) {
            return Ok(Reply::Decline);
        }
        let Some(g) = read_array(cx, &ar, *vals, d) else { return Ok(Reply::Decline) };
        cx.tick(1 + g.data.len() as u64 / 64)?;
        let mut ws = Vec::with_capacity(d);
        for i in 0..d {
            let Some(k) = order(h, ks[i]) else { return Ok(Reply::Decline) };
            let Some(cs) = numbers(cx, &ar, grids[i]) else { return Ok(Reply::Decline) };
            if cs.len() != g.dims[i] {
                return Ok(Reply::Decline);
            }
            let Some(w) = weights(&cs, k, &points[i]) else { return Ok(Reply::Decline) };
            ws.push(w);
        }
        let v = value(&g, &ws, d, &[]);
        Ok(Reply::Done(ar.number(&v)))
    }
}
