//! Statistics of data lists: `mean`, `variance`, `std_dev`, `skewness`, `kurtosis`, `moment`, `median`,
//! `quantile` and `histogram_list` on a list of numbers (decision DI.7).
//!
//! - Lists with a machine real are computed in machine arithmetic straight from the packed array (compensated
//!   sums, two passes for the central moments).
//! - Otherwise the result is a formula over the list (`sum(xs)/length(xs)`, ...) evaluated by the evaluator: exact
//!   numbers give exact results, symbols give expressions.
//! - `median` and `quantile` need real numbers (they sort); `quantile(xs, q)` is the element of rank
//!   max(1, ceil(n q)) of the sorted list (the inverse of the empirical distribution function), so it is always
//!   an element of the data.
use crate::build::B;
use mc_api::{Store, View, T};
use mc_arith::number::compare;
use mc_arith::{Arith, Cmp3, Num, Real};
use mc_core::Heap;
use mc_eval::{NativeCx, Reply, Res};
use std::cmp::Ordering;

/// A data list as numbers: machine reals when one is machine, else the elements as they are.
pub enum Data<'h> {
    /// all elements real numbers, at least one a machine real (or a packed array of them)
    Machine(Vec<f64>),
    /// any other list (exact numbers, symbols)
    Terms(&'h [T<'h>]),
}

/// The data of a list term (`None` for a term that is not a list).
pub fn read<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Data<'h>> {
    let h = cx.heap();
    if let Some(mc_api::packed::Packed::Reals { dims, data }) = h.packed(t) {
        if dims.len() == 1 {
            return Some(Data::Machine(data.to_vec()));
        }
    }
    let items = cx.as_list(t)?;
    let ar = Arith::new(h);
    let mut machine = false;
    let mut xs = Vec::with_capacity(items.len());
    for &x in items {
        match ar.num(x) {
            Some(Num::Real(r)) => {
                machine |= matches!(r, Real::Machine(_));
                xs.push(r.to_f64());
            }
            _ => return Some(Data::Terms(items)),
        }
    }
    Some(if machine { Data::Machine(xs) } else { Data::Terms(items) })
}

/// A compensated (Neumaier) sum.
fn sum(xs: impl Iterator<Item = f64>) -> f64 {
    let (mut s, mut c) = (0.0f64, 0.0f64);
    for x in xs {
        let t = s + x;
        if s.abs() >= x.abs() {
            c += (s - t) + x;
        } else {
            c += (x - t) + s;
        }
        s = t;
    }
    s + c
}

/// The mean of machine data.
pub fn mean(xs: &[f64]) -> f64 {
    sum(xs.iter().copied()) / xs.len() as f64
}

/// The k-th central moment sum_i (x_i - m)^k / n of machine data with mean m.
fn central(xs: &[f64], m: f64, k: i32) -> f64 {
    sum(xs.iter().map(|&x| (x - m).powi(k))) / xs.len() as f64
}

/// The statistics of a data list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DataStat {
    /// the mean
    Mean,
    /// the sample variance (divisor n - 1)
    Variance,
    /// the sample standard deviation
    StdDev,
    /// m3 / m2^(3/2) with the central moments m_k of divisor n
    Skewness,
    /// m4 / m2^2
    Kurtosis,
    /// the k-th raw moment sum x^k / n
    Moment(i64),
    /// the k-th central moment sum (x - mean)^k / n
    Central(i64),
}

