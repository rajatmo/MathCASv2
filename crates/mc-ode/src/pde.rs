//! Partial differential equations in one space dimension by the method of lines (Schiesser, The Numerical Method of
//! Lines, Academic Press 1991, ch. 1-3): `n_dsolve(eqs, u, [t, t0, t1], [x, a, b])` for
//!
//! - one equation `derivative(m, 0)(u)(t, x) == F(t, x, u, u_x, u_xx)` (m = 1, parabolic; m = 2, hyperbolic, where
//!   F may also contain `u_t`), solved for the highest time derivative when it occurs linearly;
//! - the initial values `u(t0, x) == f(x)` (and `derivative(1, 0)(u)(t0, x) == g(x)` for m = 2);
//! - one boundary condition at each end: `u(t, a) == g(t)` (Dirichlet) or `derivative(0, 1)(u)(t, a) == g(t)`
//!   (Neumann).
//!
//! The interval is divided into [`INTERVALS`] equal parts; the space derivatives at node j are of fourth order in h:
//! the central five-point differences `(u[j-2] - 8 u[j-1] + 8 u[j+1] - u[j+2])/(12 h)` and `(-u[j-2] + 16 u[j-1] -
//! 30 u[j] + 16 u[j+1] - u[j+2])/(12 h^2)`, and next to a Dirichlet end the one-sided differences
//! `(-3 u0 - 10 u1 + 18 u2 - 6 u3 + u4)/(12 h)` and `(10 u0 - 15 u1 - 4 u2 + 14 u3 - 6 u4 + u5)/(12 h^2)` (Fornberg,
//! Math. Comp. 51, 1988). A Neumann end and its neighbour take the three-point differences with the ghost node
//! `u[-1] = u[1] - 2 h g(t)` (`u[M+1] = u[M-1] + 2 h g(t)` at b), second order; a Dirichlet node follows its datum
//! through `u' = g'(t)` (`u'' = g''(t)`) from `g(t0)`. Against the series solution of the heat equation with
//! u(t, 0) = sin t on [0, 5] the largest error of the three-point differences was 5e-5 (6% of the value in the
//! boundary layer of the first hundredth of time), of these 2e-8. The nodes' values are the components of an ordinary system solved by
//! the integrators of this crate (the stiff method where the system is stiff, as the heat equation's is); the
//! solution `interpolating_function("u", [[t0, t1], [a, b]], steps, key)` takes `(t, x)` and interpolates the node
//! values at t by the cubic through the four nearest nodes. Any other form (or a call with monitors, events
//! or `digits:`) leaves the call unevaluated.
use crate::system::{self, System, Unknown};
use crate::util::{mentions, number, real};
use mc_api::names;
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Res};

/// The number of equal parts of the space interval.
pub(crate) const INTERVALS: usize = 100;

/// One occurrence `derivative(i, j)(u)(p, q)` (or `u(p, q)`, i = j = 0).
struct Occ<'h> {
    i: usize,
    j: usize,
    p: T<'h>,
    q: T<'h>,
}

fn occurrence<'h>(h: &'h Heap, u: Sym, derivative: T<'h>, e: T<'h>) -> Option<Occ<'h>> {
    let View::App { head, args: &[p, q] } = h.view(e) else { return None };
    match h.view(head) {
        View::Sym(s) if s == u => Some(Occ { i: 0, j: 0, p, q }),
        View::App { head: op, args: &[f] } => {
            let View::App { head: d, args: &[i, j] } = h.view(op) else { return None };
            let (View::Int(i @ 0..=2), View::Int(j @ 0..=2), View::Sym(s)) = (h.view(i), h.view(j), h.view(f)) else { return None };
            (d == derivative && s == u).then_some(Occ { i: i as usize, j: j as usize, p, q })
        }
        _ => None,
    }
}

