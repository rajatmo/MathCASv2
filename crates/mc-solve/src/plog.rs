//! Equations with the unknown both inside and outside an exponential: the `product_log` normal form `w e^w = z`
//! (decision 11C.11; Corless, Gonnet, Hare, Jeffrey and Knuth, "On the Lambert W function", Adv. Comput. Math. 5
//! (1996), section 2 on solving `x b^x = a` and `a x + b = c^x` by W).
//!
//! The equation is `alpha(x) K + beta(x) = 0`, linear in one exponential kernel `K = B^g(x)` (B free of x, `e^g`
//! included), so `B^g = R(x)` with `R = -beta/alpha`, and L = log B:
//!
//! - g = c1 x + c0 and R = p x + q linear: with `u = p x + q` and `w = -c1 L/p`, `w u e^(w u) = w B^(c0 - c1 q/p)`,
//!   so `x = -W_k(z)/(c1 L) - q/p` with `z = -(c1 L/p) B^(c0 - c1 q/p)`: every branch k as the generated parameter
//!   `product_log(c(1), z)` over the complexes, the real branches W_0 (z >= -1/e) and W_-1 (-1/e <= z < 0) over the
//!   reals;
//! - g = c1 x^n + c0 and R = C x^(-m): `x^m e^(c1 L x^n) = C B^(-c0)`, raised to the power n/m: `v e^(gamma v) =
//!   (C B^(-c0))^(n/m)` for `v = x^n`, `gamma = c1 L n/m`, so `v = W(gamma (C B^(-c0))^(n/m))/gamma` and x one of
//!   the n-th roots of v (the principal branch; the raising to a power adds candidates, which the check of every
//!   solution by substitution removes).
use crate::cx::Env;
use crate::solve::Solution;
use mc_api::{Store, View, T};
use mc_poly::ratfun;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::Poly;

