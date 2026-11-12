//! Data containers (round 29): `weighted_data(data, weights)`, `event_data(data, flags)`, `time_series(values,
//! times)` and `temporal_data(values, times)`.
//!
//! They are held terms with a normal form and properties read by calling the container on a property name:
//! `ts("Values")`. A series made from time-value pairs `time_series([[t1, v1], ...])`, from values alone (times 0,
//! 1, 2, ...) or from values and a start and end time `time_series(values, [t0, t1])` (evenly spaced) is stored as `time_series(values,
//! times)`. The statistics `mean`, `median`, `variance`, `std_dev`, `quantile` of a series are those of
//! its values (decision b29.2); `mean` of weighted data is the weighted mean `dot(data, w) / total(w)`.
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res, Rhs};
use mc_eval::pat::{self, Pat};
use std::sync::Arc;

/// The containers: name, and the name their properties are read under.
pub(crate) const KINDS: &[&str] = &["weighted_data", "event_data", "time_series", "temporal_data"];

struct Container;

fn is_series(name: &str) -> bool {
    matches!(name, "time_series" | "temporal_data")
}

fn str_of<'h>(h: &'h Heap, t: T<'h>) -> Option<String> {
    match h.view(t) {
        View::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

fn is_number<'h>(h: &'h Heap, t: T<'h>) -> bool {
    matches!(h.view(t), View::Int(_) | View::Real(_) | View::Rational { .. } | View::BigInt { .. })
}

impl Native for Container {
    fn curried(&self) -> bool {
        true
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
        match h.view(head) {
            // `w("Property")`
            View::App { head: name, args: inner } => {
                let (View::Sym(s), [prop]) = (h.view(name), args) else { return Ok(Reply::Decline) };
                let Some(prop) = str_of(h, *prop) else { return Ok(Reply::Decline) };
                property(cx, h.sym_name(s).as_str(), inner, &prop)
            }
            View::Sym(s) => construct(cx, h.sym_name(s).as_str(), args),
            _ => Ok(Reply::Decline),
        }
    }
}

