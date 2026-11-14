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
