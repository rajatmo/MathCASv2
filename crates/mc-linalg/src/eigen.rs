//! Characteristic polynomials, eigenvalues and eigenvectors.
//!
//! - **Machine:** a real symmetric matrix by cyclic Jacobi; any other by the complex Schur form (Hessenberg
//!   reduction and shifted QR) with eigenvectors by back substitution. Eigenvectors have unit length with their
//!   largest component real and positive.
//! - **Exact and symbolic:** the characteristic polynomial `det(x I - m)` by fraction-free elimination, factored
//!   over the integers in the entries' kernels. Linear factors give rational (function) eigenvalues, quadratic
//!   factors radicals, higher irreducible factors `root(fn([x], p), k)` objects (decision LA.10). Eigenvectors are
//!   null spaces: of `m - lambda I` for a rational eigenvalue, over the number field of the factor otherwise
//!   (decision LA.11).
//! - Eigenvalues are listed by decreasing modulus (then decreasing real and imaginary part) with their algebraic
//!   multiplicities; a defective eigenvalue's missing eigenvectors are zero vectors (decision LA.12).
//! - Characteristic polynomials of matrices of numbers that are not exact rationals use Berkowitz's division-free
//!   algorithm.
use crate::algebraic::Alg;
use crate::dense::{self, Mat, EPS};
use crate::elim::{self, Field};
use crate::exact::{self, E};
use crate::natives::Out;
use crate::scalar::{Cx, Scalar};
use crate::systems::{exact_input, square_arg, take};
use crate::tensor::{Arr, Domain, Env};
use mc_api::Store;
use mc_arith::num::Rat;
use mc_arith::{Num, Real};
use mc_core::T;
use mc_poly::factor::factor;
use mc_poly::poly::Poly;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::z::Z;
use std::cmp::Ordering;
use std::sync::Arc;

/// The coefficients of `det(x I - a)`, highest degree first, by Berkowitz's algorithm (only ring operations:
/// O(n^4)).
pub fn berkowitz<F: Field>(a: &[F], n: usize, m: &mut Meter) -> WorkResult<Vec<F>> {
    let one = a[0].one_like();
    let zero = a[0].zero_like();
    let mut p = vec![one.clone()];
    for r in 0..n {
        m.step((r * r * r) as u64 + 1)?;
        // q = [1, -a_rr, -R C, -R A C, ..., -R A^(r-1) C] with R = a[r][..r], C = a[..r][r], A the leading block
        let mut q = vec![one.clone(), zero.sub(&a[r * n + r])];
        let mut col: Vec<F> = (0..r).map(|i| a[i * n + r].clone()).collect();
        for _ in 0..r {
            let rc = (0..r).fold(zero.clone(), |s, j| s.add(&a[r * n + j].mul(&col[j])));
            q.push(zero.sub(&rc));
            col = (0..r).map(|i| (0..r).fold(zero.clone(), |s, j| s.add(&a[i * n + j].mul(&col[j])))).collect();
        }
        // p <- T p with T the (r+2) x (r+1) lower triangular Toeplitz matrix of q
        let next: Vec<F> = (0..r + 2)
            .map(|i| (0..=r.min(i)).filter(|&j| i - j < q.len()).fold(zero.clone(), |s, j| s.add(&q[i - j].mul(&p[j]))))
            .collect();
        p = next;
    }
    Ok(p)
}

/// The polynomial term `sum c_k x^k` from coefficient terms, highest degree first.
fn poly_term<'h>(env: &Env<'h>, coefs: &[T<'h>], x: T<'h>) -> T<'h> {
    let d = coefs.len() - 1;
    let terms: Vec<T<'h>> = coefs
        .iter()
        .enumerate()
        .map(|(i, &c)| env.ar.mul(&[c, env.ar.pow(x, env.h().int((d - i) as i64))]))
        .collect();
    env.ar.add(&terms)
}

