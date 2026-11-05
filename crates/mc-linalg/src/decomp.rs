//! Decompositions: `lu`, `qr`, `cholesky`, `svd`, `singular_values`.
//!
//! Machine matrices use the dense kernels. Exact numeric matrices: `lu` by elimination over the rationals (or
//! Gaussian rationals), `qr` by Gram-Schmidt without normalisation then one square root per column, `cholesky` by
//! the square-root-free `L D L^H` then `U = sqrt(D) L^H`, singular values as square roots of the nonzero
//! eigenvalues of `m^H m` (decision LA.13). Exact `svd` and symbolic decompositions are gaps (the call stays).
use crate::dense::{self, CholeskyError, Mat, EPS};
use crate::eigen;
use crate::elim::{self, Field, Undecided};
use crate::natives::Out;
use crate::scalar::Scalar;
use crate::systems::{conj, matrix_arg, num_adjoint, num_mul, num_terms, take, undecided};
use crate::tensor::{Arr, Domain, Env};
use mc_api::Store;
use mc_arith::{Num, Real};
use mc_core::T;
use mc_poly::work::{Meter, WorkResult};

/// The only argument as a matrix.
fn one_matrix<'h>(env: &Env<'h>, args: &[T<'h>]) -> Result<Arr<'h>, Out<'h>> {
    if args.len() != 1 {
        return Err(Out::Decline);
    }
    matrix_arg(env, args, 0)
}

/// The 1-based permutation list.
fn perm_term<'h>(env: &Env<'h>, perm: &[usize]) -> T<'h> {
    env.list(&perm.iter().map(|&p| env.h().int(p as i64 + 1)).collect::<Vec<_>>())
}

/// L (unit lower, `r x r`) and U (`r x c`) from a packed machine LU.
fn split_lu<S: Scalar>(f: &dense::Lu<S>) -> (Mat<S>, Mat<S>) {
    let (r, c) = (f.lu.rows, f.lu.cols);
    let mut l = Mat::<S>::identity(r);
    let mut u = Mat::<S>::zeros(r, c);
    for i in 0..r {
        for j in 0..c {
            if j < i {
                if j < r {
                    *l.at_mut(i, j) = f.lu.at(i, j);
                }
            } else {
                *u.at_mut(i, j) = f.lu.at(i, j);
            }
        }
    }
    (l, u)
}

/// `lu(m)`: `[l, u, p]` with `l` unit lower triangular, `u` upper triangular (row echelon) and `p` the row
/// permutation, so that `m[p] == dot(l, u)` (partial pivoting).
pub fn lu<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let a = take!(one_matrix(env, args));
    let (r, c) = (a.rows(), a.cols());
    let (l, u, p) = match a.domain {
        Domain::Real => {
            let f = dense::lu(a.reals(env), m)?;
            let (l, u) = split_lu(&f);
            (env.real_matrix(&l), env.real_matrix(&u), perm_term(env, &f.perm))
        }
        Domain::Complex => {
            let f = dense::lu(a.complex(env), m)?;
            let (l, u) = split_lu(&f);
            (env.cx_matrix(&l), env.cx_matrix(&u), perm_term(env, &f.perm))
        }
        Domain::Rational | Domain::ExactComplex | Domain::Ball => match elim::lu(&a.nums(env), r, c, m)? {
            Ok(f) => (env.matrix(r, r, &num_terms(env, &f.l)), env.matrix(r, c, &num_terms(env, &f.u)), perm_term(env, &f.perm)),
            Err(Undecided) => return Ok(undecided(env)),
        },
        Domain::Symbolic => return Ok(Out::Decline),
    };
    Ok(Out::Done(env.list(&[l, u, p])))
}

/// The inner product `<u, v> = sum conj(u_k) v_k` of number vectors.
fn inner(u: &[Num], v: &[Num]) -> Num {
    u.iter().zip(v).fold(Num::int(0), |s, (x, y)| s.add(&conj(x).mul(y)))
}

/// `x^(e)` for a positive exact number `x` and `e = +-1/2` as a canonical radical term.
fn sqrt_pow<'h>(env: &Env<'h>, x: &Num, neg: bool) -> T<'h> {
    let h = env.h();
    env.ar.pow(env.num(x), env.ar.rational(h.int(if neg { -1 } else { 1 }), h.int(2)))
}