/// The solutions of `e = 0`, with `y` standing for the kernel `k` (holding the unknown x) and x also outside it;
/// `None` when the equation does not have one of the two forms.
pub(crate) fn solve<'h>(env: &Env<'h>, e: T<'h>, y: T<'h>, k: T<'h>, x: T<'h>, m: &mut Meter) -> WorkResult<Option<Vec<Solution<'h>>>> {
    let h = env.h();
    // K = B^g with B free of x
    let (b, g) = match h.view(k) {
        View::App { head, args: &[b, g] } if head == env.hd.pow && !env.contains_any(b, &[x]) => (b, g),
        _ => return Ok(None),
    };
    let log_b = if env.is_sym(b, "e") { env.int(1) } else { env.call("log", &[b]) };
    // e as alpha(x) y + beta(x)
    let ring = env.ring(&[e], &[y, x], m)?;
    let (vy, vx) = (ring.var(y).expect("y"), ring.var(x).expect("x"));
    if ring.kernels.iter().enumerate().any(|(i, &kk)| i != vy && i != vx && env.contains_any(kk, &[x, y])) {
        return Ok(None);
    }
    let Ok(f) = ratfun::read(&env.rconv(), e, &ring, m)? else { return Ok(None) };
    if f.num.degree(vy) != Some(1) {
        return Ok(None);
    }
    let mut alpha = Poly::zero(ring.nvars());
    let mut beta = Poly::zero(ring.nvars());
    for (d, c) in f.num.coeffs_in(vy) {
        if d == 1 {
            alpha = c;
        } else {
            beta = c;
        }
    }
    // g as c1 x^n + c0
    let Some((c1, n, c0)) = monomial_plus_constant(env, g, x, m)? else { return Ok(None) };
    let c = env.conv();
    let ax = alpha.degree(vx).unwrap_or(0);
    let bx = beta.degree(vx).unwrap_or(0);
    let pi = |p: &Poly, m: &mut Meter| c.int_poly_term(p, &ring, m);
    let mut out: Vec<Solution<'h>> = vec![];
    if n == 1 && bx == ax + 1 {
        // R = -beta/alpha = p x + q
        let mut pq = [env.int(0), env.int(0)];
        if let Some(quot) = beta.div_exact(&alpha, m)? {
            for (d, cc) in quot.coeffs_in(vx) {
                if d > 1 {
                    return Ok(None);
                }
                pq[d as usize] = env.ar.neg(pi(&cc, m)?);
            }
        } else if ax == 0 {
            // alpha free of x but not a divisor of beta over the integers (`x + 2 e^x == 1`): divide the
            // coefficients as terms
            let den = pi(&alpha, m)?;
            for (d, cc) in beta.coeffs_in(vx) {
                if d > 1 {
                    return Ok(None);
                }
                pq[d as usize] = env.ar.neg(env.ar.div(pi(&cc, m)?, den));
            }
        } else {
            return Ok(None);
        }
        let (q, p) = (pq[0], pq[1]);
        let q_over_p = env.ar.div(q, p);
        let z = env.ar.mul(&[
            env.int(-1),
            c1,
            log_b,
            env.ar.pow(p, env.int(-1)),
            env.ar.pow(b, env.ar.sub(c0, env.ar.mul(&[c1, q_over_p]))),
        ]);
        let value = |w: T<'h>| env.ar.sub(env.ar.neg(env.ar.div(w, env.ar.mul(&[c1, log_b]))), q_over_p);
        if env.cfg.reals {
            for w in real_branches(env, z) {
                out.push(vec![(x, value(w))]);
            }
        } else if env.cfg.periodic {
            let cp = h.app(env.gen, &[env.int(1)]);
            let w = env.call("product_log", &[cp, z]);
            let cond = env.call("element", &[cp, env.sym("integers")]);
            out.push(vec![(x, env.call("conditional_expression", &[value(w), cond]))]);
        } else {
            out.push(vec![(x, value(env.call("product_log", &[z])))]);
        }
        return Ok(Some(out));
    }
    // (a1 x + a0) B^(c1 x + c0) + beta = 0 with beta free of x (`(x + 1) e^x == 1`): u = a1 x + a0 gives
    // u e^(gamma u) = D with gamma = c1 L/a1 and D = -beta B^(-(c0 - c1 a0/a1)), so u = W(gamma D)/gamma and
    // x = (u - a0)/a1 (Corless et al. 1996, section 2, with the shift made explicit)
    let shifted = alpha.coeffs_in(vx).iter().any(|(d, _)| *d == 0);
    if n == 1 && ax == 1 && bx == 0 && shifted {
        let (mut a1, mut a0) = (env.int(0), env.int(0));
        for (d, cc) in alpha.coeffs_in(vx) {
            if d == 1 {
                a1 = pi(&cc, m)?;
            } else {
                a0 = pi(&cc, m)?;
            }
        }
        let beta_t = pi(&beta, m)?;
        let a0_over_a1 = env.ar.div(a0, a1);
        let gamma = env.ar.div(env.ar.mul(&[c1, log_b]), a1);
        let shift = env.ar.sub(c0, env.ar.mul(&[c1, a0_over_a1]));
        let dd = env.ar.mul(&[env.ar.neg(beta_t), env.ar.pow(b, env.ar.neg(shift))]);
        let z = env.ar.mul(&[gamma, dd]);
        let value = |w: T<'h>| env.ar.sub(env.ar.div(env.ar.div(w, gamma), a1), a0_over_a1);
        if env.cfg.reals {
            for w in real_branches(env, z) {
                out.push(vec![(x, value(w))]);
            }
        } else if env.cfg.periodic {
            let cp = h.app(env.gen, &[env.int(1)]);
            let w = env.call("product_log", &[cp, z]);
            let cond = env.call("element", &[cp, env.sym("integers")]);
            out.push(vec![(x, env.call("conditional_expression", &[value(w), cond]))]);
        } else {
            out.push(vec![(x, value(env.call("product_log", &[z])))]);
        }
        return Ok(Some(out));
    }
    // R = C x^(-mm): alpha and beta monomials in x
    let (Some(ca), Some(cb)) = (monomial_in(&alpha, vx), monomial_in(&beta, vx)) else { return Ok(None) };
    if ax == bx {
        return Ok(None);
    }
    let cst = env.ar.neg(env.ar.div(pi(&cb, m)?, pi(&ca, m)?));
    // B^g = C x^(bx - ax): x^mm e^(c1 L x^n) = C B^(-c0) with mm = ax - bx
    let mm = ax as i64 - bx as i64;
    let d = env.ar.mul(&[cst, env.ar.pow(b, env.ar.neg(c0))]);
    let ratio = env.ar.div(env.int(n as i64), env.int(mm));
    let gamma = env.ar.mul(&[c1, log_b, ratio]);
    // D^(n/mm) has mm values (the mm-th roots of D, each raised to n): every one gives equations `v e^(gamma v) = z`
    let root = env.ar.pow(d, env.ar.div(env.int(1), env.int(mm)));
    let order = mm.abs();
    let roots: Vec<i64> = if order <= 1 { vec![0] } else { (0..order.min(8)).collect() };
    for k in roots {
        let unit = if k == 0 { env.int(1) } else { env.ar.pow(env.int(-1), env.ar.div(env.int(2 * k), env.int(order))) };
        let z = env.ar.mul(&[gamma, env.ar.pow(env.ar.mul(&[unit, root]), env.int(n as i64))]);
        // W_0 and, for a real z in [-1/e, 0), W_-1: both give solutions over the complexes too
        let mut ws = real_branches(env, z);
        if ws.is_empty() && !env.cfg.reals {
            ws = vec![env.call("product_log", &[z])];
        }
        if env.cfg.reals {
            // the roots of D that are not real give no real solutions (a non-real z is judged by the check)
            if matches!(env.machine(env.ar.mul(&[unit, root])), Some((_, im)) if im != 0.0) {
                continue;
            }
        }
        for w in ws {
            let v = env.ar.div(w, gamma);
            let r = env.ar.pow(v, env.ar.div(env.int(1), env.int(n as i64)));
            for j in 0..n as i64 {
                let unit = if j == 0 { env.int(1) } else { env.ar.pow(env.int(-1), env.ar.div(env.int(2 * j), env.int(n as i64))) };
                out.push(vec![(x, env.ar.mul(&[unit, r]))]);
            }
        }
    }
    Ok(Some(out))
}

