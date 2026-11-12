//! Determinants, inverses, ranks, null spaces, reduced row echelon forms, linear systems and pseudo-inverses in
//! every domain: machine (LU, SVD, elimination with a tolerance), numbers (exact Gaussian rationals and balls, by
//! elimination with pivoting) and exact or symbolic (fraction-free elimination, [`crate::exact`]).
use crate::dense::{self, Mat, EPS};
use crate::elim::{self, Field, Rref, Undecided};
use crate::exact::{self, Exact};
use crate::natives::Out;
use crate::scalar::Scalar;
use crate::tensor::{Arr, Domain, Env};
use mc_api::{Store, View};
use mc_arith::Num;
use mc_core::T;
use mc_poly::work::{Meter, WorkResult};

/// Return the reply when an argument check fails.
macro_rules! take {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(out) => return Ok(out),
        }
    };
}
pub(crate) use take;

/// Argument `k` (0-based) as a vector or matrix: a call with a non-list argument stays unevaluated; a list that is
/// not a vector or matrix of non-list entries is an argument failure.
pub fn tensor_arg<'h>(env: &Env<'h>, args: &[T<'h>], k: usize) -> Result<Arr<'h>, Out<'h>> {
    let Some(&t) = args.get(k) else { return Err(Out::Decline) };
    if env.h().packed(t).is_none() && env.as_list(t).is_none() {
        return Err(Out::Decline);
    }
    env.read(t).ok_or_else(|| Out::Done(env.bad_argument(k + 1)))
}

/// Argument `k` as a matrix.
pub fn matrix_arg<'h>(env: &Env<'h>, args: &[T<'h>], k: usize) -> Result<Arr<'h>, Out<'h>> {
    let a = tensor_arg(env, args, k)?;
    if !a.is_matrix() || a.cols() == 0 {
        return Err(Out::Done(env.bad_argument(k + 1)));
    }
    Ok(a)
}

/// Argument 0 as a square matrix, the only argument.
pub fn square_arg<'h>(env: &Env<'h>, args: &[T<'h>]) -> Result<Arr<'h>, Out<'h>> {
    if args.len() != 1 {
        return Err(Out::Decline);
    }
    let a = matrix_arg(env, args, 0)?;
    if !a.is_square() {
        return Err(Out::Done(env.failure("not_square", &[])));
    }
    Ok(a)
}

/// `failure(undecided, f)`: balls containing zero left a pivot undecided (decision LA.6).
pub fn undecided<'h>(env: &Env<'h>) -> Out<'h> {
    Out::Done(env.failure("undecided", &[]))
}

/// `failure(singular, f)`.
fn singular<'h>(env: &Env<'h>) -> Out<'h> {
    Out::Done(env.failure("singular", &[]))
}

/// The tolerance below which a machine entry counts as zero in elimination: `max(r, c) eps max |a|`.
pub fn machine_tol<S: Scalar>(a: &Mat<S>) -> f64 {
    a.rows.max(a.cols) as f64 * EPS * a.max_abs()
}

/// The exact context and scaled matrix of a matrix of terms; `None` when an entry divides by zero (never for
/// evaluated input).
pub fn exact_input<'a, 'h>(
    env: &'a Env<'h>,
    terms: &[T<'h>],
    rows: usize,
    cols: usize,
    extra: &[T<'h>],
    m: &mut Meter,
) -> WorkResult<Option<(Exact<'a, 'h>, exact::ExactMat)>> {
    let ex = Exact::new(&env.ar, terms, extra, m)?;
    Ok(ex.read(terms, rows, cols, m)?.map(|x| (ex, x)))
}

/// The terms of a matrix of numbers.
pub fn num_terms<'h>(env: &Env<'h>, xs: &[Num]) -> Vec<T<'h>> {
    xs.iter().map(|x| env.num(x)).collect()
}

/// `det(m)`.
pub fn det<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let a = take!(square_arg(env, args));
    let n = a.rows();
    Ok(Out::Done(match a.domain {
        Domain::Real => env.h().real(dense::lu(a.reals(env), m)?.det()),
        Domain::Complex => env.cx(dense::lu(a.complex(env), m)?.det()),
        Domain::Ball | Domain::ExactComplex => match elim::det(a.nums(env), n, m)? {
            Ok(d) => env.num(&d),
            Err(Undecided) => return Ok(undecided(env)),
        },
        Domain::Rational | Domain::Symbolic => {
            let Some((ex, x)) = exact_input(env, &a.terms(env), n, n, &[], m)? else { return Ok(Out::Decline) };
            exact::det(&ex, &x, m)?
        }
    }))
}

