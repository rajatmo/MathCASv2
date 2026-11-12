//! Delay equations `y'(t) = f(t, y(t), y(t - tau))` (tau a positive number) with a history `y(t when t <= t0) == phi`
//! on a range `[t, t0, t1]`, by the method of steps: on each interval of length tau the delayed term is known from
//! the previous interval, so the equation is an ODE (a quadrature when `y(t)` itself does not occur), started at the
//! end value of the previous piece. The result is a `piecewise` over the intervals (`indeterminate` beyond t1).
use super::{general, occurrences, Cx, Ode, Out, Sol};
use mc_calculus::common::is_free;
use mc_api::{Store, View, T};
use mc_eval::Res;

/// Does the equation hold `y` at a shifted argument `y(t - tau)`?
pub(crate) fn is_delay<'h>(c: &Cx<'_, '_, '_, '_, 'h>, all: &[T<'h>], y: T<'h>, x: T<'h>) -> bool {
    let b = c.b;
    all.iter().any(|&e| {
        let mut occ = vec![];
        occurrences(b, e, y, &mut occ);
        occ.iter().any(|o| o.1 != x && !is_free(b, o.1, x) && !c.head_is(o.1, "condition")) && occ.iter().any(|o| o.1 == x)
    })
}

/// Method of steps.
pub(crate) fn solve<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, all: &[T<'h>], y: T<'h>, t: T<'h>, range: Option<T<'h>>) -> Res<Option<Out<'h>>> {
    let b = c.b;
    let h = b.heap();
    let Some(r) = range.and_then(|r| c.items(r)) else { return Ok(None) };
    let [_, _, t1] = r.as_slice() else { return Ok(None) };
    let mut eq = None;
    let mut hist = None;
    for &e in all {
        let View::App { args: &[l, rr], .. } = h.view(e) else { return Ok(None) };
        let mut occ = vec![];
        occurrences(b, e, y, &mut occ);
        if let Some(&(0, arg, _)) = occ.iter().find(|o| c.head_is(o.1, "condition")) {
            // y(t when t <= t0) == phi (the argument a `condition`)
            let View::App { args: &[_, cond], .. } = h.view(arg) else { return Ok(None) };
            let View::App { args: &[cl, t0], .. } = h.view(cond) else { return Ok(None) };
            if cl != t || !c.head_is(cond, "less_equal") {
                return Ok(None);
            }
            let phi = if super::occurrence(b, l, y).is_some() { rr } else { l };
            hist = Some((t0, phi));
        } else if eq.is_none() {
            eq = Some((e, occ));
        } else {
            return Ok(None);
        }
    }
    let (Some((e, occ)), Some((t0, phi))) = (eq, hist) else { return Ok(None) };
    // the delay tau: one shifted argument t - tau
    let mut tau = None;
    for o in &occ {
        if o.1 != t {
            if o.0 != 0 {
                return Ok(None);
            }
            let d = c.simp(b.sub(t, o.1))?;
            match b.ar.num(d) {
                Some(mc_arith::Num::Real(v)) if v.to_f64() > 0.0 => {}
                _ => return Ok(None),
            }
            if tau.is_some_and(|tt| tt != d) {
                return Ok(None);
            }
            tau = Some(d);
        }
    }
    let Some(tau) = tau else { return Ok(None) };
    let steps = c.simp(b.div(b.sub(*t1, t0), tau))?;
    let n = match c.ev("ceiling", &[steps]).map(|v| h.view(v)) {
        Ok(View::Int(k)) if (1..=8).contains(&k) => k as usize,
        _ => return Ok(None),
    };
    // y' = f: solve for the derivative
    let ys = super::ysyms(c, 1);
    let lag = c.sym("d_solve·lag");
    let mut pairs = vec![];
    for o in &occ {
        let s = if o.1 != t { lag } else { ys[o.0] };
        pairs.push((o.2, s));
    }
    let View::App { args: &[l, rr], .. } = h.view(e) else { return Ok(None) };
    let diff = c.eval(super::replace_terms(b, b.sub(l, rr), &pairs))?;
    let mut pieces = vec![];
    let mut prev = phi;
    let mut start = t0;
    let mut y0 = c.subst(phi, &[c.rule(t, t0)])?;
    for _ in 0..n {
        if c.late() {
            return Ok(None);
        }
        let known = c.subst(prev, &[c.rule(t, b.sub(t, tau))])?;
        let d = c.subst(diff, &[c.rule(lag, known)])?;
        let d = c.expand(d)?;
        let piece = if is_free(b, d, ys[0]) {
            // a quadrature from the start
            let eq = c.equal(d, b.zero());
            let Some(v) = c.solve_for(eq, ys[1])? else { return Ok(None) };
            let [g] = v.as_slice() else { return Ok(None) };
            let Some(ig) = c.integrate(*g, t)? else { return Ok(None) };
            let at = c.subst(ig, &[c.rule(t, start)])?;
            c.eval(b.add(&[y0, ig, b.neg(at)]))?
        } else {
            let ode = Ode { diff: d, ys: ys.clone(), x: t, n: 1 };
            let Some(gs) = general(c, &ode)? else { return Ok(None) };
            let [Sol::E(g)] = gs.as_slice() else { return Ok(None) };
            let Some(v) = super::conditions(c, *g, &[(0, start, y0)], t, 1)? else { return Ok(None) };
            let [v] = v.as_slice() else { return Ok(None) };
            *v
        };
        let piece = c.expand(piece)?;
        let piece = c.together(piece)?;
        let end = c.eval(b.add(&[start, tau]))?;
        pieces.push((start, end, piece));
        y0 = c.subst(piece, &[c.rule(t, end)])?;
        y0 = c.simp(y0)?;
        prev = piece;
        start = end;
    }
    let mut cases = vec![c.list(&[phi, b.call("less_equal", &[t, t0])])];
    for (s, e, p) in pieces {
        let cond = b.call("and", &[b.call("less", &[s, t]), b.call("less_equal", &[t, e])]);
        cases.push(c.list(&[p, cond]));
    }
    let pw = b.call("piecewise", &[c.list(&cases), c.sym("indeterminate")]);
    let pw = c.eval(pw)?;
    Ok(Some(Out::Sols(vec![vec![pw]])))
}
