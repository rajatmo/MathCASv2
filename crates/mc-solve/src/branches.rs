//! Equations with a piecewise function of the unknown (`piecewise`, `if`, `unit_step`) split into their branches:
//! `f(piecewise([[v1, c1], [v2, c2]], d)) == 0` holds exactly when `c1 and f(v1) == 0`, or `not c1 and c2 and
//! f(v2) == 0`, or `not c1 and not c2 and f(d) == 0` (a piecewise function takes the value of its first branch whose
//! condition holds). `solve`, `reduce` and `n_solve` try this when their other methods leave such an equation
//! unsolved; the branches are solved by the same function, one piecewise subterm removed at each step.
use crate::tails::{args_of, contains, head_name};
use mc_api::{names, Store, View, T};
use mc_core::Heap;
use std::cell::Cell;
use mc_eval::{NativeCx, Res};

/// The first subterm of `t` that is a piecewise function of `x` (by its head), depth first.
fn piecewise_term<'h>(h: &'h Heap, t: T<'h>, x: T<'h>) -> Option<T<'h>> {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !contains(h, u, x) {
            continue;
        }
        if matches!(head_name(h, u).as_deref(), Some("piecewise" | "if" | "unit_step")) {
            return Some(u);
        }
        if let View::App { args, .. } = h.view(u) {
            stack.extend(args.iter().rev());
        }
    }
    None
}

