//! Argument forms of `reduce` that come down to the forms the other paths take:
//!
//! - `reduce(cond)`: a condition that is already `true` or `false` is the answer; otherwise the variables are the
//!   symbols of the condition (the constants, the heads and the variables bound by `exists` and `for_all` left out),
//!   in the order of their names.
//! - `reduce(cond, element(x, dom))`: the variable and its domain, `reduce(cond, x, dom)`.
//! - `reduce(cond, xs, primes)`: the integer solutions with every variable at least 2, kept when every value is a
//!   prime; only when the integer solutions are a finite list.
//! - `reduce(eqs, x, algebraics)`: the complex solutions of equations in one unknown, kept when the value is an
//!   algebraic number (`element(v, algebraics)` decided for each; undecided gives no answer).
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The name of a symbol, or of the head of a call with a symbol head.
fn name<'h>(h: &'h impl Store, t: T<'h>) -> Option<String> {
    match h.view(t) {
        View::Sym(s) => Some(h.sym_name(s).to_string()),
        View::App { head, .. } => match h.view(head) {
            View::Sym(s) => Some(h.sym_name(s).to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// Symbols that are constants or domains, never variables of a condition.
const CONSTANTS: &[&str] = &[
    "pi", "e", "im", "inf", "true", "false", "euler_gamma", "catalan", "golden_ratio", "degree", "khinchin", "glaisher",
    "complex_infinity", "indeterminate", "reals", "integers", "complexes", "rationals", "algebraics", "primes", "booleans",
    "null", "all", "none", "automatic",
];

/// The variables of a condition: its symbols outside head positions, without constants and bound variables.
fn free_symbols<'h>(h: &'h impl Store, t: T<'h>, bound: &mut Vec<T<'h>>, out: &mut Vec<T<'h>>) {
    match h.view(t) {
        View::Sym(s) => {
            let n = h.sym_name(s);
            if !CONSTANTS.contains(&n.as_str()) && !bound.contains(&t) && !out.contains(&t) {
                out.push(t);
            }
        }
        View::App { head, args } => {
            let quantifier = matches!(name(h, t).as_deref(), Some("exists" | "for_all"));
            if quantifier && !args.is_empty() {
                let k = bound.len();
                match h.view(args[0]) {
                    View::App { args: vs, .. } if name(h, args[0]).as_deref() == Some(names::LIST) => bound.extend_from_slice(vs),
                    _ => bound.push(args[0]),
                }
                for &a in &args[1..] {
                    free_symbols(h, a, bound, out);
                }
                bound.truncate(k);
                return;
            }
            if !matches!(h.view(head), View::Sym(_)) {
                free_symbols(h, head, bound, out);
            }
            for &a in args {
                free_symbols(h, a, bound, out);
            }
        }
        _ => {}
    }
}

/// The disjuncts of `t` (`t` itself when it is no `or`); `false` has none.
fn disjuncts<'h>(h: &'h impl Store, t: T<'h>) -> Vec<T<'h>> {
    if t == h.sym_term(h.sym(names::FALSE)) {
        return vec![];
    }
    match h.view(t) {
        View::App { args, .. } if name(h, t).as_deref() == Some("or") => args.to_vec(),
        _ => vec![t],
    }
}

/// The conjuncts of `t` (`t` itself when it is no `and`).
fn conjuncts<'h>(h: &'h impl Store, t: T<'h>) -> Vec<T<'h>> {
    match h.view(t) {
        View::App { args, .. } if name(h, t).as_deref() == Some("and") => args.to_vec(),
        _ => vec![t],
    }
}

/// `x == v` as `(x, v)`.
fn equation<'h>(h: &'h impl Store, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
    match h.view(t) {
        View::App { args: &[a, b], .. } if name(h, t).as_deref() == Some("equal") => Some((a, b)),
        _ => None,
    }
}

fn or_of<'h>(h: &'h impl Store, parts: Vec<T<'h>>) -> T<'h> {
    match parts.len() {
        0 => h.sym_term(h.sym(names::FALSE)),
        1 => parts[0],
        _ => h.call(h.sym("or"), &parts),
    }
}

/// The value of a call of `reduce` in a form the other paths take; `None` when it gives no answer.
fn answered<'h>(cx: &mut NativeCx<'_, '_, 'h>, call: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let r = cx.eval(call)?;
    Ok((!matches!(name(h, r).as_deref(), Some("reduce" | names::FAILURE))).then_some(r))
}

