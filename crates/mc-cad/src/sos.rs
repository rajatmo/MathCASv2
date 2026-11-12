//! A sum-of-squares certificate for `for_all(x, p >= 0)` with one polynomial `p` (decision CAD.9): the Gram matrix
//! of `p` over the monomials between half the lowest and half the highest degree in each variable, each coefficient
//! of `p` placed on the most diagonal pair of monomials whose product it is, and an exact LDL^T decomposition over
//! the rationals. A positive semidefinite Gram matrix proves `p >= 0` on all of `R^n`; when the Gram matrix of this
//! one choice is not semidefinite, the question goes to the decomposition. Cheap and exact: it decides the common
//! textbook inequalities (`x^2 + y^2 - 2 x y >= 0`, `x^4 + y^4 - 2 x^2 y^2 >= 0`) without lifting.
use crate::poly::zrat;
use mc_arith::num::{Int, Rat};
use mc_poly::work::{Meter, WorkResult};
use mc_poly::Poly;
use rustc_hash::FxHashMap;

/// The most monomials of the Gram basis.
const MAX_BASIS: usize = 40;

/// A sentence `for_all(x, p >= 0)` (or `p <= 0`) certified by a sum of squares: `Some(true)`; `None` when the
/// problem has another shape or no certificate was found.
pub fn sentence(p: &crate::cad::Problem, m: &mut Meter) -> WorkResult<Option<bool>> {
    use crate::formula::{Fm, Quant, Rel};
    if p.nfree > 0 || p.quants.is_empty() || !p.quants.iter().all(|&q| q == Quant::ForAll) {
        return Ok(None);
    }
    let Fm::Atom(i) = p.matrix else { return Ok(None) };
    let a = &p.atoms[i];
    if !a.den.is_constant() {
        return Ok(None);
    }
    let q = match a.rel {
        Rel::Ge => a.num.clone(),
        Rel::Le => a.num.neg(),
        _ => return Ok(None),
    };
    let q = if a.den.lc().is_neg() { q.neg() } else { q };
    Ok(nonnegative(&q, m)?.then_some(true))
}

/// Is `p` certified nonnegative on `R^n` by a sum of squares?
pub fn nonnegative(p: &Poly, m: &mut Meter) -> WorkResult<bool> {
    if p.is_zero() {
        return Ok(true);
    }
    let nv = p.nvars();
    let half: Vec<u32> = (0..nv).map(|v| p.degree(v).unwrap_or(0) / 2).collect();
    let low: Vec<u32> = (0..nv).map(|v| p.low_degree(v).unwrap_or(0).div_ceil(2)).collect();
    let tdeg = p.total_degree().unwrap_or(0) as u32;
    let mut basis: Vec<Vec<u32>> = vec![vec![]];
    for v in 0..nv {
        let mut next = vec![];
        for b in &basis {
            for e in low[v]..=half[v].max(low[v]) {
                let mut c = b.clone();
                c.push(e);
                if c.iter().sum::<u32>() <= tdeg / 2 {
                    next.push(c);
                }
            }
        }
        basis = next;
        if basis.len() > MAX_BASIS {
            return Ok(false);
        }
    }
    let n = basis.len();
    if n == 0 {
        return Ok(false);
    }
    let mut pairs: FxHashMap<Vec<u32>, (usize, usize, u32)> = FxHashMap::default();
    for i in 0..n {
        for j in i..n {
            m.step(1)?;
            let e: Vec<u32> = (0..nv).map(|v| basis[i][v] + basis[j][v]).collect();
            let off: u32 = (0..nv).map(|v| basis[i][v].abs_diff(basis[j][v])).sum();
            if pairs.get(&e).is_none_or(|&(_, _, o)| off < o) {
                pairs.insert(e, (i, j, off));
            }
        }
    }
    let mut g = vec![vec![Rat::from_i64(0); n]; n];
    let half_q = Rat::new(Int::from_i64(1), Int::from_i64(2));
    for (e, c) in p.terms() {
        let Some(&(i, j, _)) = pairs.get(e) else { return Ok(false) };
        let c = zrat(c);
        if i == j {
            g[i][i] = g[i][i].add(&c);
        } else {
            let h = c.mul(&half_q);
            g[i][j] = g[i][j].add(&h);
            g[j][i] = g[j][i].add(&h);
        }
    }
    psd(g, m)
}

/// Is the symmetric rational matrix positive semidefinite? Exact symmetric elimination; a zero pivot is allowed
/// only with a zero row.
fn psd(mut a: Vec<Vec<Rat>>, m: &mut Meter) -> WorkResult<bool> {
    let n = a.len();
    for k in 0..n {
        m.step(n as u64)?;
        let p = a[k][k].clone();
        if p.num.neg {
            return Ok(false);
        }
        if p.is_zero() {
            if (k + 1..n).any(|j| !a[k][j].is_zero()) {
                return Ok(false);
            }
            continue;
        }
        let inv = p.recip().expect("a nonzero pivot");
        for i in k + 1..n {
            if a[i][k].is_zero() {
                continue;
            }
            let f = a[i][k].mul(&inv);
            let (top, bottom) = a.split_at_mut(i);
            for (x, y) in bottom[0][k..].iter_mut().zip(&top[k][k..]) {
                *x = x.sub(&f.mul(y));
            }
        }
    }
    Ok(true)
}
