//! Dense machine kernels on row-major matrices of `f64` or [`Cx`]: products, LU with partial pivoting, Householder
//! QR, Cholesky, the symmetric eigenproblem by cyclic Jacobi, the general eigenproblem by Householder reduction to
//! Hessenberg form and shifted QR (a complex Schur form) with eigenvectors by back substitution, and the singular
//! value decomposition by one-sided Jacobi.
//!
//! Every loop whose length depends on the input charges its work to a [`Meter`] (decision LA.9), which polls the
//! evaluator's fuel and interrupt.
use crate::scalar::{csqrt, phase, Cx, Scalar};
use mc_poly::work::{Meter, WorkResult};

/// The machine epsilon (unit roundoff times two).
pub const EPS: f64 = f64::EPSILON;

/// A dense row-major matrix.
#[derive(Clone, Debug, PartialEq)]
pub struct Mat<S> {
    /// the number of rows
    pub rows: usize,
    /// the number of columns
    pub cols: usize,
    /// the entries, row-major
    pub a: Vec<S>,
}

impl<S: Scalar> Mat<S> {
    /// The zero matrix.
    pub fn zeros(rows: usize, cols: usize) -> Mat<S> {
        Mat { rows, cols, a: vec![S::zero(); rows * cols] }
    }
    /// The identity.
    pub fn identity(n: usize) -> Mat<S> {
        let mut m = Mat::zeros(n, n);
        for i in 0..n {
            m.a[i * n + i] = S::one();
        }
        m
    }
    /// Entry (i, j).
    #[inline]
    pub fn at(&self, i: usize, j: usize) -> S {
        self.a[i * self.cols + j]
    }
    /// Entry (i, j), writable.
    #[inline]
    pub fn at_mut(&mut self, i: usize, j: usize) -> &mut S {
        &mut self.a[i * self.cols + j]
    }
    /// Row i.
    #[inline]
    pub fn row(&self, i: usize) -> &[S] {
        &self.a[i * self.cols..(i + 1) * self.cols]
    }
    /// The conjugate transpose.
    pub fn adjoint(&self) -> Mat<S> {
        let mut t = Mat::zeros(self.cols, self.rows);
        for i in 0..self.rows {
            for j in 0..self.cols {
                t.a[j * self.rows + i] = self.at(i, j).conj();
            }
        }
        t
    }
    /// The largest modulus of an entry.
    pub fn max_abs(&self) -> f64 {
        self.a.iter().fold(0.0, |m, x| m.max(x.abs()))
    }
    /// The Frobenius norm.
    pub fn frobenius(&self) -> f64 {
        let s = self.max_abs();
        if s == 0.0 || !s.is_finite() {
            return s;
        }
        // scaled, so that squares neither overflow nor underflow
        s * self.a.iter().map(|&x| (x / S::real(s)).abs2()).sum::<f64>().sqrt()
    }
    /// Are all entries finite?
    pub fn is_finite(&self) -> bool {
        self.a.iter().all(|x| x.is_finite())
    }
    /// The complex copy.
    pub fn to_cx(&self) -> Mat<Cx> {
        Mat { rows: self.rows, cols: self.cols, a: self.a.iter().map(|x| x.to_cx()).collect() }
    }
    /// Columns `lo..hi`.
    pub fn columns(&self, lo: usize, hi: usize) -> Mat<S> {
        let mut out = Mat::zeros(self.rows, hi - lo);
        for i in 0..self.rows {
            out.a[i * (hi - lo)..(i + 1) * (hi - lo)].copy_from_slice(&self.row(i)[lo..hi]);
        }
        out
    }
}

/// `a b`, by rows (the i-k-j order, so the inner loop runs along contiguous rows).
pub fn mul<S: Scalar>(a: &Mat<S>, b: &Mat<S>, m: &mut Meter) -> WorkResult<Mat<S>> {
    debug_assert_eq!(a.cols, b.rows);
    let mut c = Mat::zeros(a.rows, b.cols);
    let n = b.cols;
    for i in 0..a.rows {
        m.step((a.cols * n) as u64 + 1)?;
        let ci = &mut c.a[i * n..(i + 1) * n];
        for k in 0..a.cols {
            let f = a.a[i * a.cols + k];
            if f == S::zero() {
                continue;
            }
            for (x, &y) in ci.iter_mut().zip(&b.a[k * n..(k + 1) * n]) {
                *x += f * y;
            }
        }
    }
    Ok(c)
}

/// An LU factorisation with partial pivoting: `P a = L U` with `L` unit lower triangular (below the diagonal of
/// `lu`) and `U` upper triangular (on and above it); row `i` of `P a` is row `perm[i]` of `a`.
pub struct Lu<S> {
    /// L and U packed together
    pub lu: Mat<S>,
    /// the row permutation
    pub perm: Vec<usize>,
    /// the sign of the permutation
    pub sign: f64,
    /// the largest modulus of an entry of the input (the scale of the singularity test)
    pub scale: f64,
}