/// Every occurrence of `u` in `e`; `None` when `u` occurs in another way.
fn occurrences<'h>(h: &'h Heap, u: Sym, derivative: T<'h>, e: T<'h>) -> Option<Vec<Occ<'h>>> {
    let mut out = vec![];
    let mut stack = vec![e];
    while let Some(t) = stack.pop() {
        if let Some(o) = occurrence(h, u, derivative, t) {
            out.push(o);
            continue;
        }
        match h.view(t) {
            View::Sym(s) if s == u => return None,
            View::App { head, args } => {
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    Some(out)
}

/// The unknown of `n_dsolve_value(eqs, u, ...)`: the function of the first `derivative(i, j)(u)` with two orders.
pub(crate) fn unknown<'h>(h: &'h Heap, eqs: T<'h>) -> Option<Sym> {
    let derivative = h.sym_term(h.sym("derivative"));
    let mut stack = vec![eqs];
    while let Some(t) = stack.pop() {
        let View::App { head, args } = h.view(t) else { continue };
        if let View::App { head: op, args: &[f] } = h.view(head) {
            if let (View::App { head: d, args: &[_, _] }, View::Sym(s)) = (h.view(op), h.view(f)) {
                if d == derivative {
                    return Some(s);
                }
            }
        }
        stack.extend(args.iter().rev());
        stack.push(head);
    }
    None
}

/// A boundary condition: Dirichlet (`false`) or Neumann (`true`), and its datum g(t).
type Boundary<'h> = (bool, T<'h>);