/// `inverse(m)`.
pub fn inverse<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let a = take!(square_arg(env, args));
    let n = a.rows();
    Ok(match a.domain {
        Domain::Real => {
            let f = dense::lu(a.reals(env), m)?;
            if f.singular() {
                return Ok(singular(env));
            }
            Out::Done(env.real_matrix(&f.solve(&Mat::identity(n), m)?))
        }
        Domain::Complex => {
            let f = dense::lu(a.complex(env), m)?;
            if f.singular() {
                return Ok(singular(env));
            }
            Out::Done(env.cx_matrix(&f.solve(&Mat::identity(n), m)?))
        }
        Domain::Ball | Domain::ExactComplex => match num_inverse(&a.nums(env), n, m)? {
            Ok(Some(inv)) => Out::Done(env.matrix(n, n, &num_terms(env, &inv))),
            Ok(None) => singular(env),
            Err(Undecided) => undecided(env),
        },
        Domain::Rational | Domain::Symbolic => {
            let Some((ex, x)) = exact_input(env, &a.terms(env), n, n, &[], m)? else { return Ok(Out::Decline) };
            let aug = exact::augment_identity(&ex, &x);
            let f = exact::eliminate(&ex, &aug, n, true, m)?;
            if f.pivots.len() < n {
                return Ok(singular(env));
            }
            let mut out = Vec::with_capacity(n * n);
            for i in 0..n {
                for j in 0..n {
                    out.push(ex.quotient(f.at(i, n + j), &f.pivot, m)?);
                }
            }
            Out::Done(env.matrix(n, n, &out))
        }
    })
}

/// The inverse of a matrix of numbers by Gauss-Jordan elimination on `[a | I]`; `None` when singular.
pub fn num_inverse(a: &[Num], n: usize, m: &mut Meter) -> WorkResult<Result<Option<Vec<Num>>, Undecided>> {
    let mut aug = Vec::with_capacity(2 * n * n);
    for i in 0..n {
        aug.extend_from_slice(&a[i * n..(i + 1) * n]);
        aug.extend((0..n).map(|j| Num::int((i == j) as i64)));
    }
    let r = match elim::rref(aug, n, 2 * n, n, 0.0, m)? {
        Ok(r) => r,
        Err(u) => return Ok(Err(u)),
    };
    if r.rank() < n {
        return Ok(Ok(None));
    }
    Ok(Ok(Some((0..n).flat_map(|i| (0..n).map(move |j| (i, j))).map(|(i, j)| r.at(i, n + j).clone()).collect())))
}

/// The singular values of a machine matrix above the rank tolerance `max(r, c) eps s_max`, with the full SVD.
fn machine_rank<S: Scalar>(a: &Mat<S>, m: &mut Meter) -> WorkResult<usize> {
    let (_, s, _) = dense::svd(a, m)?;
    let tol = a.rows.max(a.cols) as f64 * EPS * s.first().copied().unwrap_or(0.0);
    Ok(s.iter().filter(|&&x| x > tol).count())
}

/// An integer modulo a prime p (the field GF(p)) for elimination with `modulus: p` (round 37).
#[derive(Clone, Copy, Debug, PartialEq)]
struct ModP {
    v: u64,
    p: u64,
}

impl ModP {
    fn pow(self, mut e: u64) -> ModP {
        let (mut b, mut r) = (self.v as u128, 1u128);
        let p = self.p as u128;
        while e > 0 {
            if e & 1 == 1 {
                r = r * b % p;
            }
            b = b * b % p;
            e >>= 1;
        }
        ModP { v: r as u64, p: self.p }
    }
}

