//! Linear systems by fraction-free elimination over Z[parameters] (decision 3B.5).
//!
//! The augmented matrix holds integer polynomials in the parameters (integers for numeric systems). Bareiss'
//! fraction-free elimination brings it to echelon form (every division exact), choosing as pivot the simplest
//! nonzero entry of the column; then back elimination (fraction-free, each row divided by the gcd of its entries)
//! leaves every pivot row as `D x_p + sum A_j x_j = B` over the free variables x_j, so the solution is
//! `x_p = (B - sum A_j x_j) / D` in lowest terms. A zero row with a nonzero right side means no solution; columns
//! without a pivot are free variables (they stay symbolic in the solution). A nonzero polynomial in the parameters
//! is assumed nonzero (the generic case).
use crate::cx::Env;
use mc_api::T;
use mc_poly::convert::Ring;
use mc_poly::gcd::gcd;
use mc_poly::groebner::{GPoly, MonomialOrder};
use mc_poly::work::{exact, Meter, WorkResult};
use mc_poly::{Poly, Z};

/// Is every polynomial of degree at most one in the variables `vars` (ring indices)?
pub(crate) fn is_linear(ps: &[Poly], vars: &[usize]) -> bool {
    ps.iter().all(|p| p.terms().all(|(e, _)| vars.iter().map(|&v| e[v] as u64).sum::<u64>() <= 1))
}

fn cmul(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    match (a.as_constant(), b.as_constant()) {
        (Some(x), _) => Ok(b.scale(&x)),
        (_, Some(y)) => Ok(a.scale(&y)),
        _ => a.mul(b, m),
    }
}

fn cdiv(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    if let Some(y) = b.as_constant() {
        if a.coefs().iter().all(|c| c.divisible(&y)) {
            return Ok(a.div_exact_scalar(&y));
        }
    }
    exact(a.div_exact(b, m)?, "a Bareiss division is exact")
}

fn cgcd(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    match (a.as_constant(), b.as_constant()) {
        (Some(x), Some(y)) => Ok(Poly::constant(a.nvars(), x.gcd(&y))),
        (Some(x), None) | (None, Some(x)) if x.is_zero() => Ok(if a.is_zero() { b.clone() } else { a.clone() }),
        (Some(x), None) => Ok(Poly::constant(a.nvars(), x.gcd(&b.content()))),
        (None, Some(y)) => Ok(Poly::constant(a.nvars(), y.gcd(&a.content()))),
        _ => gcd(a, b, m),
    }
}

/// The simplicity of a pivot candidate (smaller is better): constants first, then few terms.
fn cost(p: &Poly) -> (usize, u64) {
    (p.len() + if p.is_constant() { 0 } else { 1000 }, p.max_bits())
}