/// `f(xs)` for a statistic of the data list `xs` (a list term, `data` read from it).
pub fn stat<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &str, s: DataStat, xs: T<'h>, data: Data<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let b = B::new(h);
    let n = match &data {
        Data::Machine(v) => v.len(),
        Data::Terms(v) => v.len(),
    };
    let need = if matches!(s, DataStat::Variance | DataStat::StdDev) { 2 } else { 1 };
    if n < need {
        let m = if need == 2 { "the list needs at least two elements" } else { "the list is empty" };
        return Ok(Reply::Done(b.bad(f, m)));
    }
    cx.tick(n as u64)?;
    if let Data::Machine(v) = data {
        let m = mean(&v);
        let r = match s {
            DataStat::Mean => m,
            DataStat::Variance => central(&v, m, 2) * n as f64 / (n as f64 - 1.0),
            DataStat::StdDev => (central(&v, m, 2) * n as f64 / (n as f64 - 1.0)).sqrt(),
            DataStat::Skewness => central(&v, m, 3) / central(&v, m, 2).powf(1.5),
            DataStat::Kurtosis => central(&v, m, 4) / central(&v, m, 2).powi(2),
            DataStat::Moment(k) => sum(v.iter().map(|&x| x.powf(k as f64))) / n as f64,
            DataStat::Central(k) => sum(v.iter().map(|&x| (x - m).powf(k as f64))) / n as f64,
        };
        return Ok(Reply::Done(h.real(r)));
    }
    let len = b.int(n as i64);
    let mean_of = |t: T<'h>| b.div(b.call("sum", &[t]), len);
    if let DataStat::Mean = s {
        return Ok(Reply::Again(mean_of(xs)));
    }
    if let DataStat::Moment(k) = s {
        return Ok(Reply::Again(mean_of(b.powi(xs, k))));
    }
    // the deviations from the mean, the mean evaluated once
    let m = cx.eval(mean_of(xs))?;
    let dev = b.sub(xs, m);
    let cm = |k: i64| b.div(b.call("sum", &[b.powi(dev, k)]), len);
    let var = b.div(b.call("sum", &[b.powi(dev, 2)]), b.int(n as i64 - 1));
    Ok(Reply::Again(match s {
        DataStat::Variance => var,
        DataStat::StdDev => b.sqrt(var),
        DataStat::Skewness => b.div(cm(3), b.pow(cm(2), b.rat(3, 2))),
        DataStat::Central(k) => cm(k),
        _ => b.div(cm(4), b.powi(cm(2), 2)),
    }))
}

/// The elements of a list of real numbers, sorted by value (`None` when an element is not a real number or two
/// cannot be compared).
fn sorted<'h>(h: &'h Heap, xs: &'h [T<'h>]) -> Option<Vec<(Real, T<'h>)>> {
    let ar = Arith::new(h);
    let mut v: Vec<(Real, T<'h>)> = Vec::with_capacity(xs.len());
    for &x in xs {
        match ar.num(x) {
            Some(Num::Real(r)) => v.push((r, x)),
            _ => return None,
        }
    }
    let mut ok = true;
    v.sort_by(|a, b| match compare(&a.0, &b.0) {
        Cmp3::Less => Ordering::Less,
        Cmp3::Greater => Ordering::Greater,
        Cmp3::Equal => Ordering::Equal,
        _ => {
            ok = false;
            Ordering::Equal
        }
    });
    ok.then_some(v)
}

/// `median(xs)`: the middle element of the sorted list, or the mean of the two middle ones.
pub fn median<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &str, xs: T<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let b = B::new(h);
    if let Some(mc_api::packed::Packed::Reals { dims, data }) = h.packed(xs) {
        if dims.len() == 1 {
            cx.tick(data.len() as u64)?;
            let mut v = data.to_vec();
            v.sort_by(f64::total_cmp);
            let n = v.len();
            let m = if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 };
            return Ok(Reply::Done(h.real(m)));
        }
    }
    let Some(items) = cx.as_list(xs) else { return Ok(Reply::Decline) };
    if items.is_empty() {
        return Ok(Reply::Done(b.bad(f, "the list is empty")));
    }
    cx.tick(items.len() as u64)?;
    let Some(v) = sorted(h, items) else { return Ok(Reply::Decline) };
    let n = v.len();
    Ok(if n % 2 == 1 {
        Reply::Done(v[n / 2].1)
    } else {
        Reply::Again(b.mul(&[b.rat(1, 2), b.add(&[v[n / 2 - 1].1, v[n / 2].1])]))
    })
}

