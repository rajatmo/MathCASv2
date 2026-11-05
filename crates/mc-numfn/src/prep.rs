//! Preparing a body for numeric evaluation (decision N0.5): `n` evaluates its constants (`pi`, `sqrt(2)`) once, as
//! `mc-roots` always did (decision R.9). `n` would also turn the integer index of `y[2]` into `y[2.0]`, which no
//! longer indexes, so the constant indices of vector variables are first replaced by fresh symbols and put back
//! afterwards.
use crate::{Arg, Prep, Var};
use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Res};
use std::collections::HashMap;

/// `t` with every subterm `u` for which `f(u)` is `Some(v)` replaced by `v` (not looking inside `u`); shared
/// subterms are visited once, and the walk uses an explicit stack (the structure is user data).
pub fn replace<'h>(h: &'h Heap, t: T<'h>, f: &mut dyn FnMut(T<'h>) -> Option<T<'h>>) -> T<'h> {
    let mut memo: HashMap<T<'h>, T<'h>> = HashMap::new();
    let mut stack: Vec<(T<'h>, bool)> = vec![(t, false)];
    while let Some((u, expanded)) = stack.pop() {
        if !expanded {
            if memo.contains_key(&u) {
                continue;
            }
            if let Some(v) = f(u) {
                memo.insert(u, v);
                continue;
            }
        }
        match h.view(u) {
            View::App { head, args } => {
                if expanded {
                    let nh = memo[&head];
                    let na: Vec<T<'h>> = args.iter().map(|a| memo[a]).collect();
                    let same = nh == head && na.iter().zip(args).all(|(a, b)| a == b);
                    memo.insert(u, if same { u } else { h.app(nh, &na) });
                } else {
                    stack.push((u, true));
                    stack.push((head, false));
                    stack.extend(args.iter().map(|&a| (a, false)));
                }
            }
            _ => {
                memo.insert(u, u);
            }
        }
    }
    memo[&t]
}

/// `t` with the numerically stable forms of two cancelling patterns, bottom up: `log(1 + u)` becomes `log1p(u)`
/// and a sum `exp(u) - 1 + rest` becomes `expm1(u) + rest`. Both forms are exact identities; in machine arithmetic
/// they avoid the cancellation of `1 + u` (u small) and `e^u - 1` (u near 0), e.g. in `log(1 - exp(-r))` for large
/// r. Explicit stack, shared subterms once.
pub fn stable_forms<'h>(h: &'h Heap, t: T<'h>) -> T<'h> {
    let (log, add, exp) = (h.sym_term(h.sym("log")), h.sym_term(h.sym(names::ADD)), h.sym_term(h.sym("exp")));
    let (log1p, expm1) = (h.sym("log1p"), h.sym("expm1"));
    let (one, minus_one) = (h.int(1), h.int(-1));
    let local = |u: T<'h>| -> T<'h> {
        let View::App { head, args } = h.view(u) else { return u };
        // log(1 + rest) -> log1p(rest)
        if head == log && args.len() == 1 {
            if let View::App { head: ah, args: terms } = h.view(args[0]) {
                if ah == add {
                    if let Some(i) = terms.iter().position(|&a| a == one) {
                        let rest: Vec<T<'h>> = terms.iter().enumerate().filter(|&(k, _)| k != i).map(|(_, &a)| a).collect();
                        let inner = if rest.len() == 1 { rest[0] } else { h.app(add, &rest) };
                        return h.call(log1p, &[inner]);
                    }
                }
            }
        }
        // exp(w) - 1 + rest -> expm1(w) + rest
        if head == add {
            let m = args.iter().position(|&a| a == minus_one);
            let e = args.iter().position(|&a| matches!(h.view(a), View::App { head, args: [_] } if head == exp));
            if let (Some(m), Some(e)) = (m, e) {
                let View::App { args: [w], .. } = h.view(args[e]) else { return u };
                let mut rest: Vec<T<'h>> = vec![h.call(expm1, &[*w])];
                rest.extend(args.iter().enumerate().filter(|&(k, _)| k != m && k != e).map(|(_, &a)| a));
                return if rest.len() == 1 { rest[0] } else { h.app(add, &rest) };
            }
        }
        u
    };
    let mut memo: HashMap<T<'h>, T<'h>> = HashMap::new();
    let mut stack: Vec<(T<'h>, bool)> = vec![(t, false)];
    while let Some((u, expanded)) = stack.pop() {
        if !expanded && memo.contains_key(&u) {
            continue;
        }
        match h.view(u) {
            View::App { head, args } if h.packed(u).is_none() => {
                if expanded {
                    let nh = memo[&head];
                    let na: Vec<T<'h>> = args.iter().map(|a| memo[a]).collect();
                    let same = nh == head && na.iter().zip(args).all(|(a, b)| a == b);
                    let rebuilt = if same { u } else { h.app(nh, &na) };
                    memo.insert(u, local(rebuilt));
                } else {
                    stack.push((u, true));
                    stack.push((head, false));
                    stack.extend(args.iter().map(|&a| (a, false)));
                }
            }
            _ => {
                memo.insert(u, u);
            }
        }
    }
    memo[&t]
}

fn is_failure<'h>(h: &'h Heap, t: T<'h>) -> bool {
    matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym(names::FAILURE)))
}

