//! Associations made from lists: `counts` and `group_by`; and the failure tests `is_failure`, `failure_tag`.
use crate::assoc::from_distinct;
use crate::ctx::{map_calls, take, Lib, Meter};
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};
use rustc_hash::FxHashMap;

/// Distinct keys in order of first occurrence, with the positions of the elements of each.
pub(crate) fn classes<'h>(cx: &mut NativeCx<'_, '_, 'h>, keys: &[T<'h>]) -> Res<(Vec<T<'h>>, Vec<Vec<usize>>)> {
    let mut order = Vec::new();
    let mut members: Vec<Vec<usize>> = Vec::new();
    let mut at: FxHashMap<T<'h>, usize> = FxHashMap::default();
    let mut meter = Meter::default();
    for (i, &k) in keys.iter().enumerate() {
        meter.step(cx)?;
        let p = *at.entry(k).or_insert_with(|| {
            order.push(k);
            members.push(Vec::new());
            order.len() - 1
        });
        members[p].push(i);
    }
    Ok((order, members))
}

/// `counts(xs)`: the association from each distinct element of `xs` (any compound term) to the number of times it
/// occurs, in order of first occurrence.
pub(crate) fn counts<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // counts(xs, keys): the number of elements matching each key, 0 for none
    if let [xs, ks] = *args {
        let Some(ks) = cx.as_list(ks) else { return Ok(Reply::Decline) };
        let count = h.sym("count");
        let rule = h.sym("rule");
        let entries: Vec<T<'h>> = ks.iter().map(|&k| h.call(rule, &[k, h.call(count, &[xs, k])])).collect();
        return Ok(Reply::Again(h.call(h.sym("assoc"), &entries)));
    }
    let [xs] = *args else {
        return Ok(Reply::Decline);
    };
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let (keys, members) = classes(cx, xs)?;
    let entries: Vec<T<'h>> = keys.iter().zip(&members).map(|(&k, m)| lib.rule(h, k, h.int(m.len() as i64))).collect();
    Ok(Reply::Done(from_distinct(lib, h, &keys, &entries)))
}

/// `group_by(xs, f)`: the association from each distinct value `f(x)` to the list of the elements `x` with that
/// value, keys in order of first occurrence, elements in order; `group_by(xs, f, g)`: each list replaced by `g`
/// applied to it. A failure from `f` or `g` is the result. On a dataset the rows are grouped (`group_dataset`).
pub(crate) fn group_by<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, f, g) = match *args {
        [xs, f] => (xs, f, None),
        [xs, f, g] => (xs, f, Some(g)),
        _ => return Ok(Reply::Decline),
    };
    // a rule `f -> v` given last arrives as options: the rule itself
    let f = match h.view(f) {
        View::App { head, args: &[r] } if head == h.sym_term(h.sym("options")) => r,
        _ => f,
    };
    // `group_by(xs, [f1, f2, ...])` (not a dataset's column list): groups by f1, each group grouped by the rest (decision cov-core.9)
    let of_dataset = crate::table::Table::of(&lib.d, h, xs).is_some();
    if let Some(fs) = cx.as_list(f).filter(|fs| !fs.is_empty() && !of_dataset) {
        if fs.len() == 1 {
            let mut a = vec![xs, fs[0]];
            a.extend(g);
            return Ok(Reply::Again(h.call(h.sym("group_by"), &a)));
        }
        let outer = cx.eval(h.call(h.sym("group_by"), &[xs, fs[0]]))?;
        let Some(es) = lib.as_assoc(h, outer).map(|es| es.to_vec()) else { return Ok(Reply::Done(outer)) };
        let rest = cx.list(&fs[1..]);
        let mut out = Vec::with_capacity(es.len());
        for e in es {
            let Some((k, v)) = lib.entry(h, e) else { return Ok(Reply::Decline) };
            let mut a = vec![v, rest];
            a.extend(g);
            let inner = cx.eval(h.call(h.sym("group_by"), &a))?;
            out.push(lib.rule(h, k, inner));
        }
        return Ok(Reply::Done(lib.assoc(h, &out)));
    }
    // `group_by(xs, f -> v)`: keys by f, the members mapped by v (decision cov-core.9)
    let (f, fv) = match h.view(f) {
        View::App { head, args: &[a, b] } if head == h.sym_term(h.sym("rule")) || head == h.sym_term(h.sym("rule_delayed")) => {
            (a, Some(b))
        }
        _ => (f, None),
    };
    // a dataset groups its rows
    if let Some(tab) = crate::table::Table::of(&lib.d, h, xs) {
        return crate::aggregate::group_dataset(lib, cx, &tab, f, g);
    }
    // an association groups its entries by `f` of their values, each group an association (decision io-core.12)
    let entries = lib.as_assoc(h, xs).filter(|es| es.iter().all(|&e| lib.entry(h, e).is_some()));
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let tested: Vec<T<'h>> = match entries {
        Some(es) => es.iter().map(|&e| lib.value(h, e)).collect(),
        None => xs.to_vec(),
    };
    let fx = map_calls(cx, f, &tested)?;
    if let Some(&fail) = fx.iter().find(|&&v| lib.is_failure(h, v)) {
        return Ok(Reply::Done(fail));
    }
    let (keys, members) = classes(cx, &fx)?;
    let mut groups: Vec<T<'h>> = Vec::with_capacity(keys.len());
    for m in &members {
        cx.tick(m.len() as u64 / 16)?;
        let mut items: Vec<T<'h>> = m.iter().map(|&i| xs[i]).collect();
        if let Some(fv) = fv {
            for it in items.iter_mut() {
                *it = match lib.entry(h, *it) {
                    Some((k, v)) if entries.is_some() => lib.rule(h, k, cx.eval(h.app(fv, &[v]))?),
                    _ => cx.eval(h.app(fv, &[*it]))?,
                };
            }
        }
        groups.push(if entries.is_some() { lib.assoc(h, &items) } else { cx.list(&items) });
    }
    if let Some(g) = g {
        groups = map_calls(cx, g, &groups)?;
        if let Some(&fail) = groups.iter().find(|&&v| lib.is_failure(h, v)) {
            return Ok(Reply::Done(fail));
        }
    }
    let entries: Vec<T<'h>> = keys.iter().zip(&groups).map(|(&k, &v)| lib.rule(h, k, v)).collect();
    Ok(Reply::Done(from_distinct(lib, h, &keys, &entries)))
}

/// `is_failure(x)`: whether `x` is a failure `failure(...)` (it receives failures as values).
pub(crate) fn is_failure<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    Ok(Reply::Done(cx.bool(lib.is_failure(h, x))))
}

/// `failure_tag(x)`: the tag of a failure `failure(tag, ...)`; anything else (including `failure()`) is
/// `failure(argument, failure_tag, 1)`.
pub(crate) fn failure_tag<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else {
        return Ok(Reply::Decline);
    };
    Ok(Reply::Done(match h.view(x) {
        View::App { head, args: &[tag, ..] } if head == h.sym_term(lib.s.failure) => tag,
        _ => lib.bad_arg(h, t, 1),
    }))
}
