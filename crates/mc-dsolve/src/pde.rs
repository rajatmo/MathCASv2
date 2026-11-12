//! Partial differential equations for `u(x, y)`.
//!
//! - First order, linear: `a u_x + b u_y + c u = f` by characteristics. The characteristic equation
//!   `y' = b/a` is solved by the first-order solver; its constant gives the characteristic variable `xi(x, y)`;
//!   along a characteristic the equation is the linear ODE `a u' + c u = f`, whose constant becomes an arbitrary
//!   function `c(1)(xi)`.
//! - Second order, constant coefficients, homogeneous and without lower-order terms:
//!   `A u_xx + B u_xy + C u_yy = 0` with two distinct roots of `A l^2 + B l + C` gives
//!   `c(1)(y + l1 x) + c(2)(y + l2 x)` (the wave equation among them).
use super::{first, Cx, Out, Sol};
use mc_calculus::common::is_free;
use mc_api::{Store, View, T};
use mc_eval::Res;

/// The orders `(i, j)` of `t` when it is `u(x, y)` or `derivative(i, j)(u)(x, y)`.
fn occurrence<'h>(c: &Cx<'_, '_, '_, '_, 'h>, t: T<'h>, u: T<'h>, vars: &[T<'h>]) -> Option<(i64, i64)> {
    let b = c.b;
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return None };
    if args != vars {
        return None;
    }
    if head == u {
        return Some((0, 0));
    }
    let (orders, f) = b.as_derivative(head)?;
    match orders.as_slice() {
        &[i, j] if f == u => Some((i, j)),
        _ => None,
    }
}