/// The real branches of W at z: W_0 for z >= -1/e, and W_-1 too for -1/e <= z < 0 (z a closed real number); W_0
/// alone when z is not a number.
fn real_branches<'h>(env: &Env<'h>, z: T<'h>) -> Vec<T<'h>> {
    let w0 = env.call("product_log", &[z]);
    match env.machine(z) {
        Some((a, 0.0)) => {
            let lim = -(-1f64).exp();
            if a < lim - 1e-15 {
                vec![]
            } else if a < 0.0 {
                vec![env.call("product_log", &[env.int(-1), z]), w0]
            } else {
                vec![w0]
            }
        }
        _ => vec![w0],
    }
}

/// The coefficient of p as a monomial in variable v (every term has the same degree in v).
fn monomial_in(p: &Poly, v: usize) -> Option<Poly> {
    let cs = p.coeffs_in(v);
    match cs.as_slice() {
        [(_, c)] => Some(c.clone()),
        _ => None,
    }
}

/// g as `c1 x^n + c0` (n >= 1, c1 and c0 free of x).
fn monomial_plus_constant<'h>(env: &Env<'h>, g: T<'h>, x: T<'h>, m: &mut Meter) -> WorkResult<Option<(T<'h>, u32, T<'h>)>> {
    let ring = env.ring(&[g], &[x], m)?;
    let vx = ring.var(x).expect("x");
    if ring.kernels.iter().enumerate().any(|(i, &k)| i != vx && env.contains_any(k, &[x])) {
        return Ok(None);
    }
    let Ok(f) = ratfun::read(&env.rconv(), g, &ring, m)? else { return Ok(None) };
    if f.den.degree(vx).unwrap_or(0) > 0 {
        return Ok(None);
    }
    let c = env.conv();
    let den = c.int_poly_term(&f.den, &ring, m)?;
    let mut c0 = env.int(0);
    let mut lead = None;
    for (d, cc) in f.num.coeffs_in(vx) {
        let t = env.ar.div(c.int_poly_term(&cc, &ring, m)?, den);
        if d == 0 {
            c0 = t;
        } else if lead.is_none() {
            lead = Some((t, d));
        } else {
            return Ok(None);
        }
    }
    Ok(lead.map(|(c1, n)| (c1, n, c0)))
}
