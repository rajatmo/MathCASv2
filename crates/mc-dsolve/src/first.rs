//! Nonlinear first-order equations, and second-order (or higher) equations reduced to first order.
//!
//! The equation is solved for `y'` (every branch `solve` gives); each `y' = G(x, y)` goes through, in order:
//! quadrature, separable (`integrate(1/h, y) == integrate(g, x) + c(1)`), linear in `y`, Bernoulli
//! (`y' = p y + q y^k`, `v = y^(1-k)`), Riccati (`y' = a + b y + c y^2` through `u'' - (b + c'/c) u' + a c u = 0`,
//! `y = -u'/(c u)`), homogeneous (`G(x, v x)` free of x, `v = y/x`), isobaric (`G(t x, t^m y) = t^(m-1) G`,
//! `y = v x^m`), exact (`M + N y' = 0` with `M_y = N_x`, after an integrating factor in x or in y when one exists),
//! and an integrating factor `1/(eta - xi G)` from a Lie point symmetry `(xi, eta)` found from a linear ansatz.
//! Clairaut equations `y = x y' + f(y')` give the family `y = c x + f(c)`.
//!
//! Implicit solutions are solved for `y` when `solve` can (after exponentiating when logarithms of `y` occur,
//! `exp(c(1))` renamed `c(1)`); otherwise they are returned as equations.
//!
//! Higher order: an equation without `y` is solved for `p = y'` (one order lower) and integrated; an autonomous
//! second-order equation is solved for `p(y) = y'` (`y'' = p dp/dy`), then `y' = p(y)` is separable.
use super::{general, ysyms, Cx, Ode, Sol};
use mc_calculus::common::is_free;
use mc_api::{Store, View, T};
use mc_eval::Res;

