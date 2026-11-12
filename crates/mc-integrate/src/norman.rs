//! The parallel Risch–Norman ansatz over the tower (Norman and Moore 1977; Bronstein 2007, "pmint").
//!
//! With `D` the tower's derivation and `f = a/d`, the irreducible factors of `d` are split into special ones
//! (`p | Dp`: the exponential and algebraic monomials, and factors like them) and normal ones. The antiderivative is
//! looked for as
//!
//! `A/B + sum c_j log(p_j) + sum e_k T_k`,
//!
//! `B` the product of the special factors to their multiplicity and the normal ones to their multiplicity minus
//! one, `A` a polynomial in all monomials (the integration variable included) with unknown coefficients and bounded
//! degrees, `p_j` the irreducible factors of `d`, and `T_k` further candidates with known derivatives (`atan` of the
//! irreducible quadratic factors). The special-function monomials the driver adds to the tower (`erf`, `Ei`, `Si`,
//! ... of the integrand's arguments) enter `A` linearly. `D` of the ansatz equals `f` exactly when a linear system
//! over the constants has a solution; the relations of the tower (`sin^2 + cos^2 = 1`, `t^q = b` for an algebraic
//! monomial `t = b^(1/q)`) are applied to the equation's numerator before its coefficients are compared, so
//! the identity found holds in the field.
use crate::budget::Budget;
use crate::field::Fld;
use crate::linsolve::{self, Row};
use crate::tower::{Kind, Tower};
use crate::write::Writer;
use mc_api::{Store, T};
use mc_arith::num::Rat;
use mc_arith::Arith;
use mc_core::Heap;
use mc_poly::gcd::gcd;
use mc_poly::ratfun::RatFun;
use mc_poly::ring::Rationals;
use mc_poly::work::{exact, Meter, WorkResult};
use mc_poly::{Poly, Z};
use rustc_hash::FxHashMap;

/// The most unknown coefficients of `A`.
const MAX_MONOMIALS: usize = 700;
/// The largest total degree of `A`.
const MAX_DEGREE: u32 = 12;

/// A relation `v^k = r` of the tower (r of lower degree in v).
struct Rel {
    v: usize,
    k: u32,
    r: Poly,
}

/// The ansatz's answer for `f` over `tw`, as a term; `None` when no solution of the bounded ansatz exists.
pub fn integrate<'h>(tw: &mut Tower<'h>, ar: &Arith<'h, Heap>, bud: &mut Budget<'_, '_, '_, '_>) -> WorkResult<Option<T<'h>>> {
    let mut m = Meter::new(bud);
    if let Some(r) = attempt(tw, ar, false, &mut m)? {
        return Ok(Some(r));
    }
    // a second attempt with every normal factor in the denominator to its full multiplicity (`1/(1 + cos x)`
    // integrates to `sin x / (1 + cos x)`: the relations of the tower lower the multiplicity there)
    attempt(tw, ar, true, &mut m)
}