/// The answer of `name(cond, rest...)` (`solve`, `n_solve` or `reduce`) with the first piecewise subterm of the
/// condition split into its branches, each case solved by `name` and the answers joined; `None` when the call has
/// none, it does not expand to `piecewise([[v, c], ...], d)` with conditions free of piecewise functions, or a case
/// stays unsolved.
pub(crate) fn split<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h: &'h Heap = cx.heap();
    let is_sym = |t: T<'h>, n: &str| matches!(h.view(t), View::Sym(s) if h.sym_name(s) == n);
    let (cond, xs) = match args {
        [c, x] => (*c, *x),
        [c, x, d] if is_sym(*d, "reals") => (*c, *x),
        _ => return Ok(None),
    };
    let x = match cx.as_list(xs) {
        Some(&[x]) => x,
        Some(_) => return Ok(None),
        None => xs,
    };
    if !matches!(h.view(x), View::Sym(_)) {
        return Ok(None);
    }
    if !crate::realline::small(h, cond, 2_000) {
        return Ok(None);
    }
    let Some(p) = piecewise_term(h, cond, x) else { return Ok(None) };
    let pe = cx.eval(h.call(h.sym("piecewise_expand"), &[p]))?;
    if head_name(h, pe).as_deref() != Some("piecewise") {
        return Ok(None);
    }
    let (list, default) = match args_of(h, pe) {
        [l] => (*l, h.int(0)),
        [l, d] => (*l, *d),
        _ => return Ok(None),
    };
    let Some(branches) = cx.as_list(list).map(<[T<'h>]>::to_vec) else { return Ok(None) };
    let mut pairs = vec![];
    for b in branches {
        let Some(&[v, c]) = cx.as_list(b) else { return Ok(None) };
        // the conditions are in x alone and free of piecewise functions themselves
        if piecewise_term(h, c, x).is_some() {
            return Ok(None);
        }
        pairs.push((v, c));
    }
    if pairs.is_empty() || DEPTH.with(|d| d.get()) >= MAX_DEPTH {
        return Ok(None);
    }
    let not = |c: T<'h>| h.call(h.sym("not"), &[c]);
    let mut answers = vec![];
    DEPTH.with(|d| d.set(d.get() + 1));
    let mut result = Ok(());
    for k in 0..=pairs.len() {
        let value = if k < pairs.len() { pairs[k].0 } else { default };
        let body = replace(h, cond, p, value);
        let mut conj: Vec<T<'h>> = pairs[..k].iter().map(|q| not(q.1)).collect();
        if k < pairs.len() {
            conj.push(pairs[k].1);
        }
        conj.push(body);
        let case = match cx.eval(h.call(h.sym("and"), &conj)) {
            Ok(c) => c,
            Err(e) => {
                result = Err(e);
                break;
            }
        };
        // a case whose condition is false at once has no solutions
        if matches!(h.view(case), View::Sym(s) if h.sym_name(s) == "false") {
            continue;
        }
        let mut a = vec![case];
        a.extend_from_slice(&args[1..]);
        match cx.eval(h.call(h.sym(name), &a)) {
            Ok(v) => answers.push(v),
            Err(e) => {
                result = Err(e);
                break;
            }
        }
    }
    DEPTH.with(|d| d.set(d.get() - 1));
    result?;
    // every case answered (no failure, no unevaluated call): their union
    let own = h.sym_term(h.sym(name));
    let mut items = vec![];
    for &v in &answers {
        if head_name(h, v).as_deref() == Some(names::FAILURE) || matches!(h.view(v), View::App { head, .. } if head == own) {
            return Ok(None);
        }
        if name == "reduce" {
            items.push(v);
        } else {
            let Some(list) = cx.as_list(v) else { return Ok(None) };
            items.extend_from_slice(list);
        }
    }
    if name == "reduce" {
        // equations x == v with real values: in increasing order, each once
        let mut flat = vec![];
        for &it in &items {
            if matches!(h.view(it), View::Sym(s) if h.sym_name(s) == "false") {
                continue;
            }
            match head_name(h, it).as_deref() {
                Some("or") => flat.extend_from_slice(args_of(h, it)),
                _ => flat.push(it),
            }
        }
        let mut keyed = vec![];
        for &e in &flat {
            let value = match (head_name(h, e).as_deref(), args_of(h, e)) {
                (Some("equal"), &[l, v]) if l == x => {
                    let nv = cx.eval(h.call(h.sym("n"), &[v]))?;
                    match h.view(nv) {
                        View::Real(f) => Some(f),
                        View::Int(k) => Some(k as f64),
                        _ => None,
                    }
                }
                _ => None,
            };
            keyed.push((value, e));
        }
        if keyed.iter().all(|k| k.0.is_some()) {
            keyed.sort_by(|a, b| a.0.unwrap_or(0.0).total_cmp(&b.0.unwrap_or(0.0)));
            keyed.dedup_by(|a, b| a.1 == b.1);
            flat = keyed.into_iter().map(|k| k.1).collect();
        }
        let all = match flat.len() {
            0 => h.sym_term(h.sym("false")),
            1 => flat[0],
            _ => h.call(h.sym("or"), &flat),
        };
        return Ok(Some(cx.eval(all)?));
    }
    // solutions [x -> v] with real values: in increasing order, each once
    let mut keyed = vec![];
    for &s in &items {
        let value = match cx.as_list(s) {
            Some(&[rule]) => match args_of(h, rule) {
                &[l, v] if l == x => {
                    let nv = cx.eval(h.call(h.sym("n"), &[v]))?;
                    match h.view(nv) {
                        View::Real(f) => Some(f),
                        View::Int(k) => Some(k as f64),
                        _ => None,
                    }
                }
                _ => None,
            },
            _ => None,
        };
        keyed.push((value, s));
    }
    if keyed.iter().all(|k| k.0.is_some()) {
        keyed.sort_by(|a, b| a.0.unwrap_or(0.0).total_cmp(&b.0.unwrap_or(0.0)));
        keyed.dedup_by(|a, b| a.1 == b.1);
        items = keyed.into_iter().map(|k| k.1).collect();
    }
    Ok(Some(cx.list(&items)))
}

/// The most piecewise subterms split in nested calls.
const MAX_DEPTH: u32 = 4;

thread_local! {
    /// The splits in progress, nested.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// `t` with every occurrence of the subterm `p` replaced by `v` (structurally, unevaluated).
fn replace<'h>(h: &'h Heap, t: T<'h>, p: T<'h>, v: T<'h>) -> T<'h> {
    if t == p {
        return v;
    }
    match h.view(t) {
        View::App { head, args } if args.iter().any(|&a| contains(h, a, p)) => {
            let new: Vec<T<'h>> = args.iter().map(|&a| replace(h, a, p, v)).collect();
            h.app(head, &new)
        }
        _ => t,
    }
}
