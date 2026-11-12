//! Boolean candidates for `simplify`.
//!
//! - A **propositional** term (`and`, `or`, `not`, `implies`, `xor`, `equivalent`, `nand`, `nor` over symbols and
//!   `true`, `false`) is offered in its minimal sum-of-products or product-of-sums form (`boolean_minimize`), so
//!   `implies(p, implies(q, p))` is `true` and `xor(x, x, y)` is `y`.
//! - A term with **`boole`** of a logical combination, or `min`/`max` of `boole` values, is also offered as the
//!   arithmetic of the 0/1 values (`boole(a and b) = boole(a) boole(b)`, `boole(not a) = 1 - boole(a)`,
//!   `boole(a or b) = 1 - (1 - boole(a))(1 - boole(b))`, `min` of 0/1 values their product, `max` the dual), expanded,
//!   so `boole(a and b) - min(boole(a), boole(b))` is `0`. The candidate is used only if it scores simpler.
use crate::cost::Cost;
use crate::term::{apply, fixed, B};
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

const CONNECTIVES: &[&str] = &["and", "or", "not", "implies", "xor", "equivalent", "nand", "nor"];

/// Is `t` built from the connectives over symbols and `true`/`false` only, with at least one connective?
fn propositional<'h>(b: &B<'h>, t: T<'h>, top: bool) -> bool {
    let h = b.heap();
    match h.view(t) {
        View::Sym(_) => !top,
        View::App { head, args } => match h.view(head) {
            View::Sym(s) if CONNECTIVES.contains(&h.sym_name(s).as_str()) => args.iter().all(|&a| propositional(b, a, false)),
            _ => false,
        },
        _ => false,
    }
}

fn mentions<'h>(b: &B<'h>, t: T<'h>, name: &str) -> bool {
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } => {
            matches!(h.view(head), View::Sym(s) if h.sym_name(s) == name) || args.iter().any(|&a| mentions(b, a, name))
        }
        _ => false,
    }
}

/// `1 - (1 - x1)(1 - x2)...`
fn any_of<'h>(b: &B<'h>, xs: &[T<'h>]) -> T<'h> {
    let ps: Vec<T<'h>> = xs.iter().map(|&x| b.sub(b.int(1), x)).collect();
    b.sub(b.int(1), b.mul(&ps))
}

fn boole_arith<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let boole = |x: T<'h>| b.call_named("boole", &[x]);
    let r = fixed(cx, b, t, 8, &mut |_, b, node| {
        let Some((s, args)) = b.call_of(node) else { return Ok(None) };
        match (h.sym_name(s).as_str(), args) {
            ("boole", [u]) => {
                let Some((g, xs)) = b.call_of(*u) else { return Ok(None) };
                let bs: Vec<T<'h>> = xs.iter().map(|&x| boole(x)).collect();
                Ok(match h.sym_name(g).as_str() {
                    "and" => Some(b.mul(&bs)),
                    "or" => Some(any_of(b, &bs)),
                    "not" if bs.len() == 1 => Some(b.sub(b.int(1), bs[0])),
                    "nand" => Some(b.sub(b.int(1), b.mul(&bs))),
                    "nor" => Some(b.sub(b.int(1), any_of(b, &bs))),
                    // implies(p, q) = not p or q: 1 - p (1 - q)
                    "implies" if bs.len() == 2 => Some(b.sub(b.int(1), b.mul(&[bs[0], b.sub(b.int(1), bs[1])]))),
                    "xor" => Some(parity(b, &bs)),
                    // equivalent: all true or all false
                    "equivalent" if !bs.is_empty() => {
                        let nots: Vec<T<'h>> = bs.iter().map(|&x| b.sub(b.int(1), x)).collect();
                        Some(b.add(&[b.mul(&bs), b.mul(&nots)]))
                    }
                    _ => None,
                })
            }
            // the parity of a sum of 0/1 values: mod(boole(a) + boole(b) + ..., 2)
            ("mod", [s, m]) if *m == b.int(2) => {
                let terms: Vec<T<'h>> = match b.call_of(*s) {
                    Some((g, xs)) if h.sym_name(g) == "add" => xs.to_vec(),
                    _ => vec![*s],
                };
                if terms.iter().all(|&x| b.unary(x, h.sym("boole")).is_some()) {
                    Ok(Some(parity(b, &terms)))
                } else {
                    Ok(None)
                }
            }
            (n @ ("min" | "max"), xs) if xs.len() > 1 => {
                let bs: Vec<T<'h>> = xs.iter().filter_map(|&x| b.unary(x, h.sym("boole"))).collect();
                if bs.len() != xs.len() {
                    return Ok(None);
                }
                let vals: Vec<T<'h>> = bs.iter().map(|&x| boole(x)).collect();
                Ok(Some(if n == "min" { b.mul(&vals) } else { any_of(b, &vals) }))
            }
            _ => Ok(None),
        }
    })?;
    let mut e = apply(cx, b, "expand", r)?;
    // a 0/1 value is its own square: boole(u)^k = boole(u), then expanded again
    for _ in 0..3 {
        let f = fixed(cx, b, e, 4, &mut |_, b, node| {
            let Some((s, args)) = b.call_of(node) else { return Ok(None) };
            match (h.sym_name(s).as_str(), args) {
                ("pow", [base, k]) if b.unary(*base, h.sym("boole")).is_some() && matches!(h.view(*k), View::Int(n) if n >= 2) => Ok(Some(*base)),
                _ => Ok(None),
            }
        })?;
        if f == e {
            break;
        }
        e = apply(cx, b, "expand", f)?;
    }
    Ok(e)
}

/// The parity of 0/1 values: (1 - (1 - 2 x1)(1 - 2 x2)...)/2, 1 when an odd number of them is 1.
fn parity<'h>(b: &B<'h>, xs: &[T<'h>]) -> T<'h> {
    let ps: Vec<T<'h>> = xs.iter().map(|&x| b.sub(b.int(1), b.mul(&[b.int(2), x]))).collect();
    b.mul(&[b.rat(1, 2), b.sub(b.int(1), b.mul(&ps))])
}

/// The simplest of `t` and its boolean candidates.
pub fn improve<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, cost: &mut Cost<'h>, t: T<'h>) -> Res<T<'h>> {
    // a list: element by element
    if let Some((s, xs)) = b.call_of(t) {
        if b.heap().sym_name(s) == "list" {
            let xs = xs.to_vec();
            let mut out = Vec::with_capacity(xs.len());
            for x in xs {
                out.push(improve(cx, b, cost, x)?);
            }
            return Ok(b.heap().app(b.heap().sym_term(s), &out));
        }
    }
    let mut best = t;
    let mut score = None;
    let mut cands: Vec<T<'h>> = vec![];
    if propositional(b, t, true) {
        cands.push(apply(cx, b, "boolean_minimize", t)?);
    }
    if mentions(b, t, "boole") {
        cands.push(boole_arith(cx, b, t)?);
    }
    for c in cands {
        if c == t {
            continue;
        }
        let sc = match &score {
            Some(s) => *s,
            None => cost.score(cx, best)?,
        };
        let sn = cost.score(cx, c)?;
        if sn.simpler_than(&sc) {
            best = c;
            score = Some(sn);
        } else {
            score = Some(sc);
        }
    }
    Ok(best)
}
