//! `root_sum(x => p(x), y => g(y))` by the structure of `p` and `g`, before the evaluation over Q in
//! [`crate::rootsum`]:
//!
//! - **linearity**: when `g` is not a rational function of `y` it is expanded and split into its terms; the terms
//!   rational in `y` stay together, every other term has its numeric factors taken out:
//!   `root_sum(p, 5 tan(y) + 7) = 5 root_sum(p, tan) + 7 deg p`;
//! - **multiplicities with parameters**: when `p` has symbolic coefficients it is factored (`factor_list`), and
//!   `root_sum(q1^e1 q2^e2 ..., g) = e1 root_sum(q1, g) + e2 root_sum(q2, g) + ...`; the polynomial is written with a
//!   positive leading coefficient, so equal sums print alike;
//! - **low degree with parameters**: degree 1 gives `g(-c0/c1)`, degree 2 the two roots
//!   `(-b +- (b^2 - 4 a c)^(1/2)) / (2 a)`;
//! - **rational summands with parameters**: for `g = N/D`, `R(t) = Res_x(p(x), t D(x) - N(x))` is
//!   `lc(p)^k prod D(r) prod (t - g(r))` over the roots r of p with multiplicity (Cohen, *A Course in Computational
//!   Algebraic Number Theory*, 3.3.2, the resultant as a product over roots), so the sum is `-R_(d-1)/R_d` with
//!   `d = deg p`; when `R_d` is the zero polynomial (`D` and `p` share a root for every parameter value) the sum is
//!   left alone;
//! - **logarithms of polynomials**: for rational `p` and `g = log(q(y))` with `q` a rational polynomial, the product
//!   `P = prod q(r) = Res_x(p, q) / lc(p)^(deg q)` is rational and `sum log q(r) = log P + 2 pi i k` for an integer
//!   k; k is read from the numeric sum at 30 digits and accepted only when the difference is within `10^-20` of
//!   `2 pi i k` (it is exactly a multiple of `2 pi i`, so a sign error in P would show as an odd multiple of
//!   `pi i` and the sum is then left alone).
use crate::cx::Env;
use mc_api::{Store, View, T};
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use mc_poly::work::{Meter, Unlimited};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The terms of a sum (the term itself when it is not a sum).
fn add_terms<'h>(h: &'h Heap, t: T<'h>) -> Vec<T<'h>> {
    match h.view(t) {
        View::App { head, args } if head == h.sym_term(h.sym("add")) => args.to_vec(),
        _ => vec![t],
    }
}

/// The factors of a product.
fn mul_factors<'h>(h: &'h Heap, t: T<'h>) -> Vec<T<'h>> {
    match h.view(t) {
        View::App { head, args } if head == h.sym_term(h.sym("mul")) => args.to_vec(),
        _ => vec![t],
    }
}

/// Does `t` contain a machine real or a ball?
fn inexact<'h>(h: &'h Heap, t: T<'h>) -> bool {
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Real(_) | View::Ball { .. } => return true,
            View::App { head, args } => {
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    false
}

/// The kernels of `e` as a rational function of `v` other than `v`, and whether they are all free of `v` (`e` is a
/// rational function of `v`).
fn rational_in<'h>(env: &Env<'h>, e: T<'h>, v: T<'h>, m: &mut Meter) -> Option<(bool, Vec<T<'h>>)> {
    let c = env.rconv();
    let (ring, _) = c.ring(&[e], &[v], m).ok()?;
    let others: Vec<T<'h>> = ring.kernels.iter().copied().filter(|&k| k != v).collect();
    let free = others.iter().all(|&k| !env.contains(k, v));
    Some((free, others))
}

/// Does `t` hold a symbol other than `y` that is not a numeric constant (in argument positions)?
fn other_symbols<'h>(env: &Env<'h>, t: T<'h>, y: T<'h>) -> bool {
    let h = env.h();
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(_) => {
                if u != y && env.ar.num(u).is_none() {
                    return true;
                }
            }
            View::App { args, .. } => stack.extend_from_slice(args),
            _ => {}
        }
    }
    false
}