impl Field for ModP {
    fn zero_like(&self) -> ModP {
        ModP { v: 0, p: self.p }
    }
    fn one_like(&self) -> ModP {
        ModP { v: 1 % self.p, p: self.p }
    }
    fn add(&self, o: &ModP) -> ModP {
        ModP { v: ((self.v as u128 + o.v as u128) % self.p as u128) as u64, p: self.p }
    }
    fn sub(&self, o: &ModP) -> ModP {
        ModP { v: ((self.v as u128 + self.p as u128 - o.v as u128) % self.p as u128) as u64, p: self.p }
    }
    fn mul(&self, o: &ModP) -> ModP {
        ModP { v: ((self.v as u128 * o.v as u128) % self.p as u128) as u64, p: self.p }
    }
    // the inverse by Fermat's little theorem (p prime)
    fn div(&self, o: &ModP) -> Option<ModP> {
        (o.v != 0).then(|| self.mul(&o.pow(self.p - 2)))
    }
    fn weight(&self, _tol: f64) -> elim::Weight {
        if self.v == 0 { elim::Weight::Zero } else { elim::Weight::Pivot(1.0) }
    }
}

/// Is `n` prime? Deterministic Miller-Rabin for 64-bit integers (the first twelve primes as bases suffice below
/// 3.3 10^24, Sorenson and Webster 2015).
fn is_prime_u64(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    const BASES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    for &b in &BASES {
        if n.is_multiple_of(b) {
            return n == b;
        }
    }
    let (mut d, mut s) = (n - 1, 0);
    while d.is_multiple_of(2) {
        d /= 2;
        s += 1;
    }
    'bases: for &b in &BASES {
        let mut x = ModP { v: b % n, p: n }.pow(d).v;
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = ((x as u128 * x as u128) % n as u128) as u64;
            if x == n - 1 {
                continue 'bases;
            }
        }
        return false;
    }
    true
}

/// An elimination option of `rank` and `row_reduce`.
#[derive(Clone, Copy)]
enum ElimOption {
    /// none given
    Default,
    /// `modulus: p`, a prime: over GF(p)
    Modulus(u64),
    /// `tolerance: t >= 0`: machine entries (pivots, singular values) at most t times the largest are zero
    Tolerance(f64),
}