/// `quantile(xs, q)` (q a real in [0, 1], or a list of them): the element of rank max(1, ceil(n q)).
pub fn quantile<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &str, xs: T<'h>, q: T<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let b = B::new(h);
    let ar = Arith::new(h);
    let Some(items) = cx.as_list(xs) else { return Ok(Reply::Decline) };
    if items.is_empty() {
        return Ok(Reply::Done(b.bad(f, "the list is empty")));
    }
    cx.tick(items.len() as u64)?;
    let Some(v) = sorted(h, items) else { return Ok(Reply::Decline) };
    let n = v.len();
    let qs: Vec<T<'h>> = match cx.as_list(q) {
        Some(qs) => qs.to_vec(),
        None => vec![q],
    };
    let mut out = Vec::with_capacity(qs.len());
    for &qt in &qs {
        let Some(Num::Real(r)) = ar.num(qt) else { return Ok(Reply::Decline) };
        let in_range = matches!(compare(&r, &Real::int(0)), Cmp3::Greater | Cmp3::Equal)
            && matches!(compare(&r, &Real::int(1)), Cmp3::Less | Cmp3::Equal);
        if !in_range {
            return Ok(Reply::Done(b.bad(f, "the probability must be a real number in [0, 1]")));
        }
        // ceil(n q) exactly for exact q
        let rank = match &r {
            Real::Exact(qq) => {
                let nq = qq.mul(&mc_arith::num::Rat::from_i64(n as i64));
                let (fl, fr) = nq.floor_fract();
                let c = fl.to_i64().unwrap_or(n as i64) + i64::from(!fr.is_zero());
                c as usize
            }
            r => (n as f64 * r.to_f64()).ceil() as usize,
        };
        out.push(v[rank.clamp(1, n) - 1].1);
    }
    Ok(Reply::Done(if cx.as_list(q).is_some() { cx.list(&out) } else { out[0] }))
}

/// The bins of `histogram_list`: edges as numbers (kept exact when the specification is exact), and whether the
/// last bin is closed on the right.
struct Bins {
    edges: Vec<Real>,
    last_closed: bool,
}

/// The most bins one call makes.
const MAX_BINS: usize = 1 << 20;

/// A "nice" bin width near w: 1, 2 or 5 times a power of 10, exactly.
fn nice_width(w: f64) -> Real {
    let e = w.log10().floor() as i32;
    let m = w / 10f64.powi(e);
    let k = if m <= 1.0 {
        1
    } else if m <= 2.0 {
        2
    } else if m <= 5.0 {
        5
    } else {
        10
    };
    let ten = Real::int(10);
    let p = if e >= 0 {
        let mut r = Real::int(1);
        for _ in 0..e {
            r = r.mul(&ten);
        }
        r
    } else {
        let mut r = Real::int(1);
        for _ in 0..(-e) {
            r = r.mul(&ten);
        }
        Real::int(1).div(&r).unwrap_or(Real::int(1))
    };
    Real::int(k).mul(&p)
}

/// Edges a + i w from the multiple of w at or below lo up past hi.
fn grid(lo: f64, hi: f64, w: &Real, start: Option<&Real>) -> Option<Vec<Real>> {
    let wf = w.to_f64();
    if wf.is_nan() || wf <= 0.0 {
        return None;
    }
    let first = match start {
        Some(a) => a.clone(),
        None => w.mul(&Real::int((lo / wf).floor() as i64)),
    };
    let count = ((hi - first.to_f64()) / wf).floor() as i64 + 2;
    if count < 2 || count as usize > MAX_BINS + 1 {
        return None;
    }
    Some((0..count).map(|i| first.add(&w.mul(&Real::int(i)))).collect())
}