/// The general solutions of a first-order equation.
pub(crate) fn solve<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ode: &Ode<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    if let Some(s) = clairaut(c, ode)? {
        return Ok(Some(s));
    }
    let (y, p) = (ode.ys[0], ode.ys[1]);
    let eq = c.equal(ode.diff, b.zero());
    let Some(branches) = c.solve_for(eq, p)? else {
        return Ok(None);
    };
    if branches.is_empty() || branches.len() > 4 {
        return Ok(None);
    }
    let mut out = vec![];
    for g in branches {
        if !c.free(g, p) {
            return Ok(None);
        }
        match explicit(c, ode.x, y, g)? {
            Some(v) => out.extend(v),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// `y' = g(x, y)`.
pub(crate) fn explicit<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, g: T<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let g = c.simp(g)?;
    if is_free(b, g, y) {
        let Some(i) = c.integrate(g, x)? else { return Ok(None) };
        return Ok(Some(vec![Sol::E(c.eval(b.add(&[i, c.constant(1)]))?)]));
    }
    // linear in y
    let dg = c.d(g, y)?;
    let dg = c.simp(dg)?;
    if is_free(b, dg, y) {
        let q = c.simp(b.sub(g, b.mul(&[dg, y])))?;
        if is_free(b, q, y) {
            let coefs = [b.neg(dg), b.one()];
            return Ok(super::linear::solve(c, &coefs, q, x)?.map(|s| vec![Sol::E(s)]));
        }
    }
    type Method = for<'a, 'b2, 'c2, 'k, 'h2> fn(&mut Cx<'a, 'b2, 'c2, 'k, 'h2>, T<'h2>, T<'h2>, T<'h2>) -> Res<Option<Vec<Sol<'h2>>>>;
    let methods: [Method; 8] = [separable, bernoulli, linear_argument, riccati, homogeneous, isobaric, exact, symmetry];
    // the methods at once (roadmap item 16A), in this order until one gives an explicit solution; the first implicit
    // one otherwise. A method started past the deadline gives nothing, as the loop stopped there.
    let explicit = |s: &Option<Vec<Sol<'h>>>| s.as_ref().is_some_and(|s| s.iter().any(|v| matches!(v, Sol::E(_))));
    let found = c.par_until(methods.len(), &explicit, &|c, i| if c.late() { Ok(None) } else { methods[i](c, x, y, g) })?;
    let mut implicit = None;
    for s in found.into_iter().flatten() {
        if s.iter().any(|v| matches!(v, Sol::E(_))) {
            return Ok(Some(s));
        }
        if implicit.is_none() {
            implicit = Some(s);
        }
    }
    Ok(implicit)
}

/// `lhs == rhs` (an implicit solution) solved for `y`: explicit branches, or the equation.
pub(crate) fn implicit<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, lhs: T<'h>, rhs: T<'h>, y: T<'h>) -> Res<Vec<Sol<'h>>> {
    let b = c.b;
    let eq = c.equal(lhs, rhs);
    if let Some(v) = c.solve_for(eq, y)? {
        let v: Vec<T<'h>> = v.into_iter().filter(|&s| is_free(b, s, y)).collect();
        if !v.is_empty() {
            return Ok(v.into_iter().map(Sol::E).collect());
        }
    }
    // logarithms of y: exponentiate (exp(c(1)) becomes the constant c(1))
    let log = c.sym("log");
    if !is_free(b, lhs, log) {
        // the constant added to the right-hand side
        let Some(c1) = (1..=super::MAX_ORDER).map(|i| c.constant(i)).find(|&k| !is_free(b, rhs, k)) else {
            return Ok(vec![Sol::I(c.equal(lhs, rhs))]);
        };
        let r0 = c.simp(b.sub(rhs, c1))?;
        if is_free(b, r0, c1) {
            let el = c.simp(b.call("exp", &[lhs]))?;
            let er = c.simp(b.call("exp", &[r0]))?;
            if is_free(b, el, log) || is_free(b, c.ev("power_expand", &[el])?, log) {
                let eq2 = c.equal(el, b.mul(&[c1, er]));
                if let Some(v) = c.solve_for(eq2, y)? {
                    let v: Vec<T<'h>> = v.into_iter().filter(|&s| is_free(b, s, y)).collect();
                    if !v.is_empty() {
                        return Ok(v.into_iter().map(Sol::E).collect());
                    }
                }
            }
        }
    }
    Ok(vec![Sol::I(c.equal(lhs, rhs))])
}

/// `g = g1(x) h(y)`: `integrate(1/h, y) == integrate(g1, x) + c(1)`.
fn separable<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, g: T<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    // h(y) = g at a point x* where it is finite and nonzero; g/h must be free of y
    let mut hy = None;
    for xs in [b.int(1), b.int(2), b.rat(1, 3), b.int(0), b.int(3)] {
        let at = c.subst(g, &[c.rule(x, xs)])?;
        if !c.bad(at) && at != b.zero() {
            hy = Some(at);
            break;
        }
    }
    let Some(hy) = hy else { return Ok(None) };
    let g1 = c.simp(b.div(g, hy))?;
    if !is_free(b, g1, y) {
        return Ok(None);
    }
    let Some(rhs) = c.integrate(g1, x)? else { return Ok(None) };
    let rhs = b.add(&[rhs, c.constant(1)]);
    let rhs = c.eval(rhs)?;
    let Some(lhs) = c.integrate(b.powi(hy, -1), y)? else {
        // the integral of 1/h(y) has no closed form: the implicit solution keeps it as an integral
        // (`integrate(1/(y + exp(y)), y) == x + c(1)`)
        return Ok(Some(vec![Sol::I(c.equal(b.call("integrate", &[b.powi(hy, -1), y]), rhs))]));
    };
    Ok(Some(implicit(c, lhs, rhs, y)?))
}

/// `g` as `sum g_e(x) y^e`: the pairs `(e, g_e)`, `None` unless every exponent is a number.
fn powers<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, g: T<'h>, y: T<'h>) -> Res<Option<Vec<(T<'h>, T<'h>)>>> {
    let b = c.b;
    let e = c.expand(g)?;
    let mut out: Vec<(T<'h>, T<'h>)> = vec![];
    for t in c.terms(e) {
        let dt = c.d(t, y)?;
        let k = c.simp(b.div(b.mul(&[y, dt]), t))?;
        if b.ar.num(k).is_none() {
            return Ok(None);
        }
        let coef = c.simp(b.div(t, b.pow(y, k)))?;
        if !is_free(b, coef, y) {
            return Ok(None);
        }
        match out.iter_mut().find(|o| o.0 == k) {
            Some(o) => o.1 = b.add(&[o.1, coef]),
            None => out.push((k, coef)),
        }
    }
    for o in out.iter_mut() {
        o.1 = c.simp(o.1)?;
    }
    out.retain(|o| o.1 != b.zero());
    Ok(Some(out))
}

