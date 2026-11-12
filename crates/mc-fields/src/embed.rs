//! Embeddings of finite fields: F_(p^m) into F_(p^n) when m divides n (Lidl and Niederreiter, Finite Fields,
//! theorem 2.6), given by the image of the generator x of the smaller field, a root of its defining polynomial in
//! the larger one.
//!
//! - Between two fields defined by Conway polynomials the image is the canonical x^((p^n - 1)/(p^m - 1)): the
//!   compatibility of Conway polynomials makes these embeddings commute.
//! - Otherwise it is the root with the least polynomial index (roots by Cantor and Zassenhaus, `fqpoly::roots`).
//! - `a -> b` with a generating the smaller field and b a root of its minimal polynomial fixes the embedding too:
//!   x = h(a) for the h found by linear algebra over F_p, and x goes to h(b).
//!
//! The projection back is the F_p-linear left inverse that reads the coordinates of the image at the first rows
//! where the images of 1, x, ..., x^(m-1) are independent: it inverts the embedding on its image and is linear, not
//! multiplicative, elsewhere.
use crate::conway;
use crate::field::{El, Field};
use crate::fp::Fp;
use crate::fqpoly;
use mc_eval::Res;
use mc_numtheory::budget::Budget;

/// An embedding of `small` into `big`: the image of the generator and the projection data.
pub struct Embedding {
    /// the image of the generator of the smaller field
    pub image: El,
    /// the images of x^j, j < m, as coordinate columns (n entries each)
    pub basis: Vec<Vec<u64>>,
}

/// Solve the square system M v = r over F_p (M given by rows); None when singular.
pub fn solve(f: Fp, m: &[Vec<u64>], r: &[u64]) -> Option<Vec<u64>> {
    let n = m.len();
    let mut a: Vec<Vec<u64>> = m.iter().zip(r).map(|(row, &x)| {
        let mut v = row.clone();
        v.push(x);
        v
    }).collect();
    for c in 0..n {
        let piv = (c..n).find(|&i| a[i][c] != 0)?;
        a.swap(c, piv);
        let inv = f.inv(a[c][c])?;
        for x in a[c].iter_mut() {
            *x = f.mul(*x, inv);
        }
        for i in 0..n {
            if i != c && a[i][c] != 0 {
                let k = a[i][c];
                let row = a[c].clone();
                for (x, y) in a[i].iter_mut().zip(&row) {
                    *x = f.sub(*x, f.mul(k, *y));
                }
            }
        }
    }
    Some(a.into_iter().map(|row| row[n]).collect())
}

fn coords(a: &El, n: usize) -> Vec<u64> {
    (0..n).map(|i| a.get(i).copied().unwrap_or(0)).collect()
}

/// The canonical embedding of `small` into `big` (same characteristic, degree dividing).
pub fn canonical(small: &Field, big: &Field, b: &mut dyn Budget) -> Res<Option<El>> {
    if small.p() != big.p() || !big.n.is_multiple_of(small.n) {
        return Ok(None);
    }
    let p = small.p();
    let cm = conway::conway(p, small.n, conway::RUNTIME_STEPS, b)?;
    let cn = conway::conway(p, big.n, conway::RUNTIME_STEPS, b)?;
    if cm.as_deref() == Some(&small.modulus[..]) && cn.as_deref() == Some(&big.modulus[..]) {
        let e = big.order.div(&small.order);
        return Ok(Some(big.pow(&big.gen(), &e)));
    }
    let lifted: fqpoly::QPoly = small.modulus.iter().map(|&c| big.constant(c)).collect();
    let roots = fqpoly::roots(big, &lifted, b)?;
    Ok(roots.into_iter().next())
}