/// `histogram_list(data)`, `histogram_list(data, n)`, `histogram_list(data, [w])`, `histogram_list(data, [a, b, w])`,
/// `histogram_list(data, [[e1, e2, ...]])`: `[edges, counts]`, bins [e_i, e_{i+1}) (the last one closed for the
/// count form `n`, whose n equal bins span [min, max]).
pub fn histogram<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &str, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let b = B::new(h);
    let ar = Arith::new(h);
    let fail = |m: &str| Ok(Reply::Done(b.bad(f, m)));
    let (xs, spec) = match args {
        [xs] => (*xs, None),
        [xs, s] => (*xs, Some(*s)),
        _ => return Ok(Reply::Decline),
    };
    // the data as numbers, kept exact for the edges of the count form
    let vals: Vec<Real> = if let Some(mc_api::packed::Packed::Reals { dims, data }) = h.packed(xs) {
        if dims.len() != 1 {
            return fail("the data must be a list of real numbers");
        }
        data.iter().map(|&x| Real::Machine(x)).collect()
    } else {
        let Some(items) = cx.as_list(xs) else { return Ok(Reply::Decline) };
        let mut v = Vec::with_capacity(items.len());
        for &x in items {
            match ar.num(x) {
                Some(Num::Real(r)) => v.push(r),
                _ => return fail("the data must be a list of real numbers"),
            }
        }
        v
    };
    if vals.is_empty() {
        return fail("the data list is empty");
    }
    cx.tick(vals.len() as u64)?;
    let mut fs: Vec<f64> = vals.iter().map(Real::to_f64).collect();
    let (mut lo_i, mut hi_i) = (0, 0);
    for (i, &x) in fs.iter().enumerate() {
        if x < fs[lo_i] {
            lo_i = i;
        }
        if x > fs[hi_i] {
            hi_i = i;
        }
    }
    let (lo, hi) = (fs[lo_i], fs[hi_i]);
    let real = |t: T<'h>| match ar.num(t) {
        Some(Num::Real(r)) => Some(r),
        _ => None,
    };
    let bins = match spec {
        None => {
            // Sturges' count, rounded to a nice width
            let k = (vals.len() as f64).log2().ceil() + 1.0;
            let w = if hi > lo { nice_width((hi - lo) / k) } else { Real::int(1) };
            grid(lo, hi, &w, None).map(|edges| Bins { edges, last_closed: false })
        }
        Some(s) => match (h.view(s), cx.as_list(s)) {
            (View::Int(k), _) if k >= 1 && (k as usize) <= MAX_BINS => {
                if hi > lo {
                    let (a, z) = (vals[lo_i].clone(), vals[hi_i].clone());
                    let width = z.sub(&a);
                    let kk = Real::int(k);
                    let edges = (0..=k)
                        .map(|i| a.add(&width.mul(&Real::int(i)).div(&kk).unwrap_or(Real::int(0))))
                        .collect();
                    Some(Bins { edges, last_closed: true })
                } else {
                    // all values equal: one unit bin around them per requested bin
                    let a = vals[lo_i].clone();
                    let edges = (0..=k).map(|i| a.add(&Real::int(i))).collect();
                    Some(Bins { edges, last_closed: false })
                }
            }
            (_, Some(&[w])) => match cx.as_list(w) {
                Some(es) => {
                    let edges: Option<Vec<Real>> = es.iter().map(|&e| real(e)).collect();
                    match edges {
                        Some(e) if e.len() >= 2 && e.windows(2).all(|p| p[0].to_f64() < p[1].to_f64()) => {
                            Some(Bins { edges: e, last_closed: false })
                        }
                        _ => return fail("the edges must be at least two increasing real numbers"),
                    }
                }
                None => real(w).and_then(|w| grid(lo, hi, &w, None)).map(|edges| Bins { edges, last_closed: false }),
            },
            (_, Some(&[a, z, w])) => match (real(a), real(z), real(w)) {
                (Some(a), Some(z), Some(w)) if a.to_f64() < z.to_f64() => {
                    grid(a.to_f64(), z.to_f64() - w.to_f64() * 1e-12, &w, Some(&a))
                        .map(|edges| Bins { edges, last_closed: false })
                }
                _ => return fail("the bin specification [a, b, w] needs reals a < b and w > 0"),
            },
            _ => None,
        },
    };
    let Some(bins) = bins else {
        return fail("the bin specification must be a count n >= 1, [w], [a, b, w] with w > 0 or [[e1, e2, ...]]");
    };
    let ef: Vec<f64> = bins.edges.iter().map(Real::to_f64).collect();
    let nb = ef.len() - 1;
    let mut counts = vec![0i64; nb];
    fs.sort_by(f64::total_cmp);
    cx.tick(nb as u64)?;
    // bin i holds [e_i, e_{i+1}): the count of values below e_{i+1} minus those below e_i
    let below = |e: f64| fs.partition_point(|&x| x < e);
    for (i, c) in counts.iter_mut().enumerate() {
        let upper = if i == nb - 1 && bins.last_closed { fs.partition_point(|&x| x <= ef[i + 1]) } else { below(ef[i + 1]) };
        *c = (upper - below(ef[i])) as i64;
    }
    let edges: Vec<T<'h>> = bins.edges.iter().map(|r| ar.number(&Num::Real(r.clone()))).collect();
    Ok(Reply::Done(cx.list(&[cx.list(&edges), h.ints(&counts)])))
}