/// Bernoulli: `y' = p y + q y^k`.
fn bernoulli<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, g: T<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let Some(ps) = powers(c, g, y)? else { return Ok(None) };
    let one = b.one();
    let others: Vec<&(T<'h>, T<'h>)> = ps.iter().filter(|o| o.0 != one).collect();
    let [&(k, q)] = others.as_slice() else { return Ok(None) };
    if k == b.zero() {
        return Ok(None);
    }
    let p = ps.iter().find(|o| o.0 == one).map_or(b.zero(), |o| o.1);
    // v = y^(1-k): v' = (1-k) (p v + q)
    let m = c.simp(b.sub(one, k))?;
    let coefs = [b.neg(b.mul(&[m, p])), one];
    let Some(v) = super::linear::solve(c, &coefs, b.mul(&[m, q]), x)? else { return Ok(None) };
    let e = c.simp(b.pow(m, b.int(-1)))?;
    let yv = c.simp(b.pow(v, e))?;
    // an even root: both signs
    let even = matches!(b.heap().view(e), View::Rational { den, .. } if b.ar.num(den).is_some_and(|d| matches!(d, mc_arith::Num::Real(r) if (r.to_f64() as i64) % 2 == 0)));
    if even {
        return Ok(Some(vec![Sol::E(c.simp(b.neg(yv))?), Sol::E(yv)]));
    }
    Ok(Some(vec![Sol::E(yv)]))
}

/// `y' = F(y + l x)` (l = g_x / g_y a constant): `u = y + l x` gives the separable `u' = l + F(u)`.
fn linear_argument<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, g: T<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let gx = c.d(g, x)?;
    if gx == b.zero() {
        return Ok(None);
    }
    let gy = c.d(g, y)?;
    let l = c.simp(b.div(gx, gy))?;
    if !c.free_all(l, &[x, y]) {
        return Ok(None);
    }
    let u = c.sym("d_solve·u·lin");
    let f = c.subst(g, &[c.rule(y, b.sub(u, b.mul(&[l, x])))])?;
    let f = c.simp(f)?;
    if !is_free(b, f, x) {
        return Ok(None);
    }
    let den = c.simp(b.add(&[l, f]))?;
    if den == b.zero() {
        return Ok(None);
    }
    let Some(lhs) = c.integrate(b.powi(den, -1), u)? else { return Ok(None) };
    let lhs = c.subst(lhs, &[c.rule(u, b.add(&[y, b.mul(&[l, x])]))])?;
    let rhs = c.eval(b.add(&[x, c.constant(1)]))?;
    let sols = implicit(c, lhs, rhs, y)?;
    let mut out = vec![];
    for s in sols {
        out.push(match s {
            Sol::E(e) => Sol::E(c.simp(e)?),
            i => i,
        });
    }
    Ok(Some(out))
}

/// Riccati: `y' = a + b y + c y^2` through the linear second-order equation.
fn riccati<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, g: T<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let Some(ps) = powers(c, g, y)? else { return Ok(None) };
    let (zero, one, two) = (b.zero(), b.one(), b.int(2));
    if ps.iter().any(|o| o.0 != zero && o.0 != one && o.0 != two) || !ps.iter().any(|o| o.0 == two) || !ps.iter().any(|o| o.0 == zero) {
        return Ok(None);
    }
    let get = |e: T<'h>| ps.iter().find(|o| o.0 == e).map_or(zero, |o| o.1);
    let (a, bb, cc) = (get(zero), get(one), get(two));
    let dc = c.d(cc, x)?;
    let p1 = c.simp(b.add(&[bb, b.div(dc, cc)]))?;
    let q0 = c.simp(b.mul(&[a, cc]))?;
    let coefs = [q0, b.neg(p1), one];
    c.depth += 1;
    let ys = super::linear::basis(c, &coefs, x);
    c.depth -= 1;
    let Some(ys) = ys? else { return riccati_particular(c, x, a, bb, cc) };
    let [u1, u2] = ys.as_slice() else { return Ok(None) };
    let u = c.eval(b.add(&[b.mul(&[c.constant(1), *u1]), *u2]))?;
    let du = c.d(u, x)?;
    let s = c.simp(b.neg(b.div(du, b.mul(&[cc, u]))))?;
    Ok(Some(vec![Sol::E(s)]))
}

