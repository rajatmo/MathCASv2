//! The symmetric eigenproblem at arbitrary precision (round 36): eigenvalues and eigenvectors of a real symmetric
//! matrix of balls, computed on the midpoints with binary floats of `w` bits. Householder reduction to tridiagonal
//! form (Golub and Van Loan, "Matrix Computations", 4th ed., algorithm 8.3.1), then implicit symmetric QR steps with
//! the Wilkinson shift and deflation (algorithm 8.3.3), the rotations accumulated for the eigenvectors. The
//! eigenvalue radii follow Weyl's inequality |lambda_k(A + E) - lambda_k(A)| <= |E|_2 <= |E|_F (the input radii as
//! E) plus the rounding of the method (a few units of 2^-w times n |A|); an eigenvector's radius is that bound over
//! the gap to the nearest other eigenvalue (Davis and Kahan, the sin-theta theorem). Machine arithmetic would lose
//! everything below 2^-53 |A| (the smallest eigenvalue of the 100 x 100 Hilbert matrix is 1.0e-150).
use mc_arith::num::{Ball, Float, Mag};
use mc_arith::{Kind, Num, Real};
use mc_poly::work::{Meter, WorkResult};
use std::cmp::Ordering;

/// Binary floats of a fixed precision.
#[derive(Clone, Copy)]
pub(crate) struct P(pub u64);

impl P {
    fn add(self, a: &Float, b: &Float) -> Float {
        a.add(b, self.0).0
    }
    fn sub(self, a: &Float, b: &Float) -> Float {
        a.sub(b, self.0).0
    }
    fn mul(self, a: &Float, b: &Float) -> Float {
        a.mul(b, self.0).0
    }
    fn div(self, a: &Float, b: &Float) -> Float {
        a.div(b, self.0).0
    }
    fn sqrt(self, a: &Float) -> Float {
        a.sqrt(self.0).0
    }
    fn hypot(self, a: &Float, b: &Float) -> Float {
        self.sqrt(&self.add(&self.mul(a, a), &self.mul(b, b)))
    }
}

fn int(v: i64) -> Float {
    Float::from_i64(v)
}

/// |x| <= 2^-k |y| (x negligible against y), for y nonzero.
fn negligible(x: &Float, y: &Float, k: i64) -> bool {
    x.is_zero() || (!y.is_zero() && x.top() < y.top() - k)
}