/// `qr(m)`: `[q, r]` with `m == dot(q, r)`, `q` with orthonormal columns and `r` upper triangular with a real
/// non-negative diagonal (`k = min(rows, columns)` columns of q for machine matrices; the rank for exact ones).
pub fn qr<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let a = take!(one_matrix(env, args));
    let (r, c) = (a.rows(), a.cols());
    let (q, rr) = match a.domain {
        Domain::Real => {
            let (q, rr) = dense::qr(a.reals(env), m)?;
            (env.real_matrix(&q), env.real_matrix(&rr))
        }
        Domain::Complex => {
            let (q, rr) = dense::qr(a.complex(env), m)?;
            (env.cx_matrix(&q), env.cx_matrix(&rr))
        }
        Domain::Rational | Domain::ExactComplex => {
            let x = a.nums(env);
            let cols: Vec<Vec<Num>> = (0..c).map(|j| (0..r).map(|i| x[i * c + j].clone()).collect()).collect();
            // Gram-Schmidt without normalisation: w_j = a_j - sum <w_i, a_j>/<w_i, w_i> w_i
            let mut ws: Vec<(Vec<Num>, Num)> = vec![];
            for (j, aj) in cols.iter().enumerate() {
                m.step((r * (ws.len() + 1)) as u64 + 1)?;
                let mut w = aj.clone();
                for (wi, ni) in &ws {
                    let f = inner(wi, aj).div(ni).expect("a nonzero norm");
                    for (x, y) in w.iter_mut().zip(wi) {
                        *x = x.sub(&f.mul(y));
                    }
                }
                let nw = inner(&w, &w);
                if !nw.is_exact_zero() {
                    ws.push((w, nw));
                }
                let _ = j;
            }
            let k = ws.len();
            let mut qt = vec![env.h().int(0); r * k];
            let mut rt = vec![env.h().int(0); k * c];
            for (i, (w, nw)) in ws.iter().enumerate() {
                let s = sqrt_pow(env, nw, true);
                for row in 0..r {
                    qt[row * k + i] = env.ar.mul(&[env.num(&w[row]), s]);
                }
                for (j, aj) in cols.iter().enumerate() {
                    let d = inner(w, aj);
                    if !d.is_exact_zero() {
                        rt[i * c + j] = env.ar.mul(&[env.num(&d), s]);
                    }
                }
            }
            (env.matrix(r, k, &qt), env.matrix(k, c, &rt))
        }
        Domain::Ball | Domain::Symbolic => return Ok(Out::Decline),
    };
    Ok(Out::Done(env.list(&[q, rr])))
}

/// `cholesky(m)`: the upper triangular `u` with `m == dot(conjugate_transpose(u), u)` for a Hermitian (symmetric)
/// positive definite `m`.
pub fn cholesky<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let a = take!(one_matrix(env, args));
    if !a.is_square() {
        return Ok(Out::Done(env.failure("not_square", &[])));
    }
    let n = a.rows();
    let fail = |e: CholeskyError| {
        Out::Done(env.failure(if e == CholeskyError::NotHermitian { "not_hermitian" } else { "not_positive_definite" }, &[]))
    };
    Ok(match a.domain {
        Domain::Real => match dense::cholesky(&a.reals(env), m)? {
            Ok(u) => Out::Done(env.real_matrix(&u)),
            Err(e) => fail(e),
        },
        Domain::Complex => match dense::cholesky(&a.complex(env), m)? {
            Ok(u) => Out::Done(env.cx_matrix(&u)),
            Err(e) => fail(e),
        },
        Domain::Rational | Domain::ExactComplex => {
            let x = a.nums(env);
            let at = |i: usize, j: usize| &x[i * n + j];
            for i in 0..n {
                for j in 0..=i {
                    if at(i, j).add(&conj(at(j, i)).neg()).weight(0.0) != elim::Weight::Zero {
                        return Ok(fail(CholeskyError::NotHermitian));
                    }
                }
            }
            // L D L^H: l (unit lower) and d (positive rationals)
            let mut l = vec![Num::int(0); n * n];
            let mut d: Vec<Num> = Vec::with_capacity(n);
            for j in 0..n {
                m.step((n * n) as u64 + 1)?;
                let mut dj = at(j, j).clone();
                for k in 0..j {
                    let ljk = &l[j * n + k];
                    dj = dj.sub(&ljk.mul(&conj(ljk)).mul(&d[k]));
                }
                let positive = matches!(&dj, Num::Real(Real::Exact(q)) if !q.num.neg && !q.is_zero());
                if !positive {
                    return Ok(fail(CholeskyError::NotPositiveDefinite));
                }
                l[j * n + j] = Num::int(1);
                for i in j + 1..n {
                    let mut s = at(i, j).clone();
                    for k in 0..j {
                        s = s.sub(&l[i * n + k].mul(&conj(&l[j * n + k])).mul(&d[k]));
                    }
                    l[i * n + j] = s.div(&dj).expect("a positive pivot");
                }
                d.push(dj);
            }
            let mut u = vec![env.h().int(0); n * n];
            for k in 0..n {
                let s = sqrt_pow(env, &d[k], false);
                for j in k..n {
                    let lkj = conj(&l[j * n + k]);
                    if !lkj.is_exact_zero() {
                        u[k * n + j] = env.ar.mul(&[env.num(&lkj), s]);
                    }
                }
            }
            Out::Done(env.matrix(n, n, &u))
        }
        Domain::Ball | Domain::Symbolic => Out::Decline,
    })
}