/// Riccati with a particular solution `y1 = k x^m` (m from -2 to 2) found by ansatz: `y = y1 + 1/v` with the linear
/// `v' = -(b + 2 c y1) v - c`.
fn riccati_particular<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, a: T<'h>, bb: T<'h>, cc: T<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let k = c.sym("d_solve·rk");
    for m in [0i64, 1, -1, 2, -2] {
        if c.late() {
            return Ok(None);
        }
        let y1 = b.mul(&[k, b.powi(x, m)]);
        let dy1 = c.d(y1, x)?;
        let r = c.eval(b.sub(dy1, b.add(&[a, b.mul(&[bb, y1]), b.mul(&[cc, b.powi(y1, 2)])])))?;
        let r = c.together(r)?;
        let num = c.ev("numerator", &[r])?;
        let num = c.expand(num)?;
        let cl = c.ev("coefficient_list", &[num, x])?;
        let Some(eqs) = c.items(cl) else { continue };
        if eqs.iter().any(|&e| !is_free(b, e, x)) {
            continue;
        }
        let eqs: Vec<T<'h>> = eqs.into_iter().filter(|&e| e != b.zero()).map(|e| c.equal(e, b.zero())).collect();
        if eqs.is_empty() {
            continue;
        }
        let Some(ks) = c.solve_for(c.list(&eqs), k)? else { continue };
        for kv in ks {
            if kv == b.zero() || !is_free(b, kv, x) {
                continue;
            }
            let y1v = c.subst(y1, &[c.rule(k, kv)])?;
            let p = c.eval(b.add(&[bb, b.mul(&[b.int(2), cc, y1v])]))?;
            let coefs = [p, b.one()];
            let Some(v) = super::linear::solve(c, &coefs, b.neg(cc), x)? else { continue };
            let s = c.simp(b.add(&[y1v, b.powi(v, -1)]))?;
            return Ok(Some(vec![Sol::E(s)]));
        }
    }
    Ok(None)
}

/// Homogeneous: `g(x, v x)` free of x.
fn homogeneous<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, g: T<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let v = c.sym("d_solve·v");
    let f = c.subst(g, &[c.rule(y, b.mul(&[v, x]))])?;
    let f = c.simp(f)?;
    if !is_free(b, f, x) {
        let f2 = c.ev("power_expand", &[f])?;
        let f2 = c.simp(f2)?;
        if !is_free(b, f2, x) {
            return Ok(None);
        }
        return scaled(c, x, y, v, f2, b.one());
    }
    scaled(c, x, y, v, f, b.one())
}

/// After `y = v x^m`: `x v' = f(v) - m v`, separable in v and log(x).
fn scaled<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, v: T<'h>, f: T<'h>, m: T<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let r = c.simp(b.sub(f, b.mul(&[m, v])))?;
    if r == b.zero() {
        // y = c(1) x^m
        return Ok(Some(vec![Sol::E(c.eval(b.mul(&[c.constant(1), b.pow(x, m)]))?)]));
    }
    let Some(lhs) = c.integrate(b.powi(r, -1), v)? else { return Ok(None) };
    let vy = b.mul(&[y, b.pow(x, b.neg(m))]);
    let lhs = c.subst(lhs, &[c.rule(v, vy)])?;
    let rhs = c.eval(b.add(&[b.call("log", &[x]), c.constant(1)]))?;
    let sols = implicit(c, lhs, rhs, y)?;
    let mut out = vec![];
    for s in sols {
        out.push(match s {
            Sol::E(e) => Sol::E(c.simp(e)?),
            i => i,
        });
    }
    Ok(Some(out))
}

/// Isobaric: `g(t x, t^m y) = t^(m-1) g(x, y)` for a rational m (not 1).
fn isobaric<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, g: T<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let t = c.sym("d_solve·t");
    for m in [b.int(2), b.int(-1), b.int(-2), b.rat(1, 2), b.rat(-1, 2), b.int(3), b.int(-3), b.rat(3, 2), b.rat(1, 3), b.rat(2, 3)] {
        if c.late() {
            return Ok(None);
        }
        let s = c.subst(g, &[c.rule(x, b.mul(&[t, x])), c.rule(y, b.mul(&[b.pow(t, m), y]))])?;
        let ratio = c.simp(b.div(s, b.mul(&[b.pow(t, b.sub(m, b.one())), g])))?;
        let ratio = if ratio == b.one() { ratio } else { c.ev("power_expand", &[ratio])? };
        let ratio = c.simp(ratio)?;
        if ratio != b.one() {
            continue;
        }
        let v = c.sym("d_solve·v");
        // x v' = x^(1-m) g(x, v x^m) - m v
        let f = c.subst(g, &[c.rule(y, b.mul(&[v, b.pow(x, m)]))])?;
        let f = c.simp(b.mul(&[f, b.pow(x, b.sub(b.one(), m))]))?;
        let f = c.ev("power_expand", &[f])?;
        let f = c.simp(f)?;
        if !is_free(b, f, x) {
            continue;
        }
        return scaled(c, x, y, v, f, m);
    }
    Ok(None)
}