/// The LU factorisation of a square or rectangular matrix. A column without a nonzero candidate is skipped (its
/// pivot is zero), so the factorisation always exists.
pub fn lu<S: Scalar>(mut a: Mat<S>, m: &mut Meter) -> WorkResult<Lu<S>> {
    let (r, c) = (a.rows, a.cols);
    let scale = a.max_abs();
    let mut perm: Vec<usize> = (0..r).collect();
    let mut sign = 1.0;
    for k in 0..r.min(c) {
        m.step(((r - k) * (c - k)) as u64 + 1)?;
        let (mut p, mut best) = (k, a.at(k, k).abs());
        for i in k + 1..r {
            let v = a.at(i, k).abs();
            if v > best {
                (p, best) = (i, v);
            }
        }
        if best == 0.0 {
            continue;
        }
        if p != k {
            for j in 0..c {
                a.a.swap(k * c + j, p * c + j);
            }
            perm.swap(k, p);
            sign = -sign;
        }
        let piv = a.at(k, k);
        let (top, rest) = a.a.split_at_mut((k + 1) * c);
        let pivot_row = &top[k * c + k + 1..(k + 1) * c];
        for i in 0..r - k - 1 {
            let row = &mut rest[i * c..(i + 1) * c];
            let l = row[k] / piv;
            row[k] = l;
            if l == S::zero() {
                continue;
            }
            for (x, &u) in row[k + 1..].iter_mut().zip(pivot_row) {
                *x -= l * u;
            }
        }
    }
    Ok(Lu { lu: a, perm, sign, scale })
}

impl<S: Scalar> Lu<S> {
    /// The determinant (square input): the signed product of the pivots.
    pub fn det(&self) -> S {
        let n = self.lu.rows;
        (0..n).fold(S::real(self.sign), |d, i| d * self.lu.at(i, i))
    }
    /// Is the (square) matrix singular to working precision: a pivot below `n eps` times its largest entry?
    pub fn singular(&self) -> bool {
        let n = self.lu.rows;
        let tol = n as f64 * EPS * self.scale;
        self.scale == 0.0 || (0..n).any(|i| self.lu.at(i, i).abs().partial_cmp(&tol) != Some(std::cmp::Ordering::Greater))
    }
    /// The solution `x` of `a x = b` for a nonsingular square `a` and `b` with any number of columns.
    pub fn solve(&self, b: &Mat<S>, m: &mut Meter) -> WorkResult<Mat<S>> {
        let n = self.lu.rows;
        let k = b.cols;
        let mut x = Mat::zeros(n, k);
        for i in 0..n {
            x.a[i * k..(i + 1) * k].copy_from_slice(b.row(self.perm[i]));
        }
        // forward substitution with the unit lower triangle, a row at a time
        for i in 1..n {
            m.step((i * k) as u64 + 1)?;
            let (done, rest) = x.a.split_at_mut(i * k);
            let xi = &mut rest[..k];
            for (j, &l) in self.lu.row(i)[..i].iter().enumerate() {
                if l == S::zero() {
                    continue;
                }
                for (v, &w) in xi.iter_mut().zip(&done[j * k..(j + 1) * k]) {
                    *v -= l * w;
                }
            }
        }
        // back substitution with the upper triangle
        for i in (0..n).rev() {
            m.step(((n - i) * k) as u64 + 1)?;
            let (head, tail) = x.a.split_at_mut((i + 1) * k);
            let xi = &mut head[i * k..];
            for (j, &u) in self.lu.row(i)[i + 1..].iter().enumerate() {
                if u == S::zero() {
                    continue;
                }
                for (v, &w) in xi.iter_mut().zip(&tail[j * k..(j + 1) * k]) {
                    *v -= u * w;
                }
            }
            let d = self.lu.at(i, i);
            for v in xi.iter_mut() {
                *v = *v / d;
            }
        }
        Ok(x)
    }
}

/// Householder QR of an `r x c` matrix: `a = q rr` with `q` (`r x k`, orthonormal columns) and `rr` (`k x c`, upper
/// triangular), `k = min(r, c)`. The diagonal of `rr` is made real and non-negative.
pub fn qr<S: Scalar>(mut a: Mat<S>, m: &mut Meter) -> WorkResult<(Mat<S>, Mat<S>)> {
    let (r, c) = (a.rows, a.cols);
    let k = r.min(c);
    let mut vs: Vec<Vec<S>> = Vec::with_capacity(k);
    for j in 0..k {
        m.step(((r - j) * (c - j)) as u64 * 2 + 1)?;
        let v = householder(&(j..r).map(|i| a.at(i, j)).collect::<Vec<S>>());
        if let Some(v) = &v {
            reflect_left(&mut a, v, j, j);
        }
        vs.push(v.unwrap_or_default());
    }
    let mut q = Mat::zeros(r, k);
    for i in 0..k {
        *q.at_mut(i, i) = S::one();
    }
    for j in (0..k).rev() {
        m.step(((r - j) * k) as u64 * 2 + 1)?;
        if !vs[j].is_empty() {
            reflect_left(&mut q, &vs[j], j, 0);
        }
    }
    let mut rr = Mat::zeros(k, c);
    for i in 0..k {
        for j in i..c {
            *rr.at_mut(i, j) = a.at(i, j);
        }
    }
    // a unit phase per column of q and row of rr makes the diagonal of rr real and non-negative
    for i in 0..k {
        let d = rr.at(i, i);
        if d.abs() == 0.0 {
            continue;
        }
        let p = phase(d);
        for j in i..c {
            *rr.at_mut(i, j) = p.conj() * rr.at(i, j);
        }
        *rr.at_mut(i, i) = S::real(d.abs());
        for row in 0..r {
            *q.at_mut(row, i) = q.at(row, i) * p;
        }
    }
    Ok((q, rr))
}