/// The method-of-lines system of `eqs` for the unknown `u` over `[x, a, b]` with the time variable `t` from `t0`;
/// `None` when the equations are not of the supported form (the call stays unevaluated).
#[allow(clippy::too_many_arguments)]
pub(crate) fn system<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    eqs: T<'h>,
    u: Sym,
    t: Sym,
    t0: &Num,
    x: Sym,
    (a, b): (f64, f64),
) -> Res<Option<System<'h>>> {
    let h = cx.heap();
    let derivative = h.sym_term(h.sym("derivative"));
    let equal = h.sym_term(h.sym("equal"));
    let (tt, xt) = (h.sym_term(t), h.sym_term(x));
    let t0f = real(t0).expect("a real start");
    let at = |cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>| -> Res<Option<f64>> { Ok(number(cx, ar, v, None)?.as_ref().and_then(real)) };
    let mut pde: Option<T<'h>> = None;
    let mut order = 0;
    let mut initial: [Option<T<'h>>; 2] = [None, None];
    let mut ends: [Option<Boundary<'h>>; 2] = [None, None];
    for e in system::equations(cx, eqs)? {
        let View::App { head, args: &[l, r] } = h.view(e) else { return Ok(None) };
        if head != equal {
            return Ok(None);
        }
        let Some(occ) = occurrences(h, u, derivative, e) else { return Ok(None) };
        if occ.is_empty() {
            return Ok(None);
        }
        if occ.iter().all(|o| o.p == tt && o.q == xt) {
            if pde.is_some() || occ.iter().any(|o| o.i > 0 && o.j > 0) {
                return Ok(None);
            }
            order = occ.iter().map(|o| o.i).max().unwrap_or(0);
            if order == 0 {
                return Ok(None);
            }
            pde = Some(ar.sub(l, r));
            continue;
        }
        // a condition: one side the single occurrence, the other its datum
        let (o, v) = match (occurrence(h, u, derivative, l), occurrence(h, u, derivative, r)) {
            (Some(o), _) if occ.len() == 1 => (o, r),
            (_, Some(o)) if occ.len() == 1 => (o, l),
            _ => return Ok(None),
        };
        if o.q == xt && o.j == 0 && o.i <= 1 && o.p != tt {
            // an initial value at t0
            if at(cx, o.p)? != Some(t0f) || mentions(cx, v, &[t])? || initial[o.i].is_some() {
                return Ok(None);
            }
            initial[o.i] = Some(v);
        } else if o.p == tt && o.i == 0 && o.j <= 1 && o.q != xt {
            let Some(q) = at(cx, o.q)? else { return Ok(None) };
            let side = if q == a { 0 } else if q == b { 1 } else { return Ok(None) };
            if ends[side].is_some() || mentions(cx, v, &[x])? {
                return Ok(None);
            }
            ends[side] = Some((o.j == 1, v));
        } else {
            return Ok(None);
        }
    }
    let (Some(pde), Some(left), Some(right)) = (pde, ends[0], ends[1]) else { return Ok(None) };
    if order > 2 || initial[0].is_none() || (order == 2) != initial[1].is_some() {
        return Ok(None);
    }
    // the highest time derivative H, and the placeholders of u, u_x, u_xx and u_t in F
    let place = |name: &str| h.sym(&format!("ode·pde·{name}"));
    let (hs, us, uxs, uxxs, uts) = (place("h"), place("u"), place("ux"), place("uxx"), place("ut"));
    let swapped = mc_numfn::replace(h, pde, &mut |e| {
        let o = occurrence(h, u, derivative, e)?;
        Some(h.sym_term(match (o.i, o.j) {
            (i, 0) if i == order => hs,
            (0, 0) => us,
            (0, 1) => uxs,
            (0, 2) => uxxs,
            (1, 0) => uts,
            _ => return None,
        }))
    });
    let swapped = cx.eval(swapped)?;
    if mentions(cx, swapped, &[u])? {
        return Ok(None);
    }
    let coef = cx.eval(h.call(h.sym("d"), &[swapped, h.sym_term(hs)]))?;
    if mentions(cx, coef, &[hs, h.sym("d"), h.sym("derivative")])? || ar.num(coef).is_some_and(|n| n.is_exact_zero()) {
        return Ok(None);
    }
    let rest = mc_numfn::replace(h, swapped, &mut |e| match h.view(e) {
        View::Sym(s) if s == hs => Some(h.int(0)),
        _ => None,
    });
    let f = cx.eval(ar.neg(ar.div(rest, coef)))?;
    if mentions(cx, f, &[hs])? {
        return Ok(None);
    }
    // the grid and the state: node j holds u (and u_t for order 2)
    let m = INTERVALS;
    let step = (b - a) / m as f64;
    let xs: Vec<f64> = (0..=m).map(|j| if j == m { b } else { a + step * j as f64 }).collect();
    let name = h.sym_name(u);
    let n = (m + 1) * order;
    let state: Vec<Sym> = (1..=n).map(|k| h.sym(&format!("ode·s{k}"))).collect();
    let unknowns: Vec<Unknown> = (0..=m)
        .map(|j| Unknown { sym: h.sym(&crate::vector::component_name(&name, j + 1)), order, at: j * order })
        .collect();
    let s = |j: usize| h.sym_term(state[j * order]);
    let v = |j: usize| h.sym_term(state[j * order + 1]);
    let hh = h.real(step);
    // the value at a neighbour of node j, a ghost node past a Neumann end
    let neighbour = |j: usize, up: bool| -> T<'h> {
        match (j, up) {
            (0, false) => ar.sub(s(1), ar.mul(&[h.int(2), hh, left.1])),
            (j, true) if j == m => ar.add(&[s(m - 1), ar.mul(&[h.int(2), hh, right.1])]),
            (j, false) => s(j - 1),
            (j, true) => s(j + 1),
        }
    };
    let mut rhs = Vec::with_capacity(n);
    let mut init = Vec::with_capacity(n);
    for (j, &xj) in xs.iter().enumerate() {
        let end = if j == 0 { Some(left) } else if j == m { Some(right) } else { None };
        let xv = h.real(xj);
        let dirichlet = end.filter(|e| !e.0).map(|e| e.1);
        let (rate, u0, ut0) = match dirichlet {
            Some(g) => {
                // the node follows its datum: u' = g'(t) (u'' = g''(t)) from g(t0) (and g'(t0))
                let spec = if order == 1 { tt } else { cx.list(&[tt, h.int(2)]) };
                let dg = cx.eval(h.call(h.sym("d"), &[g, spec]))?;
                let g1 = cx.eval(h.call(h.sym("d"), &[g, tt]))?;
                let at_t0 = |w: T<'h>| mc_numfn::replace(h, w, &mut |e| (e == tt).then(|| ar.number(t0)));
                (dg, at_t0(g), (order == 2).then(|| at_t0(g1)))
            }
            None => {
                let (lo, hi) = (neighbour(j, false), neighbour(j, true));
                // sum_k c_k u[j_k] / (12 h^p): a stencil of fourth order
                let comb = |cs: &[(i64, usize)], p: i64| {
                    let terms: Vec<T<'h>> = cs.iter().map(|&(c, k)| ar.mul(&[h.int(c), s(k)])).collect();
                    ar.div(ar.add(&terms), ar.mul(&[h.int(12), ar.pow(hh, h.int(p))]))
                };
                let (fixed_left, fixed_right) = (!left.0, !right.0);
                let (ux, uxx) = if j >= 2 && j + 2 <= m {
                    // central five-point differences
                    (comb(&[(1, j - 2), (-8, j - 1), (8, j + 1), (-1, j + 2)], 1), comb(&[(-1, j - 2), (16, j - 1), (-30, j), (16, j + 1), (-1, j + 2)], 2))
                } else if j == 1 && fixed_left {
                    // next to a fixed end: the one-sided six-point (five-point for u_x) differences
                    (comb(&[(-3, 0), (-10, 1), (18, 2), (-6, 3), (1, 4)], 1), comb(&[(10, 0), (-15, 1), (-4, 2), (14, 3), (-6, 4), (1, 5)], 2))
                } else if j + 1 == m && fixed_right {
                    (comb(&[(3, m), (10, m - 1), (-18, m - 2), (6, m - 3), (-1, m - 4)], 1), comb(&[(10, m), (-15, m - 1), (-4, m - 2), (14, m - 3), (-6, m - 4), (1, m - 5)], 2))
                } else {
                    // at a Neumann end and next to it: the three-point differences with the ghost node
                    (
                        ar.div(ar.sub(hi, lo), ar.mul(&[h.int(2), hh])),
                        ar.div(ar.add(&[hi, ar.mul(&[h.int(-2), s(j)]), lo]), ar.pow(hh, h.int(2))),
                    )
                };
                let ut = if order == 2 { v(j) } else { h.int(0) };
                let body = mc_numfn::replace(h, f, &mut |e| match h.view(e) {
                    View::Sym(q) if q == us => Some(s(j)),
                    View::Sym(q) if q == uxs => Some(ux),
                    View::Sym(q) if q == uxxs => Some(uxx),
                    View::Sym(q) if q == uts => Some(ut),
                    View::Sym(q) if q == x => Some(xv),
                    _ => None,
                });
                let at_x = |w: T<'h>| mc_numfn::replace(h, w, &mut |e| (e == xt).then_some(xv));
                (body, at_x(initial[0].expect("checked")), initial[1].map(at_x))
            }
        };
        if order == 2 {
            rhs.push(v(j));
        }
        rhs.push(rate);
        for w in std::iter::once(u0).chain(ut0) {
            match number(cx, ar, w, None)? {
                Some(val) if real(&val).is_some() => init.push(val),
                _ => return Ok(None),
            }
        }
    }
    // the rates of the derivatives were pushed after the values for order 2: [v, F] per node
    let rhs = cx.eval(cx.list(&rhs))?;
    if mentions(cx, rhs, &[us, uxs, uxxs, uts, x, u])? {
        return Ok(None);
    }
    Ok(Some(System {
        t,
        unknowns,
        state,
        rhs,
        start: ar.number(t0),
        initial: init,
        initial_terms: vec![],
        free: vec![],
        targets: vec![],
        space: Some(xs),
    }))
}