/// The forms above; `None` leaves the call to the other paths.
pub(crate) fn forms<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    match args {
        [c] => {
            if *c == h.sym_term(h.sym(names::TRUE)) || *c == h.sym_term(h.sym(names::FALSE)) {
                return Ok(Some(*c));
            }
            // a failure or an unevaluated call inside is no condition
            if matches!(name(h, *c).as_deref(), Some(names::FAILURE)) {
                return Ok(None);
            }
            let mut vars = vec![];
            free_symbols(h, *c, &mut vec![], &mut vars);
            if vars.is_empty() {
                return Ok(None);
            }
            vars.sort_by_key(|&v| name(h, v).unwrap_or_default());
            let xs = if vars.len() == 1 { vars[0] } else { cx.list(&vars) };
            answered(cx, h.call(h.sym("reduce"), &[*c, xs]))
        }
        [c, xs] if name(h, *xs).as_deref() != Some("element") => {
            // conjuncts `element(x, dom)` for every variable, one domain: reduce(rest, xs, dom)
            let vars: Vec<T<'h>> = match cx.as_list(*xs) {
                Some(v) => v.to_vec(),
                None => vec![*xs],
            };
            let mut rest = vec![];
            let mut dom: Option<T<'h>> = None;
            let mut covered = vec![];
            for p in conjuncts(h, *c) {
                match h.view(p) {
                    View::App { args: &[v, d], .. }
                        if name(h, p).as_deref() == Some("element")
                            && vars.contains(&v)
                            && matches!(name(h, d).as_deref(), Some("integers" | "reals"))
                            && dom.is_none_or(|e| e == d) =>
                    {
                        dom = Some(d);
                        covered.push(v);
                    }
                    _ => rest.push(p),
                }
            }
            let Some(dom) = dom else { return Ok(None) };
            if !vars.iter().all(|v| covered.contains(v)) {
                return Ok(None);
            }
            let rest = match rest.as_slice() {
                [] => h.sym_term(h.sym(names::TRUE)),
                [p] => *p,
                _ => h.call(h.sym("and"), &rest),
            };
            answered(cx, h.call(h.sym("reduce"), &[rest, *xs, dom]))
        }
        [c, d] if name(h, *d).as_deref() == Some("element") => {
            let View::App { args: &[x, dom], .. } = h.view(*d) else { return Ok(None) };
            if !matches!(h.view(dom), View::Sym(_)) {
                return Ok(None);
            }
            answered(cx, h.call(h.sym("reduce"), &[*c, x, dom]))
        }
        [c, xs, d] if name(h, *d).as_deref() == Some("primes") => primes(cx, *c, *xs),
        [c, x, d] if name(h, *d).as_deref() == Some("algebraics") => algebraics(cx, *c, *x),
        _ => Ok(None),
    }
}

/// The most integers of one bounded interval listed one by one; a longer run is written as its bounds.
const MAX_LISTED: i64 = 1000;

/// An end of an interval of the real solution set: the bound and whether it is included.
type End<'h> = Option<(T<'h>, bool)>;

/// The pieces of a real solution set in `x` (`reduce`'s form): `(lower, upper)` per interval, a point as a closed
/// interval of one value; `None` for any other form.
fn real_pieces<'h>(h: &'h impl Store, r: T<'h>, x: T<'h>) -> Option<Vec<(End<'h>, End<'h>)>> {
    let mut out = vec![];
    for d in disjuncts(h, r) {
        if d == h.sym_term(h.sym(names::TRUE)) {
            out.push((None, None));
            continue;
        }
        let (mut lo, mut hi): (End<'h>, End<'h>) = (None, None);
        for p in conjuncts(h, d) {
            let View::App { args: &[a, b], .. } = h.view(p) else { return None };
            let rel = name(h, p)?;
            match (rel.as_str(), a == x, b == x) {
                ("equal", true, false) => {
                    lo = Some((b, true));
                    hi = Some((b, true));
                }
                ("less", true, false) => hi = Some((b, false)),
                ("less_equal", true, false) => hi = Some((b, true)),
                ("greater", true, false) => lo = Some((b, false)),
                ("greater_equal", true, false) => lo = Some((b, true)),
                ("less", false, true) => lo = Some((a, false)),
                ("less_equal", false, true) => lo = Some((a, true)),
                ("greater", false, true) => hi = Some((a, false)),
                ("greater_equal", false, true) => hi = Some((a, true)),
                _ => return None,
            }
        }
        out.push((lo, hi));
    }
    Some(out)
}