/// `characteristic_polynomial(m, x)`: `det(x I - m)`, monic in `x`.
pub fn characteristic_polynomial<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [mt, x] = args else { return Ok(Out::Decline) };
    let a = take!(square_arg(env, &[*mt]));
    let n = a.rows();
    let x = *x;
    let coefs: Vec<T<'h>> = match a.domain {
        Domain::Real => berkowitz(&a.reals(env).a, n, m)?.iter().map(|&c| env.h().real(c)).collect(),
        Domain::Complex => berkowitz(&a.complex(env).a, n, m)?.iter().map(|&c| env.cx(c)).collect(),
        Domain::Ball | Domain::ExactComplex => berkowitz(&a.nums(env), n, m)?.iter().map(|c| env.num(c)).collect(),
        Domain::Rational | Domain::Symbolic => {
            let terms = shifted(env, &a.terms(env), n, x, true);
            let Some((ex, xm)) = exact_input(env, &terms, n, n, &[x], m)? else { return Ok(Out::Decline) };
            return Ok(Out::Done(exact::det(&ex, &xm, m)?));
        }
    };
    Ok(Out::Done(poly_term(env, &coefs, x)))
}

/// `x I - a` (`flip`) or `a - x I` as terms.
fn shifted<'h>(env: &Env<'h>, a: &[T<'h>], n: usize, x: T<'h>, flip: bool) -> Vec<T<'h>> {
    let ar = &env.ar;
    (0..n * n)
        .map(|k| {
            let (i, j) = (k / n, k % n);
            match (i == j, flip) {
                (true, true) => ar.sub(x, a[k]),
                (true, false) => ar.sub(a[k], x),
                (false, true) => ar.neg(a[k]),
                (false, false) => a[k],
            }
        })
        .collect()
}

/// The order of eigenvalues: decreasing modulus, then decreasing real part, then decreasing imaginary part.
fn eigen_order(a: Cx, b: Cx) -> Ordering {
    b.abs().total_cmp(&a.abs()).then(b.re.total_cmp(&a.re)).then(b.im.total_cmp(&a.im))
}

/// Machine eigenvalues (and eigenvectors) of a machine matrix, in eigenvalue order; `None` if the QR iteration
/// does not converge.
#[allow(clippy::type_complexity)]
fn machine_eigen(env: &Env<'_>, a: &Arr<'_>, vectors: bool, m: &mut Meter) -> WorkResult<Option<(Vec<Cx>, Vec<Vec<Cx>>)>> {
    let n = a.rows();
    let (vals, vecs): (Vec<Cx>, Option<Mat<Cx>>) = if a.domain == Domain::Real && dense::is_hermitian(&a.reals(env)) {
        let (v, q) = dense::symmetric_eigen(&a.reals(env), m)?;
        (v.iter().map(|&x| Cx::new(x, 0.0)).collect(), Some(q.to_cx()))
    } else {
        let c = a.complex(env);
        let Some((t, q)) = dense::schur(&c, m)? else { return Ok(None) };
        let hermitian = dense::is_hermitian(&c);
        let (mut vals, vecs) = if vectors {
            let (v, x) = dense::schur_eigenvectors(&t, &q, m)?;
            (v, Some(x))
        } else {
            ((0..n).map(|i| t.at(i, i)).collect(), None)
        };
        // a real or Hermitian matrix's real eigenvalues come out with rounding-level imaginary parts
        let tol = 64.0 * n as f64 * EPS * c.frobenius();
        if a.domain == Domain::Real || hermitian {
            for z in &mut vals {
                if hermitian || z.im.abs() <= tol {
                    z.im = 0.0;
                }
            }
        }
        (vals, vecs)
    };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| eigen_order(vals[i], vals[j]));
    let sorted: Vec<Cx> = order.iter().map(|&i| vals[i]).collect();
    let mut out_vecs = Vec::new();
    if let (true, Some(q)) = (vectors, vecs) {
        for &k in &order {
            let mut v: Vec<Cx> = (0..n).map(|i| q.at(i, k)).collect();
            dense::normalize(&mut v);
            if vals[k].im == 0.0 && a.domain == Domain::Real {
                let tol = 64.0 * n as f64 * EPS;
                if v.iter().all(|z| z.im.abs() <= tol) {
                    for z in &mut v {
                        z.im = 0.0;
                    }
                }
            }
            out_vecs.push(v);
        }
    }
    Ok(Some((sorted, out_vecs)))
}