/// The eigenvalues (unsorted) of the symmetric n x n matrix `a` (row-major) and, with `vectors`, the matrix whose
/// column k is the unit eigenvector of eigenvalue k (row-major).
pub(crate) fn symmetric(a: &[Float], n: usize, p: P, vectors: bool, m: &mut Meter) -> WorkResult<(Vec<Float>, Vec<Float>)> {
    let mut a = a.to_vec();
    let at = |i: usize, j: usize| i * n + j;
    let mut q: Vec<Float> = if vectors { (0..n * n).map(|k| if k / n == k % n { int(1) } else { Float::zero() }).collect() } else { vec![] };
    let two = int(2);
    // Householder: zero column k below the subdiagonal by H = I - beta v v^T acting on rows/columns k+1..n
    for k in 0..n.saturating_sub(2) {
        m.step(((n - k) * (n - k)) as u64 * 4 + 1)?;
        let mut v: Vec<Float> = (k + 1..n).map(|i| a[at(i, k)].clone()).collect();
        let tail = v[1..].iter().fold(Float::zero(), |s, x| p.add(&s, &p.mul(x, x)));
        if tail.is_zero() {
            continue;
        }
        let norm = p.sqrt(&p.add(&p.mul(&v[0], &v[0]), &tail));
        let alpha = if v[0].neg { norm } else { norm.neg() };
        v[0] = p.sub(&v[0], &alpha);
        let vv = v.iter().fold(Float::zero(), |s, x| p.add(&s, &p.mul(x, x)));
        let beta = p.div(&two, &vv);
        let r = n - k - 1;
        // pv = beta A' v (A' the trailing block)
        let mut pv: Vec<Float> = Vec::with_capacity(r);
        for i in 0..r {
            let s = (0..r).fold(Float::zero(), |s, j| p.add(&s, &p.mul(&a[at(k + 1 + i, k + 1 + j)], &v[j])));
            pv.push(p.mul(&beta, &s));
        }
        // w = pv - (beta pv.v / 2) v
        let pvv = (0..r).fold(Float::zero(), |s, i| p.add(&s, &p.mul(&pv[i], &v[i])));
        let half = p.div(&p.mul(&beta, &pvv), &two);
        let w: Vec<Float> = (0..r).map(|i| p.sub(&pv[i], &p.mul(&half, &v[i]))).collect();
        for i in 0..r {
            for j in 0..r {
                let d = p.add(&p.mul(&v[i], &w[j]), &p.mul(&w[i], &v[j]));
                let e = &mut a[at(k + 1 + i, k + 1 + j)];
                *e = p.sub(e, &d);
            }
        }
        a[at(k + 1, k)] = alpha.clone();
        a[at(k, k + 1)] = alpha;
        for i in k + 2..n {
            a[at(i, k)] = Float::zero();
            a[at(k, i)] = Float::zero();
        }
        if vectors {
            // Q = Q H: each row x of Q (on columns k+1..) becomes x - beta (x.v) v
            for row in 0..n {
                let xv = (0..r).fold(Float::zero(), |s, j| p.add(&s, &p.mul(&q[at(row, k + 1 + j)], &v[j])));
                let f = p.mul(&beta, &xv);
                for j in 0..r {
                    let e = &mut q[at(row, k + 1 + j)];
                    *e = p.sub(e, &p.mul(&f, &v[j]));
                }
            }
        }
    }
    let mut d: Vec<Float> = (0..n).map(|i| a[at(i, i)].clone()).collect();
    let mut e: Vec<Float> = (0..n.saturating_sub(1)).map(|i| a[at(i + 1, i)].clone()).collect();
    // implicit QR steps on the unreduced trailing blocks
    let tol = p.0 as i64 - 8;
    let mut hi = n;
    let mut iterations = 0usize;
    while hi > 1 {
        // deflate negligible off-diagonal entries
        for i in 0..hi - 1 {
            let s = p.add(&d[i].abs(), &d[i + 1].abs());
            if negligible(&e[i], &s, tol) || (s.is_zero() && e[i].top() < -(p.0 as i64) * 4) {
                e[i] = Float::zero();
            }
        }
        if e[hi - 2].is_zero() {
            hi -= 1;
            continue;
        }
        let mut lo = hi - 2;
        while lo > 0 && !e[lo - 1].is_zero() {
            lo -= 1;
        }
        iterations += 1;
        if iterations > 60 * n + 60 {
            break;
        }
        m.step(((hi - lo) * if vectors { n + 8 } else { 8 }) as u64)?;
        // the Wilkinson shift from the trailing 2 x 2 block
        let (dm, dm1, em) = (&d[hi - 1], &d[hi - 2], &e[hi - 2]);
        let delta = p.div(&p.sub(dm1, dm), &two);
        let root = p.hypot(&delta, em);
        let denom = if delta.neg { p.sub(&delta, &root) } else { p.add(&delta, &root) };
        let mu = if denom.is_zero() { dm.clone() } else { p.sub(dm, &p.div(&p.mul(em, em), &denom)) };
        let mut x = p.sub(&d[lo], &mu);
        let mut z = e[lo].clone();
        for k in lo..hi - 1 {
            let r = p.hypot(&x, &z);
            let (c, s) = if r.is_zero() { (int(1), Float::zero()) } else { (p.div(&x, &r), p.div(&z, &r)) };
            if k > lo {
                e[k - 1] = r;
            }
            let (a0, b0, c0) = (d[k].clone(), e[k].clone(), d[k + 1].clone());
            let cc = p.mul(&c, &c);
            let ss = p.mul(&s, &s);
            let cs = p.mul(&c, &s);
            let csb2 = p.mul(&p.mul(&cs, &b0), &two);
            d[k] = p.add(&p.add(&p.mul(&cc, &a0), &csb2), &p.mul(&ss, &c0));
            d[k + 1] = p.add(&p.sub(&p.mul(&ss, &a0), &csb2), &p.mul(&cc, &c0));
            e[k] = p.add(&p.mul(&p.sub(&cc, &ss), &b0), &p.mul(&cs, &p.sub(&c0, &a0)));
            if k + 1 < hi - 1 {
                z = p.mul(&s, &e[k + 1]);
                e[k + 1] = p.mul(&c, &e[k + 1]);
                x = e[k].clone();
            }
            if vectors {
                for row in 0..n {
                    let (u, w) = (q[at(row, k)].clone(), q[at(row, k + 1)].clone());
                    q[at(row, k)] = p.add(&p.mul(&c, &u), &p.mul(&s, &w));
                    q[at(row, k + 1)] = p.sub(&p.mul(&c, &w), &p.mul(&s, &u));
                }
            }
        }
    }
    Ok((d, q))
}

/// 2^k as a radius.
fn pow2(k: i64) -> Mag {
    Mag::pow2(k)
}

