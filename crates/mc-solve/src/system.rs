//! Polynomial systems: a lexicographic Groebner basis over Q(parameters), then back substitution (decision 3B.6).
//!
//! With the variables x_1 > ... > x_n in lex order, the basis elements whose leading variable is x_k involve
//! only x_k, ..., x_n (and parameters). Going from x_n up, each partial solution (values of x_(k+1) .. x_n) is
//! substituted into those elements; by the extension theorem of Gianni and Kalkbrener the specialised element of
//! least degree in x_k whose leading coefficient does not vanish generates the specialised elimination ideal, so
//! its roots are exactly the extensions (taken without multiplicity). No such element (or all specialise to zero):
//! x_k is free and stays symbolic. A specialised element that is a nonzero constant: no extension. Leading
//! coefficients are tested with [`Env::is_zero`], so radicals of earlier values are handled through the
//! canonicaliser and complex balls.
use crate::cx::Env;
use crate::univariate::poly_roots;
use mc_api::T;
use mc_poly::convert::Ring;
use mc_poly::fglm::fglm;
use mc_poly::groebner::{groebner, GPoly, MonomialOrder};
use mc_poly::ratfun;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::Poly;

/// The lex Groebner basis of `nums` in the variables `vars` (ring indices), the other ring variables being
/// parameters; elements as polynomials of the ring.
pub(crate) fn lex_basis(nums: &[Poly], ring: &Ring<'_>, vars: &[usize], m: &mut Meter) -> WorkResult<Vec<Poly>> {
    let nv = ring.nvars();
    let params: Vec<usize> = (0..nv).filter(|i| !vars.contains(i)).collect();
    // without parameters: a grevlex basis converted by FGLM when the ideal is zero-dimensional (far faster than a
    // direct lex computation), else the lex basis directly
    let basis = if params.is_empty() {
        let gs: Vec<GPoly> = nums.iter().map(|p| GPoly::from_poly(p, vars, &params, MonomialOrder::DegRevLex)).collect();
        let grev = groebner(&gs, MonomialOrder::DegRevLex, m)?;
        match fglm(&grev, m)? {
            Some(b) => b,
            None => {
                let gs: Vec<GPoly> = nums.iter().map(|p| GPoly::from_poly(p, vars, &params, MonomialOrder::Lex)).collect();
                groebner(&gs, MonomialOrder::Lex, m)?
            }
        }
    } else {
        let gs: Vec<GPoly> = nums.iter().map(|p| GPoly::from_poly(p, vars, &params, MonomialOrder::Lex)).collect();
        groebner(&gs, MonomialOrder::Lex, m)?
    };
    Ok(basis.iter().map(|g| g.to_poly(nv, vars, &params)).collect())
}

