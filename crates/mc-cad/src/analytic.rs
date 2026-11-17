//! `find_instance` over the complex numbers for equations with analytic functions that no exact method takes
//! (`hankel_h1(1, x) == spherical_hankel_h1(1, y) and x != y`, `sin(x + y) == 1/2 and exp(x) - y == 1`): an
//! instance is any point where the conditions hold, so the unknowns are fixed one by one.
//!
//! - An equation that `solve` solves for one unknown gives that unknown as an expression in the others (any one of
//!   its solutions serves); it is put into the other conditions, until one equation is left.
//! - The other unknowns of the last equation are fixed at small numbers (1, 2, 1/2, ...); the last unknown is then
//!   given by `solve` exactly, or else by a root object of the analytic function (`mc_algebra::transroot`: Newton's
//!   iteration from a `find_root` point, the root proved simple and isolated by Krawczyk's test).
//! - The equations hold at the point by construction (solutions of `solve`, a proved root); their residuals at 30
//!   digits must vanish (a solution of `solve` valid only generically is dropped), and every other condition must
//!   evaluate to `true` there (`a != b` through an enclosure of `a - b` at 30 digits away from 0). The Hankel and
//!   spherical Bessel functions are written with Bessel functions first (DLMF 10.4.3, 10.47.3), whose enclosures
//!   the root objects use.
use mc_api::{names, Store, View, T};
use mc_core::Heap;
use mc_eval::{NativeCx, Res};

/// The values tried for an unknown that is fixed.
const FIXED: [(i64, i64); 6] = [(1, 1), (2, 1), (1, 2), (3, 1), (-1, 1), (1, 3)];
/// The starting points of the numeric search for the last unknown: (re, im).
const STARTS: [(f64, f64); 8] = [(1.0, 0.0), (1.0, 1.0), (-1.0, 1.0), (0.5, 0.5), (2.0, 0.0), (0.0, 1.0), (-1.0, -1.0), (2.0, 2.0)];

fn head_name(h: &Heap, t: T<'_>) -> Option<String> {
    match h.view(t) {
        View::App { head, .. } => match h.view(head) {
            View::Sym(s) => Some(h.sym_name(s)),
            _ => None,
        },
        _ => None,
    }
}

fn args_of<'h>(h: &'h Heap, t: T<'h>) -> &'h [T<'h>] {
    match h.view(t) {
        View::App { args, .. } => args,
        _ => &[],
    }
}

fn contains(h: &Heap, t: T<'_>, x: T<'_>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// Is some unknown inside a function other than sums, products and integer powers?
fn transcendental<'h>(h: &'h Heap, t: T<'h>, vars: &[T<'h>]) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !vars.iter().any(|&v| contains(h, u, v)) {
            continue;
        }
        let View::App { args, .. } = h.view(u) else { continue };
        match head_name(h, u).as_deref() {
            Some("add" | "mul") => stack.extend_from_slice(args),
            Some("pow") if matches!(h.view(args[1]), View::Int(_)) => stack.push(args[0]),
            _ => return true,
        }
    }
    false
}

