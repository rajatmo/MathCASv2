//! Decompositions of formulas whose coefficients are real constants other than rationals (`sqrt(2)`, root objects,
//! `exp(pi)`, `log(3)`, ...).
//!
//! Each such constant c_i becomes a new variable p_i, ordered before the variables of the decomposition, and the
//! formula is conjoined with a small rational box a_i < p_i < b_i around the constant (its enclosure, checked). The
//! decomposition of the new formula in (p_1, ..., p_k, x, y, ...) describes the solution set for every parameter
//! point of the box; putting the constants back for the p_i, each condition that holds only the constants is
//! decided by the evaluator's exact comparison (an enclosure separates two different reals), and what is left is
//! the solution set at the constants. When a condition on the constants cannot be decided (the constants on a
//! boundary of the parameter cells, which an enclosure cannot prove), the answer is not given.
//!
//! This is the decomposition with parameters (Collins 1975; Basu, Pollack and Roy, *Algorithms in Real Algebraic
//! Geometry*, chapter 11) specialised at a point that is known only through enclosures; the cylindrical structure
//! over the parameter cell that contains the point is the structure at the point.
use crate::cad::Limits;
use crate::natives::{solve_formula_within, Style};
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The most constants read as parameters.
const MAX_CONSTANTS: usize = 4;
/// The cells the decomposition with the parameters may build.
const MAX_CELLS: usize = 20_000;
/// The work (units of the evaluator's fuel) it may take.
const FUEL: u64 = 20_000;

fn is_rational(h: &mc_core::Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::Int(_) | View::BigInt { .. } | View::Rational { .. })
}

/// The maximal subterms of `t` that are real constants other than rationals (exact, numeric, no variable), in order
/// of appearance; `None` when there are more than [`MAX_CONSTANTS`], or a machine number or string occurs.
fn constants<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, out: &mut Vec<T<'h>>, depth: usize) -> Res<Option<()>> {
    let h = cx.heap();
    if depth > 100 {
        return Ok(None);
    }
    match h.view(t) {
        View::Int(_) | View::BigInt { .. } | View::Rational { .. } => return Ok(Some(())),
        View::Real(_) | View::Ball { .. } | View::Str(_) => return Ok(None),
        // pi, e, euler_gamma, ... are numeric symbols
        View::Sym(_) | View::App { .. } => {}
    }
    let num = cx.eval(h.call(h.sym("is_numeric"), &[t]))?;
    if cx.is_true(num) {
        if !out.contains(&t) {
            out.push(t);
        }
        return Ok((out.len() <= MAX_CONSTANTS).then_some(()));
    }
    let View::App { args, .. } = h.view(t) else { return Ok(Some(())) };
    for &a in args {
        if constants(cx, a, out, depth + 1)?.is_none() {
            return Ok(None);
        }
    }
    Ok(Some(()))
}

/// Does the term hold a relation none of whose sides mentions a variable of `vars` (an undecided comparison of
/// constants)?
fn constant_relation<'h>(h: &mc_core::Heap, t: T<'h>, vars: &[T<'h>]) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        let View::App { head, args } = h.view(u) else { continue };
        let rel = matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "less" | "greater" | "less_equal" | "greater_equal" | "equal" | "unequal" | "inequality"));
        if rel && !mentions_any(h, u, vars) {
            return true;
        }
        stack.extend(args.iter().copied());
    }
    false
}

fn mentions_any<'h>(h: &mc_core::Heap, t: T<'h>, vars: &[T<'h>]) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if vars.contains(&u) {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// `cylindrical_decomposition(cond, vars)` of a formula with constant coefficients other than rationals; `None` when
/// there are none, when one is not real, or when the decomposition or a decision on the constants does not finish.
pub fn decompose_with_constants<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, cond: T<'h>, vars: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let mut cs = vec![];
    if constants(cx, cond, &mut cs, 0)?.is_none() || cs.is_empty() {
        return Ok(None);
    }
    let scale = h.call(h.sym(names::POW), &[h.int(10), h.int(12)]);
    let mut params = vec![];
    let mut rules = vec![];
    let mut back = vec![];
    let mut boxes = vec![];
    for (i, &c) in cs.iter().enumerate() {
        // a rational box around the constant, checked by exact comparison (so a constant that is not real fails)
        let k = cx.eval(h.call(h.sym("floor"), &[h.call(h.sym(names::MUL), &[c, scale])]))?;
        if !matches!(h.view(k), View::Int(_) | View::BigInt { .. }) {
            return Ok(None);
        }
        let lo = cx.eval(h.call(h.sym(names::MUL), &[k, h.call(h.sym(names::POW), &[scale, h.int(-1)])]))?;
        let k1 = cx.eval(h.call(h.sym(names::ADD), &[k, h.int(1)]))?;
        let hi = cx.eval(h.call(h.sym(names::MUL), &[k1, h.call(h.sym(names::POW), &[scale, h.int(-1)])]))?;
        if !is_rational(h, lo) || !is_rational(h, hi) {
            return Ok(None);
        }
        let inside = cx.eval(h.call(h.sym("and"), &[h.call(h.sym("less"), &[lo, c]), h.call(h.sym("less"), &[c, hi])]))?;
        if !cx.is_true(inside) {
            return Ok(None);
        }
        let p = h.sym_term(h.sym(&format!("cad$constant{}", i + 1)));
        params.push(p);
        rules.push(h.call(h.sym(names::RULE), &[c, p]));
        back.push(h.call(h.sym(names::RULE), &[p, c]));
        boxes.push(h.call(h.sym("less"), &[lo, p]));
        boxes.push(h.call(h.sym("less"), &[p, hi]));
    }
    let rl = cx.list(&rules);
    let with_params = cx.eval(h.call(h.sym("rewrite"), &[cond, rl]))?;
    if cs.iter().any(|&c| mentions_any(h, with_params, &[c])) {
        return Ok(None);
    }
    boxes.push(with_params);
    let f = h.call(h.sym("and"), &boxes);
    let mut free = params.clone();
    free.extend(vars.iter().copied());
    let lim = Limits { max_cells: MAX_CELLS, fuel: Some(FUEL), ..Limits::default() };
    let Some(t) = solve_formula_within(cx, stop, f, &free, Style::Cylindrical, &lim)? else { return Ok(None) };
    let bl = cx.list(&back);
    let t = cx.eval(h.call(h.sym("rewrite"), &[t, bl]))?;
    let h = cx.heap();
    if mentions_any(h, t, &params) || constant_relation(h, t, vars) {
        return Ok(None);
    }
    Ok(Some(t))
}