/// The solutions of the polynomial system `nums = 0` in the variables `vars` (ring indices): each a list of
/// (variable, value) for the determined variables.
pub(crate) fn solve_system<'h>(
    env: &Env<'h>,
    nums: &[Poly],
    ring: &Ring<'h>,
    vars: &[usize],
    m: &mut Meter,
) -> WorkResult<Vec<Vec<(T<'h>, T<'h>)>>> {
    let basis = lex_basis(nums, ring, vars, m)?;
    if basis.len() == 1 && basis[0].degrees().iter().enumerate().all(|(i, &d)| d == 0 || !vars.contains(&i)) {
        // a polynomial in the parameters only: inconsistent (generically)
        return Ok(vec![]);
    }
    let n = vars.len();
    // the basis elements by leading variable (the first variable of `vars` that occurs)
    let mut by_var: Vec<Vec<&Poly>> = vec![vec![]; n];
    for g in &basis {
        let d = g.degrees();
        if let Some(k) = (0..n).find(|&k| d[vars[k]] > 0) {
            by_var[k].push(g);
        }
    }
    let c = env.conv();
    let xs: Vec<T<'h>> = vars.iter().map(|&v| ring.kernels[v]).collect();
    let mut partials: Vec<Vec<Option<T<'h>>>> = vec![vec![None; n]];
    for k in (0..n).rev() {
        let mut next = vec![];
        for a in partials {
            m.step(1)?;
            let rules: Vec<(T<'h>, T<'h>)> = (k + 1..n).filter_map(|j| a[j].map(|v| (xs[j], v))).collect();
            // the specialised elements: (true degree in x_k, polynomial, ring)
            let mut best: Option<(u32, Poly, Ring<'h>)> = None;
            let mut dead = false;
            for g in &by_var[k] {
                let t = c.int_poly_term(g, ring, m)?;
                let t = env.subst(t, &rules, m)?;
                let Some((p, r2, kv)) = specialise(env, t, xs[k], m)? else { continue };
                let deg = p.degree(kv).unwrap_or(0);
                if deg == 0 {
                    // a nonzero constant (in x_k): no extension of this partial solution
                    dead = true;
                    break;
                }
                if best.as_ref().is_none_or(|b| deg < b.0) {
                    best = Some((deg, p, r2));
                }
            }
            if dead {
                continue;
            }
            match best {
                None => next.push(a),
                Some((_, p, r2)) => {
                    let kv = r2.var(xs[k]).expect("the variable is in its ring");
                    let roots = poly_roots(env, &p, &r2, kv, m)?.unwrap_or_default();
                    let mut seen: Vec<T<'h>> = vec![];
                    for r in roots {
                        if seen.contains(&r) {
                            continue;
                        }
                        seen.push(r);
                        let mut b = a.clone();
                        b[k] = Some(r);
                        next.push(b);
                    }
                }
            }
        }
        partials = next;
    }
    Ok(partials
        .into_iter()
        .map(|a| (0..n).filter_map(|k| a[k].map(|v| (xs[k], v))).collect())
        .collect())
}

/// `t` as a polynomial in `x` with its true degree: the numerator of `t` read in its kernels (written back once so
/// that the canonicaliser combines interacting kernels such as radicals), leading coefficients that are zero by
/// [`Env::is_zero`] removed. `None` when t is zero. Returns the polynomial, its ring and x's index.
pub(crate) fn specialise<'h>(
    env: &Env<'h>,
    t: T<'h>,
    x: T<'h>,
    m: &mut Meter,
) -> WorkResult<Option<(Poly, Ring<'h>, usize)>> {
    let rc = env.rconv();
    let mut t = t;
    for _ in 0..2 {
        let ring = env.ring(&[t], &[x], m)?;
        let Ok(f) = ratfun::read(&rc, t, &ring, m)? else { return Ok(None) };
        if !ring.interacting() {
            return trim(env, f.num, ring, x, m);
        }
        t = rc.int_poly_term(&f.num, &ring, m)?;
    }
    let ring = env.ring(&[t], &[x], m)?;
    let Ok(f) = ratfun::read(&rc, t, &ring, m)? else { return Ok(None) };
    trim(env, f.num, ring, x, m)
}

/// Drop leading coefficients in x that are zero.
fn trim<'h>(env: &Env<'h>, p: Poly, ring: Ring<'h>, x: T<'h>, m: &mut Meter) -> WorkResult<Option<(Poly, Ring<'h>, usize)>> {
    let v = ring.var(x).expect("x is in the ring");
    let c = env.conv();
    let mut cs = p.coeffs_in(v);
    while let Some((_, lc)) = cs.first() {
        let t = c.int_poly_term(lc, &ring, m)?;
        if env.is_zero(t, m)? {
            cs.remove(0);
        } else {
            break;
        }
    }
    if cs.is_empty() {
        return Ok(None);
    }
    let nv = ring.nvars();
    let mut q = Poly::zero(nv);
    for (k, cp) in cs {
        let mut e = vec![0u32; nv];
        e[v] = k;
        q = q.add(&cp.mul(&Poly::monomial(&e, mc_poly::Z::ONE), m)?);
    }
    Ok(Some((q, ring, v)))
}