/// The normal form of a series: `time_series(values, times)`.
fn construct<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, args: &[T<'h>]) -> Res<Reply<'h>> {
    if !is_series(name) {
        return Ok(Reply::Decline);
    }
    let h = cx.heap();
    let head = h.sym(name);
    let ints = |n: usize| -> Vec<T<'h>> { (0..n as i64).map(|i| h.int(i)).collect() };
    let (values, times): (Vec<T<'h>>, Vec<T<'h>>) = match args {
        [one] => {
            let Some(l) = cx.as_list(*one) else { return Ok(Reply::Decline) };
            let pairs: Option<Vec<(T<'h>, T<'h>)>> =
                l.iter().map(|&p| cx.as_list(p).and_then(|q| if q.len() == 2 && is_number(h, q[0]) { Some((q[0], q[1])) } else { None })).collect();
            match pairs {
                Some(ps) if !ps.is_empty() => ps.into_iter().unzip(),
                _ => (l.to_vec(), ints(l.len())),
            }
        }
        [vals, spec] => {
            let (Some(v), Some(s)) = (cx.as_list(*vals), Some(*spec)) else { return Ok(Reply::Decline) };
            match cx.as_list(s) {
                // times given one by one
                // (already the normal form)
                Some(ts) if ts.len() == v.len() => return Ok(Reply::Decline),
                // [t0, t1]: the times spaced evenly from t0 to t1 (the last one is t1 itself)
                Some(&[t0, t1]) => {
                    let n = v.len() as i64;
                    let times: Vec<T<'h>> = (0..n)
                        .map(|i| {
                            if i == 0 || n == 1 {
                                t0
                            } else if i == n - 1 {
                                t1
                            } else {
                                let span = h.call(h.sym("add"), &[t1, h.call(h.sym("mul"), &[h.int(-1), t0])]);
                                let part = h.call(h.sym("mul"), &[h.int(i), span, h.call(h.sym("pow"), &[h.int(n - 1), h.int(-1)])]);
                                h.call(h.sym("add"), &[t0, part])
                            }
                        })
                        .collect();
                    (v.to_vec(), times)
                }
                Some(_) => return Ok(Reply::Decline),
                // a start time
                None if is_number(h, s) => {
                    let times: Vec<T<'h>> = (0..v.len() as i64).map(|i| h.call(h.sym("add"), &[s, h.int(i)])).collect();
                    (v.to_vec(), times)
                }
                None => return Ok(Reply::Decline),
            }
        }
        _ => return Ok(Reply::Decline),
    };
    Ok(Reply::Again(h.call(head, &[cx.list(&values), cx.list(&times)])))
}

fn property<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, inner: &[T<'h>], prop: &str) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[a, b] = inner else { return Ok(Reply::Decline) };
    let (Some(la), Some(lb)) = (cx.as_list(a).map(|l| l.to_vec()), cx.as_list(b).map(|l| l.to_vec())) else { return Ok(Reply::Decline) };
    if la.len() != lb.len() {
        return Ok(Reply::Decline);
    }
    let call = |f: &str, x: &[T<'h>]| h.call(h.sym(f), x);
    let last = la.len().checked_sub(1);
    let t = match (name, prop) {
        ("weighted_data", "Data") | ("event_data", "Data") | (_, "Values") => a,
        ("weighted_data", "Weights") => b,
        ("event_data", "Flags") | (_, "Times") => b,
        (_, "Length") | (_, "PathLength") | (_, "Count") => h.int(la.len() as i64),
        ("weighted_data", "TotalWeight") => call("total", &[b]),
        (n, "Path") if is_series(n) => {
            let ps: Vec<T<'h>> = lb.iter().zip(&la).map(|(&t, &v)| cx.list(&[t, v])).collect();
            cx.list(&ps)
        }
        (n, "FirstValue") if is_series(n) => *la.first().unwrap_or(&a),
        (n, "LastValue") if is_series(n) => last.map_or(a, |i| la[i]),
        (n, "FirstTime") if is_series(n) => *lb.first().unwrap_or(&b),
        (n, "LastTime") if is_series(n) => last.map_or(b, |i| lb[i]),
        (n, "Duration") if is_series(n) && !la.is_empty() => call("add", &[lb[la.len() - 1], call("mul", &[h.int(-1), lb[0]])]),
        _ => return Ok(Reply::Decline),
    };
    Ok(Reply::Again(t))
}

/// The statistics of a series' values: `f(ts @ time_series(v, t), rest...)` is `f(v, rest...)`.
const SERIES_STATS: &[&str] = &["mean", "median", "variance", "std_dev", "quantile"];

/// Register the containers.
pub fn install(k: &mut Kernel, h: &Heap) {
    for &name in KINDS {
        k.register(h.sym(name), Attributes::default(), Box::new(Container));
    }
    let any = || Pat::Seq { var: None, min: 0, ty: None, mark: None };
    for &head in &["time_series", "temporal_data"] {
        let hs = h.sym(head);
        for &name in SERIES_STATS {
            let f = h.sym(name);
            let lhs = pat::call(h, f, vec![pat::named(0, pat::call(h, hs, vec![pat::var(1), any()])), pat::seq0(2)]);
            let rhs: Rhs = Arc::new(move |b, h| {
                let mut args = vec![b.one(1)];
                args.extend_from_slice(b.seq(2));
                h.call(f, &args)
            });
            k.define_attached(h, lhs, rhs, hs).expect("the series rules are well formed");
        }
    }
    weighted_mean(k, h);
}

/// `mean(weighted_data(d, w))` is `dot(d, w) / total(w)`.
fn weighted_mean(k: &mut Kernel, h: &Heap) {
    let hs: Sym = h.sym("weighted_data");
    let f = h.sym("mean");
    let lhs = pat::call(h, f, vec![pat::call(h, hs, vec![pat::var(0), pat::var(1)])]);
    let rhs: Rhs = Arc::new(move |b, h| {
        let (d, w) = (b.one(0), b.one(1));
        let inv = h.call(h.sym("pow"), &[h.call(h.sym("total"), &[w]), h.int(-1)]);
        h.call(h.sym("mul"), &[h.call(h.sym("dot"), &[d, w]), inv])
    });
    k.define_attached(h, lhs, rhs, hs).expect("the weighted mean rule is well formed");
}