/// Solve the PDE (no conditions).
pub(crate) fn solve<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, all: &[T<'h>], u: T<'h>, vars: &[T<'h>]) -> Res<Option<Out<'h>>> {
    let b = c.b;
    let h = b.heap();
    let [eq] = all else { return Ok(None) };
    let [x, y] = vars else { return Ok(None) };
    let (x, y) = (*x, *y);
    // occurrences
    let mut occ: Vec<((i64, i64), T<'h>)> = vec![];
    let mut stack = vec![*eq];
    while let Some(t) = stack.pop() {
        if let Some(o) = occurrence(c, t, u, vars) {
            if !occ.iter().any(|p| p.1 == t) {
                occ.push((o, t));
            }
            continue;
        }
        if let View::App { head, args } = h.view(t) {
            if head == u || b.as_derivative(head).is_some_and(|d| d.1 == u) {
                // u at other arguments
                return Ok(None);
            }
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    let order = occ.iter().map(|o| o.0 .0 + o.0 .1).max().unwrap_or(0);
    let syms: Vec<(T<'h>, (i64, i64))> = occ.iter().map(|o| (c.sym(&format!("d_solve·u{}{}", o.0 .0, o.0 .1)), o.0)).collect();
    let pairs: Vec<(T<'h>, T<'h>)> = occ.iter().zip(&syms).map(|(o, s)| (o.1, s.0)).collect();
    let View::App { args: &[l, r], .. } = h.view(*eq) else { return Ok(None) };
    let diff = c.eval(super::replace_terms(b, b.sub(l, r), &pairs))?;
    let diff = c.expand(diff)?;
    let sym_of = |o: (i64, i64)| syms.iter().find(|s| s.1 == o).map(|s| s.0);
    let all_syms: Vec<T<'h>> = syms.iter().map(|s| s.0).collect();
    // linear: coefficients free of the symbols
    let coef = |c: &mut Cx<'_, '_, '_, '_, 'h>, o: (i64, i64)| -> Res<Option<T<'h>>> {
        let Some(s) = sym_of(o) else { return Ok(Some(c.b.zero())) };
        let d = c.d(diff, s)?;
        let d = c.simp(d)?;
        Ok(if c.free_all(d, &all_syms) { Some(d) } else { None })
    };
    let zeros: Vec<T<'h>> = all_syms.iter().map(|&s| c.rule(s, b.zero())).collect();
    let f0 = c.subst(diff, &zeros)?;
    let f = c.simp(b.neg(f0))?;
    let sol = if order == 1 {
        let (Some(a), Some(bb), Some(cc)) = (coef(c, (1, 0))?, coef(c, (0, 1))?, coef(c, (0, 0))?) else { return Ok(None) };
        if a == b.zero() && bb == b.zero() {
            return Ok(None);
        }
        let s = if a != b.zero() {
            first_order(c, x, y, a, bb, cc, f)?
        } else {
            // a u_x absent: the roles of x and y swapped
            first_order(c, y, x, bb, a, cc, f)?
        };
        let Some(s) = s else { return Ok(None) };
        s
    } else if order == 2 {
        if syms.iter().any(|s| s.1 .0 + s.1 .1 < 2) || f != b.zero() {
            return Ok(None);
        }
        let (Some(aa), Some(bb), Some(cc)) = (coef(c, (2, 0))?, coef(c, (1, 1))?, coef(c, (0, 2))?) else { return Ok(None) };
        if !c.free_all(aa, &[x, y]) || !c.free_all(bb, &[x, y]) || !c.free_all(cc, &[x, y]) || aa == b.zero() {
            return Ok(None);
        }
        // F(y + l x): F'' (A l^2 + B l + C) = 0
        let Some(rts) = super::linear::roots(c, &[cc, bb, aa])? else { return Ok(None) };
        let [(l1, 1), (l2, 1)] = rts.as_slice() else { return Ok(None) };
        let t1 = h.app(c.constant(1), &[c.eval(b.add(&[y, b.mul(&[*l1, x])]))?]);
        let t2 = h.app(c.constant(2), &[c.eval(b.add(&[y, b.mul(&[*l2, x])]))?]);
        c.eval(b.add(&[t1, t2]))?
    } else {
        return Ok(None);
    };
    // check: the residual simplifies to 0 (arbitrary functions stay symbolic)
    let mut rules = vec![];
    for s in &syms {
        let (i, j) = s.1;
        let mut dv = sol;
        if i > 0 {
            dv = c.dn(dv, x, i as usize)?;
        }
        if j > 0 {
            dv = c.dn(dv, y, j as usize)?;
        }
        rules.push(c.rule(s.0, dv));
    }
    let res = c.subst(diff, &rules)?;
    if !c.is_zero(res)? {
        let e = c.expand(res)?;
        if !c.is_zero(e)? {
            return Ok(None);
        }
    }
    Ok(Some(Out::Sols(vec![vec![sol]])))
}

/// `a u_x + b u_y + c u = f` by characteristics (x the parameter along them).
fn first_order<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, a: T<'h>, bb: T<'h>, cc: T<'h>, f: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let w = c.sym("d_solve·w·pde");
    let s = c.sym("d_solve·s·pde");
    let c1 = c.constant(1);
    // the characteristics: w' = b/a
    let g = c.simp(b.div(bb, a))?;
    let g = c.subst(g, &[c.rule(y, w)])?;
    c.depth += 1;
    let ch = first::explicit(c, x, w, g);
    c.depth -= 1;
    let Some(ch) = ch? else { return Ok(None) };
    // xi(x, w): the constant of the characteristic through (x, w); W(x, s): the characteristic of constant s
    let (xi, wx) = match ch.as_slice() {
        [Sol::E(e)] => {
            let eq = c.equal(*e, w);
            let Some(v) = c.solve_for(eq, c1)? else { return Ok(None) };
            let [xi] = v.as_slice() else { return Ok(None) };
            (*xi, c.subst(*e, &[c.rule(c1, s)])?)
        }
        _ => return Ok(None),
    };
    let xi = c.simp(xi)?;
    // along it: a u' + c u = f with w = W(x, s)
    let on = |c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>| -> Res<T<'h>> {
        let t = c.subst(t, &[c.rule(y, wx)])?;
        c.simp(t)
    };
    let a1 = on(c, a)?;
    let c0 = on(c, cc)?;
    let f1 = on(c, f)?;
    c.depth += 1;
    let r = super::linear::solve(c, &[c0, a1], f1, x);
    c.depth -= 1;
    let Some(sol) = r? else { return Ok(None) };
    // the constant becomes an arbitrary function of xi, s becomes xi
    let h = b.heap();
    let fxi = h.app(c1, &[xi]);
    let sol = super::replace_terms(b, sol, &[(c1, fxi)]);
    let sol = c.subst(sol, &[c.rule(s, xi)])?;
    let sol = c.subst(sol, &[c.rule(w, y)])?;
    let sol = c.simp(sol)?;
    if is_free(b, sol, h.sym_term(h.sym("c"))) && is_free(b, sol, c.gen) {
        return Ok(None);
    }
    Ok(Some(sol))
}