fn attempt<'h>(tw: &mut Tower<'h>, ar: &Arith<'h, Heap>, full: bool, m: &mut Meter) -> WorkResult<Option<T<'h>>> {
    let fld = tw.fld;
    let nv = fld.nv;
    let mons = tw.monomials();
    let is_mon = {
        let mut v = vec![false; nv];
        for &k in &mons {
            v[k] = true;
        }
        v
    };
    let f = tw.f.clone();
    let rels = relations(tw, m)?;
    // q: the common denominator of the derivations; Dt[v] = q D v
    let mut q = Poly::constant(nv, Z::ONE);
    for &v in &mons {
        q = lcm(&q, &tw.deriv[v].den, m)?;
    }
    let mut dt: Vec<Option<Poly>> = vec![None; nv];
    for &v in &mons {
        let d = &tw.deriv[v];
        if d.num.is_zero() {
            continue;
        }
        let k = exact(q.div_exact(&d.den, m)?, "the lcm divides")?;
        dt[v] = Some(fld.reduce_i(&d.num.mul(&k, m)?));
    }
    let dtilde = |p: &Poly, m: &mut Meter| -> WorkResult<Poly> {
        let mut acc = Poly::zero(nv);
        for &v in &mons {
            let Some(dv) = &dt[v] else { continue };
            if p.degree(v).unwrap_or(0) == 0 {
                continue;
            }
            acc = acc.add(&p.derivative(v).mul(dv, m)?);
        }
        Ok(fld.reduce_i(&acc))
    };
    // the factors of the denominator
    let fac = mc_poly::factor::factor(&f.den, false, m)?;
    let mut b_poly = Poly::constant(nv, Z::ONE);
    let mut special_part = Poly::constant(nv, Z::ONE);
    let mut log_args: Vec<Poly> = vec![];
    let mut normal = false;
    for (p, e) in &fac.factors {
        if mons.iter().all(|&v| p.degree(v).unwrap_or(0) == 0) {
            continue;
        }
        // special: p divides D p (the derivation itself, not D~ = q D, which every factor of q divides)
        let dp = tw.d(&fld.poly(p.clone()), m)?;
        let special = dp.num.is_zero() || (dp.den.div_exact(p, m)?.is_none() && dp.num.div_exact(p, m)?.is_some());
        if special {
            b_poly = b_poly.mul(&p.pow(*e as u64, m)?, m)?;
            special_part = special_part.mul(p, m)?;
            // a special factor that is not a monomial alone may still be a logarithm's argument
            if p.len() > 1 {
                log_args.push(p.clone());
            }
        } else {
            normal = true;
            let k = if full { *e } else { *e - 1 };
            if k > 0 {
                b_poly = b_poly.mul(&p.pow(k as u64, m)?, m)?;
            }
            log_args.push(p.clone());
        }
    }
    if full && !normal {
        return Ok(None);
    }
    // atan candidates: irreducible quadratics with a constant discriminant
    let mut extra: Vec<(T<'h>, RatFun)> = vec![];
    for p in log_args.clone() {
        if let Some(c) = atan_candidate(tw, ar, &p, &mons, &dtilde, &q, m)? {
            extra.push(c);
        }
    }
    // degree bounds
    let totdeg = |p: &Poly| -> u32 {
        p.terms().map(|(e, _)| mons.iter().map(|&v| e[v]).sum::<u32>()).max().unwrap_or(0)
    };
    let dg = (1 + totdeg(&special_part) + totdeg(&f.num).max(totdeg(&f.den))).min(MAX_DEGREE);
    let mut bounds: Vec<u32> = vec![0; nv];
    for &v in &mons {
        bounds[v] = if tw.vars[v].candidate {
            1
        } else {
            f.num.degree(v).unwrap_or(0).max(f.den.degree(v).unwrap_or(0)) + b_poly.degree(v).unwrap_or(0) + 1
        };
    }
    // the relations bound the degrees too: s^2 = 1 - c^2 leaves s to degree 1
    for r in &rels {
        bounds[r.v] = bounds[r.v].min(r.k - 1).max(if r.k > 1 { 1 } else { 0 }).min(bounds[r.v]);
    }
    let mut dg = dg;
    // the candidates enter alone, with a constant coefficient
    let (cands, plain): (Vec<usize>, Vec<usize>) = mons.iter().partition(|&&v| tw.vars[v].candidate);
    let mut monos = loop {
        let ms = monomials(&plain, &bounds, dg, MAX_MONOMIALS + 1);
        if ms.len() <= MAX_MONOMIALS || dg <= 1 {
            break ms;
        }
        dg -= 1;
    };
    if monos.len() > MAX_MONOMIALS {
        return Ok(None);
    }
    // (a candidate is an extra term with its derivative)
    for &c in &cands {
        extra.push((tw.vars[c].term, tw.deriv[c].clone()));
    }
    let monos = std::mem::take(&mut monos);
    // the equation's numerators over the common denominator L
    let b2 = b_poly.mul(&b_poly, m)?;
    let qb2 = q.mul(&b2, m)?;
    let db = dtilde(&b_poly, m)?;
    let mut l = lcm(&qb2, &f.den, m)?;
    let mut log_dens = vec![];
    for p in &log_args {
        let qp = q.mul(p, m)?;
        l = lcm(&l, &qp, m)?;
        log_dens.push(qp);
    }
    for (_, d) in &extra {
        l = lcm(&l, &d.den, m)?;
    }
    let reduce = |p: Poly, m: &mut Meter| -> WorkResult<Poly> { reduce_rels(&fld, p, &rels, m) };
    let mut cols: Vec<Poly> = vec![];
    let lb = exact(l.div_exact(&qb2, m)?, "the lcm divides")?;
    for e in &monos {
        m.step(4)?;
        let mon = Poly::monomial(e, Z::ONE);
        let n = dtilde(&mon, m)?.mul(&b_poly, m)?.sub(&mon.mul(&db, m)?);
        cols.push(reduce(n.mul(&lb, m)?, m)?);
    }
    for (p, qp) in log_args.iter().zip(&log_dens) {
        let k = exact(l.div_exact(qp, m)?, "the lcm divides")?;
        cols.push(reduce(dtilde(p, m)?.mul(&k, m)?, m)?);
    }
    for (_, d) in &extra {
        let k = exact(l.div_exact(&d.den, m)?, "the lcm divides")?;
        cols.push(reduce(fld.reduce_i(&d.num.mul(&k, m)?), m)?);
    }
    let kf = exact(l.div_exact(&f.den, m)?, "the lcm divides")?;
    // every column is D(unknown) L: the monomials' D~(m/B) over q B^2, the logarithms' D~p over q p, the extras'
    // D directly; the right-hand side is f L
    let rhs = reduce(fld.reduce_i(&f.num.mul(&kf, m)?), m)?;
    let n_mon_log = monos.len() + log_args.len();
    // coefficients of the monomials in the tower's variables, as polynomials in the constants
    let split = |p: &Poly| -> Vec<(Vec<u32>, Poly)> {
        let mut by: FxHashMap<Vec<u32>, Vec<(Vec<u32>, Z)>> = FxHashMap::default();
        for (e, c) in p.terms() {
            let key: Vec<u32> = mons.iter().map(|&v| e[v]).collect();
            let mut ce = e.to_vec();
            for &v in &mons {
                ce[v] = 0;
            }
            by.entry(key).or_default().push((ce, c.clone()));
        }
        by.into_iter().map(|(k, ts)| (k, Poly::from_terms(nv, ts))).collect()
    };
    let mut keys: FxHashMap<Vec<u32>, usize> = FxHashMap::default();
    let mut rows: Vec<(Vec<(usize, Poly)>, Poly)> = vec![];
    for (j, c) in cols.iter().enumerate() {
        m.step(c.len() as u64)?;
        for (k, p) in split(c) {
            let n = keys.len();
            let r = *keys.entry(k).or_insert(n);
            if r == rows.len() {
                rows.push((vec![], Poly::zero(nv)));
            }
            rows[r].0.push((j, p));
        }
    }
    for (k, p) in split(&rhs) {
        let n = keys.len();
        let r = *keys.entry(k).or_insert(n);
        if r == rows.len() {
            rows.push((vec![], Poly::zero(nv)));
        }
        rows[r].1 = p;
    }
    let nunk = cols.len();
    let constant_coefs = rows.iter().all(|(es, b)| es.iter().all(|(_, p)| p.is_constant() || p.is_zero()) && (b.is_constant() || b.is_zero()));
    let sol: Vec<RatFun> = if constant_coefs {
        let qr = Rationals;
        let to_q = |p: &Poly| -> Rat { p.as_constant().map(|z| Rat::from_int(z.to_int())).unwrap_or_else(|| Rat::from_i64(0)) };
        let rs: Vec<Row<Rat>> = rows
            .iter()
            .map(|(es, b)| {
                let mut cs: Vec<(usize, Rat)> = es.iter().map(|(j, p)| (*j, to_q(p))).filter(|(_, v)| !v.is_zero()).collect();
                cs.sort_by_key(|e| e.0);
                Row { cols: cs, rhs: to_q(b) }
            })
            .collect();
        let Some(u) = linsolve::solve(&qr, rs, nunk, m)? else { return Ok(None) };
        u.iter()
            .map(|r| {
                let (n, d) = mc_poly::qpoly::rat_parts(r);
                RatFun::constant(nv, n, d)
            })
            .collect()
    } else {
        let rs: Vec<Row<RatFun>> = rows
            .iter()
            .map(|(es, b)| {
                let mut cs: Vec<(usize, RatFun)> = es.iter().filter(|(_, p)| !p.is_zero()).map(|(j, p)| (*j, fld.poly(p.clone()))).collect();
                cs.sort_by_key(|e| e.0);
                Row { cols: cs, rhs: fld.poly(b.clone()) }
            })
            .collect();
        let Some(u) = linsolve::solve(&fld, rs, nunk, m)? else { return Ok(None) };
        u
    };
    // the answer
    let mut a = fld.int(0);
    for (e, c) in monos.iter().zip(&sol) {
        if c.num.is_zero() {
            continue;
        }
        let mon = fld.poly(Poly::monomial(e, Z::ONE));
        a = fld.addm(&a, &fld.mulm(c, &mon, m)?, m)?;
    }
    let rat = exact(fld.divm(&a, &fld.poly(b_poly.clone()), m)?, "B is not zero")?;
    let w = Writer { tw, ar };
    let h = ar.store();
    let mut terms = vec![w.ratfun(&rat)];
    for (j, p) in log_args.iter().enumerate() {
        let c = &sol[monos.len() + j];
        if c.num.is_zero() {
            continue;
        }
        let arg = w.poly(p, &[]);
        terms.push(ar.mul(&[w.ratfun(c), h.call(h.sym("log"), &[arg])]));
    }
    for (k, (t, _)) in extra.iter().enumerate() {
        let c = &sol[n_mon_log + k];
        if c.num.is_zero() {
            continue;
        }
        terms.push(ar.mul(&[w.ratfun(c), *t]));
    }
    let _ = is_mon;
    Ok(Some(ar.add(&terms)))
}

/// The lcm of two polynomials.
fn lcm(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    if a.as_constant() == Some(Z::ONE) {
        return Ok(b.clone());
    }
    if b.as_constant() == Some(Z::ONE) {
        return Ok(a.clone());
    }
    let g = gcd(a, b, m)?;
    let k = exact(b.div_exact(&g, m)?, "the gcd divides")?;
    a.mul(&k, m)
}

/// The exponent vectors over the variables `vs` with `e_v <= bound_v` and total degree at most `dg`, at most `cap`.
fn monomials(vs: &[usize], bounds: &[u32], dg: u32, cap: usize) -> Vec<Vec<u32>> {
    let nv = bounds.len();
    let mut out = vec![];
    let mut cur = vec![0u32; nv];
    fn go(i: usize, left: u32, vs: &[usize], bounds: &[u32], cur: &mut Vec<u32>, out: &mut Vec<Vec<u32>>, cap: usize) {
        if out.len() >= cap {
            return;
        }
        if i == vs.len() {
            out.push(cur.clone());
            return;
        }
        let v = vs[i];
        for k in 0..=bounds[v].min(left) {
            cur[v] = k;
            go(i + 1, left - k, vs, bounds, cur, out, cap);
        }
        cur[v] = 0;
    }
    go(0, dg, vs, bounds, &mut cur, &mut out, cap);
    out
}

/// The relations of the tower: `sin(u)^2 = 1 - cos(u)^2` for every pair, `t^q = b` for algebraic monomials
/// `t = b^(1/q)` with a polynomial `b`.
fn relations(tw: &Tower<'_>, m: &mut Meter) -> WorkResult<Vec<Rel>> {
    let fld = &tw.fld;
    let nv = fld.nv;
    let mut out = vec![];
    for (k, v) in tw.vars.iter().enumerate() {
        match &v.kind {
            Kind::Sin { partner, .. } => {
                let c2 = Poly::var(nv, *partner).pow(2, m)?;
                out.push(Rel { v: k, k: 2, r: Poly::constant(nv, Z::ONE).sub(&c2) });
            }
            Kind::Power { q: Some(q), .. } => {
                if let Some(b) = &tw.bases[k] {
                    if b.den.as_constant() == Some(Z::ONE) {
                        out.push(Rel { v: k, k: *q, r: b.num.clone() });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// `p` reduced by the relations (every exponent of a related variable below its relation's degree).
fn reduce_rels(fld: &Fld, p: Poly, rels: &[Rel], m: &mut Meter) -> WorkResult<Poly> {
    if rels.is_empty() {
        return Ok(p);
    }
    let nv = fld.nv;
    let mut p = p;
    for _round in 0..64 {
        let mut changed = false;
        for r in rels {
            if p.degree(r.v).unwrap_or(0) < r.k {
                continue;
            }
            changed = true;
            let mut keep = vec![];
            let mut acc = Poly::zero(nv);
            for (e, c) in p.terms() {
                m.step(1)?;
                if e[r.v] < r.k {
                    keep.push((e.to_vec(), c.clone()));
                    continue;
                }
                let mut f = e.to_vec();
                f[r.v] -= r.k;
                let t = Poly::monomial(&f, c.clone()).mul(&r.r, m)?;
                acc = acc.add(&t);
            }
            p = Poly::from_terms(nv, keep).add(&acc);
        }
        if !changed {
            break;
        }
    }
    Ok(fld.reduce_i(&p))
}

/// `atan((2 A v + B)/s)` for an irreducible `p = A v^2 + B v + C` with a constant discriminant `s^2 = 4AC - B^2`
/// (not a negative number), and its derivative `s D(2Av + B) / (4 A p)`.
fn atan_candidate<'h>(
    tw: &mut Tower<'h>,
    ar: &Arith<'h, Heap>,
    p: &Poly,
    mons: &[usize],
    dtilde: &dyn Fn(&Poly, &mut Meter) -> WorkResult<Poly>,
    q: &Poly,
    m: &mut Meter,
) -> WorkResult<Option<(T<'h>, RatFun)>> {
    let fld = tw.fld;
    let nv = fld.nv;
    // the main variable: the last monomial of degree 2
    let Some(&v) = mons.iter().rev().find(|&&v| p.degree(v).unwrap_or(0) > 0) else { return Ok(None) };
    if p.degree(v) != Some(2) {
        return Ok(None);
    }
    let cs = p.coeffs_in(v);
    let get = |k: u32| cs.iter().find(|(e, _)| *e == k).map(|(_, c)| c.clone()).unwrap_or_else(|| Poly::zero(nv));
    let (a, b, c) = (get(2), get(1), get(0));
    let disc = Poly::constant(nv, Z::S(4)).mul(&a, m)?.mul(&c, m)?.sub(&b.mul(&b, m)?);
    let disc = fld.reduce_i(&disc);
    if mons.iter().any(|&u| disc.degree(u).unwrap_or(0) > 0) || disc.is_zero() {
        return Ok(None);
    }
    if disc.as_constant().is_some_and(|d| d.is_neg()) {
        return Ok(None);
    }
    let (s_el, s_term) = match disc.as_constant().and_then(|d| d.as_i64()).and_then(isqrt) {
        Some(r) => (fld.int(r), ar.int(r)),
        None => {
            let dterm = Writer { tw, ar }.poly(&disc, &[]);
            let t = ar.pow(dterm, ar.rational(ar.int(1), ar.int(2)));
            match spare(tw, t) {
                Some(s) => s,
                None => return Ok(None),
            }
        }
    };
    let w = Writer { tw, ar };
    let lin = Poly::constant(nv, Z::S(2)).mul(&a, m)?.mul(&Poly::var(nv, v), m)?.add(&b);
    let term = ar.store().call(ar.store().sym("atan"), &[ar.mul(&[w.poly(&lin, &[]), ar.pow(s_term, ar.int(-1))])]);
    // D(2Av + B) = D~(.)/q
    let dl = dtilde(&lin, m)?;
    let num = fld.poly(dl).num.mul(&s_el.num, m)?;
    let den = Poly::constant(nv, Z::S(4)).mul(&a, m)?.mul(p, m)?.mul(q, m)?;
    let d = fld.quo(num, den, m)?;
    // the column is scaled by q later: store D, not D~
    Ok(Some((term, d)))
}

/// A spare constant variable for the term `t`: its element and term.
fn spare<'h>(tw: &mut Tower<'h>, t: T<'h>) -> Option<(RatFun, T<'h>)> {
    let k = tw.spare.len();
    if k >= crate::tower::SPARE - 1 {
        // the last spare is the decision procedure's resultant variable
        return None;
    }
    tw.spare.push(t);
    Some((tw.fld.var(tw.vars.len() + k), t))
}

fn isqrt(n: i64) -> Option<i64> {
    if n < 0 {
        return None;
    }
    let r = (n as f64).sqrt().round() as i64;
    (r * r == n).then_some(r)
}