/// The solution of the linear system `nums = 0` (polynomials of degree at most one in `vars`): `None` when it is
/// inconsistent, else the (variable, value) pairs of the pivot variables in the order of `vars`.
pub(crate) fn solve_linear<'h>(
    env: &Env<'h>,
    nums: &[Poly],
    ring: &Ring<'h>,
    vars: &[usize],
    m: &mut Meter,
) -> WorkResult<Option<Vec<(T<'h>, T<'h>)>>> {
    let nv = ring.nvars();
    let params: Vec<usize> = (0..nv).filter(|i| !vars.contains(i)).collect();
    let np = params.len();
    let n = vars.len();
    // rows: coefficients of the variables, then the right side
    let mut rows: Vec<Vec<Poly>> = Vec::with_capacity(nums.len());
    for p in nums {
        let g = GPoly::from_poly(p, vars, &params, MonomialOrder::Lex);
        let mut row = vec![Poly::zero(np); n + 1];
        for (e, c) in g.terms {
            match e.iter().position(|&x| x == 1) {
                Some(j) => row[j] = c,
                None => row[n] = c.neg(),
            }
        }
        if row.iter().any(|x| !x.is_zero()) {
            rows.push(row);
        }
    }
    let mut prev = Poly::constant(np, Z::ONE);
    let mut r = 0;
    let mut pivots: Vec<usize> = vec![];
    for c in 0..n {
        if r == rows.len() {
            break;
        }
        let Some(p) = (r..rows.len()).filter(|&i| !rows[i][c].is_zero()).min_by_key(|&i| cost(&rows[i][c])) else {
            continue;
        };
        rows.swap(r, p);
        let (top, rest) = rows.split_at_mut(r + 1);
        let pr = &top[r];
        for row in rest.iter_mut() {
            m.step((n + 1 - c) as u64)?;
            let f = row[c].clone();
            for j in c + 1..=n {
                let t = cmul(&pr[c], &row[j], m)?.sub(&cmul(&f, &pr[j], m)?);
                row[j] = cdiv(&t, &prev, m)?;
            }
            row[c] = Poly::zero(np);
        }
        prev = rows[r][c].clone();
        pivots.push(c);
        r += 1;
    }
    if rows[r..].iter().any(|row| !row[n].is_zero()) {
        return Ok(None);
    }
    rows.truncate(r);
    // back elimination: clear the pivot columns above each pivot
    for k in (0..r).rev() {
        let c = pivots[k];
        for i in 0..k {
            if rows[i][c].is_zero() {
                continue;
            }
            m.step((n + 1) as u64)?;
            let f = rows[i][c].clone();
            let pk = rows[k][c].clone();
            let g = cgcd(&f, &pk, m)?;
            let (a, b) = (cdiv(&pk, &g, m)?, cdiv(&f, &g, m)?);
            let mut new = Vec::with_capacity(n + 1);
            for (x, y) in rows[i].iter().zip(&rows[k]) {
                new.push(cmul(&a, x, m)?.sub(&cmul(&b, y, m)?));
            }
            rows[i] = primitive_row(new, m)?;
        }
    }
    // x_c = (B - sum over free j of A_j x_j) / D
    let free: Vec<usize> = (0..n).filter(|j| !pivots.contains(j)).collect();
    let mut out = vec![];
    let c_ = env.conv();
    for (k, &c) in pivots.iter().enumerate() {
        let row = &rows[k];
        let mut num = row[n].embed(nv, &params);
        for &j in &free {
            if !row[j].is_zero() {
                num = num.sub(&row[j].embed(nv, &params).mul(&Poly::var(nv, vars[j]), m)?);
            }
        }
        let den = row[c].embed(nv, &params);
        let g = gcd(&num, &den, m)?;
        let (mut num, mut den) = if g.is_zero() || g.as_constant().is_some_and(|x| x.is_one()) {
            (num, den)
        } else {
            (exact(num.div_exact(&g, m)?, "gcd divides")?, exact(den.div_exact(&g, m)?, "gcd divides")?)
        };
        if den.lc().is_neg() {
            num = num.neg();
            den = den.neg();
        }
        let nt = c_.int_poly_term(&num, ring, m)?;
        let value = if den.as_constant().is_some_and(|x| x.is_one()) {
            nt
        } else {
            env.ar.div(nt, c_.int_poly_term(&den, ring, m)?)
        };
        out.push((ring.kernels[vars[c]], value));
    }
    Ok(Some(out))
}

/// The row divided by the gcd of its entries.
fn primitive_row(row: Vec<Poly>, m: &mut Meter) -> WorkResult<Vec<Poly>> {
    let np = row[0].nvars();
    let mut g = Poly::zero(np);
    for x in &row {
        if x.is_zero() {
            continue;
        }
        g = if g.is_zero() { x.clone() } else { cgcd(&g, x, m)? };
        if g.as_constant().is_some_and(|z| z.is_one()) {
            return Ok(row);
        }
    }
    if g.is_zero() {
        return Ok(row);
    }
    if g.lc().is_neg() {
        g = g.neg();
    }
    row.iter().map(|x| if x.is_zero() { Ok(x.clone()) } else { cdiv(x, &g, m) }).collect()
}