/// `t` with the rules applied, evaluated.
fn subst<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, rules: &[T<'h>]) -> Res<T<'h>> {
    if rules.is_empty() {
        return Ok(t);
    }
    let h = cx.heap();
    let rs = cx.list(rules);
    cx.eval(h.call(h.sym("rewrite"), &[t, rs]))
}

/// `t` with the Hankel functions as Bessel functions (H(1,2)_nu = J_nu +- i Y_nu, DLMF 10.4.3) and the spherical
/// Bessel and Hankel functions as Bessel functions of half-integer order (j_n(z) = (pi/(2 z))^(1/2) J_(n+1/2)(z),
/// DLMF 10.47.3; y_n likewise with Y), unevaluated: the forms the enclosures of the root objects know.
fn as_bessel<'h>(h: &'h Heap, t: T<'h>) -> T<'h> {
    let View::App { head, args } = h.view(t) else { return t };
    let new: Vec<T<'h>> = args.iter().map(|&a| as_bessel(h, a)).collect();
    let i = h.sym_term(h.sym("im"));
    let call = |f: &str, a: &[T<'h>]| h.call(h.sym(f), a);
    let half = |nu: T<'h>| call("add", &[nu, call("pow", &[h.int(2), h.int(-1)])]);
    let factor = |z: T<'h>| call("pow", &[call("mul", &[h.sym_term(h.sym("pi")), call("pow", &[call("mul", &[h.int(2), z]), h.int(-1)])]), call("pow", &[h.int(2), h.int(-1)])]);
    let spherical = |f: &str, nu: T<'h>, z: T<'h>| call("mul", &[factor(z), call(f, &[half(nu), z])]);
    match (head_name(h, t).as_deref(), new.as_slice()) {
        (Some("hankel_h1"), &[nu, z]) => call("add", &[call("bessel_j", &[nu, z]), call("mul", &[i, call("bessel_y", &[nu, z])])]),
        (Some("hankel_h2"), &[nu, z]) => call("add", &[call("bessel_j", &[nu, z]), call("mul", &[h.int(-1), i, call("bessel_y", &[nu, z])])]),
        (Some("spherical_bessel_j"), &[nu, z]) => spherical("bessel_j", nu, z),
        (Some("spherical_bessel_y"), &[nu, z]) => spherical("bessel_y", nu, z),
        (Some("spherical_hankel_h1"), &[nu, z]) => {
            call("add", &[spherical("bessel_j", nu, z), call("mul", &[i, spherical("bessel_y", nu, z)])])
        }
        (Some("spherical_hankel_h2"), &[nu, z]) => {
            call("add", &[spherical("bessel_j", nu, z), call("mul", &[h.int(-1), i, spherical("bessel_y", nu, z)])])
        }
        _ => h.app(head, &new),
    }
}

/// One value of `y` with `g == 0` (g a function of y alone): exact from `solve`, else a proved root object.
fn one_root<'h>(cx: &mut NativeCx<'_, '_, 'h>, g: T<'h>, y: T<'h>, avoid: &[T<'h>]) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let mut out = vec![];
    let s = cx.eval(h.call(h.sym("solve"), &[h.call(h.sym("equal"), &[g, h.int(0)]), y]))?;
    if let Some(items) = cx.as_list(s).map(<[T<'h>]>::to_vec) {
        for it in items {
            if let Some(&[r]) = cx.as_list(it) {
                if let &[_, v] = args_of(h, r) {
                    // a value with a generated parameter (a family) or still holding y is not a point
                    if !contains(h, v, y) && !contains(h, v, h.sym_term(h.sym("c"))) && !avoid.contains(&v) {
                        out.push(v);
                    }
                }
            }
        }
    }
    if !out.is_empty() {
        return Ok(out);
    }
    let slot = h.sym_term(h.sym("&1"));
    let g = cx.eval(as_bessel(h, g))?;
    let body = subst(cx, g, &[h.call(h.sym(names::RULE), &[y, slot])])?;
    let f = h.call(h.sym("fn"), &[h.call(h.sym(names::LIST), &[slot]), body]);
    for &(re, im) in &STARTS {
        cx.tick(8)?;
        let start = if im == 0.0 { h.real(re) } else { cx.eval(h.call(h.sym("add"), &[h.real(re), h.call(h.sym("mul"), &[h.real(im), h.sym_term(h.sym("im"))])]))? };
        let fr = cx.eval(h.call(h.sym("find_root"), &[g, h.call(h.sym(names::LIST), &[y, start])]))?;
        let Some(&[rule]) = cx.as_list(fr) else { continue };
        let &[_, y0] = args_of(h, rule) else { continue };
        let obj = cx.eval(h.call(h.sym("root"), &[h.call(h.sym(names::LIST), &[f, y0])]))?;
        // proved: the root object (or an exact value it turned into), not the call left as it was
        let unproved = head_name(h, obj).as_deref() == Some("root") && args_of(h, obj).first().is_some_and(|&a| args_of(h, a).first() == Some(&f));
        if unproved || head_name(h, obj).as_deref() == Some(names::FAILURE) || avoid.contains(&obj) || out.contains(&obj) {
            continue;
        }
        out.push(obj);
    }
    Ok(out)
}

/// `find_instance(cond, vars)` or `(cond, vars, n)` over the complex numbers by fixing the unknowns (module docs);
/// `None` when the call is not of this kind or no point is proved. Without `any` an equation must have an unknown
/// inside a function that is not polynomial (polynomial systems are the business of the other methods).
pub fn find<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], any: bool) -> Res<Option<T<'h>>> {
    let h: &'h Heap = cx.heap();
    let (cond, xs, n) = match args {
        [c, x] => (*c, *x, 1usize),
        [c, x, k] => match h.view(*k) {
            View::Int(k @ 1..=20) => (*c, *x, k as usize),
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    let vars: Vec<T<'h>> = match cx.as_list(xs) {
        Some(v) => v.to_vec(),
        None => vec![xs],
    };
    if vars.is_empty() || vars.len() > 4 || vars.iter().any(|&v| !matches!(h.view(v), View::Sym(_))) {
        return Ok(None);
    }
    // a large condition (a deeply shared term): not tried
    {
        let mut nodes = 0usize;
        let mut stack = vec![cond];
        while let Some(u) = stack.pop() {
            nodes += 1;
            if nodes > 2_000 {
                return Ok(None);
            }
            if let View::App { head, args } = h.view(u) {
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
    }
    let parts: Vec<T<'h>> = match head_name(h, cond).as_deref() {
        Some("and") => args_of(h, cond).to_vec(),
        _ => vec![cond],
    };
    let mut eqs = vec![];
    let mut others = vec![];
    for p in parts {
        if head_name(h, p).as_deref() == Some("equal") && args_of(h, p).len() == 2 {
            let &[l, r] = args_of(h, p) else { return Ok(None) };
            eqs.push(cx.eval(h.call(h.sym("add"), &[l, h.call(h.sym("mul"), &[h.int(-1), r])]))?);
        } else {
            others.push(p);
        }
    }
    if eqs.is_empty() || eqs.len() > vars.len() || (!any && !eqs.iter().any(|&e| transcendental(h, e, &vars))) {
        return Ok(None);
    }
    let eqs_all = eqs.clone();
    // eliminate: an equation solved for an unknown by solve (its first solution), put into the rest
    let mut defined: Vec<(T<'h>, T<'h>)> = vec![];
    let mut free: Vec<T<'h>> = vars.clone();
    while eqs.len() > 1 {
        cx.tick(16)?;
        let mut step = None;
        'find: for (i, &e) in eqs.iter().enumerate() {
            for (j, &u) in free.iter().enumerate() {
                if !contains(h, e, u) {
                    continue;
                }
                let s = cx.eval(h.call(h.sym("solve"), &[h.call(h.sym("equal"), &[e, h.int(0)]), u]))?;
                let Some(&first) = cx.as_list(s).and_then(|l| l.first()) else { continue };
                let Some(&[r]) = cx.as_list(first) else { continue };
                let &[_, v] = args_of(h, r) else { continue };
                if contains(h, v, u) || contains(h, v, h.sym_term(h.sym("c"))) {
                    continue;
                }
                step = Some((i, j, v));
                break 'find;
            }
        }
        let Some((i, j, v)) = step else { return Ok(None) };
        let u = free.remove(j);
        eqs.remove(i);
        let rule = h.call(h.sym(names::RULE), &[u, v]);
        for e in eqs.iter_mut() {
            *e = subst(cx, *e, &[rule])?;
        }
        for d in defined.iter_mut() {
            d.1 = subst(cx, d.1, &[rule])?;
        }
        defined.push((u, v));
    }
    let e = eqs[0];
    // the unknown solved for last: one the equation holds, the others fixed
    let Some(&y) = free.iter().rev().find(|&&u| contains(h, e, u)) else { return Ok(None) };
    let fixed: Vec<T<'h>> = free.iter().copied().filter(|&u| u != y).collect();
    let mut found: Vec<T<'h>> = vec![];
    let tries = if fixed.is_empty() { 1 } else { FIXED.len() };
    for &(p, q) in FIXED.iter().take(tries) {
        cx.tick(16)?;
        if cx.stopped() {
            return Ok(None);
        }
        let val = if q == 1 { h.int(p) } else { cx.eval(h.call(h.sym("mul"), &[h.int(p), h.call(h.sym("pow"), &[h.int(q), h.int(-1)])]))? };
        let fixed_rules: Vec<T<'h>> = fixed.iter().map(|&u| h.call(h.sym(names::RULE), &[u, val])).collect();
        let g = subst(cx, e, &fixed_rules)?;
        if !contains(h, g, y) {
            continue;
        }
        let avoid: Vec<T<'h>> = vec![];
        for yv in one_root(cx, g, y, &avoid)? {
            let mut rules = fixed_rules.clone();
            rules.push(h.call(h.sym(names::RULE), &[y, yv]));
            // the eliminated unknowns, last defined first
            let mut point: Vec<(T<'h>, T<'h>)> = fixed.iter().map(|&u| (u, val)).collect();
            point.push((y, yv));
            for &(u, expr) in defined.iter().rev() {
                let v = subst(cx, expr, &rules)?;
                rules.push(h.call(h.sym(names::RULE), &[u, v]));
                point.push((u, v));
            }
            // the equations hold by construction (solutions of solve, a proved root); a value of solve that holds
            // only generically is caught by the residuals at 30 digits; the other conditions must evaluate to true
            let mut holds = true;
            for &c in eqs_all.iter() {
                let r = subst(cx, as_bessel(h, c), &rules)?;
                let mut v = cx.eval(h.call(h.sym("n"), &[r, h.int(30)]))?;
                // a value near 0 that n cannot give to 30 digits comes as its enclosure inside the failure
                if head_name(h, v).as_deref() == Some(names::FAILURE) {
                    if let &[tag, w] = args_of(h, v) {
                        if matches!(h.view(tag), View::Sym(s) if h.sym_name(s) == "precision_not_reached") {
                            v = w;
                        }
                    }
                }
                let small = cx.eval(h.call(h.sym("less"), &[h.call(h.sym("abs"), &[v]), h.call(h.sym("pow"), &[h.int(10), h.int(-20)])]))?;
                if !matches!(h.view(small), View::Sym(s) if h.sym_name(s) == "true") {
                    holds = false;
                    break;
                }
            }
            for &c in &others {
                if !holds {
                    break;
                }
                let at = subst(cx, c, &rules)?;
                holds = matches!(h.view(at), View::Sym(s) if h.sym_name(s) == "true");
                // a != b: the difference enclosed at 30 digits away from 0
                if let (false, Some("unequal"), &[a, b]) = (holds, head_name(h, c).as_deref(), args_of(h, c)) {
                    let d = h.call(h.sym("add"), &[a, h.call(h.sym("mul"), &[h.int(-1), b])]);
                    let d = subst(cx, as_bessel(h, d), &rules)?;
                    let big = cx.eval(h.call(h.sym("greater"), &[h.call(h.sym("abs"), &[h.call(h.sym("n"), &[d, h.int(30)])]), h.call(h.sym("pow"), &[h.int(10), h.int(-10)])]))?;
                    holds = matches!(h.view(big), View::Sym(s) if h.sym_name(s) == "true");
                }
            }
            if !holds {
                continue;
            }
            let ordered: Vec<T<'h>> = vars
                .iter()
                .map(|&u| {
                    let v = point.iter().find(|p| p.0 == u).map(|p| p.1).unwrap_or(u);
                    h.call(h.sym(names::RULE), &[u, v])
                })
                .collect();
            let sol = cx.list(&ordered);
            if !found.contains(&sol) {
                found.push(sol);
            }
            if found.len() == n {
                return Ok(Some(cx.list(&found)));
            }
        }
    }
    Ok(None)
}
