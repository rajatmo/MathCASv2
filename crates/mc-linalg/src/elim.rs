//! Gaussian elimination over a field, generic over the entries: machine reals and complex numbers (with a
//! negligibility tolerance), `mc-arith` numbers (exact Gaussian rationals and balls) and algebraic extensions
//! (`crate::algebraic`). Pivoting takes the candidate of largest weight in its column (partial pivoting for machine
//! numbers and balls, the first nonzero entry for exact ones).
//!
//! A ball that may be zero but is not exactly zero cannot serve as a pivot, and cannot be declared zero either: a
//! column whose only candidates are such balls makes the elimination [`Undecided`] (decision LA.6).
use crate::scalar::Scalar;
use mc_arith::{Num, Real};
use mc_poly::work::{Meter, WorkResult};

/// What an entry is worth as a pivot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Weight {
    /// a usable pivot; larger is preferred
    Pivot(f64),
    /// zero (or negligible at the tolerance)
    Zero,
    /// may be zero, may not (a ball containing zero)
    Unknown,
}

/// The elimination met a column it cannot decide (balls containing zero).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Undecided;

/// The entries of a matrix that elimination works on.
pub trait Field: Clone {
    /// Zero of the same field as `self`.
    fn zero_like(&self) -> Self;
    /// One of the same field as `self`.
    fn one_like(&self) -> Self;
    /// `self + o`
    fn add(&self, o: &Self) -> Self;
    /// `self - o`
    fn sub(&self, o: &Self) -> Self;
    /// `self o`
    fn mul(&self, o: &Self) -> Self;
    /// `self / o`; `None` when it has no value (a ball divisor containing zero).
    fn div(&self, o: &Self) -> Option<Self>;
    /// The pivot weight at tolerance `tol` (used by the machine types only).
    fn weight(&self, tol: f64) -> Weight;
}

impl<S: Scalar> Field for S {
    fn zero_like(&self) -> S {
        S::zero()
    }
    fn one_like(&self) -> S {
        S::one()
    }
    fn add(&self, o: &S) -> S {
        *self + *o
    }
    fn sub(&self, o: &S) -> S {
        *self - *o
    }
    fn mul(&self, o: &S) -> S {
        *self * *o
    }
    fn div(&self, o: &S) -> Option<S> {
        Some(*self / *o)
    }
    fn weight(&self, tol: f64) -> Weight {
        let a = self.abs();
        if a > tol {
            Weight::Pivot(a)
        } else {
            Weight::Zero
        }
    }
}

/// The weight of one real part.
fn real_weight(r: &Real) -> Weight {
    match r {
        Real::Exact(q) if q.is_zero() => Weight::Zero,
        Real::Exact(_) => Weight::Pivot(1.0),
        Real::Ball(b, _) if b.mid.is_zero() && b.rad.is_zero() => Weight::Zero,
        Real::Ball(b, _) if b.contains_zero() => Weight::Unknown,
        Real::Ball(..) => Weight::Pivot(r.to_f64().abs()),
        Real::Machine(v) if *v == 0.0 => Weight::Zero,
        Real::Machine(v) => Weight::Pivot(v.abs()),
    }
}

impl Field for Num {
    fn zero_like(&self) -> Num {
        Num::int(0)
    }
    fn one_like(&self) -> Num {
        Num::int(1)
    }
    fn add(&self, o: &Num) -> Num {
        Num::add(self, o)
    }
    fn sub(&self, o: &Num) -> Num {
        Num::add(self, &o.neg())
    }
    fn mul(&self, o: &Num) -> Num {
        Num::mul(self, o)
    }
    fn div(&self, o: &Num) -> Option<Num> {
        o.recip().ok().map(|r| Num::mul(self, &r))
    }
    fn weight(&self, _tol: f64) -> Weight {
        match self {
            Num::Real(r) => real_weight(r),
            Num::Complex(a, b) => match (real_weight(a), real_weight(b)) {
                (Weight::Zero, Weight::Zero) => Weight::Zero,
                (Weight::Pivot(x), Weight::Pivot(y)) => Weight::Pivot(x.hypot(y)),
                (Weight::Pivot(x), _) | (_, Weight::Pivot(x)) => Weight::Pivot(x),
                _ => Weight::Unknown,
            },
        }
    }
}

/// The reduced row echelon form of a matrix, with its pivot columns.
#[derive(Clone, Debug)]
pub struct Rref<F> {
    /// the entries, row-major
    pub a: Vec<F>,
    /// rows
    pub rows: usize,
    /// columns
    pub cols: usize,
    /// the pivot column of each of the first `pivots.len()` rows
    pub pivots: Vec<usize>,
}

