//! Small dense linear algebra for the Newton steps: Gaussian elimination with partial pivoting on machine scalars
//! and on numbers (balls), and the regularised least-squares step used where a Jacobian is singular.
//!
//! The systems are as large as the user's list of equations, so the cubic cost is charged as fuel once per solve.
use crate::scalar::Scalar;
use mc_arith::Num;

/// Solve `a x = b` (`a` row-major, n by n) by Gaussian elimination with partial pivoting. `None` when a pivot is
/// zero or negligible (below n·2^-52 times the largest entry of its column), or the result is not finite.
pub(crate) fn solve<S: Scalar>(n: usize, a: &[S], b: &[S]) -> Option<Vec<S>> {
    let mut m = a.to_vec();
    let mut x = b.to_vec();
    let scale = a.iter().fold(0.0f64, |s, v| s.max(v.abs()));
    if scale == 0.0 || !scale.is_finite() {
        return None;
    }
    let tiny = scale * f64::EPSILON * n as f64;
    for k in 0..n {
        let p = (k..n).max_by(|&i, &j| m[i * n + k].abs().total_cmp(&m[j * n + k].abs()))?;
        if m[p * n + k].abs() <= tiny {
            return None;
        }
        if p != k {
            for j in 0..n {
                m.swap(k * n + j, p * n + j);
            }
            x.swap(k, p);
        }
        let piv = m[k * n + k];
        for i in k + 1..n {
            let f = m[i * n + k] / piv;
            if f.abs() == 0.0 {
                continue;
            }
            for j in k..n {
                m[i * n + j] = m[i * n + j] - f * m[k * n + j];
            }
            x[i] = x[i] - f * x[k];
        }
    }
    for k in (0..n).rev() {
        let mut s = x[k];
        for j in k + 1..n {
            s = s - m[k * n + j] * x[j];
        }
        x[k] = s / m[k * n + k];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// The Levenberg–Marquardt step: the solution of `(J^H J + mu I) d = -J^H r` with `mu` a small multiple of the
/// largest diagonal entry, a descent direction for |r|^2 even where `J` is singular. `None` if `J` is zero.
pub(crate) fn damped_least_squares<S: Scalar>(n: usize, j: &[S], r: &[S]) -> Option<Vec<S>> {
    let mut a = vec![S::real(0.0); n * n];
    let mut g = vec![S::real(0.0); n];
    for p in 0..n {
        for q in 0..n {
            let mut s = S::real(0.0);
            for k in 0..n {
                s = s + j[k * n + p].conj() * j[k * n + q];
            }
            a[p * n + q] = s;
        }
        let mut s = S::real(0.0);
        for k in 0..n {
            s = s + j[k * n + p].conj() * r[k];
        }
        g[p] = S::real(0.0) - s;
    }
    let dmax = (0..n).fold(0.0f64, |m, i| m.max(a[i * n + i].abs()));
    if dmax == 0.0 || !dmax.is_finite() {
        return None;
    }
    let mu = dmax * 1e-6;
    for i in 0..n {
        a[i * n + i] = a[i * n + i] + S::real(mu);
    }
    solve(n, &a, &g)
}

/// Solve `a x = b` on numbers (balls at the working precision, or exact numbers): Gaussian elimination pivoting on
/// the largest midpoint magnitude. `None` when a pivot has no reciprocal (a ball containing zero).
pub(crate) fn solve_num(n: usize, a: &[Num], b: &[Num]) -> Option<Vec<Num>> {
    let mut m = a.to_vec();
    let mut x = b.to_vec();
    let mag = |v: &Num| match v {
        Num::Real(r) => r.to_f64().abs(),
        Num::Complex(re, im) => re.to_f64().hypot(im.to_f64()),
    };
    for k in 0..n {
        let p = (k..n).max_by(|&i, &j| mag(&m[i * n + k]).total_cmp(&mag(&m[j * n + k])))?;
        if p != k {
            for j in 0..n {
                m.swap(k * n + j, p * n + j);
            }
            x.swap(k, p);
        }
        let inv = m[k * n + k].recip().ok()?;
        for i in k + 1..n {
            let f = m[i * n + k].mul(&inv);
            if f.is_exact_zero() {
                continue;
            }
            for j in k..n {
                m[i * n + j] = m[i * n + j].add(&f.mul(&m[k * n + j]).neg());
            }
            x[i] = x[i].add(&f.mul(&x[k]).neg());
        }
    }
    for k in (0..n).rev() {
        let mut s = x[k].clone();
        for j in k + 1..n {
            s = s.add(&m[k * n + j].mul(&x[j]).neg());
        }
        x[k] = s.mul(&m[k * n + k].recip().ok()?);
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_numeric::machine::C64;

    #[test]
    fn real_and_complex_systems() {
        let a = [2.0, 1.0, 1.0, 3.0];
        let x = solve(2, &a, &[3.0, 5.0]).unwrap();
        assert!((x[0] - 0.8).abs() < 1e-15 && (x[1] - 1.4).abs() < 1e-15);
        // a zero leading entry needs the pivot swap
        let x = solve(2, &[0.0, 1.0, 1.0, 0.0], &[2.0, 3.0]).unwrap();
        assert_eq!(x, vec![3.0, 2.0]);
        assert!(solve(2, &[1.0, 2.0, 2.0, 4.0], &[1.0, 1.0]).is_none());
        let i = C64::new(0.0, 1.0);
        let x = solve(1, &[i], &[C64::real(1.0)]).unwrap();
        assert_eq!(x[0], C64::new(0.0, -1.0));
        // singular: the damped step still descends
        let d = damped_least_squares(2, &[1.0, 2.0, 2.0, 4.0], &[1.0, 1.0]).unwrap();
        assert!(d[0] * 3.0 + d[1] * 6.0 < 0.0);
    }

    #[test]
    fn number_systems() {
        let a: Vec<Num> = [2, 1, 1, 3].iter().map(|&v| Num::int(v)).collect();
        let b = [Num::int(3), Num::int(5)];
        let x = solve_num(2, &a, &b).unwrap();
        let q = |n: &Num| n.as_real().unwrap().to_f64();
        assert_eq!((q(&x[0]), q(&x[1])), (0.8, 1.4));
        assert!(solve_num(1, &[Num::int(0)], &[Num::int(1)]).is_none());
    }
}
