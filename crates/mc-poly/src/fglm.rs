//! Change of order for zero-dimensional ideals (Faugère, Gianni, Lazard, Mora): a lexicographic Groebner basis
//! from a degree-reverse-lexicographic one by linear algebra in the quotient ring (package 3B).
//!
//! A lex basis computed directly by Buchberger's algorithm is often far slower than a grevlex one (Katsura-3:
//! 0.8 s against 0.3 ms); for a zero-dimensional ideal over Q, FGLM converts in O(n D^3) for D standard monomials.
//! Monomials are visited in increasing lex order; each one's normal form (a vector over the grevlex standard
//! monomials) is either independent of the accepted ones (the monomial joins the lex staircase and its multiples are
//! queued) or a combination of them, which gives a new lex basis element. Rationals throughout (no parameters).
use crate::groebner::{GPoly, MonomialOrder};
use crate::poly::Poly;
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use mc_arith::num::{Int, Rat};
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn divides(a: &[u32], b: &[u32]) -> bool {
    a.iter().zip(b).all(|(x, y)| x <= y)
}

fn rat(z: &Z) -> Rat {
    Rat::from_int(z.to_int())
}

/// Rational terms (exponents, coefficient).
type Terms = Vec<(Vec<u32>, Rat)>;

/// A grevlex basis element as monic rational terms (leading monomial first).
struct Monic {
    lm: Vec<u32>,
    /// the other terms
    tail: Vec<(Vec<u32>, Rat)>,
}

/// The largest number of standard monomials accepted (beyond it the dense linear algebra is too large).
pub const MAX_DIMENSION: usize = 2000;

