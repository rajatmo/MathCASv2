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
use crate::common::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// Rewrite the conditions in place; `Some(answer)` when the complex region was reduced.
pub fn finish<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, cs: &mut Vec<T<'h>>, vars: &[T<'h>], complex: bool) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let ordering = |t: T<'h>| {
        let mut stack = mc_api::walk::Walk::new(t);
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
    // a region (an ordering among the conditions): over the real and imaginary parts; not with parameters, which
    // are complex too and would be read as real there (`re(a) > 0` as `a > 0`)
    if complex && vars.len() == 1 && cs.iter().any(|&c| ordering(c)) && super::spec::params(b, cs, vars).is_empty() {
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
        periodic_forms(cx, b, &mut out, vars[0])?;
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

/// Is `c` an order relation (`<`, `<=`, `>`, `>=`)?
fn is_order<'h>(b: &Build<'h>, c: T<'h>) -> bool {
    ["less", "less_equal", "greater", "greater_equal"].iter().any(|n| sets::is_head(b, c, n))
}

/// Does `t` hold a circular function of `x`?
fn has_circular<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if b.sym_of(head).is_some_and(|s| matches!(h.sym_name(s).as_str(), "sin" | "cos" | "tan" | "cot" | "sec" | "csc")) && !is_free(b, u, x) {
                return true;
            }
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// Order relations with circular functions of `x` solved:
/// - with polynomial relations beside them (a bounded range of x, as `x^2 < 1`), the conjunction by `reduce`;
/// - alone, through the period P of `l - r` (`function_period`): the solutions in one period [0, P) by `reduce`, an
///   interval that starts at 0 and one that ends at P joined across the end, then shifted by P c(1) with
///   `element(c(1), integers)`. Both steps are exact, so the family is the solution set.
fn periodic_forms<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, out: &mut Vec<T<'h>>, x: T<'h>) -> Res<()> {
    let h = b.heap();
    let reals = h.sym_term(h.sym("reals"));
    let trig: Vec<usize> = (0..out.len()).filter(|&i| is_order(b, out[i]) && !sets::polynomial_formula(b, out[i], &[x]) && has_circular(b, out[i], x)).collect();
    if trig.is_empty() {
        return Ok(());
    }
    let poly: Vec<usize> = (0..out.len()).filter(|&i| is_order(b, out[i]) && sets::polynomial_formula(b, out[i], &[x]) && !is_free(b, out[i], x)).collect();
    let others = (0..out.len()).any(|i| !trig.contains(&i) && !poly.contains(&i) && !sets::is_head(b, out[i], "not_element") && !is_free(b, out[i], x));
    if others {
        return Ok(());
    }
    let ok = |r: T<'h>| !sets::is_head(b, r, "reduce") && !sets::is_head(b, r, names::FAILURE) && !has_circular(b, r, x);
    if !poly.is_empty() {
        let parts: Vec<T<'h>> = trig.iter().chain(&poly).map(|&i| out[i]).collect();
        let r = cx.eval(b.call("reduce", &[h.call(h.sym("and"), &parts), x, reals]))?;
        if ok(r) {
            let mut kept: Vec<T<'h>> = (0..out.len()).filter(|i| !trig.contains(i) && !poly.contains(i)).map(|i| out[i]).collect();
            match h.view(r) {
                View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => kept.extend_from_slice(args),
                _ => kept.push(r),
            }
            *out = kept;
        }
        return Ok(());
    }
    if trig.len() != 1 {
        return Ok(());
    }
    let c = out[trig[0]];
    let View::App { args: &[l, r], .. } = h.view(c) else { return Ok(()) };
    let p = cx.eval(b.call("function_period", &[b.sub(l, r), x]))?;
    if !is_free(b, p, x) || sets::is_head(b, p, "function_period") || { let pos = cx.eval(b.call("greater", &[p, b.zero()]))?; !cx.is_true(pos) } {
        return Ok(());
    }
    let window = h.call(h.sym("and"), &[c, b.call("less_equal", &[b.zero(), x]), b.call("less", &[x, p])]);
    let w = cx.eval(b.call("reduce", &[window, x, reals]))?;
    if !ok(w) {
        return Ok(());
    }
    // the intervals (lo, lo strict, hi, hi strict) of the solutions in [0, P)
    let disjuncts: Vec<T<'h>> = match h.view(w) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym("or")) => args.to_vec(),
        _ => vec![w],
    };
    let mut ivs: Vec<(T<'h>, bool, T<'h>, bool)> = vec![];
    for d in disjuncts {
        let View::App { head, args: &[r1, r2] } = h.view(d) else { return Ok(()) };
        if b.sym_of(head) != Some(h.sym("and")) {
            return Ok(());
        }
        let side = |rel: T<'h>| -> Option<(bool, T<'h>, bool)> {
            // (x is on the right, the bound, strict)
            let View::App { head, args: &[a, c] } = h.view(rel) else { return None };
            let strict = match b.sym_of(head).map(|s| h.sym_name(s).to_string()).as_deref() {
                Some("less") => true,
                Some("less_equal") => false,
                _ => return None,
            };
            if c == x && is_free(b, a, x) {
                Some((true, a, strict))
            } else if a == x && is_free(b, c, x) {
                Some((false, c, strict))
            } else {
                None
            }
        };
        let (Some((true, lo, ls)), Some((false, hi, hs))) = (side(r1), side(r2)) else { return Ok(()) };
        ivs.push((lo, ls, hi, hs));
    }
    if ivs.is_empty() {
        return Ok(());
    }
    // an interval [0, a) and one (b, P) are one interval (b - P, a) of the family
    if ivs.len() > 1 {
        let (f0, l0) = (ivs[0], ivs[ivs.len() - 1]);
        if f0.0 == b.zero() && !f0.1 && l0.2 == p && l0.3 {
            let lo = cx.eval(b.sub(l0.0, p))?;
            ivs.pop();
            ivs[0] = (lo, l0.1, f0.2, f0.3);
        }
    }
    let k = h.call(h.sym("c"), &[h.int(1)]);
    let shift = b.mul(&[p, k]);
    let mut fam = vec![];
    for (lo, ls, hi, hs) in ivs {
        let lo = cx.eval(b.add(&[lo, shift]))?;
        let hi = cx.eval(b.add(&[hi, shift]))?;
        fam.push((b.call(if ls { "less" } else { "less_equal" }, &[lo, x]), b.call(if hs { "less" } else { "less_equal" }, &[x, hi])));
    }
    let mut kept: Vec<T<'h>> = (0..out.len()).filter(|&i| i != trig[0]).map(|i| out[i]).collect();
    kept.push(b.call("element", &[k, h.sym_term(h.sym("integers"))]));
    if fam.len() == 1 {
        kept.extend([fam[0].0, fam[0].1]);
    } else {
        let alts: Vec<T<'h>> = fam.iter().map(|&(a, c)| h.call(h.sym("and"), &[a, c])).collect();
        kept.push(h.call(h.sym("or"), &alts));
    }
    *out = kept;
    Ok(())
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