/// `svd(m)`: `[u, s, v]` with `m == dot(u, s, conjugate_transpose(v))`, `s` the diagonal matrix of the
/// `k = min(rows, columns)` singular values in decreasing order (a thin decomposition). Machine matrices only.
pub fn svd<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let a = take!(one_matrix(env, args));
    let diag = |s: &[f64]| {
        let k = s.len();
        let mut d = Mat::<f64>::zeros(k, k);
        for (i, &x) in s.iter().enumerate() {
            *d.at_mut(i, i) = x;
        }
        env.real_matrix(&d)
    };
    Ok(match a.domain {
        Domain::Real => {
            let (u, s, v) = dense::svd(&a.reals(env), m)?;
            Out::Done(env.list(&[env.real_matrix(&u), diag(&s), env.real_matrix(&v)]))
        }
        Domain::Complex => {
            let (u, s, v) = dense::svd(&a.complex(env), m)?;
            Out::Done(env.list(&[env.cx_matrix(&u), diag(&s), env.cx_matrix(&v)]))
        }
        _ => Out::Decline,
    })
}

/// The machine singular values above the rank tolerance.
fn machine_singular_values<S: Scalar>(a: &Mat<S>, m: &mut Meter) -> WorkResult<Vec<f64>> {
    let (_, s, _) = dense::svd(a, m)?;
    let tol = a.rows.max(a.cols) as f64 * EPS * s.first().copied().unwrap_or(0.0);
    Ok(s.into_iter().filter(|&x| x > tol).collect())
}

/// The exact nonzero singular values of an exact numeric matrix: square roots of the nonzero eigenvalues of
/// `a^H a`, decreasing; `None` when the eigenvalues cannot be computed.
fn exact_singular_values<'h>(env: &Env<'h>, a: &Arr<'h>, m: &mut Meter) -> WorkResult<Option<Vec<T<'h>>>> {
    let (r, c) = (a.rows(), a.cols());
    let x = a.nums(env);
    // the smaller Gram matrix has the same nonzero eigenvalues
    let g = if r < c { num_mul(&x, &num_adjoint(&x, r, c), r, c, r, m)? } else { num_mul(&num_adjoint(&x, r, c), &x, c, r, c, m)? };
    let k = r.min(c);
    let gt = num_terms(env, &g);
    let Some(vals) = eigen::exact_eigenvalues(env, &gt, k, m)? else { return Ok(None) };
    let half = env.ar.rational(env.h().int(1), env.h().int(2));
    Ok(Some(vals.into_iter().filter(|&v| !eigen::is_zero_term(env, v)).map(|v| env.ar.pow(v, half)).collect()))
}

/// `singular_values(m)`: the nonzero singular values in decreasing order.
pub fn singular_values<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let a = take!(one_matrix(env, args));
    Ok(match a.domain {
        Domain::Real => Out::Done(env.real_vector(&machine_singular_values(&a.reals(env), m)?)),
        Domain::Complex => Out::Done(env.real_vector(&machine_singular_values(&a.complex(env), m)?)),
        Domain::Rational | Domain::ExactComplex => match exact_singular_values(env, &a, m)? {
            Some(v) => Out::Again(env.list(&v)),
            None => Out::Decline,
        },
        Domain::Ball | Domain::Symbolic => Out::Decline,
    })
}

/// The largest singular value (the matrix 2-norm) of a non-machine matrix: exact numeric matrices only.
pub fn largest_singular_value<'h>(env: &Env<'h>, a: &Arr<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    if !a.domain.exact_numeric() {
        return Ok(Out::Decline);
    }
    Ok(match exact_singular_values(env, a, m)? {
        Some(v) => Out::Again(v.first().copied().unwrap_or_else(|| env.h().int(0))),
        None => Out::Decline,
    })
}