/// The unit vector `v` of the reflection `I - 2 v v^H` that maps `x` to a multiple of the first unit vector;
/// `None` when `x` is zero.
fn householder<S: Scalar>(x: &[S]) -> Option<Vec<S>> {
    let s = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if s == 0.0 {
        return None;
    }
    let norm = s * x.iter().map(|v| v.scale(1.0 / s).abs2()).sum::<f64>().sqrt();
    let alpha = -(phase(x[0]).scale(norm));
    let mut v = x.to_vec();
    v[0] -= alpha;
    let vs = v.iter().fold(0.0f64, |m, e| m.max(e.abs()));
    let vn = vs * v.iter().map(|e| e.scale(1.0 / vs).abs2()).sum::<f64>().sqrt();
    if vn == 0.0 {
        return None;
    }
    for e in &mut v {
        *e = e.scale(1.0 / vn);
    }
    Some(v)
}

/// `a[r0.., c0..] = (I - 2 v v^H) a[r0.., c0..]`.
fn reflect_left<S: Scalar>(a: &mut Mat<S>, v: &[S], r0: usize, c0: usize) {
    let c = a.cols;
    let mut w = vec![S::zero(); c - c0];
    for (i, &vi) in v.iter().enumerate() {
        let vc = vi.conj();
        for (wj, &x) in w.iter_mut().zip(&a.a[(r0 + i) * c + c0..(r0 + i + 1) * c]) {
            *wj += vc * x;
        }
    }
    for (i, &vi) in v.iter().enumerate() {
        let f = vi.scale(2.0);
        for (x, &wj) in a.a[(r0 + i) * c + c0..(r0 + i + 1) * c].iter_mut().zip(&w) {
            *x -= f * wj;
        }
    }
}

/// `a[r0.., c0..] = a[r0.., c0..] (I - 2 v v^H)`.
fn reflect_right<S: Scalar>(a: &mut Mat<S>, v: &[S], r0: usize, c0: usize) {
    let c = a.cols;
    for i in r0..a.rows {
        let row = &mut a.a[i * c + c0..i * c + c0 + v.len()];
        let mut d = S::zero();
        for (&x, &vj) in row.iter().zip(v) {
            d += x * vj;
        }
        let d = d.scale(2.0);
        for (x, &vj) in row.iter_mut().zip(v) {
            *x -= d * vj.conj();
        }
    }
}

/// Is the square matrix Hermitian (symmetric when real) to working precision?
pub fn is_hermitian<S: Scalar>(a: &Mat<S>) -> bool {
    let n = a.rows;
    let tol = 64.0 * EPS * a.max_abs();
    (0..n).all(|i| (0..=i).all(|j| (a.at(i, j) - a.at(j, i).conj()).abs() <= tol))
}

/// Why a Cholesky factorisation does not exist.
#[derive(Debug, PartialEq, Eq)]
pub enum CholeskyError {
    /// the matrix is not Hermitian
    NotHermitian,
    /// a pivot is not positive
    NotPositiveDefinite,
}

/// The Cholesky factor: upper triangular `u` with `a = u^H u`, for a Hermitian positive definite `a`.
pub fn cholesky<S: Scalar>(a: &Mat<S>, m: &mut Meter) -> WorkResult<Result<Mat<S>, CholeskyError>> {
    let n = a.rows;
    if !is_hermitian(a) {
        return Ok(Err(CholeskyError::NotHermitian));
    }
    let mut u: Mat<S> = Mat::zeros(n, n);
    for j in 0..n {
        m.step((j * n) as u64 + 1)?;
        let mut s = a.at(j, j).re();
        for k in 0..j {
            s -= u.at(k, j).abs2();
        }
        if s.is_nan() || s <= 0.0 {
            return Ok(Err(CholeskyError::NotPositiveDefinite));
        }
        let d = s.sqrt();
        *u.at_mut(j, j) = S::real(d);
        for i in j + 1..n {
            let mut t = a.at(j, i);
            for k in 0..j {
                t -= u.at(k, j).conj() * u.at(k, i);
            }
            *u.at_mut(j, i) = t.scale(1.0 / d);
        }
    }
    Ok(Ok(u))
}

/// The eigenvalues and orthonormal eigenvectors (the columns of the second result) of a real symmetric matrix:
/// Householder reduction to tridiagonal form, then the implicit QL algorithm with Wilkinson-type shifts (the
/// classical `tred2`/`tql2` pair, decision LA.8). Matrices up to 16 x 16 use cyclic Jacobi instead, which is
/// accurate to high relative precision for small eigenvalues.
pub fn symmetric_eigen(a: &Mat<f64>, m: &mut Meter) -> WorkResult<(Vec<f64>, Mat<f64>)> {
    if a.rows <= 16 {
        return jacobi_eigen(a, m);
    }
    let n = a.rows;
    let mut v = a.clone();
    let mut d = vec![0.0; n];
    let mut e = vec![0.0; n];
    tred2(&mut v, &mut d, &mut e, m)?;
    tql2(&mut v, &mut d, &mut e, m)?;
    Ok((d, v))
}