/// The matrix argument and the option of `rank(m, opt)` / `row_reduce(m, opt)`; `None` for anything else (the
/// call stays).
fn elim_option<'h>(env: &Env<'h>, args: &[T<'h>]) -> Option<ElimOption> {
    let h = env.h();
    match args {
        [_] => Some(ElimOption::Default),
        [_, o] => {
            let View::App { head, args: &[r] } = h.view(*o) else { return None };
            if head != env.sym("options") {
                return None;
            }
            let View::App { args: &[k, v], .. } = h.view(r) else { return None };
            let View::Sym(name) = h.view(k) else { return None };
            match h.sym_name(name).as_str() {
                "modulus" => match h.view(v) {
                    View::Int(p) if p >= 2 && is_prime_u64(p as u64) => Some(ElimOption::Modulus(p as u64)),
                    View::Int(0) => Some(ElimOption::Default),
                    _ => None,
                },
                "tolerance" => {
                    let t = env.ar.num(v).and_then(|n| n.as_real().map(|r| r.to_f64()))?;
                    (t.is_finite() && t >= 0.0).then_some(ElimOption::Tolerance(t))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// The entries of an exact matrix modulo the prime p (a rational a/b as a b^-1); `None` when an entry is not a
/// rational or a denominator is divisible by p.
fn mod_entries<'h>(env: &Env<'h>, a: &Arr<'h>, p: u64) -> Option<Vec<ModP>> {
    let pn = mc_arith::num::Nat::from_u64(p);
    let modp = |i: &mc_arith::num::Int| -> u64 { i.divrem_floor(&pn).1.to_u128().map_or(0, |v| v as u64) };
    let mut out = Vec::new();
    for t in a.terms(env) {
        let q = env.ar.num(t)?.as_rat()?.clone();
        let den = ModP { v: modp(&mc_arith::num::Int::from_nat(q.den.clone())), p };
        let num = ModP { v: modp(&q.num), p };
        out.push(num.div(&den)?);
    }
    Some(out)
}

/// `rank(m)`; `rank(m, modulus: p)` over the integers modulo a prime p; `rank(m, tolerance: t)`: the singular
/// values of a machine matrix above t times the largest (round 37).
pub fn rank<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let Some(opt) = elim_option(env, args) else { return Ok(Out::Decline) };
    let a = take!(matrix_arg(env, args, 0));
    let (r, c) = (a.rows(), a.cols());
    match opt {
        ElimOption::Modulus(p) => {
            if !a.domain.exact_numeric() || a.domain == Domain::ExactComplex {
                return Ok(Out::Decline);
            }
            let Some(xs) = mod_entries(env, &a, p) else { return Ok(Out::Decline) };
            let Ok(x) = elim::rref(xs, r, c, c, 0.0, m)? else { return Ok(Out::Decline) };
            return Ok(Out::Done(env.h().int(x.rank() as i64)));
        }
        ElimOption::Tolerance(t) if a.domain.machine() => {
            let (_, s, _) = dense::svd(&a.complex(env), m)?;
            let tol = t * s.first().copied().unwrap_or(0.0);
            return Ok(Out::Done(env.h().int(s.iter().filter(|&&x| x > tol).count() as i64)));
        }
        // a tolerance does not change exact elimination
        ElimOption::Tolerance(_) | ElimOption::Default => {}
    }
    let k = match a.domain {
        Domain::Real => machine_rank(&a.reals(env), m)?,
        Domain::Complex => machine_rank(&a.complex(env), m)?,
        Domain::Ball | Domain::ExactComplex => match elim::rref(a.nums(env), r, c, c, 0.0, m)? {
            Ok(x) => x.rank(),
            Err(Undecided) => return Ok(undecided(env)),
        },
        Domain::Rational | Domain::Symbolic => {
            let Some((ex, x)) = exact_input(env, &a.terms(env), r, c, &[], m)? else { return Ok(Out::Decline) };
            exact::eliminate(&ex, &x, c, false, m)?.pivots.len()
        }
    };
    Ok(Out::Done(env.h().int(k as i64)))
}

/// An orthonormal basis of the null space of a machine matrix: the right singular vectors of the singular values
/// at most the rank tolerance (the matrix padded with zero rows to at least square, so that all right singular
/// vectors exist).
fn machine_null<S: Scalar>(a: &Mat<S>, m: &mut Meter) -> WorkResult<Vec<Vec<S>>> {
    let (r, c) = (a.rows, a.cols);
    let mut sq = a.clone();
    if r < c {
        sq.a.resize(c * c, S::zero());
        sq.rows = c;
    }
    let (_, s, v) = dense::svd(&sq, m)?;
    let tol = r.max(c) as f64 * EPS * s.first().copied().unwrap_or(0.0);
    let mut out = Vec::new();
    for (k, &sk) in s.iter().enumerate() {
        if sk <= tol {
            let mut col: Vec<S> = (0..c).map(|i| v.at(i, k)).collect();
            dense::normalize(&mut col);
            out.push(col);
        }
    }
    Ok(out)
}

/// `null_space(m)`: a basis of the vectors `x` with `m . x == 0`.
pub fn null_space<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    if args.len() != 1 {
        return Ok(Out::Decline);
    }
    let a = take!(matrix_arg(env, args, 0));
    let (r, c) = (a.rows(), a.cols());
    let vecs: Vec<T<'h>> = match a.domain {
        Domain::Real => machine_null(&a.reals(env), m)?.iter().map(|v| env.real_vector(v)).collect(),
        Domain::Complex => machine_null(&a.complex(env), m)?.iter().map(|v| env.cx_vector(v)).collect(),
        Domain::Ball | Domain::ExactComplex => match elim::rref(a.nums(env), r, c, c, 0.0, m)? {
            Ok(x) => elim::null_basis(&x, c).iter().map(|v| env.list(&num_terms(env, v))).collect(),
            Err(Undecided) => return Ok(undecided(env)),
        },
        Domain::Rational | Domain::Symbolic => {
            let Some((ex, x)) = exact_input(env, &a.terms(env), r, c, &[], m)? else { return Ok(Out::Decline) };
            let f = exact::eliminate(&ex, &x, c, true, m)?;
            exact::null_basis(&ex, &f, c, m)?.iter().map(|v| env.list(v)).collect()
        }
    };
    Ok(Out::Done(env.list(&vecs)))
}

/// The reduced row echelon form of a machine matrix at the machine tolerance.
fn machine_rref<S: Scalar>(a: &Mat<S>, cols_pivot: usize, m: &mut Meter) -> WorkResult<Rref<S>> {
    let tol = machine_tol(a);
    // invariant: machine elimination decides every pivot by the tolerance (only exact elimination can be undecided)
    Ok(elim::rref(a.a.clone(), a.rows, a.cols, cols_pivot, tol, m)?.expect("machine elimination always decides"))
}

/// `row_reduce(m)`: the reduced row echelon form; `row_reduce(m, modulus: p)` over the integers modulo a prime p
/// (entries 0 to p - 1); `row_reduce(m, tolerance: t)`: machine entries at most t times the largest are zero
/// (round 37).
pub fn row_reduce<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let Some(opt) = elim_option(env, args) else { return Ok(Out::Decline) };
    let a = take!(matrix_arg(env, args, 0));
    let (r, c) = (a.rows(), a.cols());
    match opt {
        ElimOption::Modulus(p) => {
            if !a.domain.exact_numeric() || a.domain == Domain::ExactComplex {
                return Ok(Out::Decline);
            }
            let Some(xs) = mod_entries(env, &a, p) else { return Ok(Out::Decline) };
            let Ok(x) = elim::rref(xs, r, c, c, 0.0, m)? else { return Ok(Out::Decline) };
            let h = env.h();
            let terms: Vec<T<'h>> = x.a.iter().map(|e| h.int(e.v as i64)).collect();
            return Ok(Out::Done(env.matrix(r, c, &terms)));
        }
        ElimOption::Tolerance(t) if a.domain.machine() => {
            return Ok(Out::Done(match a.domain {
                Domain::Real => {
                    let ma = a.reals(env);
                    let tol = t * ma.max_abs();
                    let Ok(x) = elim::rref(ma.a.clone(), r, c, c, tol, m)? else { return Ok(Out::Decline) };
                    env.real_matrix(&Mat { rows: r, cols: c, a: x.a })
                }
                _ => {
                    let ma = a.complex(env);
                    let tol = t * ma.max_abs();
                    let Ok(x) = elim::rref(ma.a.clone(), r, c, c, tol, m)? else { return Ok(Out::Decline) };
                    env.cx_matrix(&Mat { rows: r, cols: c, a: x.a })
                }
            }));
        }
        ElimOption::Tolerance(_) | ElimOption::Default => {}
    }
    Ok(Out::Done(match a.domain {
        Domain::Real => {
            let x = machine_rref(&a.reals(env), c, m)?;
            env.real_matrix(&Mat { rows: r, cols: c, a: x.a })
        }
        Domain::Complex => {
            let x = machine_rref(&a.complex(env), c, m)?;
            env.cx_matrix(&Mat { rows: r, cols: c, a: x.a })
        }
        Domain::Ball | Domain::ExactComplex => match elim::rref(a.nums(env), r, c, c, 0.0, m)? {
            Ok(x) => env.matrix(r, c, &num_terms(env, &x.a)),
            Err(Undecided) => return Ok(undecided(env)),
        },
        Domain::Rational | Domain::Symbolic => {
            let Some((ex, x)) = exact_input(env, &a.terms(env), r, c, &[], m)? else { return Ok(Out::Decline) };
            let f = exact::eliminate(&ex, &x, c, true, m)?;
            env.matrix(r, c, &exact::rref_terms(&ex, &f, m)?)
        }
    }))
}