/// `reduce(cond, x, integers)` in one variable without parameters: the integers of the real solution set, each
/// bounded run of at most [`MAX_LISTED`] integers as `x == k`, a longer or unbounded run as `element(x, integers)`
/// with its integer bounds. `None` when the real solution set is not found or has parameters.
pub(crate) fn integers_one<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let [c, x, d] = args else { return Ok(None) };
    if name(h, *d).as_deref() != Some("integers") || !matches!(h.view(*x), View::Sym(_)) {
        return Ok(None);
    }
    let mut syms = vec![];
    free_symbols(h, *c, &mut vec![], &mut syms);
    if syms.iter().any(|s| s != x) {
        return Ok(None);
    }
    let r = cx.eval(h.call(h.sym("reduce"), &[*c, *x, h.sym_term(h.sym("reals"))]))?;
    if matches!(name(h, r).as_deref(), Some("reduce" | names::FAILURE)) {
        return Ok(None);
    }
    // the symbols of the answer outside root objects (whose polynomials are functions of a slot)
    let mut stack = mc_api::walk::Walk::new(r);
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(_) => {
                let mut syms = vec![];
                free_symbols(h, u, &mut vec![], &mut syms);
                if syms.iter().any(|s| s != x) {
                    return Ok(None);
                }
            }
            View::App { args, .. } if name(h, u).as_deref() != Some("root") => stack.extend_from_slice(args),
            _ => {}
        }
    }
    let Some(pieces) = real_pieces(h, r, *x) else { return Ok(None) };
    // an integer bound of an end: the least integer >= lo (> lo when open), the greatest <= hi (< hi when open)
    let int_end = |cx: &mut NativeCx<'_, '_, 'h>, e: (T<'h>, bool), upper: bool| -> Res<Option<i64>> {
        let (b, closed) = e;
        let name = if upper { "floor" } else { "ceiling" };
        let f = cx.eval(h.call(h.sym(name), &[b]))?;
        if !matches!(h.view(f), View::Int(_)) {
            // a root object or another algebraic number: the floor of a 40-digit value, kept when the exact
            // comparisons k < b < k + 1 (k - 1 < b < k for the ceiling) hold, so b is no integer
            let v = cx.eval(h.call(h.sym("n"), &[b, h.int(40)]))?;
            let g = cx.eval(h.call(h.sym(name), &[v]))?;
            let View::Int(k) = h.view(g) else { return Ok(None) };
            let (lo, hi) = if upper { (k, k + 1) } else { (k - 1, k) };
            let a = cx.eval(h.call(h.sym("less"), &[h.int(lo), b]))?;
            let c = cx.eval(h.call(h.sym("less"), &[b, h.int(hi)]))?;
            if !cx.is_true(a) || !cx.is_true(c) {
                return Ok(None);
            }
            return Ok(Some(k));
        }
        let View::Int(k) = h.view(f) else { return Ok(None) };
        let mut is_int = cx.eval(h.call(h.sym("equal"), &[f, b]))?;
        if !cx.is_true(is_int) && !cx.is_false(is_int) {
            // an equation with a root object that stays: b differs from k when it lies strictly on its side
            let strict = if upper { [f, b] } else { [b, f] };
            let s = cx.eval(h.call(h.sym("less"), &strict))?;
            if !cx.is_true(s) {
                return Ok(None);
            }
            is_int = cx.bool(false);
        }
        let step = if upper { -1 } else { 1 };
        Ok(if cx.is_true(is_int) && !closed { k.checked_add(step) } else { Some(k) })
    };
    let elem = h.call(h.sym("element"), &[*x, h.sym_term(h.sym("integers"))]);
    let mut parts = vec![];
    for (lo, hi) in pieces {
        let lo = match lo {
            Some(e) => match int_end(cx, e, false)? {
                Some(k) => Some(k),
                None => return Ok(None),
            },
            None => None,
        };
        let hi = match hi {
            Some(e) => match int_end(cx, e, true)? {
                Some(k) => Some(k),
                None => return Ok(None),
            },
            None => None,
        };
        match (lo, hi) {
            (Some(a), Some(b)) => {
                if a > b {
                    continue;
                }
                if b.checked_sub(a).is_some_and(|n| n < MAX_LISTED) {
                    for k in a..=b {
                        cx.tick(1)?;
                        parts.push(h.call(h.sym("equal"), &[*x, h.int(k)]));
                    }
                } else {
                    let l = h.call(h.sym("less_equal"), &[h.int(a), *x]);
                    let u = h.call(h.sym("less_equal"), &[*x, h.int(b)]);
                    parts.push(h.call(h.sym("and"), &[elem, l, u]));
                }
            }
            (Some(a), None) => parts.push(h.call(h.sym("and"), &[elem, h.call(h.sym("greater_equal"), &[*x, h.int(a)])])),
            (None, Some(b)) => parts.push(h.call(h.sym("and"), &[elem, h.call(h.sym("less_equal"), &[*x, h.int(b)])])),
            (None, None) => parts.push(elem),
        }
    }
    Ok(Some(or_of(h, parts)))
}