/// Householder tridiagonalisation: on return `v` holds the orthogonal transformation, `d` the diagonal and `e`
/// the subdiagonal (in `e[1..]`). The loops index several arrays in step, as in the classical formulation.
#[allow(clippy::needless_range_loop)]
fn tred2(v: &mut Mat<f64>, d: &mut [f64], e: &mut [f64], m: &mut Meter) -> WorkResult<()> {
    let n = v.rows;
    for j in 0..n {
        d[j] = v.at(n - 1, j);
    }
    for i in (1..n).rev() {
        m.step((i * n) as u64 * 2 + 1)?;
        let mut scale = 0.0;
        let mut h = 0.0;
        for &dk in &d[..i] {
            scale += dk.abs();
        }
        if scale == 0.0 {
            e[i] = d[i - 1];
            for j in 0..i {
                d[j] = v.at(i - 1, j);
                *v.at_mut(i, j) = 0.0;
                *v.at_mut(j, i) = 0.0;
            }
        } else {
            for dk in &mut d[..i] {
                *dk /= scale;
                h += *dk * *dk;
            }
            let mut f = d[i - 1];
            let mut g = h.sqrt();
            if f > 0.0 {
                g = -g;
            }
            e[i] = scale * g;
            h -= f * g;
            d[i - 1] = f - g;
            for ej in &mut e[..i] {
                *ej = 0.0;
            }
            for j in 0..i {
                f = d[j];
                *v.at_mut(j, i) = f;
                g = e[j] + v.at(j, j) * f;
                for k in j + 1..i {
                    g += v.at(k, j) * d[k];
                    e[k] += v.at(k, j) * f;
                }
                e[j] = g;
            }
            f = 0.0;
            for j in 0..i {
                e[j] /= h;
                f += e[j] * d[j];
            }
            let hh = f / (h + h);
            for j in 0..i {
                e[j] -= hh * d[j];
            }
            for j in 0..i {
                f = d[j];
                g = e[j];
                for k in j..i {
                    *v.at_mut(k, j) -= f * e[k] + g * d[k];
                }
                d[j] = v.at(i - 1, j);
                *v.at_mut(i, j) = 0.0;
            }
        }
        d[i] = h;
    }
    // accumulate the transformations
    for i in 0..n.saturating_sub(1) {
        m.step((i * n) as u64 * 2 + 1)?;
        let x = v.at(i, i);
        *v.at_mut(n - 1, i) = x;
        *v.at_mut(i, i) = 1.0;
        let h = d[i + 1];
        if h != 0.0 {
            for k in 0..=i {
                d[k] = v.at(k, i + 1) / h;
            }
            for j in 0..=i {
                let mut g = 0.0;
                for k in 0..=i {
                    g += v.at(k, i + 1) * v.at(k, j);
                }
                for k in 0..=i {
                    *v.at_mut(k, j) -= g * d[k];
                }
            }
        }
        for k in 0..=i {
            *v.at_mut(k, i + 1) = 0.0;
        }
    }
    for j in 0..n {
        d[j] = v.at(n - 1, j);
        *v.at_mut(n - 1, j) = 0.0;
    }
    *v.at_mut(n - 1, n - 1) = 1.0;
    e[0] = 0.0;
    Ok(())
}

/// The implicit QL algorithm on the tridiagonal `d`, `e`, accumulating the rotations into `v` (at most 64
/// iterations per eigenvalue; non-convergence leaves the current approximation).
#[allow(clippy::needless_range_loop)]
fn tql2(v: &mut Mat<f64>, d: &mut [f64], e: &mut [f64], m: &mut Meter) -> WorkResult<()> {
    let n = v.rows;
    for i in 1..n {
        e[i - 1] = e[i];
    }
    e[n - 1] = 0.0;
    let mut f = 0.0;
    let mut tst1: f64 = 0.0;
    for l in 0..n {
        tst1 = tst1.max(d[l].abs() + e[l].abs());
        let mut mm = l;
        while mm < n {
            if e[mm].abs() <= EPS * tst1 {
                break;
            }
            mm += 1;
        }
        let mm = mm.min(n - 1);
        if mm > l {
            for _iter in 0..64 {
                m.step((n * (mm - l + 1)) as u64 * 4 + 1)?;
                let mut g = d[l];
                let mut p = (d[l + 1] - g) / (2.0 * e[l]);
                let mut r = p.hypot(1.0);
                if p < 0.0 {
                    r = -r;
                }
                d[l] = e[l] / (p + r);
                d[l + 1] = e[l] * (p + r);
                let dl1 = d[l + 1];
                let mut h = g - d[l];
                for di in &mut d[l + 2..n] {
                    *di -= h;
                }
                f += h;
                p = d[mm];
                let mut c = 1.0;
                let mut c2 = c;
                let mut c3 = c;
                let el1 = e[l + 1];
                let mut s = 0.0;
                let mut s2 = 0.0;
                for i in (l..mm).rev() {
                    c3 = c2;
                    c2 = c;
                    s2 = s;
                    g = c * e[i];
                    h = c * p;
                    r = p.hypot(e[i]);
                    e[i + 1] = s * r;
                    s = e[i] / r;
                    c = p / r;
                    p = c * d[i] - s * g;
                    d[i + 1] = h + s * (c * g + s * d[i]);
                    for k in 0..n {
                        h = v.at(k, i + 1);
                        *v.at_mut(k, i + 1) = s * v.at(k, i) + c * h;
                        *v.at_mut(k, i) = c * v.at(k, i) - s * h;
                    }
                }
                p = -s * s2 * c3 * el1 * e[l] / dl1;
                e[l] = s * p;
                d[l] = c * p;
                if e[l].abs() <= EPS * tst1 {
                    break;
                }
            }
        }
        d[l] += f;
        e[l] = 0.0;
    }
    Ok(())
}