/// One irreducible factor of an exact characteristic polynomial and its roots.
struct Group<'h> {
    /// its roots as terms
    roots: Vec<T<'h>>,
    /// their machine values, when the factor is numeric
    approx: Vec<Option<Cx>>,
    /// the multiplicity of the factor
    mult: u32,
    /// the factor's coefficients in x, lowest degree first
    coefs: Vec<Poly>,
    /// the factor has coefficients involving other kernels
    params: bool,
    /// for numeric quadratics: each root as `p + q sqrt(d)`
    quadratic: Option<(Vec<(Rat, Rat)>, Z)>,
}

fn z_rat(z: &Z) -> Rat {
    Rat::from_int(z.to_int())
}

fn rat_f64(q: &Rat) -> f64 {
    Real::Exact(q.clone()).to_f64()
}

/// The machine roots of a polynomial with machine coefficients (lowest degree first, nonzero leading one): the
/// eigenvalues of its companion matrix, ordered real ones ascending, then the others by real then imaginary part.
fn companion_roots(c: &[f64], m: &mut Meter) -> WorkResult<Option<Vec<Cx>>> {
    let d = c.len() - 1;
    let lead = c[d];
    let mut a = Mat::<Cx>::zeros(d, d);
    for j in 0..d {
        *a.at_mut(0, j) = Cx::new(-c[d - 1 - j] / lead, 0.0);
    }
    for i in 1..d {
        *a.at_mut(i, i - 1) = Cx::new(1.0, 0.0);
    }
    let Some((t, _)) = dense::schur(&a, m)? else { return Ok(None) };
    let scale = a.frobenius().max(1.0);
    let mut roots: Vec<Cx> = (0..d)
        .map(|i| {
            let z = t.at(i, i);
            if z.im.abs() <= 1e-10 * scale { Cx::new(z.re, 0.0) } else { z }
        })
        .collect();
    roots.sort_by(|a, b| (a.im != 0.0).cmp(&(b.im != 0.0)).then(a.re.total_cmp(&b.re)).then(a.im.total_cmp(&b.im)));
    Ok(Some(roots))
}