/// The potential of the exact equation `m + n y' = 0`: `F` with `F_x = m`, `F_y = n`.
fn potential<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, m: T<'h>, n: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let Some(fx) = c.integrate(m, x)? else { return Ok(None) };
    let fy = c.d(fx, y)?;
    let hp = c.simp(b.sub(n, fy))?;
    if !is_free(b, hp, x) {
        return Ok(None);
    }
    let Some(hy) = c.integrate(hp, y)? else { return Ok(None) };
    Ok(Some(c.simp(b.add(&[fx, hy]))?))
}

/// Exact `m + n y' = 0` (g = -m/n), with an integrating factor in x or in y.
fn exact<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, g: T<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let tg = c.together(g)?;
    let num = c.ev("numerator", &[tg])?;
    let den = c.ev("denominator", &[tg])?;
    let m = b.neg(num);
    let n = den;
    let my = c.d(m, y)?;
    let nx = c.d(n, x)?;
    let diff = c.simp(b.sub(my, nx))?;
    let (m, n) = if diff == b.zero() {
        (m, n)
    } else {
        let fx = c.simp(b.div(diff, n))?;
        let fy = c.simp(b.div(b.neg(diff), m))?;
        let mu = if is_free(b, fx, y) {
            let Some(i) = c.integrate(fx, x)? else { return Ok(None) };
            c.simp(b.call("exp", &[i]))?
        } else if is_free(b, fy, x) {
            let Some(i) = c.integrate(fy, y)? else { return Ok(None) };
            c.simp(b.call("exp", &[i]))?
        } else {
            return Ok(None);
        };
        (c.simp(b.mul(&[mu, m]))?, c.simp(b.mul(&[mu, n]))?)
    };
    let Some(f) = potential(c, x, y, m, n)? else { return Ok(None) };
    Ok(Some(implicit(c, f, c.constant(1), y)?))
}

/// An integrating factor from a Lie point symmetry: `xi = a1 + a2 x + a3 y`, `eta = b1 + b2 x + b3 y` solving the
/// linearised symmetry condition `eta_x + (eta_y - xi_x) g - xi_y g^2 - xi g_x - eta g_y = 0`; then
/// `(dy - g dx) / (eta - xi g)` is exact.
fn symmetry<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, x: T<'h>, y: T<'h>, g: T<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let a: Vec<T<'h>> = (0..6).map(|i| c.sym(&format!("d_solve·s·{i}"))).collect();
    let xi = b.add(&[a[0], b.mul(&[a[1], x]), b.mul(&[a[2], y])]);
    let eta = b.add(&[a[3], b.mul(&[a[4], x]), b.mul(&[a[5], y])]);
    let gx = c.d(g, x)?;
    let gy = c.d(g, y)?;
    let e = b.add(&[a[4], b.mul(&[b.sub(a[5], a[1]), g]), b.neg(b.mul(&[a[2], b.powi(g, 2)])), b.neg(b.mul(&[xi, gx])), b.neg(b.mul(&[eta, gy]))]);
    let e = c.eval(e)?;
    let e = c.together(e)?;
    let num = c.ev("numerator", &[e])?;
    let num = c.expand(num)?;
    let cl = c.ev("coefficient_list", &[num, c.list(&[x, y])])?;
    let flat = c.ev("flatten", &[cl])?;
    let Some(eqs) = c.items(flat) else { return Ok(None) };
    if eqs.iter().any(|&q| !is_free(b, q, x) || !is_free(b, q, y)) || eqs.len() > 200 {
        return Ok(None);
    }
    let eqs: Vec<T<'h>> = eqs.into_iter().filter(|&q| q != b.zero()).map(|q| c.equal(q, b.zero())).collect();
    let sol = if eqs.is_empty() {
        vec![]
    } else {
        let s = c.ev("solve", &[c.list(&eqs), c.list(&a)])?;
        let Some(first) = c.items(s).and_then(|v| v.first().copied()) else { return Ok(None) };
        let Some(rules) = c.items(first) else { return Ok(None) };
        rules
    };
    let xi0 = c.subst(xi, &sol)?;
    let eta0 = c.subst(eta, &sol)?;
    // the free parameters: one at a time 1, the rest 0
    let free: Vec<T<'h>> = a.iter().copied().filter(|&ai| !is_free(b, xi0, ai) || !is_free(b, eta0, ai)).collect();
    for &k in &free {
        if c.late() {
            return Ok(None);
        }
        let rules: Vec<T<'h>> = free.iter().map(|&f| c.rule(f, if f == k { b.one() } else { b.zero() })).collect();
        let xs = c.subst(xi0, &rules)?;
        let es = c.subst(eta0, &rules)?;
        let den = c.simp(b.sub(es, b.mul(&[xs, g])))?;
        if den == b.zero() {
            continue;
        }
        let mu = b.powi(den, -1);
        let m = c.simp(b.neg(b.mul(&[g, mu])))?;
        let n = c.simp(mu)?;
        if let Some(f) = potential(c, x, y, m, n)? {
            return Ok(Some(implicit(c, f, c.constant(1), y)?));
        }
    }
    Ok(None)
}