/// The eigenvalues and orthonormal eigenvectors of a real symmetric matrix by the cyclic Jacobi method with
/// threshold: sweeps of plane rotations until the off-diagonal part is below `EPS` times the Frobenius norm (at
/// most 64 sweeps; quadratic convergence makes 6 to 10 typical).
fn jacobi_eigen(a: &Mat<f64>, m: &mut Meter) -> WorkResult<(Vec<f64>, Mat<f64>)> {
    let n = a.rows;
    let mut a = a.clone();
    let mut v = Mat::<f64>::identity(n);
    let total = a.frobenius();
    for _sweep in 0..64 {
        m.step((n * n * n) as u64 + 1)?;
        let off: f64 = (0..n).flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j))).map(|(i, j)| a.at(i, j).powi(2)).sum();
        if off.sqrt() <= EPS * total || total == 0.0 {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = a.at(p, q);
                if apq == 0.0 {
                    continue;
                }
                let (app, aqq) = (a.at(p, p), a.at(q, q));
                let theta = (aqq - app) / (2.0 * apq);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                // a = J^T a J on rows and columns p, q
                for k in 0..n {
                    let (akp, akq) = (a.at(k, p), a.at(k, q));
                    *a.at_mut(k, p) = c * akp - s * akq;
                    *a.at_mut(k, q) = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a.at(p, k), a.at(q, k));
                    *a.at_mut(p, k) = c * apk - s * aqk;
                    *a.at_mut(q, k) = s * apk + c * aqk;
                }
                for k in 0..n {
                    let (vkp, vkq) = (v.at(k, p), v.at(k, q));
                    *v.at_mut(k, p) = c * vkp - s * vkq;
                    *v.at_mut(k, q) = s * vkp + c * vkq;
                }
            }
        }
    }
    Ok(((0..n).map(|i| a.at(i, i)).collect(), v))
}

/// A complex Schur form `a = q t q^H` (`t` upper triangular, `q` unitary): Householder reduction to Hessenberg
/// form, then shifted QR steps by Givens rotations with Wilkinson shifts (an exceptional shift every 10 steps
/// without deflation), deflating on negligible subdiagonal entries. `None` if 60 n steps do not converge.
pub fn schur(a: &Mat<Cx>, m: &mut Meter) -> WorkResult<Option<(Mat<Cx>, Mat<Cx>)>> {
    let n = a.rows;
    let mut h = a.clone();
    let mut q = Mat::<Cx>::identity(n);
    // Hessenberg reduction
    for k in 0..n.saturating_sub(2) {
        m.step((n * n) as u64 * 4 + 1)?;
        let x: Vec<Cx> = (k + 1..n).map(|i| h.at(i, k)).collect();
        if let Some(v) = householder(&x) {
            reflect_left(&mut h, &v, k + 1, 0);
            reflect_right(&mut h, &v, 0, k + 1);
            reflect_right(&mut q, &v, 0, k + 1);
        }
        for i in k + 2..n {
            *h.at_mut(i, k) = Cx::default();
        }
    }
    let norm = h.frobenius();
    let mut hi = n.saturating_sub(1);
    let mut iter = 0usize;
    let mut total = 0usize;
    while hi > 0 {
        // deflate at a negligible subdiagonal entry
        let mut l = hi;
        while l > 0 {
            let s = h.at(l - 1, l - 1).abs() + h.at(l, l).abs();
            let s = if s == 0.0 { norm } else { s };
            if h.at(l, l - 1).abs() <= EPS * s {
                *h.at_mut(l, l - 1) = Cx::default();
                break;
            }
            l -= 1;
        }
        if l == hi {
            hi -= 1;
            iter = 0;
            continue;
        }
        iter += 1;
        total += 1;
        if total > 60 * n {
            return Ok(None);
        }
        m.step((n * (hi - l + 1)) as u64 * 6 + 1)?;
        let mu = if iter.is_multiple_of(10) {
            // an exceptional shift breaks cycles
            h.at(hi, hi) + Cx::new(0.75 * h.at(hi, hi - 1).abs(), 0.0)
        } else {
            wilkinson(h.at(hi - 1, hi - 1), h.at(hi - 1, hi), h.at(hi, hi - 1), h.at(hi, hi))
        };
        for i in l..=hi {
            *h.at_mut(i, i) -= mu;
        }
        let mut rots = Vec::with_capacity(hi - l);
        for k in l..hi {
            let (c, s) = givens(h.at(k, k), h.at(k + 1, k));
            for j in k..n {
                let (x, y) = (h.at(k, j), h.at(k + 1, j));
                *h.at_mut(k, j) = x.scale(c) + s * y;
                *h.at_mut(k + 1, j) = y.scale(c) - s.conj() * x;
            }
            rots.push((c, s));
        }
        for (k, &(c, s)) in (l..hi).zip(&rots) {
            for i in 0..=(k + 2).min(hi) {
                let (x, y) = (h.at(i, k), h.at(i, k + 1));
                *h.at_mut(i, k) = x.scale(c) + y * s.conj();
                *h.at_mut(i, k + 1) = y.scale(c) - x * s;
            }
            for i in 0..n {
                let (x, y) = (q.at(i, k), q.at(i, k + 1));
                *q.at_mut(i, k) = x.scale(c) + y * s.conj();
                *q.at_mut(i, k + 1) = y.scale(c) - x * s;
            }
        }
        for i in l..=hi {
            *h.at_mut(i, i) += mu;
        }
    }
    for i in 1..n {
        for j in 0..i {
            *h.at_mut(i, j) = Cx::default();
        }
    }
    Ok(Some((h, q)))
}