/// The value at x of the node values `ys` at the nodes `xs` (equally spaced): the cubic through the four nearest
/// nodes (the parabola or line for fewer nodes).
pub(crate) fn interpolate(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    let m = xs.len();
    if m == 1 {
        return ys[0];
    }
    let step = (xs[m - 1] - xs[0]) / (m - 1) as f64;
    let k = (((x - xs[0]) / step).floor() as isize).clamp(0, m as isize - 2) as usize;
    let lo = k.saturating_sub(1).min(m.saturating_sub(4));
    let hi = (lo + 4).min(m);
    let mut v = 0.0;
    for i in lo..hi {
        let mut w = 1.0;
        for jx in lo..hi {
            if jx != i {
                w *= (x - xs[jx]) / (xs[i] - xs[jx]);
            }
        }
        v += w * ys[i];
    }
    v
}

/// `interpolating_function(name, [[t0, t1], [a, b]], steps, key)`.
pub(crate) fn function_term<'h>(h: &'h Heap, sol: &crate::solution::Solution, key: u64, name: &str) -> T<'h> {
    let xs = sol.space.as_ref().expect("a space grid");
    let pair = |p: f64, q: f64| h.call(h.sym(names::LIST), &[h.real(p), h.real(q)]);
    let dom = h.call(h.sym(names::LIST), &[pair(sol.lo, sol.hi), pair(xs[0], xs[xs.len() - 1])]);
    h.call(h.sym("interpolating_function"), &[h.string(name), dom, h.int(sol.steps as i64), h.string(&format!("{key:016x}"))])
}