/// `reduce(cond, xs, primes)`: the finite list of integer solutions with every variable >= 2, the points whose
/// coordinates are all prime.
fn primes<'h>(cx: &mut NativeCx<'_, '_, 'h>, cond: T<'h>, xs: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let vars: Vec<T<'h>> = match cx.as_list(xs) {
        Some(v) => v.to_vec(),
        None => vec![xs],
    };
    if vars.is_empty() || !vars.iter().all(|&v| matches!(h.view(v), View::Sym(_))) {
        return Ok(None);
    }
    let mut parts = vec![cond];
    parts.extend(vars.iter().map(|&v| h.call(h.sym("greater_equal"), &[v, h.int(2)])));
    let r = cx.eval(h.call(h.sym("reduce"), &[h.call(h.sym("and"), &parts), xs, h.sym_term(h.sym("integers"))]))?;
    let mut kept = vec![];
    for d in disjuncts(h, r) {
        let eqs = conjuncts(h, d);
        if eqs.len() != vars.len() {
            return Ok(None);
        }
        let mut all_prime = true;
        for &v in &vars {
            let Some((_, val)) = eqs.iter().filter_map(|&e| equation(h, e)).find(|&(a, _)| a == v) else { return Ok(None) };
            if !matches!(h.view(val), View::Int(_) | View::BigInt { .. }) {
                return Ok(None);
            }
            let p = cx.eval(h.call(h.sym("is_prime"), &[val]))?;
            if !cx.is_true(p) {
                all_prime = false;
            }
        }
        if all_prime {
            kept.push(d);
        }
    }
    Ok(Some(or_of(h, kept)))
}

/// `reduce(eqs, x, algebraics)`: the complex solutions of the equations whose values are algebraic numbers.
fn algebraics<'h>(cx: &mut NativeCx<'_, '_, 'h>, cond: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if !matches!(h.view(x), View::Sym(_)) || !conjuncts(h, cond).iter().all(|&p| name(h, p).as_deref() == Some("equal")) {
        return Ok(None);
    }
    let r = cx.eval(h.call(h.sym("reduce"), &[cond, x, h.sym_term(h.sym("complexes"))]))?;
    let mut kept = vec![];
    for d in disjuncts(h, r) {
        let Some((a, v)) = equation(h, d) else { return Ok(None) };
        if a != x {
            return Ok(None);
        }
        let e = cx.eval(h.call(h.sym("element"), &[v, h.sym_term(h.sym("algebraics"))]))?;
        if cx.is_true(e) {
            kept.push(d);
        } else if !cx.is_false(e) {
            return Ok(None);
        }
    }
    Ok(Some(or_of(h, kept)))
}