/// The exact eigenvalue groups of a matrix of terms; `None` when the characteristic polynomial cannot be read.
fn exact_groups<'h>(env: &Env<'h>, terms: &[T<'h>], n: usize, m: &mut Meter) -> WorkResult<Option<Vec<Group<'h>>>> {
    let lam = env.sym("lambda$");
    let ct = shifted(env, terms, n, lam, true);
    let Some((ex, xm)) = exact_input(env, &ct, n, n, &[lam], m)? else { return Ok(None) };
    let f = exact::eliminate(&ex, &xm, n, false, m)?;
    let E::P(p) = &f.pivot else { return Ok(None) };
    let Some(xv) = ex.var(lam) else { return Ok(None) };
    let fac = factor(p, false, m)?;
    let ar = &env.ar;
    let h = env.h();
    let half = ar.rational(h.int(1), h.int(2));
    let mut groups = Vec::new();
    for (g, mult) in &fac.factors {
        let dx = g.degree(xv).unwrap_or(0) as usize;
        if dx == 0 {
            continue;
        }
        let mut coefs = vec![Poly::zero(g.nvars()); dx + 1];
        for (k, c) in g.coeffs_in(xv) {
            coefs[k as usize] = c;
        }
        let params = coefs.iter().any(|c| !c.is_constant());
        let consts: Option<Vec<Z>> = coefs.iter().map(|c| if c.is_zero() { Some(Z::ZERO) } else { c.as_constant() }).collect();
        let mut group = Group { roots: vec![], approx: vec![], mult: *mult, coefs: coefs.clone(), params, quadratic: None };
        match (dx, &consts) {
            (1, _) => {
                group.roots.push(ex.quotient(&E::P(coefs[0].neg()), &E::P(coefs[1].clone()), m)?);
                group.approx.push(consts.as_ref().map(|c| Cx::new(-rat_f64(&z_rat(&c[0])) / rat_f64(&z_rat(&c[1])), 0.0)));
            }
            (2, Some(c)) => {
                let (c0, c1, c2) = (&c[0], &c[1], &c[2]);
                let disc = c1.mul(c1).sub(&c0.mul(c2).mul_i64(4));
                let two_a = Rat::from_int(c2.mul_i64(2).to_int());
                let p = z_rat(&c1.neg()).mul(&two_a.recip().expect("a nonzero leading coefficient"));
                let q = two_a.recip().expect("a nonzero leading coefficient");
                let s = ar.pow(ex.ar.integer(&disc.to_int()), half);
                let (df, pf, qf) = (rat_f64(&z_rat(&disc)), rat_f64(&p), rat_f64(&q));
                let sf = if df >= 0.0 { Cx::new(df.sqrt(), 0.0) } else { Cx::new(0.0, (-df).sqrt()) };
                let mut forms = vec![];
                for sign in [1i64, -1] {
                    let qs = q.mul(&Rat::from_i64(sign));
                    group.roots.push(ar.add(&[env.num(&Num::rat(p.clone())), ar.mul(&[env.num(&Num::rat(qs.clone())), s])]));
                    group.approx.push(Some(Cx::new(pf, 0.0) + sf.scale(qf * sign as f64)));
                    forms.push((p.clone(), qs));
                }
                group.quadratic = Some((forms, disc));
            }
            (2, None) => {
                let neg_b = ex.poly_term(&coefs[1].neg(), m)?;
                let two_a = ex.poly_term(&coefs[2].scale(&Z::S(2)), m)?;
                let disc = coefs[1].mul(&coefs[1], m)?.sub(&coefs[0].mul(&coefs[2], m)?.scale(&Z::S(4)));
                let s = ar.pow(ex.poly_term(&disc, m)?, half);
                let inv = ar.pow(two_a, h.int(-1));
                for sign in [1i64, -1] {
                    let num = ar.add(&[neg_b, ar.mul(&[h.int(sign), s])]);
                    group.roots.push(ar.mul(&[num, inv]));
                    group.approx.push(None);
                }
            }
            _ => {
                let x = env.sym("x");
                let ct: Vec<T<'h>> = coefs.iter().rev().map(|c| ex.poly_term(c, m)).collect::<WorkResult<_>>()?;
                let body = poly_term(env, &ct, x);
                let closure = env.call("fn", &[env.list(&[x]), body]);
                let approx = match &consts {
                    Some(c) => companion_roots(&c.iter().map(|z| rat_f64(&z_rat(z))).collect::<Vec<_>>(), m)?,
                    None => None,
                };
                for k in 0..dx {
                    group.roots.push(env.call("root", &[closure, h.int(k as i64 + 1)]));
                    group.approx.push(approx.as_ref().map(|r| r[k]));
                }
            }
        }
        groups.push(group);
    }
    Ok(Some(groups))
}

/// Eigenvalue slots in output order: (group, root index).
fn ordered_slots(groups: &[Group<'_>]) -> Vec<(usize, usize)> {
    let mut slots: Vec<(usize, usize)> = vec![];
    for (g, group) in groups.iter().enumerate() {
        for r in 0..group.roots.len() {
            for _ in 0..group.mult {
                slots.push((g, r));
            }
        }
    }
    if slots.iter().all(|&(g, r)| groups[g].approx[r].is_some()) {
        slots.sort_by(|&(g1, r1), &(g2, r2)| {
            eigen_order(groups[g1].approx[r1].expect("numeric"), groups[g2].approx[r2].expect("numeric"))
        });
    }
    slots
}

/// The eigenvectors of one root of an exact group (at most `mult` of them, possibly fewer when defective); `None`
/// when the group needs a number field over parameters (a gap).
fn group_vectors<'h>(
    env: &Env<'h>,
    terms: &[T<'h>],
    n: usize,
    group: &Group<'h>,
    root: usize,
    m: &mut Meter,
) -> WorkResult<Option<Vec<Vec<T<'h>>>>> {
    let ar = &env.ar;
    if group.coefs.len() == 2 {
        let shiftedm = shifted(env, terms, n, group.roots[root], false);
        let Some((ex, x)) = exact_input(env, &shiftedm, n, n, &[], m)? else { return Ok(None) };
        let f = exact::eliminate(&ex, &x, n, true, m)?;
        return Ok(Some(exact::null_basis(&ex, &f, n, m)?));
    }
    if group.params {
        return Ok(None);
    }
    // the number field of the factor, with the monic modulus
    let d = group.coefs.len() - 1;
    let lead = z_rat(&group.coefs[d].as_constant().expect("numeric")).recip().expect("nonzero");
    let modulus: Vec<Rat> = group.coefs.iter().map(|c| z_rat(&c.as_constant().unwrap_or(Z::ZERO)).mul(&lead)).collect();
    let modulus = Arc::new(modulus);
    let t = Alg::generator(&modulus);
    let mut a = Vec::with_capacity(n * n);
    for (k, &x) in terms.iter().enumerate() {
        let Some(q) = env.ar.num(x).and_then(|v| v.as_rat().cloned()) else { return Ok(None) };
        let e = Alg::rational(q, &modulus);
        a.push(if k / n == k % n { e.sub(&t) } else { e });
    }
    let Ok(r) = elim::rref(a, n, n, n, 0.0, m)? else { return Ok(None) };
    let basis = elim::null_basis(&r, n);
    let mut out = Vec::with_capacity(basis.len());
    for v in basis {
        let mut tv = Vec::with_capacity(n);
        for e in v {
            tv.push(match &group.quadratic {
                // c0 + c1 (p + q s) = (c0 + c1 p) + c1 q s
                Some((forms, disc)) => {
                    let (p, q) = &forms[root];
                    let re = e.c[0].add(&e.c[1].mul(p));
                    let im = e.c[1].mul(q);
                    let s = ar.pow(ar.integer(&disc.to_int()), ar.rational(env.h().int(1), env.h().int(2)));
                    ar.add(&[env.num(&Num::rat(re)), ar.mul(&[env.num(&Num::rat(im)), s])])
                }
                None => {
                    let r = group.roots[root];
                    let parts: Vec<T<'h>> = e
                        .c
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| !c.is_zero())
                        .map(|(j, c)| ar.mul(&[env.num(&Num::rat(c.clone())), ar.pow(r, env.h().int(j as i64))]))
                        .collect();
                    ar.add(&parts)
                }
            });
        }
        out.push(tv);
    }
    Ok(Some(out))
}