/// The rotation `[c s; -conj(s) c]` (c real) mapping `(a, b)` to `(r, 0)`.
fn givens(a: Cx, b: Cx) -> (f64, Cx) {
    let (aa, ab) = (a.abs(), b.abs());
    if ab == 0.0 {
        return (1.0, Cx::default());
    }
    if aa == 0.0 {
        return (0.0, b.conj().scale(1.0 / ab));
    }
    let r = aa.hypot(ab);
    (aa / r, phase(a) * b.conj().scale(1.0 / r))
}

/// The eigenvalue of `[a b; c d]` nearer to `d`.
fn wilkinson(a: Cx, b: Cx, c: Cx, d: Cx) -> Cx {
    let half = (a - d).scale(0.5);
    let disc = csqrt(half * half + b * c);
    let mid = (a + d).scale(0.5);
    let (m1, m2) = (mid + disc, mid - disc);
    if (m1 - d).abs() <= (m2 - d).abs() {
        m1
    } else {
        m2
    }
}

/// Eigenvalues and unit eigenvectors (columns) of a general complex matrix from its Schur form: the diagonal of
/// `t`, and for each eigenvalue the back substitution in the leading triangle of `t - lambda I` (tiny divisors
/// replaced by `EPS` times the norm, as perturbation theory allows), mapped back by `q`. Each vector is scaled to
/// unit length with its largest component real and positive.
pub fn schur_eigenvectors(t: &Mat<Cx>, q: &Mat<Cx>, m: &mut Meter) -> WorkResult<(Vec<Cx>, Mat<Cx>)> {
    let n = t.rows;
    let small = (EPS * t.frobenius()).max(f64::MIN_POSITIVE);
    let mut vecs = Mat::zeros(n, n);
    for k in 0..n {
        m.step((n * n) as u64 + 1)?;
        let lambda = t.at(k, k);
        let mut x = vec![Cx::default(); n];
        x[k] = Cx::new(1.0, 0.0);
        for i in (0..k).rev() {
            let mut s = t.at(i, k);
            for (j, &xj) in x.iter().enumerate().take(k).skip(i + 1) {
                s += t.at(i, j) * xj;
            }
            let mut d = t.at(i, i) - lambda;
            if d.abs() < small {
                d = Cx::new(small, 0.0);
            }
            x[i] = -(s / d);
        }
        let mut v: Vec<Cx> = (0..n).map(|i| (0..=k).fold(Cx::default(), |acc, j| acc + q.at(i, j) * x[j])).collect();
        normalize(&mut v);
        for (i, &e) in v.iter().enumerate() {
            *vecs.at_mut(i, k) = e;
        }
    }
    Ok(((0..n).map(|k| t.at(k, k)).collect(), vecs))
}

/// Scale to unit 2-norm with the component of largest modulus real and positive.
pub fn normalize<S: Scalar>(v: &mut [S]) {
    let big = v.iter().copied().fold(S::zero(), |b, x| if x.abs() > b.abs() { x } else { b });
    if big.abs() == 0.0 {
        return;
    }
    let p = phase(big).conj();
    for x in v.iter_mut() {
        *x = *x * p;
    }
    let s = v.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let norm = s * v.iter().map(|x| x.scale(1.0 / s).abs2()).sum::<f64>().sqrt();
    for x in v.iter_mut() {
        *x = x.scale(1.0 / norm);
    }
}

