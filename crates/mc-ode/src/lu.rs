//! Dense LU factorisation with partial pivoting: the Rosenbrock method factors its iteration matrix once per step
//! and solves six systems with it (decision N3.6).

/// The LU factors of an n by n matrix (row-major, unit lower triangle below the diagonal) and the row exchanges.
pub(crate) struct Lu {
    n: usize,
    a: Vec<f64>,
    piv: Vec<usize>,
}

impl Lu {
    /// An empty factorisation for systems of size `n`.
    pub fn new(n: usize) -> Lu {
        Lu { n, a: vec![0.0; n * n], piv: vec![0; n] }
    }

    /// Factor `m` (row-major, n by n) in place of the previous factors; `false` when a pivot is zero or not finite
    /// (the matrix is singular to working precision).
    pub fn factor(&mut self, m: &[f64]) -> bool {
        let n = self.n;
        self.a.copy_from_slice(m);
        let a = &mut self.a;
        for k in 0..n {
            let mut p = k;
            for i in k + 1..n {
                if a[i * n + k].abs() > a[p * n + k].abs() {
                    p = i;
                }
            }
            self.piv[k] = p;
            let pivot = a[p * n + k];
            if pivot == 0.0 || !pivot.is_finite() {
                return false;
            }
            if p != k {
                for j in 0..n {
                    a.swap(k * n + j, p * n + j);
                }
            }
            for i in k + 1..n {
                let f = a[i * n + k] / pivot;
                a[i * n + k] = f;
                if f != 0.0 {
                    for j in k + 1..n {
                        a[i * n + j] -= f * a[k * n + j];
                    }
                }
            }
        }
        true
    }

    /// Solve `m x = b` in place with the factors of `m`.
    pub fn solve(&self, b: &mut [f64]) {
        let (n, a) = (self.n, &self.a);
        // the row exchanges first: the factors were exchanged as whole rows
        for k in 0..n {
            b.swap(k, self.piv[k]);
        }
        for k in 0..n {
            let bk = b[k];
            if bk != 0.0 {
                for i in k + 1..n {
                    b[i] -= a[i * n + k] * bk;
                }
            }
        }
        for k in (0..n).rev() {
            let mut s = b[k];
            for j in k + 1..n {
                s -= a[k * n + j] * b[j];
            }
            b[k] = s / a[k * n + k];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Lu;

    #[test]
    fn solves_with_pivoting() {
        let mut lu = Lu::new(3);
        // a zero leading entry needs the row exchange
        let m = [0.0, 2.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 3.0];
        assert!(lu.factor(&m));
        // the same factors serve several right-hand sides
        for rhs in [[1.0, -2.0, 3.0], [2.0, 1.0, 1.0]] {
            let mut x = rhs.to_vec();
            lu.solve(&mut x);
            for i in 0..3 {
                let r: f64 = (0..3).map(|j| m[i * 3 + j] * x[j]).sum::<f64>() - rhs[i];
                assert!(r.abs() < 1e-14, "{x:?}");
            }
        }
        assert!(!lu.factor(&[1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 0.0, 0.0, 0.0]));
    }
}