impl<F: Field> Rref<F> {
    /// Entry (i, j).
    pub fn at(&self, i: usize, j: usize) -> &F {
        &self.a[i * self.cols + j]
    }
    /// The rank.
    pub fn rank(&self) -> usize {
        self.pivots.len()
    }
}

/// The reduced row echelon form of the `rows x cols` matrix `a` (row-major), pivoting only in the first
/// `pivot_cols` columns (the rest is an augmented part carried along). Entries the tolerance declares negligible
/// are set to zero.
pub fn rref<F: Field>(
    mut a: Vec<F>,
    rows: usize,
    cols: usize,
    pivot_cols: usize,
    tol: f64,
    m: &mut Meter,
) -> WorkResult<Result<Rref<F>, Undecided>> {
    let mut pivots = Vec::new();
    let mut r = 0;
    for c in 0..pivot_cols {
        if r == rows {
            break;
        }
        m.step((rows * cols) as u64 + 1)?;
        let mut best: Option<(usize, f64)> = None;
        let mut unknown = false;
        for i in r..rows {
            match a[i * cols + c].weight(tol) {
                Weight::Pivot(w) if best.is_none_or(|(_, b)| w > b) => best = Some((i, w)),
                Weight::Pivot(_) => {}
                Weight::Unknown => unknown = true,
                Weight::Zero => {
                    let z = a[i * cols + c].zero_like();
                    a[i * cols + c] = z;
                }
            }
        }
        let Some((p, _)) = best else {
            if unknown {
                return Ok(Err(Undecided));
            }
            continue;
        };
        if p != r {
            for j in 0..cols {
                a.swap(r * cols + j, p * cols + j);
            }
        }
        let piv = a[r * cols + c].clone();
        for j in c..cols {
            let Some(q) = a[r * cols + j].div(&piv) else { return Ok(Err(Undecided)) };
            a[r * cols + j] = q;
        }
        let pivot_row: Vec<F> = a[r * cols..(r + 1) * cols].to_vec();
        for i in 0..rows {
            if i == r {
                continue;
            }
            let f = a[i * cols + c].clone();
            if f.weight(0.0) == Weight::Zero {
                continue;
            }
            for j in c..cols {
                let v = a[i * cols + j].sub(&f.mul(&pivot_row[j]));
                a[i * cols + j] = v;
            }
            a[i * cols + c] = f.zero_like();
        }
        pivots.push(c);
        r += 1;
    }
    // tidy: negligible leftovers become zero (machine tolerance)
    if tol > 0.0 {
        for x in &mut a {
            if x.weight(tol) == Weight::Zero {
                *x = x.zero_like();
            }
        }
    }
    Ok(Ok(Rref { a, rows, cols, pivots }))
}

/// The determinant of the square `n x n` matrix `a` (`n >= 1`) by elimination with pivoting.
pub fn det<F: Field>(mut a: Vec<F>, n: usize, m: &mut Meter) -> WorkResult<Result<F, Undecided>> {
    let mut d = a[0].one_like();
    let mut neg = false;
    for c in 0..n {
        m.step(((n - c) * (n - c)) as u64 + 1)?;
        let mut best: Option<(usize, f64)> = None;
        let mut unknown = false;
        for i in c..n {
            match a[i * n + c].weight(0.0) {
                Weight::Pivot(w) if best.is_none_or(|(_, b)| w > b) => best = Some((i, w)),
                Weight::Pivot(_) | Weight::Zero => {}
                Weight::Unknown => unknown = true,
            }
        }
        let Some((p, _)) = best else {
            return Ok(if unknown { Err(Undecided) } else { Ok(a[0].zero_like()) });
        };
        if p != c {
            for j in 0..n {
                a.swap(c * n + j, p * n + j);
            }
            neg = !neg;
        }
        let piv = a[c * n + c].clone();
        d = d.mul(&piv);
        for i in c + 1..n {
            let Some(f) = a[i * n + c].div(&piv) else { return Ok(Err(Undecided)) };
            if f.weight(0.0) == Weight::Zero {
                continue;
            }
            for j in c + 1..n {
                let v = a[i * n + j].sub(&f.mul(&a[c * n + j]));
                a[i * n + j] = v;
            }
        }
    }
    Ok(Ok(if neg { d.zero_like().sub(&d) } else { d }))
}