/// A thin singular value decomposition `a = u diag(s) v^H` (`u`: `r x k`, `v`: `c x k`, `k = min(r, c)`), singular
/// values in decreasing order, by one-sided Jacobi (Hestenes) on the columns (on `a^H` when `r < c`). Columns of
/// `u` for zero singular values are completed to an orthonormal set.
pub fn svd<S: Scalar>(a: &Mat<S>, m: &mut Meter) -> WorkResult<(Mat<S>, Vec<f64>, Mat<S>)> {
    if a.rows < a.cols {
        let (u, s, v) = svd(&a.adjoint(), m)?;
        return Ok((v, s, u));
    }
    let (r, c) = (a.rows, a.cols);
    // work on columns: store the transpose so that each column is a contiguous row
    let mut u = a.adjoint();
    for x in &mut u.a {
        *x = x.conj();
    }
    let mut v = Mat::<S>::identity(c);
    let scale = a.max_abs();
    if scale > 0.0 && scale.is_finite() {
        for x in &mut u.a {
            *x = x.scale(1.0 / scale);
        }
    }
    for _sweep in 0..80 {
        m.step((c * c * r) as u64 + 1)?;
        let mut rotated = false;
        for p in 0..c {
            for q in p + 1..c {
                let (up, uq) = (u.row(p), u.row(q));
                let alpha: f64 = up.iter().map(|x| x.abs2()).sum();
                let beta: f64 = uq.iter().map(|x| x.abs2()).sum();
                let gamma = up.iter().zip(uq).fold(S::zero(), |s, (&x, &y)| s + x.conj() * y);
                let g = gamma.abs();
                if g == 0.0 || g <= EPS * (alpha * beta).sqrt() {
                    continue;
                }
                rotated = true;
                // the phase of gamma moves into column q, leaving a real rotation
                let e = phase(gamma).conj();
                let zeta = (beta - alpha) / (2.0 * g);
                let t = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let t = if zeta == 0.0 { 1.0 } else { t };
                let cs = 1.0 / (1.0 + t * t).sqrt();
                let sn = cs * t;
                rotate_rows(&mut u, p, q, e, cs, sn);
                rotate_rows_cols(&mut v, p, q, e, cs, sn);
            }
        }
        if !rotated {
            break;
        }
    }
    let s: Vec<f64> = (0..c).map(|j| u.row(j).iter().map(|x| x.abs2()).sum::<f64>().sqrt()).collect();
    let mut order: Vec<usize> = (0..c).collect();
    order.sort_by(|&i, &j| s[j].total_cmp(&s[i]));
    let mut uu = Mat::zeros(r, c);
    let mut vv = Mat::zeros(c, c);
    let mut sorted = Vec::with_capacity(c);
    let top = order.first().map_or(0.0, |&j| s[j]);
    for (k, &j) in order.iter().enumerate() {
        let sj = s[j];
        sorted.push(sj * if scale > 0.0 && scale.is_finite() { scale } else { 1.0 });
        if sj > top * EPS * (r as f64) && sj > 0.0 {
            for i in 0..r {
                *uu.at_mut(i, k) = u.at(j, i).scale(1.0 / sj);
            }
        }
        for i in 0..c {
            *vv.at_mut(i, k) = v.at(i, j);
        }
    }
    complete_columns(&mut uu, &sorted, top * scale.max(0.0) * EPS * (r as f64));
    Ok((uu, sorted, vv))
}

/// Rotate rows p and q of `u` (columns of the working matrix): `u_q *= e`, then the real rotation.
fn rotate_rows<S: Scalar>(u: &mut Mat<S>, p: usize, q: usize, e: S, c: f64, s: f64) {
    let n = u.cols;
    for k in 0..n {
        let x = u.a[p * n + k];
        let y = u.a[q * n + k] * e;
        u.a[p * n + k] = x.scale(c) - y.scale(s);
        u.a[q * n + k] = x.scale(s) + y.scale(c);
    }
}

/// The same transformation on columns p and q of `v`.
fn rotate_rows_cols<S: Scalar>(v: &mut Mat<S>, p: usize, q: usize, e: S, c: f64, s: f64) {
    let n = v.cols;
    for k in 0..v.rows {
        let x = v.a[k * n + p];
        let y = v.a[k * n + q] * e;
        v.a[k * n + p] = x.scale(c) - y.scale(s);
        v.a[k * n + q] = x.scale(s) + y.scale(c);
    }
}