/// `expr` prepared as `prep` says for evaluation in `vars`: through `n` (machine precision or `d` digits), with
/// the constant indices `y[k]` of vector variables kept (and normalised to `1..=n`). A failure of `n` leaves the
/// expression as it is.
pub fn prepare<'h>(cx: &mut NativeCx<'_, '_, 'h>, expr: T<'h>, vars: &[Var], prep: Prep) -> Res<T<'h>> {
    Ok(prepared(cx, expr, vars, prep)?.unwrap_or(expr))
}

/// [`prepare`], or `None` when the body stays as it is (`Prep::None`, or `n` failed).
pub(crate) fn prepared<'h>(cx: &mut NativeCx<'_, '_, 'h>, expr: T<'h>, vars: &[Var], prep: Prep) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let call = match prep {
        Prep::None => return Ok(None),
        Prep::Machine => None,
        Prep::Digits(d) => Some(h.int(d.min(i64::MAX as u64) as i64)),
    };
    let part = h.sym_term(h.sym("part"));
    let length = h.sym_term(h.sym("length"));
    let vectors: Vec<(T<'h>, usize, String)> = vars
        .iter()
        .filter_map(|v| match v.arg {
            Arg::Vector(n) => Some((h.sym_term(v.sym), n, h.sym_name(v.sym))),
            Arg::Scalar => None,
        })
        .collect();
    // y[k] -> a fresh symbol per (y, k)
    let mut fresh: Vec<(T<'h>, T<'h>)> = vec![];
    let hidden = if vectors.is_empty() {
        expr
    } else {
        replace(h, expr, &mut |u| {
            // length(y) is the vector's length
            if let View::App { head, args: [y] } = h.view(u) {
                if head == length {
                    return vectors.iter().find(|v| v.0 == *y).map(|v| h.int(v.1 as i64));
                }
            }
            let View::App { head, args: [y, k] } = h.view(u) else { return None };
            if head != part {
                return None;
            }
            let (_, n, name) = vectors.iter().find(|v| v.0 == *y)?;
            let View::Int(k) = h.view(*k) else { return None };
            let n = *n as i64;
            let k = match k {
                1.. if k <= n => k,
                ..=-1 if -k <= n => n + 1 + k,
                _ => return None,
            };
            let s = h.sym_term(h.sym(&format!("numfn·{name}·{k}")));
            let back = h.app(part, &[*y, h.int(k)]);
            if !fresh.iter().any(|f| f.0 == s) {
                fresh.push((s, back));
            }
            Some(s)
        })
    };
    let hidden = stable_forms(h, hidden);
    let n = h.call(h.sym("n"), &match call {
        None => vec![hidden],
        Some(d) => vec![hidden, d],
    });
    let v = cx.eval(n)?;
    if is_failure(h, v) {
        return Ok(None);
    }
    if fresh.is_empty() {
        return Ok(Some(v));
    }
    Ok(Some(replace(h, v, &mut |u| fresh.iter().find(|f| f.0 == u).map(|f| f.1))))
}