/// A particular solution from the reduced form of `[a | b]` (`n` unknowns, `k` right sides): `None` when
/// inconsistent.
fn particular<F: Field>(x: &Rref<F>, n: usize, k: usize) -> Option<Vec<F>> {
    let zero = x.a[0].zero_like();
    for i in x.rank()..x.rows {
        if (0..k).any(|j| x.at(i, n + j).weight(0.0) != elim::Weight::Zero) {
            return None;
        }
    }
    let mut out = vec![zero; n * k];
    for (i, &p) in x.pivots.iter().enumerate() {
        for j in 0..k {
            out[p * k + j] = x.at(i, n + j).clone();
        }
    }
    Some(out)
}

/// `[a | b]` row-major.
fn augment<X: Clone>(a: &[X], b: &[X], r: usize, n: usize, k: usize) -> Vec<X> {
    let mut out = Vec::with_capacity(r * (n + k));
    for i in 0..r {
        out.extend_from_slice(&a[i * n..(i + 1) * n]);
        out.extend_from_slice(&b[i * k..(i + 1) * k]);
    }
    out
}

/// `linear_solve(m, b)`: a solution `x` of `m . x == b` (`b` a vector or a matrix of right sides); the unique one
/// when `m` is square and nonsingular, otherwise the particular solution with the free unknowns zero.
pub fn linear_solve<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    if args.len() != 2 {
        return Ok(Out::Decline);
    }
    let a = take!(matrix_arg(env, args, 0));
    let b = take!(tensor_arg(env, args, 1));
    let (r, n) = (a.rows(), a.cols());
    let (vector, k) = if b.is_matrix() { (false, b.cols()) } else { (true, 1) };
    let b_rows = if b.is_matrix() { b.rows() } else { b.cols() };
    if b_rows != r || k == 0 {
        return Ok(Out::Done(env.failure("shape_mismatch", &[])));
    }
    let no_solution = || Out::Done(env.failure("no_solution", &[]));
    let shape = |rows: Vec<T<'h>>| -> T<'h> { if vector { env.list(&rows) } else { env.matrix(n, k, &rows) } };
    Ok(match a.domain.join(b.domain) {
        Domain::Real => {
            let (am, bm) = (a.reals(env), Mat { rows: r, cols: k, a: b.reals(env).a });
            match machine_solve(&am, &bm, m)? {
                Some(x) => Out::Done(shape(x.iter().map(|&v| env.h().real(v)).collect())),
                None => no_solution(),
            }
        }
        Domain::Complex => {
            let (am, bm) = (a.complex(env), Mat { rows: r, cols: k, a: b.complex(env).a });
            match machine_solve(&am, &bm, m)? {
                Some(x) => Out::Done(shape(x.iter().map(|&v| env.cx(v)).collect())),
                None => no_solution(),
            }
        }
        Domain::Ball | Domain::ExactComplex => {
            let aug = augment(&a.nums(env), &b.nums(env), r, n, k);
            match elim::rref(aug, r, n + k, n, 0.0, m)? {
                Ok(x) => match particular(&x, n, k) {
                    Some(sol) => Out::Done(shape(num_terms(env, &sol))),
                    None => no_solution(),
                },
                Err(Undecided) => undecided(env),
            }
        }
        Domain::Rational | Domain::Symbolic => {
            let terms = augment(&a.terms(env), &b.terms(env), r, n, k);
            let Some((ex, x)) = exact_input(env, &terms, r, n + k, &[], m)? else { return Ok(Out::Decline) };
            let f = exact::eliminate(&ex, &x, n, true, m)?;
            let rank = f.pivots.len();
            if (rank..r).any(|i| (0..k).any(|j| !f.at(i, n + j).is_zero())) {
                return Ok(no_solution());
            }
            let mut out = vec![env.ar.int(0); n * k];
            for (i, &p) in f.pivots.iter().enumerate() {
                for j in 0..k {
                    out[p * k + j] = ex.quotient(f.at(i, n + j), &f.pivot, m)?;
                }
            }
            Out::Done(shape(out))
        }
    })
}