/// Is `t` a negative number, or a product whose numeric factor is negative?
fn negative<'h>(env: &Env<'h>, t: T<'h>) -> bool {
    let h = env.h();
    let num_neg = |u: T<'h>| match h.view(u) {
        View::Int(v) => v < 0,
        View::BigInt { neg, .. } => neg,
        View::Rational { num, .. } => matches!(h.view(num), View::Int(v) if v < 0) || matches!(h.view(num), View::BigInt { neg: true, .. }),
        _ => false,
    };
    num_neg(t) || mul_factors(h, t).first().is_some_and(|&f| f != t && num_neg(f))
}

/// The root sum by structure, or `None` (the evaluation over Q continues).
pub(crate) fn structural<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h: &'h Heap = cx.heap();
    let env = Env::new(h, stop.clone(), "root_sum");
    let mut unlimited = Unlimited;
    let mut m = Meter::new(&mut unlimited);
    let &[f, g] = args else { return Ok(None) };
    let (Some((x, body)), Some((y, gb))) = (env.function_parts(f), env.function_parts(g)) else { return Ok(None) };
    // the library builds summands on the placeholder `calc_rs_z` (a function made from a computed body by
    // substitution would rename a slot parameter); they are written back with the slot `&1`
    let slot = env.sym("&1");
    let placeholder = env.sym("calc_rs_z");
    let to_slot = |v: T<'h>, b: T<'h>, m: &mut Meter| -> Option<T<'h>> {
        if v != placeholder || env.contains(b, slot) {
            return None;
        }
        env.subst(b, v, slot, m).ok().map(|nb| env.function(slot, nb))
    };
    let (nf, ng) = (to_slot(x, body, &mut m), to_slot(y, gb, &mut m));
    if nf.is_some() || ng.is_some() {
        return Ok(Some(env.call("root_sum", &[nf.unwrap_or(f), ng.unwrap_or(g)])));
    }
    // slot functions in evaluated (canonical) form: a function is held as written, so `&(exp(-a &1)/&1)` and the
    // same body built by a computation would otherwise be different terms (a named parameter may have a value, so
    // only slot bodies are evaluated)
    let canon = |cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>, b: T<'h>| -> Res<T<'h>> { if v == slot { cx.eval(b) } else { Ok(b) } };
    let (cb, cg) = (canon(cx, x, body)?, canon(cx, y, gb)?);
    if cb != body || cg != gb {
        return Ok(Some(env.call("root_sum", &[env.function(x, cb), env.function(y, cg)])));
    }
    if inexact(h, body) || inexact(h, gb) {
        return Ok(None);
    }
    let Some((p_poly, p_others)) = rational_in(&env, body, x, &mut m) else { return Ok(None) };
    if !p_poly {
        return Ok(None);
    }
    let p_params = p_others.iter().any(|&k| env.ar.num(k).is_none());
    let Some((g_rational, _)) = rational_in(&env, gb, y, &mut m) else { return Ok(None) };
    let rs = |q: T<'h>, b: T<'h>| env.call("root_sum", &[env.function(x, q), env.function(y, b)]);

    // linearity
    if !g_rational {
        let expanded = cx.eval(env.call("expand", &[gb]))?;
        let terms = add_terms(h, expanded);
        let mut rational = vec![];
        let mut parts = vec![];
        let mut changed = false;
        let n_terms = terms.len();
        for t in terms {
            if rational_in(&env, t, y, &mut m).is_some_and(|(r, _)| r) {
                rational.push(t);
                continue;
            }
            let (consts, rest): (Vec<T<'h>>, Vec<T<'h>>) = mul_factors(h, t).into_iter().partition(|&u| !env.contains(u, y) && env.ar.num(u).is_some());
            if !consts.is_empty() {
                changed = true;
            }
            let rest = if rest.len() == 1 { rest[0] } else { env.call("mul", &rest) };
            parts.push(env.call("mul", &[env.call("mul", &consts), rs(body, rest)]));
        }
        // split only where a part is gained: a rational part (summed exactly) or a numeric factor; a sum of terms
        // that are all not rational stays one sum (antiderivatives keep their form), and so does a summand with
        // another variable, whose parts may cancel in a limit or a series in it ((1 - cos(a t))/a^2 at a = 0)
        if n_terms > 1 && other_symbols(&env, gb, y) {
            changed = false;
        } else if !rational.is_empty() && n_terms > 1 {
            changed = true;
        }
        if changed {
            if !rational.is_empty() {
                parts.push(rs(body, env.call("add", &rational)));
            }
            return Ok(Some(env.call("add", &parts)));
        }
    }
    if p_params {
        // multiplicities: a sum over each factor depending on x
        let fl = cx.eval(env.call("factor_list", &[body]))?;
        if let Some(rows) = env.as_list(fl) {
            let mut fs = vec![];
            for &r in rows {
                let Some(&[q, e]) = env.as_list(r) else { return Ok(None) };
                if env.contains(q, x) {
                    fs.push((q, e));
                }
            }
            if fs.len() > 1 || fs.iter().any(|&(_, e)| e != env.int(1)) {
                let parts: Vec<T<'h>> = fs.iter().map(|&(q, e)| env.call("mul", &[e, rs(q, gb)])).collect();
                return Ok(Some(env.call("add", &parts)));
            }
        }
        let deg = cx.eval(env.call("exponent", &[body, x]))?;
        let View::Int(d) = h.view(deg) else { return Ok(None) };
        let coeff = |cx: &mut NativeCx<'_, '_, 'h>, i: i64| cx.eval(env.call("coefficient", &[body, x, env.int(i)]));
        let lc = coeff(cx, d)?;
        if negative(&env, lc) {
            let nb = cx.eval(env.call("expand", &[env.call("mul", &[env.int(-1), body])]))?;
            return Ok(Some(rs(nb, gb)));
        }
        let apply = |r: T<'h>, m: &mut Meter| env.subst(gb, y, r, m);
        match d {
            1 => {
                let c0 = coeff(cx, 0)?;
                let r = env.call("mul", &[env.int(-1), c0, env.call("pow", &[lc, env.int(-1)])]);
                let Ok(v) = apply(r, &mut m) else { return Ok(None) };
                return Ok(Some(v));
            }
            2 => {
                let (c0, c1) = (coeff(cx, 0)?, coeff(cx, 1)?);
                let disc = env.call(
                    "add",
                    &[env.call("pow", &[c1, env.int(2)]), env.call("mul", &[env.int(-4), lc, c0])],
                );
                let disc = cx.eval(env.call("expand", &[disc]))?;
                let sq = env.call("pow", &[disc, env.ar.number(&mc_arith::Num::rat(crate::qpoly::qq(1, 2)))]);
                let inv = env.call("pow", &[env.call("mul", &[env.int(2), lc]), env.int(-1)]);
                let mut parts = vec![];
                for s in [-1, 1] {
                    let r = env.call("mul", &[inv, env.call("add", &[env.call("mul", &[env.int(-1), c1]), env.call("mul", &[env.int(s), sq])])]);
                    let Ok(v) = apply(r, &mut m) else { return Ok(None) };
                    parts.push(v);
                }
                return Ok(Some(env.call("add", &parts)));
            }
            _ => {}
        }
        if !g_rational {
            return Ok(None);
        }
        // a rational summand: the resultant of p and t D - N
        let tg = cx.eval(env.call("together", &[gb]))?;
        let num = cx.eval(env.call("numerator", &[tg]))?;
        let den = cx.eval(env.call("denominator", &[tg]))?;
        if !env.contains(den, y) {
            // a polynomial summand: power sums (the evaluation over Q handles it)
            return Ok(None);
        }
        let t = h.sym_term(h.sym("calc_rs_t"));
        let (Ok(nx), Ok(dx)) = (env.subst(num, y, x, &mut m), env.subst(den, y, x, &mut m)) else { return Ok(None) };
        let lin = env.call("add", &[env.call("mul", &[t, dx]), env.call("mul", &[env.int(-1), nx])]);
        let res = cx.eval(env.call("resultant", &[body, lin, x]))?;
        if env.args_of(res, "resultant").is_some() {
            return Ok(None);
        }
        let rd = cx.eval(env.call("coefficient", &[res, t, deg]))?;
        let rd1 = cx.eval(env.call("coefficient", &[res, t, env.int(d - 1)]))?;
        let rd_zero = cx.eval(env.call("expand", &[rd]))?;
        if rd_zero == env.int(0) || env.contains(rd, t) || env.contains(rd1, t) {
            return Ok(None);
        }
        let s = env.call("mul", &[env.int(-1), rd1, env.call("pow", &[rd, env.int(-1)])]);
        return Ok(Some(env.call("together", &[s])));
    }
    // logarithms of rational polynomials over a rational p
    if p_others.is_empty() {
        if let Some(&[q]) = env.args_of(gb, "log") {
            let Some((true, qk)) = rational_in(&env, q, y, &mut m) else { return Ok(None) };
            if !qk.is_empty() {
                return Ok(None);
            }
            let tq = cx.eval(env.call("together", &[q]))?;
            let den = cx.eval(env.call("denominator", &[tq]))?;
            if env.contains(den, y) {
                return Ok(None);
            }
            let Ok(qx) = env.subst(q, y, x, &mut m) else { return Ok(None) };
            let res = cx.eval(env.call("resultant", &[body, qx, x]))?;
            let dq = cx.eval(env.call("exponent", &[qx, x]))?;
            let dp = cx.eval(env.call("exponent", &[body, x]))?;
            let lc = cx.eval(env.call("coefficient", &[body, x, dp]))?;
            let prod = cx.eval(env.call("mul", &[res, env.call("pow", &[lc, env.call("mul", &[env.int(-1), dq])])]))?;
            if prod == env.int(0) || env.ar.num(prod).is_none() {
                return Ok(None);
            }
            let logp = env.call("log", &[prod]);
            let two_pi_i = cx.eval(env.call("mul", &[env.call("complex", &[env.int(0), env.int(2)]), env.sym("pi")]))?;
            // the numeric sum over the numbered roots (not `n` of the root sum, which would come back here)
            let View::Int(dpi) = h.view(dp) else { return Ok(None) };
            let mut terms = vec![];
            for k in 1..=dpi {
                let Ok(v) = env.subst(gb, y, env.call("root", &[f, env.int(k)]), &mut m) else { return Ok(None) };
                terms.push(v);
            }
            let diff = env.call("add", &[env.call("add", &terms), env.call("mul", &[env.int(-1), logp])]);
            let ratio = env.call("n", &[env.call("mul", &[diff, env.call("pow", &[two_pi_i, env.int(-1)])]), env.int(30)]);
            let ratio = cx.eval(ratio)?;
            let k = cx.eval(env.call("round", &[env.call("re", &[ratio])]))?;
            if !matches!(h.view(k), View::Int(_)) {
                return Ok(None);
            }
            let tiny = env.call("pow", &[env.int(10), env.int(-20)]);
            let re_off = env.call("abs", &[env.call("add", &[env.call("re", &[ratio]), env.call("mul", &[env.int(-1), k])])]);
            let im_off = env.call("abs", &[env.call("im_part", &[ratio])]);
            let ok = env.call("and", &[env.call("less", &[re_off, tiny]), env.call("less", &[im_off, tiny])]);
            if cx.eval(ok)? != env.sym(mc_api::names::TRUE) {
                return Ok(None);
            }
            return Ok(Some(env.call("add", &[logp, env.call("mul", &[k, two_pi_i])])));
        }
    }
    Ok(None)
}