/// Eigenvalues (and eigenvectors) of an argument as terms; `None` declines.
#[allow(clippy::type_complexity)]
fn eigen<'h>(env: &Env<'h>, a: &Arr<'h>, vectors: bool, m: &mut Meter) -> WorkResult<Result<(Vec<T<'h>>, Vec<T<'h>>), Out<'h>>> {
    let n = a.rows();
    match a.domain {
        Domain::Real | Domain::Complex => {
            let Some((vals, vecs)) = machine_eigen(env, a, vectors, m)? else {
                return Ok(Err(Out::Done(env.failure("not_converged", &[]))));
            };
            let vals = vals.iter().map(|&z| env.cx(z)).collect();
            let vecs = vecs.iter().map(|v| env.cx_vector(v)).collect();
            Ok(Ok((vals, vecs)))
        }
        Domain::Rational | Domain::Symbolic => {
            let terms = a.terms(env);
            let Some(groups) = exact_groups(env, &terms, n, m)? else { return Ok(Err(Out::Decline)) };
            let slots = ordered_slots(&groups);
            let vals: Vec<T<'h>> = slots.iter().map(|&(g, r)| groups[g].roots[r]).collect();
            if !vectors {
                return Ok(Ok((vals, vec![])));
            }
            let mut cache: Vec<Option<Vec<Vec<T<'h>>>>> = vec![None; groups.iter().map(|g| g.roots.len()).sum()];
            let base: Vec<usize> = groups.iter().scan(0, |s, g| {
                let b = *s;
                *s += g.roots.len();
                Some(b)
            }).collect();
            let mut used = vec![0usize; cache.len()];
            let zero = env.list(&vec![env.h().int(0); n]);
            let mut vecs = Vec::with_capacity(n);
            for &(g, r) in &slots {
                let k = base[g] + r;
                if cache[k].is_none() {
                    match group_vectors(env, &terms, n, &groups[g], r, m)? {
                        Some(vs) => cache[k] = Some(vs),
                        None => return Ok(Err(Out::Decline)),
                    }
                }
                let vs = cache[k].as_ref().expect("filled");
                vecs.push(vs.get(used[k]).map_or(zero, |v| env.list(v)));
                used[k] += 1;
            }
            Ok(Ok((vals, vecs)))
        }
        // exact complex and ball matrices (gap): the call stays
        Domain::ExactComplex | Domain::Ball => Ok(Err(Out::Decline)),
    }
}