/// The lex basis of the ideal of `grevlex` (a reduced grevlex basis without parameters), or `None` when the ideal is
/// not zero-dimensional or too large (the caller then computes the lex basis directly).
pub fn fglm(grevlex: &[GPoly], m: &mut Meter) -> WorkResult<Option<Vec<GPoly>>> {
    let Some(first) = grevlex.first() else { return Ok(None) };
    let nv = first.nv;
    if first.np != 0 || grevlex.iter().any(|g| g.is_constant()) {
        return Ok(None);
    }
    // zero-dimensional: a pure power of every variable leads some element
    for v in 0..nv {
        if !grevlex.iter().any(|g| g.lm().iter().enumerate().all(|(i, &e)| (i == v) == (e > 0))) {
            return Ok(None);
        }
    }
    let basis: Vec<Monic> = grevlex
        .iter()
        .map(|g| {
            let lc = rat(&g.lc().as_constant().expect("no parameters"));
            let inv = lc.recip().expect("nonzero");
            Monic {
                lm: g.lm().to_vec(),
                tail: g.terms[1..]
                    .iter()
                    .map(|(e, c)| (e.clone(), rat(&c.as_constant().expect("no parameters")).mul(&inv)))
                    .collect(),
            }
        })
        .collect();
    // the standard monomials of the grevlex basis
    let mut standard: Vec<Vec<u32>> = vec![];
    let mut seen: BTreeSet<Vec<u32>> = BTreeSet::new();
    let mut queue = vec![vec![0u32; nv]];
    while let Some(e) = queue.pop() {
        m.step(1)?;
        if seen.contains(&e) || basis.iter().any(|g| divides(&g.lm, &e)) {
            continue;
        }
        seen.insert(e.clone());
        standard.push(e.clone());
        if standard.len() > MAX_DIMENSION {
            return Ok(None);
        }
        for v in 0..nv {
            let mut f = e.clone();
            f[v] += 1;
            queue.push(f);
        }
    }
    let index: HashMap<Vec<u32>, usize> = standard.iter().cloned().enumerate().map(|(i, e)| (e, i)).collect();
    let dim = standard.len();
    let order = MonomialOrder::DegRevLex;
    // the normal form of a polynomial (rational terms) as a dense vector over the standard monomials
    let normal_form = |p: BTreeMap<Vec<u32>, Rat>, m: &mut Meter| -> WorkResult<Vec<Rat>> {
        // reduce from the largest monomial down (grevlex): keep a work map sorted by the order
        let mut work: Vec<(Vec<u32>, Rat)> = p.into_iter().filter(|(_, c)| !c.is_zero()).collect();
        let mut out = vec![Rat::from_i64(0); dim];
        while !work.is_empty() {
            m.step(1)?;
            // the largest term
            let k = (0..work.len()).max_by(|&a, &b| order.cmp(&work[a].0, &work[b].0)).expect("nonempty");
            let (e, c) = work.swap_remove(k);
            if let Some(&i) = index.get(&e) {
                out[i] = out[i].add(&c);
                continue;
            }
            let g = basis.iter().find(|g| divides(&g.lm, &e)).expect("a nonstandard monomial is reducible");
            let q: Vec<u32> = e.iter().zip(&g.lm).map(|(a, b)| a - b).collect();
            for (f, d) in &g.tail {
                let ef: Vec<u32> = f.iter().zip(&q).map(|(a, b)| a + b).collect();
                let v = c.mul(d).neg();
                match work.iter_mut().find(|(x, _)| *x == ef) {
                    Some(t) => t.1 = t.1.add(&v),
                    None => work.push((ef, v)),
                }
            }
            work.retain(|(_, c)| !c.is_zero());
        }
        Ok(out)
    };
    // echelon rows: (pivot, vector with pivot 1, combination over the accepted monomials)
    let mut rows: Vec<(usize, Vec<Rat>, Vec<Rat>)> = vec![];
    let mut accepted: Vec<(Vec<u32>, Vec<Rat>)> = vec![];
    let mut lex_basis: Vec<(Vec<u32>, Terms)> = vec![];
    let mut todo: BTreeSet<Vec<u32>> = BTreeSet::new();
    todo.insert(vec![0u32; nv]);
    while let Some(e) = todo.pop_first() {
        m.step(dim as u64)?;
        if lex_basis.iter().any(|(l, _)| divides(l, &e)) {
            continue;
        }
        // normal form: from an accepted divisor x_v * b, as x_v NF(b); 1 directly
        let nf = match accepted.iter().find_map(|(b, nb)| {
            let d: Vec<i64> = e.iter().zip(b).map(|(x, y)| *x as i64 - *y as i64).collect();
            (d.iter().all(|&x| x >= 0) && d.iter().sum::<i64>() == 1).then(|| (d.iter().position(|&x| x == 1), nb))
        }) {
            Some((Some(v), nb)) => {
                let mut p: BTreeMap<Vec<u32>, Rat> = BTreeMap::new();
                for (i, c) in nb.iter().enumerate() {
                    if !c.is_zero() {
                        let mut f = standard[i].clone();
                        f[v] += 1;
                        p.insert(f, c.clone());
                    }
                }
                normal_form(p, m)?
            }
            _ => {
                let mut p = BTreeMap::new();
                p.insert(e.clone(), Rat::from_i64(1));
                normal_form(p, m)?
            }
        };
        // reduce against the echelon rows, tracking the combination
        let k = accepted.len();
        let mut v = nf.clone();
        let mut comb = vec![Rat::from_i64(0); k + 1];
        comb[k] = Rat::from_i64(1);
        for (p, row, rc) in &rows {
            if v[*p].is_zero() {
                continue;
            }
            let f = v[*p].clone();
            for i in 0..dim {
                if !row[i].is_zero() {
                    v[i] = v[i].sub(&f.mul(&row[i]));
                }
            }
            for (i, c) in rc.iter().enumerate() {
                if !c.is_zero() {
                    comb[i] = comb[i].sub(&f.mul(c));
                }
            }
        }
        match v.iter().position(|c| !c.is_zero()) {
            None => {
                // e + sum comb_j b_j (comb_k = 1 for e itself): a lex basis element
                let terms: Vec<(Vec<u32>, Rat)> = comb
                    .into_iter()
                    .enumerate()
                    .filter(|(_, c)| !c.is_zero())
                    .map(|(i, c)| (if i < k { accepted[i].0.clone() } else { e.clone() }, c))
                    .collect();
                lex_basis.push((e, terms));
            }
            Some(p) => {
                let inv = v[p].recip().expect("nonzero");
                let row: Vec<Rat> = v.iter().map(|c| c.mul(&inv)).collect();
                let rc: Vec<Rat> = comb.iter().map(|c| c.mul(&inv)).collect();
                rows.push((p, row, rc));
                accepted.push((e.clone(), nf));
                for w in 0..nv {
                    let mut f = e.clone();
                    f[w] += 1;
                    todo.insert(f);
                }
            }
        }
    }
    // the lex elements as primitive integer polynomials
    let mut out = vec![];
    for (_, terms) in lex_basis {
        let mut den = Z::ONE;
        for (_, c) in &terms {
            let d = Z::from_int(Int::from_nat(c.den.clone()));
            den = den.mul(&d).div_exact(&den.gcd(&d));
        }
        let ints: Vec<(Vec<u32>, Z)> = terms
            .into_iter()
            .map(|(e, c)| {
                let d = Z::from_int(Int::from_nat(c.den.clone()));
                (e, Z::from_int(c.num.clone()).mul(&den.div_exact(&d)))
            })
            .collect();
        let p = Poly::from_terms(nv, ints);
        let main: Vec<usize> = (0..nv).collect();
        out.push(GPoly::from_poly(&p, &main, &[], MonomialOrder::Lex).primitive(m)?);
    }
    out.sort_by(|a, b| MonomialOrder::Lex.cmp(a.lm(), b.lm()));
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groebner::groebner;
    use crate::work::Unlimited;

    #[test]
    fn agrees_with_lex() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let k = |t: &[(&[u32], i64)]| Poly::from_terms(4, t.iter().map(|(e, c)| (e.to_vec(), Z::S(*c))).collect());
        let ps = [
            k(&[(&[1, 0, 0, 0], 1), (&[0, 1, 0, 0], 2), (&[0, 0, 1, 0], 2), (&[0, 0, 0, 1], 2), (&[0, 0, 0, 0], -1)]),
            k(&[(&[2, 0, 0, 0], 1), (&[0, 2, 0, 0], 2), (&[0, 0, 2, 0], 2), (&[0, 0, 0, 2], 2), (&[1, 0, 0, 0], -1)]),
            k(&[(&[1, 1, 0, 0], 2), (&[0, 1, 1, 0], 2), (&[0, 0, 1, 1], 2), (&[0, 1, 0, 0], -1)]),
            k(&[(&[0, 2, 0, 0], 1), (&[1, 0, 1, 0], 2), (&[0, 1, 0, 1], 2), (&[0, 0, 1, 0], -1)]),
        ];
        let main = [0, 1, 2, 3];
        let g = |o| ps.iter().map(|p| GPoly::from_poly(p, &main, &[], o)).collect::<Vec<_>>();
        let lex = groebner(&g(MonomialOrder::Lex), MonomialOrder::Lex, &mut m).unwrap();
        let grev = groebner(&g(MonomialOrder::DegRevLex), MonomialOrder::DegRevLex, &mut m).unwrap();
        let conv = fglm(&grev, &mut m).unwrap().unwrap();
        assert_eq!(conv, lex);
        // not zero-dimensional: x y - 1 alone
        let p = Poly::from_terms(2, vec![(vec![1, 1], Z::S(1)), (vec![0, 0], Z::S(-1))]);
        let g = groebner(&[GPoly::from_poly(&p, &[0, 1], &[], MonomialOrder::DegRevLex)], MonomialOrder::DegRevLex, &mut m)
            .unwrap();
        assert!(fglm(&g, &mut m).unwrap().is_none());
    }
}