/// The embedding of `small` into `big` with x mapped to `image` (a root of the defining polynomial of `small`).
pub fn with_image(small: &Field, big: &Field, image: El) -> Option<Embedding> {
    let lifted: fqpoly::QPoly = small.modulus.iter().map(|&c| big.constant(c)).collect();
    if !fqpoly::eval(big, &lifted, &image).is_empty() {
        return None;
    }
    let mut basis = vec![];
    let mut pw = big.one();
    for _ in 0..small.n {
        basis.push(coords(&pw, big.n));
        pw = big.mul(&pw, &image);
    }
    Some(Embedding { image, basis })
}

/// The embedding fixed by a -> b: a in `small` of full degree, b in `big` a root of the minimal polynomial of a.
pub fn from_rule(small: &Field, big: &Field, a: &El, bb: &El) -> Option<Embedding> {
    if small.p() != big.p() || !big.n.is_multiple_of(small.n) || small.degree_of(a) != small.n {
        return None;
    }
    let ma = small.minimal_polynomial(a);
    let lifted: fqpoly::QPoly = ma.iter().map(|&c| big.constant(c)).collect();
    if !fqpoly::eval(big, &lifted, bb).is_empty() {
        return None;
    }
    // x = sum h_j a^j: the columns of the system are the coordinates of a^j
    let m = small.n;
    let mut cols = vec![];
    let mut pw = small.one();
    for _ in 0..m {
        cols.push(coords(&pw, m));
        pw = small.mul(&pw, a);
    }
    let rows: Vec<Vec<u64>> = (0..m).map(|i| cols.iter().map(|c| c[i]).collect()).collect();
    let hcoef = solve(small.fp, &rows, &coords(&small.gen(), m))?;
    // the image of x is h(b)
    let mut img = vec![];
    let mut pw = big.one();
    for &hc in &hcoef {
        img = big.add(&img, &big.scale(&pw, hc));
        pw = big.mul(&pw, bb);
    }
    with_image(small, big, img)
}

impl Embedding {
    /// The image of c in the smaller field.
    pub fn apply(&self, small: &Field, big: &Field, c: &El) -> El {
        let _ = small;
        let mut out = vec![];
        for (j, &cj) in c.iter().enumerate() {
            if cj != 0 {
                let col: El = {
                    let mut v = self.basis[j].clone();
                    crate::fp::trim(&mut v);
                    v
                };
                out = big.add(&out, &big.scale(&col, cj));
            }
        }
        out
    }

    /// The projection of d in the larger field back to the smaller one (a left inverse of [`Embedding::apply`]).
    pub fn project(&self, small: &Field, big: &Field, d: &El) -> Option<El> {
        let m = small.n;
        let n = big.n;
        // rows of the n x m matrix of the basis, greedily independent from the top
        let f = small.fp;
        let mut chosen: Vec<usize> = vec![];
        let mut echelon: Vec<Vec<u64>> = vec![];
        for i in 0..n {
            let mut row: Vec<u64> = self.basis.iter().map(|c| c[i]).collect();
            // reduce against the echelon rows
            for e in &echelon {
                // invariant: every echelon row has a leading one
                let lead = e.iter().position(|&x| x != 0).expect("a nonzero row");
                if row[lead] != 0 {
                    let k = row[lead];
                    for (x, y) in row.iter_mut().zip(e) {
                        *x = f.sub(*x, f.mul(k, *y));
                    }
                }
            }
            if let Some(lead) = row.iter().position(|&x| x != 0) {
                let inv = f.inv(row[lead])?;
                for x in row.iter_mut() {
                    *x = f.mul(*x, inv);
                }
                echelon.push(row);
                chosen.push(i);
                if chosen.len() == m {
                    break;
                }
            }
        }
        if chosen.len() < m {
            return None;
        }
        let rows: Vec<Vec<u64>> = chosen.iter().map(|&i| self.basis.iter().map(|c| c[i]).collect()).collect();
        let rhs: Vec<u64> = chosen.iter().map(|&i| d.get(i).copied().unwrap_or(0)).collect();
        let v = solve(f, &rows, &rhs)?;
        Some(small.reduce(&v))
    }
}