/// Solve a machine system: LU when square and nonsingular, otherwise elimination at the machine tolerance (the
/// particular solution); `None` when inconsistent.
fn machine_solve<S: Scalar>(a: &Mat<S>, b: &Mat<S>, m: &mut Meter) -> WorkResult<Option<Vec<S>>> {
    let (r, n, k) = (a.rows, a.cols, b.cols);
    if r == n {
        let f = dense::lu(a.clone(), m)?;
        if !f.singular() {
            return Ok(Some(f.solve(b, m)?.a));
        }
    }
    let aug = Mat { rows: r, cols: n + k, a: augment(&a.a, &b.a, r, n, k) };
    let tol = machine_tol(&aug);
    // invariant: machine elimination decides every pivot by the tolerance (only exact elimination can be undecided)
    let x = elim::rref(aug.a, r, n + k, n, tol, m)?.expect("machine elimination always decides");
    Ok(particular(&x, n, k))
}

/// The conjugate of a number.
pub fn conj(x: &Num) -> Num {
    match x {
        Num::Complex(a, b) => Num::Complex(a.clone(), b.neg()),
        r => r.clone(),
    }
}

/// `a b` for matrices of numbers.
pub fn num_mul(a: &[Num], b: &[Num], r: usize, n: usize, c: usize, m: &mut Meter) -> WorkResult<Vec<Num>> {
    let mut out = Vec::with_capacity(r * c);
    for i in 0..r {
        m.step((n * c) as u64 + 1)?;
        for j in 0..c {
            let mut s = Num::int(0);
            for k in 0..n {
                s = s.add(&a[i * n + k].mul(&b[k * c + j]));
            }
            out.push(s);
        }
    }
    Ok(out)
}

