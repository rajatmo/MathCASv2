//! The form of `function_domain`'s conditions (decision R25P.12).
//!
//! - `cos(u) != 0` is `not_element(1/2 + u/pi, integers)`, `sin(u) != 0` is `not_element(u/pi, integers)`,
//!   `cosh(u) != 0` is `not_element(1/2 - im u/pi, integers)` and `sinh(u) != 0` is `not_element(-im u/pi,
//!   integers)`: the excluded points as a lattice; integer terms of a `not_element` argument are dropped and its
//!   sign made positive (`not_element(1 - x^2, integers)` is `not_element(x^2, integers)`).
//! - The `not_element` conditions come first.
//! - One complex variable with polynomial conditions (`abs(z) < 1` read as `re(z)^2 + im_part(z)^2 < 1`): the
//!   region over the real and imaginary parts by `reduce` (the cylindrical form), written back in `re(z)` and
//!   `im_part(z)`.
//! - A disjunction in one real variable: each relation reduced on its own.
use super::sets;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// Rewrite the conditions in place; `Some(answer)` when the complex region was reduced.
pub fn finish<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, cs: &mut Vec<T<'h>>, vars: &[T<'h>], complex: bool) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let ordering = |t: T<'h>| {
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if let View::App { head, args } = h.view(u) {
                if b.sym_of(head).is_some_and(|s| matches!(h.sym_name(s).as_str(), "less" | "less_equal" | "greater" | "greater_equal")) {
                    return true;
                }
                stack.extend(args.iter().copied());
            }
        }
        false
    };
    // a region (an ordering among the conditions): over the real and imaginary parts
    if complex && vars.len() == 1 && cs.iter().any(|&c| ordering(c)) {
        let z = vars[0];
        let mut parts = vec![];
        for &c in cs.iter() {
            parts.push(abs_squared(cx, b, c)?);
        }
        let all = match parts.as_slice() {
            [p] => *p,
            _ => h.call(h.sym("and"), &parts),
        };
        let (rv, rf, _) = sets::realify(cx, b, all, &[z], &[])?;
        let rf = cx.eval(rf)?;
        if sets::polynomial_formula(b, rf, &rv) {
            let l = cx.list(&rv);
            let r = cx.eval(b.call("reduce", &[rf, l, h.sym_term(h.sym("reals"))]))?;
            if !sets::is_head(b, r, "reduce") && !sets::is_head(b, r, names::FAILURE) && !cx.is_false(r) {
                let back = cx.list(&[
                    h.call(h.sym(names::RULE), &[rv[0], b.call("re", &[z])]),
                    h.call(h.sym(names::RULE), &[rv[1], b.call("im_part", &[z])]),
                ]);
                return Ok(Some(cx.eval(h.call(h.sym("rewrite"), &[r, back]))?));
            }
        }
    }
    let mut out = vec![];
    for &c in cs.iter() {
        out.push(lattice(cx, b, c)?);
    }
    if !complex && vars.len() == 1 {
        for o in out.iter_mut() {
            *o = reduce_disjuncts(cx, b, *o, vars[0])?;
        }
    }
    // not_element first
    let (mut ne, rest): (Vec<T<'h>>, Vec<T<'h>>) = out.into_iter().partition(|&c| sets::is_head(b, c, "not_element"));
    ne.extend(rest);
    *cs = ne;
    Ok(None)
}

/// `abs(u) op r` as `re(u)^2 + im_part(u)^2 op r^2` for a nonnegative number `r`.
fn abs_squared<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    if let View::App { head, args: &[l, r] } = h.view(c) {
        let rel = b.sym_of(head).is_some_and(|s| matches!(h.sym_name(s).as_str(), "less" | "less_equal" | "greater" | "greater_equal"));
        if rel && sets::is_head(b, l, "abs") && b.ar.num(r).is_some() {
            let View::App { args: &[u], .. } = h.view(l) else { return Ok(c) };
            let sq = b.add(&[b.powi(b.call("re", &[u]), 2), b.powi(b.call("im_part", &[u]), 2)]);
            return cx.eval(h.app(head, &[sq, b.powi(r, 2)]));
        }
    }
    Ok(c)
}

/// The lattice forms of the module comment.
fn lattice<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let ints = h.sym_term(h.sym("integers"));
    let pi = h.sym_term(h.sym("pi"));
    let imu = h.sym_term(h.sym("im"));
    if let View::App { head, args: &[l, r] } = h.view(c) {
        if b.sym_of(head) == Some(h.sym("unequal")) && r == b.zero() {
            if let View::App { head: fh, args: &[u] } = h.view(l) {
                let arg = match b.sym_of(fh).map(|s| h.sym_name(s).to_string()).as_deref() {
                    Some("cos") => Some(b.add(&[b.rat(1, 2), b.div(u, pi)])),
                    Some("sin") => Some(b.div(u, pi)),
                    Some("cosh") => Some(b.sub(b.rat(1, 2), b.div(b.mul(&[imu, u]), pi))),
                    Some("sinh") => Some(b.neg(b.div(b.mul(&[imu, u]), pi))),
                    _ => None,
                };
                if let Some(a) = arg {
                    let a = cx.eval(a)?;
                    return cx.eval(b.call("not_element", &[a, ints]));
                }
            }
        }
    }
    if let View::App { head, args } = h.view(c) {
        if b.sym_of(head) == Some(h.sym("not_element")) && args.len() == 2 && args[1] == ints {
            let a = integer_shift(cx, b, args[0])?;
            return cx.eval(b.call("not_element", &[a, ints]));
        }
        if b.sym_of(head) == Some(h.sym("or")) {
            let mut v = vec![];
            for &a in args {
                v.push(lattice(cx, b, a)?);
            }
            return cx.eval(h.app(head, &v));
        }
    }
    Ok(c)
}

/// `e` without integer terms, with a positive sign (`1 - x^2` is `x^2`); the set of integers is unchanged.
fn integer_shift<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, e: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let mut e = e;
    if let View::App { head, args } = h.view(e) {
        if b.sym_of(head) == Some(b.hd.add) {
            let rest: Vec<T<'h>> = args.iter().copied().filter(|&a| !matches!(h.view(a), View::Int(_))).collect();
            if rest.len() < args.len() && !rest.is_empty() {
                e = cx.eval(b.add(&rest))?;
            }
        }
    }
    // a leading -1 factor
    if let View::App { head, args } = h.view(e) {
        if b.sym_of(head) == Some(b.hd.mul) && args.first().is_some_and(|&a| a == b.int(-1)) {
            return cx.eval(b.neg(e));
        }
    }
    Ok(e)
}

/// A disjunction in `x`: each relation reduced on its own.
fn reduce_disjuncts<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>, x: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let View::App { head, args } = h.view(c) else { return Ok(c) };
    if b.sym_of(head) != Some(h.sym("or")) {
        return Ok(c);
    }
    let mut v = vec![];
    for &a in args {
        if sets::polynomial_formula(b, a, &[x]) {
            let r = cx.eval(b.call("reduce", &[a, x, h.sym_term(h.sym("reals"))]))?;
            if !sets::is_head(b, r, "reduce") && !sets::is_head(b, r, names::FAILURE) {
                v.push(r);
                continue;
            }
        }
        v.push(a);
    }
    cx.eval(h.app(head, &v))
}