/// Results containing `root(...)` objects are evaluated again (their package may define them).
fn finish(t: T<'_>, groups_have_roots: bool) -> Out<'_> {
    if groups_have_roots {
        Out::Again(t)
    } else {
        Out::Done(t)
    }
}

/// Whether some value is a `root(...)` call.
fn has_root<'h>(env: &Env<'h>, vals: &[T<'h>]) -> bool {
    let root = env.h().sym("root");
    vals.iter().any(|&v| mentions(env, v, root))
}

/// Does `t` contain a call of `s` (an explicit stack, bounded by the term's size)?
fn mentions<'h>(env: &Env<'h>, t: T<'h>, s: mc_api::Sym) -> bool {
    let h = env.h();
    let mut stack = vec![t];
    while let Some(x) = stack.pop() {
        if let mc_api::View::App { head, args } = h.view(x) {
            if head == h.sym_term(s) {
                return true;
            }
            if h.packed(x).is_none() {
                stack.extend_from_slice(args);
            }
        }
    }
    false
}

/// `eigenvalues(m)`.
pub fn eigenvalues<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let a = take!(square_arg(env, args));
    let (vals, _) = match eigen(env, &a, false, m)? {
        Ok(v) => v,
        Err(out) => return Ok(out),
    };
    let t = env.list(&vals);
    Ok(finish(t, has_root(env, &vals)))
}

/// `eigenvectors(m)`: one eigenvector per eigenvalue, in the order of `eigenvalues(m)`.
pub fn eigenvectors<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let a = take!(square_arg(env, args));
    let (vals, vecs) = match eigen(env, &a, true, m)? {
        Ok(v) => v,
        Err(out) => return Ok(out),
    };
    Ok(finish(env.list(&vecs), has_root(env, &vals)))
}

/// `eigensystem(m)`: `[eigenvalues(m), eigenvectors(m)]`.
pub fn eigensystem<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let a = take!(square_arg(env, args));
    let (vals, vecs) = match eigen(env, &a, true, m)? {
        Ok(v) => v,
        Err(out) => return Ok(out),
    };
    let t = env.list(&[env.list(&vals), env.list(&vecs)]);
    Ok(finish(t, has_root(env, &vals)))
}

/// The exact eigenvalues of a matrix of terms (for singular values); `None` when they cannot be computed.
pub fn exact_eigenvalues<'h>(env: &Env<'h>, terms: &[T<'h>], n: usize, m: &mut Meter) -> WorkResult<Option<Vec<T<'h>>>> {
    let Some(groups) = exact_groups(env, terms, n, m)? else { return Ok(None) };
    let slots = ordered_slots(&groups);
    Ok(Some(slots.iter().map(|&(g, r)| groups[g].roots[r]).collect()))
}

/// Is the eigenvalue term an exact zero?
pub fn is_zero_term(env: &Env<'_>, t: T<'_>) -> bool {
    env.ar.num(t).is_some_and(|x| x.is_exact_zero())
}