/// An LU factorisation with partial pivoting `P a = L U` of an `r x c` matrix: `L` (`r x r`, unit lower
/// triangular), `U` (`r x c`) and the permutation (row `i` of `P a` is row `perm[i]` of `a`).
pub struct Lu<F> {
    /// L, row-major
    pub l: Vec<F>,
    /// U, row-major
    pub u: Vec<F>,
    /// the permutation
    pub perm: Vec<usize>,
}

/// The LU factorisation; a column without a pivot is skipped.
pub fn lu<F: Field>(a: &[F], r: usize, c: usize, m: &mut Meter) -> WorkResult<Result<Lu<F>, Undecided>> {
    let mut u = a.to_vec();
    let one = a[0].one_like();
    let zero = a[0].zero_like();
    let mut l = vec![zero.clone(); r * r];
    let mut perm: Vec<usize> = (0..r).collect();
    for k in 0..r.min(c) {
        m.step(((r - k) * c) as u64 + 1)?;
        let mut best: Option<(usize, f64)> = None;
        let mut unknown = false;
        for i in k..r {
            match u[i * c + k].weight(0.0) {
                Weight::Pivot(w) if best.is_none_or(|(_, b)| w > b) => best = Some((i, w)),
                Weight::Pivot(_) | Weight::Zero => {}
                Weight::Unknown => unknown = true,
            }
        }
        let Some((p, _)) = best else {
            if unknown {
                return Ok(Err(Undecided));
            }
            continue;
        };
        if p != k {
            for j in 0..c {
                u.swap(k * c + j, p * c + j);
            }
            for j in 0..k {
                l.swap(k * r + j, p * r + j);
            }
            perm.swap(k, p);
        }
        let piv = u[k * c + k].clone();
        for i in k + 1..r {
            let Some(f) = u[i * c + k].div(&piv) else { return Ok(Err(Undecided)) };
            for j in k..c {
                let v = u[i * c + j].sub(&f.mul(&u[k * c + j]));
                u[i * c + j] = v;
            }
            u[i * c + k] = zero.clone();
            l[i * r + k] = f;
        }
    }
    for i in 0..r {
        l[i * r + i] = one.clone();
    }
    Ok(Ok(Lu { l, u, perm }))
}

/// A basis of the null space from a reduced row echelon form (of the first `n` columns): one vector per free
/// column, with that column's entry one, listed from the last free column to the first.
pub fn null_basis<F: Field>(r: &Rref<F>, n: usize) -> Vec<Vec<F>> {
    let zero = r.a.first().map(|x| x.zero_like());
    let Some(zero) = zero else { return vec![] };
    let one = zero.one_like();
    let mut free: Vec<usize> = (0..n).filter(|c| !r.pivots.contains(c)).collect();
    free.reverse();
    free.iter()
        .map(|&f| {
            let mut v = vec![zero.clone(); n];
            v[f] = one.clone();
            for (i, &p) in r.pivots.iter().enumerate() {
                v[p] = zero.sub(r.at(i, f));
            }
            v
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_poly::work::Unlimited;

    #[test]
    fn exact_elimination() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let a: Vec<Num> = [1, 2, 3, 4, 5, 6, 7, 8, 10].iter().map(|&v| Num::int(v)).collect();
        let d = det(a.clone(), 3, &mut m).unwrap().unwrap();
        assert_eq!(d.as_rat().unwrap(), &mc_arith::num::Rat::from_i64(-3));
        let r = rref(a, 3, 3, 3, 0.0, &mut m).unwrap().unwrap();
        assert_eq!(r.rank(), 3);
        let s: Vec<Num> = [1, 2, 3, 2, 4, 6].iter().map(|&v| Num::int(v)).collect();
        let r = rref(s, 2, 3, 3, 0.0, &mut m).unwrap().unwrap();
        assert_eq!(r.pivots, vec![0]);
        let ns = null_basis(&r, 3);
        assert_eq!(ns.len(), 2);
        // listed from the last free column: [-3, 0, 1], then [-2, 1, 0]
        assert_eq!(ns[0][0].as_rat().unwrap(), &mc_arith::num::Rat::from_i64(-3));
        assert_eq!(ns[1][0].as_rat().unwrap(), &mc_arith::num::Rat::from_i64(-2));
    }

    #[test]
    fn machine_tolerance() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let r = rref(a, 3, 3, 3, 3.0 * f64::EPSILON * 9.0, &mut m).unwrap().unwrap();
        assert_eq!(r.rank(), 2);
    }
}