/// The conjugate transpose of an `r x c` matrix of numbers.
pub fn num_adjoint(a: &[Num], r: usize, c: usize) -> Vec<Num> {
    (0..c).flat_map(|j| (0..r).map(move |i| (i, j))).map(|(i, j)| conj(&a[i * c + j])).collect()
}

/// The pseudo-inverse of a matrix of numbers by its rank factorisation `a = C F` (C the pivot columns of `a`, F
/// the nonzero rows of its reduced form): `a+ = F^H (F F^H)^-1 (C^H C)^-1 C^H`.
fn num_pseudo_inverse(a: &[Num], r: usize, c: usize, m: &mut Meter) -> WorkResult<Result<Vec<Num>, Undecided>> {
    let x = match elim::rref(a.to_vec(), r, c, c, 0.0, m)? {
        Ok(x) => x,
        Err(u) => return Ok(Err(u)),
    };
    let k = x.rank();
    if k == 0 {
        return Ok(Ok(vec![Num::int(0); c * r]));
    }
    let f: Vec<Num> = x.a[..k * c].to_vec();
    let cm: Vec<Num> = (0..r).flat_map(|i| x.pivots.iter().map(move |&p| (i, p))).map(|(i, p)| a[i * c + p].clone()).collect();
    let fh = num_adjoint(&f, k, c);
    let ch = num_adjoint(&cm, r, k);
    let ffh = num_mul(&f, &fh, k, c, k, m)?;
    let chc = num_mul(&ch, &cm, k, r, k, m)?;
    let (Ok(Some(i1)), Ok(Some(i2))) = (num_inverse(&ffh, k, m)?, num_inverse(&chc, k, m)?) else { return Ok(Err(Undecided)) };
    let left = num_mul(&fh, &i1, c, k, k, m)?;
    let mid = num_mul(&left, &i2, c, k, k, m)?;
    Ok(Ok(num_mul(&mid, &ch, c, k, r, m)?))
}

/// The pseudo-inverse of a machine matrix from its SVD: `v diag(1/s) u^H` over the singular values above the
/// rank tolerance.
fn machine_pseudo_inverse<S: Scalar>(a: &Mat<S>, m: &mut Meter) -> WorkResult<Mat<S>> {
    let (u, s, v) = dense::svd(a, m)?;
    let tol = a.rows.max(a.cols) as f64 * EPS * s.first().copied().unwrap_or(0.0);
    let mut vs = v.clone();
    for (k, &sk) in s.iter().enumerate() {
        let f = if sk > tol { 1.0 / sk } else { 0.0 };
        for i in 0..vs.rows {
            *vs.at_mut(i, k) = vs.at(i, k).scale(f);
        }
    }
    dense::mul(&vs, &u.adjoint(), m)
}

/// `pseudo_inverse(m)`: the Moore-Penrose inverse.
pub fn pseudo_inverse<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    if args.len() != 1 {
        return Ok(Out::Decline);
    }
    let a = take!(matrix_arg(env, args, 0));
    let (r, c) = (a.rows(), a.cols());
    Ok(match a.domain {
        Domain::Real => Out::Done(env.real_matrix(&machine_pseudo_inverse(&a.reals(env), m)?)),
        Domain::Complex => Out::Done(env.cx_matrix(&machine_pseudo_inverse(&a.complex(env), m)?)),
        Domain::Ball | Domain::ExactComplex | Domain::Rational => match num_pseudo_inverse(&a.nums(env), r, c, m)? {
            Ok(p) => Out::Done(env.matrix(c, r, &num_terms(env, &p))),
            Err(Undecided) => undecided(env),
        },
        // symbolic entries would need their conjugates (gap)
        Domain::Symbolic => Out::Decline,
    })
}