/// Is the matrix real and symmetric, of balls and exact numbers with at least one ball?
pub(crate) fn applies(nums: &[Num], n: usize) -> bool {
    let mut ball = false;
    for x in nums {
        match x {
            Num::Real(Real::Ball(..)) => ball = true,
            Num::Real(Real::Exact(_)) => {}
            _ => return false,
        }
    }
    let same = |a: &Num, b: &Num| match (a, b) {
        (Num::Real(Real::Ball(x, _)), Num::Real(Real::Ball(y, _))) => x == y,
        (Num::Real(Real::Exact(x)), Num::Real(Real::Exact(y))) => x == y,
        _ => false,
    };
    ball && nums.len() == n * n && (0..n).all(|i| (0..i).all(|j| same(&nums[i * n + j], &nums[j * n + i])))
}

/// The eigenvalues of a real symmetric matrix of balls (and exact numbers), in the order of decreasing modulus
/// (then decreasing value), and with `vectors` the unit eigenvectors (largest entry positive), all as balls at the
/// input's precision; `None` when an entry is not a real ball or exact number, no entry is a ball, or the matrix
/// is not symmetric.
#[allow(clippy::type_complexity)]
pub(crate) fn ball_symmetric(nums: &[Num], n: usize, vectors: bool, m: &mut Meter) -> WorkResult<Option<(Vec<Num>, Vec<Vec<Num>>)>> {
    let mut prec = 0u64;
    for x in nums {
        match x {
            Num::Real(Real::Ball(_, p)) => prec = prec.max(*p),
            Num::Real(Real::Exact(_)) => {}
            _ => return Ok(None),
        }
    }
    if prec == 0 || n == 0 {
        return Ok(None);
    }
    let bits_n = 64 - (n as u64).leading_zeros() as i64;
    let w = prec + 64 + 4 * bits_n as u64;
    let mut mids = Vec::with_capacity(n * n);
    let mut maxrad = Mag::ZERO;
    for x in nums {
        let Num::Real(Real::Ball(b, _)) = (match x {
            Num::Real(r) => Num::Real(r.to_kind(Kind::Ball, w)),
            _ => return Ok(None),
        }) else {
            return Ok(None);
        };
        if b.rad.log2() > maxrad.log2() || maxrad.is_zero() {
            maxrad = b.rad;
        }
        mids.push(b.mid);
    }
    for i in 0..n {
        for j in 0..i {
            if mids[i * n + j].cmp_exact(&mids[j * n + i]) != Ordering::Equal {
                return Ok(None);
            }
        }
    }
    let (vals, q) = symmetric(&mids, n, P(w), vectors, m)?;
    // the error bound: Weyl with |E|_2 <= n max |e_ij|, and the method's rounding at w bits
    let top = mids.iter().map(Float::top).max().unwrap_or(0);
    let rounding = pow2(top - w as i64 + 2 * bits_n + 8);
    let rad = maxrad.mul(&Mag::from_float_up(&Float::from_i64(n as i64))).add(&rounding);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| vals[j].abs().cmp_exact(&vals[i].abs()).then(vals[j].cmp_exact(&vals[i])));
    let ball = |mid: &Float, r: Mag| Num::Real(Real::Ball(Ball { mid: Float::round(mid.neg, mid.man.clone(), mid.exp, w).0, rad: r }, prec));
    let out_vals: Vec<Num> = order.iter().map(|&k| ball(&vals[k], rad)).collect();
    let mut out_vecs = vec![];
    if vectors {
        for &k in &order {
            // the gap to the nearest other eigenvalue (sin-theta bound rad / gap)
            let gap = (0..n).filter(|&j| j != k).map(|j| vals[k].sub(&vals[j], 64).0.abs()).min_by(|a, b| a.cmp_exact(b));
            let vrad = match gap {
                None => rounding,
                Some(g) if g.is_zero() || (g.top() - 1) as f64 <= rad.log2() + 1.0 => Mag::from_float_up(&Float::from_i64(2)),
                Some(g) => pow2((rad.log2().ceil() as i64) - (g.top() - 1) + 1).add(&rounding),
            };
            let mut v: Vec<Float> = (0..n).map(|i| q[i * n + k].clone()).collect();
            // the largest entry positive
            let big = (0..n).max_by(|&a, &b| v[a].abs().cmp_exact(&v[b].abs())).unwrap_or(0);
            if v[big].neg {
                v = v.iter().map(Float::neg).collect();
            }
            out_vecs.push(v.iter().map(|x| ball(x, vrad)).collect());
        }
    }
    Ok(Some((out_vals, out_vecs)))
}