/// Replace the columns of `u` whose singular value is at most `tol` by unit vectors orthogonal to the others
/// (Gram-Schmidt against the standard basis, twice for stability).
fn complete_columns<S: Scalar>(u: &mut Mat<S>, s: &[f64], tol: f64) {
    let (r, k) = (u.rows, u.cols);
    let mut basis = 0usize;
    for (j, &sj) in s.iter().enumerate().take(k) {
        if sj > tol && sj > 0.0 {
            continue;
        }
        loop {
            if basis >= r {
                return;
            }
            let mut w = vec![S::zero(); r];
            w[basis] = S::one();
            basis += 1;
            for _ in 0..2 {
                for c in 0..k {
                    if c == j || (u.at(0, c) == S::zero() && (0..r).all(|i| u.at(i, c) == S::zero())) {
                        continue;
                    }
                    let d = (0..r).fold(S::zero(), |acc, i| acc + u.at(i, c).conj() * w[i]);
                    for (i, x) in w.iter_mut().enumerate() {
                        *x -= d * u.at(i, c);
                    }
                }
            }
            let norm: f64 = w.iter().map(|x| x.abs2()).sum::<f64>().sqrt();
            if norm > 0.5 {
                for (i, x) in w.iter().enumerate() {
                    *u.at_mut(i, j) = x.scale(1.0 / norm);
                }
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_poly::work::Unlimited;

    fn mat(r: usize, c: usize, a: &[f64]) -> Mat<f64> {
        Mat { rows: r, cols: c, a: a.to_vec() }
    }

    fn close<S: Scalar>(a: &Mat<S>, b: &Mat<S>, tol: f64) -> bool {
        a.rows == b.rows && a.cols == b.cols && a.a.iter().zip(&b.a).all(|(&x, &y)| (x - y).abs() <= tol)
    }

    #[test]
    fn lu_solves_and_inverts() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let a = mat(3, 3, &[2.0, 1.0, 1.0, 4.0, -6.0, 0.0, -2.0, 7.0, 2.0]);
        let f = lu(a.clone(), &mut m).unwrap();
        assert!((f.det() - (-16.0)).abs() < 1e-12);
        assert!(!f.singular());
        let inv = f.solve(&Mat::identity(3), &mut m).unwrap();
        assert!(close(&mul(&a, &inv, &mut m).unwrap(), &Mat::identity(3), 1e-14));
        let s = lu(mat(2, 2, &[1.0, 2.0, 2.0, 4.0]), &mut m).unwrap();
        assert!(s.singular());
        assert_eq!(s.det(), 0.0);
    }

    #[test]
    fn qr_and_cholesky_reconstruct() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let a = mat(3, 2, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let (q, r) = qr(a.clone(), &mut m).unwrap();
        assert!(close(&mul(&q, &r, &mut m).unwrap(), &a, 1e-14));
        assert!(close(&mul(&q.adjoint(), &q, &mut m).unwrap(), &Mat::identity(2), 1e-14));
        assert!(r.at(0, 0) > 0.0 && r.at(1, 1) > 0.0 && r.at(1, 0) == 0.0);
        let s = mat(2, 2, &[4.0, 2.0, 2.0, 3.0]);
        let u = cholesky(&s, &mut m).unwrap().unwrap();
        assert!(close(&mul(&u.adjoint(), &u, &mut m).unwrap(), &s, 1e-14));
        assert_eq!(cholesky(&mat(2, 2, &[1.0, 2.0, 2.0, 1.0]), &mut m).unwrap(), Err(CholeskyError::NotPositiveDefinite));
    }

    #[test]
    fn tridiagonal_ql_agrees_with_jacobi() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let n = 40;
        let mut a = Mat::<f64>::zeros(n, n);
        let mut x = 0.5f64;
        for i in 0..n {
            for j in 0..=i {
                x = (x * 3.7 + 0.13).fract();
                *a.at_mut(i, j) = x - 0.5;
                *a.at_mut(j, i) = x - 0.5;
            }
        }
        let (mut d1, v) = symmetric_eigen(&a, &mut m).unwrap();
        let (mut d2, _) = jacobi_eigen(&a, &mut m).unwrap();
        // a v = v diag(d)
        let av = mul(&a, &v, &mut m).unwrap();
        for (k, &dk) in d1.iter().enumerate() {
            for i in 0..n {
                assert!((av.at(i, k) - v.at(i, k) * dk).abs() < 1e-13);
            }
        }
        d1.sort_by(f64::total_cmp);
        d2.sort_by(f64::total_cmp);
        for (p, q) in d1.iter().zip(&d2) {
            assert!((p - q).abs() < 1e-13, "{p} {q}");
        }
    }

    #[test]
    fn eigen_and_svd() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let a = mat(2, 2, &[2.0, 1.0, 1.0, 2.0]);
        let (mut vals, _) = symmetric_eigen(&a, &mut m).unwrap();
        vals.sort_by(f64::total_cmp);
        assert!((vals[0] - 1.0).abs() < 1e-15 && (vals[1] - 3.0).abs() < 1e-15);
        // a rotation: eigenvalues +-i
        let r = mat(2, 2, &[0.0, 1.0, -1.0, 0.0]).to_cx();
        let (t, q) = schur(&r, &mut m).unwrap().unwrap();
        let (ev, vecs) = schur_eigenvectors(&t, &q, &mut m).unwrap();
        let mut ims: Vec<f64> = ev.iter().map(|z| z.im).collect();
        ims.sort_by(f64::total_cmp);
        assert!((ims[0] + 1.0).abs() < 1e-14 && (ims[1] - 1.0).abs() < 1e-14);
        // r v = lambda v
        let rv = mul(&r, &vecs, &mut m).unwrap();
        for (k, &e) in ev.iter().enumerate() {
            for i in 0..2 {
                assert!((rv.at(i, k) - e * vecs.at(i, k)).abs() < 1e-14);
            }
        }
        let b = mat(2, 3, &[3.0, 2.0, 2.0, 2.0, 3.0, -2.0]);
        let (u, s, v) = svd(&b, &mut m).unwrap();
        assert!((s[0] - 5.0).abs() < 1e-14 && (s[1] - 3.0).abs() < 1e-14);
        let mut us = u.clone();
        for i in 0..2 {
            for (k, &sk) in s.iter().enumerate().take(2) {
                *us.at_mut(i, k) = us.at(i, k) * sk;
            }
        }
        assert!(close(&mul(&us, &v.adjoint(), &mut m).unwrap(), &b, 1e-14));
    }
}