/// Clairaut: `y = x y' + f(y')` gives `y = c(1) x + f(c(1))`.
fn clairaut<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ode: &Ode<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let (x, y, p) = (ode.x, ode.ys[0], ode.ys[1]);
    let dy = c.d(ode.diff, y)?;
    let dy = c.simp(dy)?;
    if dy == b.zero() || !c.free_all(dy, &[x, y, p]) {
        return Ok(None);
    }
    let at0 = c.subst(ode.diff, &[c.rule(y, b.zero())])?;
    let h = c.simp(b.neg(b.div(at0, dy)))?;
    let f = c.simp(b.sub(h, b.mul(&[x, p])))?;
    if !is_free(b, f, x) || is_free(b, f, p) {
        return Ok(None);
    }
    let c1 = c.constant(1);
    let s = c.subst(b.add(&[b.mul(&[c1, x]), f]), &[c.rule(p, c1)])?;
    Ok(Some(vec![Sol::E(c.simp(s)?)]))
}

/// Higher order: without `y` (order lowered by `p = y'`), or autonomous of second order (`p(y) = y'`).
pub(crate) fn reduce<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ode: &Ode<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    let b = c.b;
    let n = ode.n;
    if c.free(ode.diff, ode.ys[0]) {
        c.depth += 1;
        let ps = ysyms(c, n - 1);
        let rules: Vec<T<'h>> = (1..=n).map(|k| c.rule(ode.ys[k], ps[k - 1])).collect();
        let diff = c.subst(ode.diff, &rules);
        let r = match diff {
            Ok(diff) => general(c, &Ode { diff, ys: ps, x: ode.x, n: n - 1 }),
            Err(e) => Err(e),
        };
        c.depth -= 1;
        let Some(sols) = r? else { return Ok(None) };
        let mut out = vec![];
        for s in sols {
            let Sol::E(p) = s else { return Ok(None) };
            let Some(i) = c.integrate(p, ode.x)? else { return Ok(None) };
            out.push(Sol::E(c.eval(b.add(&[i, c.constant(n)]))?));
        }
        return Ok(Some(out));
    }
    if n == 2 && c.free(ode.diff, ode.x) {
        // y'' = p dp/dy, y' = p, in the variable w for y
        let w = c.sym(&format!("d_solve·w·{}", c.depth));
        c.depth += 1;
        let ps = ysyms(c, 1);
        let rules = [c.rule(ode.ys[2], b.mul(&[ps[0], ps[1]])), c.rule(ode.ys[1], ps[0]), c.rule(ode.ys[0], w)];
        let diff = c.subst(ode.diff, &rules);
        let r = match diff {
            Ok(diff) => match c.expand(diff) {
                Ok(diff) => general(c, &Ode { diff, ys: ps, x: w, n: 1 }),
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        };
        c.depth -= 1;
        let Some(sols) = r? else { return Ok(None) };
        let mut out = vec![];
        for s in sols {
            let Sol::E(p) = s else { return Ok(None) };
            if p == b.zero() {
                continue;
            }
            // dy / p(y) = dx: integrate(1/p, y) == x + c(2)
            let py = c.subst(p, &[c.rule(w, ode.ys[0])])?;
            let rhs = c.eval(b.add(&[ode.x, c.constant(2)]))?;
            match c.integrate(b.powi(py, -1), ode.ys[0])? {
                Some(lhs) => out.extend(implicit(c, lhs, rhs, ode.ys[0])?),
                // no closed form (an elliptic integral): the implicit solution keeps the integral
                None => out.push(Sol::I(c.equal(b.call("integrate", &[b.powi(py, -1), ode.ys[0]]), rhs))),
            }
        }
        if out.is_empty() {
            return Ok(None);
        }
        return Ok(Some(out));
    }
    Ok(None)
}
