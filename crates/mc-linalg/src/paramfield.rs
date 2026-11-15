//! Eigenvectors of an eigenvalue whose minimal polynomial has coefficients in parameters.
//!
//! The eigenvalue `t` is a root of an irreducible factor `g` of the characteristic polynomial whose coefficients
//! are integer polynomials in the kernels of the entries (the parameters). `g` is irreducible in `Z[params][t]` and
//! of positive degree in `t`, so by Gauss's lemma it is irreducible in `K[t]` with `K = Q(params)`, the field of
//! rational functions, and `K[t]/(g)` is a field. The eigenvectors of `t` are the null space of `m - t I` over that
//! field: Gauss-Jordan elimination there gives the reduced row echelon form, which is unique, so the null basis
//! does not depend on the pivots chosen.
//!
//! Elements are polynomials in `t` of degree below `deg g` with coefficients in `K` (`mc-poly`'s [`RatFun`], kept in
//! lowest terms); products are reduced by the monic `g`, inverses come from the extended Euclidean algorithm in
//! `K[t]` (von zur Gathen and Gerhard, Modern Computer Algebra, 3rd ed. 2013, algorithm 3.14 and section 4.2).
//! Zero is decided as the zero rational function, which is right because the kernels are independent (a ring of
//! interacting kernels is declined by the caller). The answer is the generic one: it holds for the parameter
//! values where no denominator vanishes.
use mc_poly::poly::Poly;
use mc_poly::ratfun::RatFun;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::z::Z;

/// The field `K[t]/(g)` of a monic `g` of degree at least 1 over the rational functions in `nv` variables.
pub struct PField {
    /// the monic modulus, lowest degree first (length `deg g + 1`)
    g: Vec<RatFun>,
    nv: usize,
}

/// An element: the coefficients of `1, t, ..., t^(d-1)`.
pub type El = Vec<RatFun>;

fn rzero(nv: usize) -> RatFun {
    RatFun::constant(nv, Z::ZERO, Z::ONE)
}

fn rneg(a: &RatFun) -> RatFun {
    RatFun { num: a.num.neg(), den: a.den.clone() }
}

fn rsub(a: &RatFun, b: &RatFun, m: &mut Meter) -> WorkResult<RatFun> {
    if b.num.is_zero() {
        return Ok(a.clone());
    }
    a.add(&rneg(b), m)
}

/// `a / b`; `None` when `b` is zero.
fn rdiv(a: &RatFun, b: &RatFun, m: &mut Meter) -> WorkResult<Option<RatFun>> {
    match b.inv(1, m)? {
        Ok(i) => Ok(Some(a.mul(&i, m)?)),
        Err(_) => Ok(None),
    }
}

/// Drops the zero coefficients at the top of a polynomial in `t`.
fn trim(mut p: Vec<RatFun>) -> Vec<RatFun> {
    while p.last().is_some_and(|c| c.num.is_zero()) {
        p.pop();
    }
    p
}

impl PField {
    /// The field of the polynomial with coefficients `coefs` (lowest degree first, integer polynomials in the
    /// parameters, the leading one nonzero and the degree at least 1).
    pub fn new(coefs: &[Poly], m: &mut Meter) -> WorkResult<Option<PField>> {
        let Some(lead) = coefs.last().filter(|c| !c.is_zero()) else { return Ok(None) };
        if coefs.len() < 2 {
            return Ok(None);
        }
        let nv = lead.nvars();
        let mut g = Vec::with_capacity(coefs.len());
        for c in coefs {
            g.push(RatFun::reduced(c.clone(), lead.clone(), m)?);
        }
        Ok(Some(PField { g, nv }))
    }

    /// The degree of the modulus.
    pub fn degree(&self) -> usize {
        self.g.len() - 1
    }

    /// Zero.
    pub fn zero(&self) -> El {
        vec![rzero(self.nv); self.degree()]
    }

    /// One.
    pub fn one(&self) -> El {
        let mut e = self.zero();
        e[0] = RatFun::constant(self.nv, Z::ONE, Z::ONE);
        e
    }

    /// Is the element zero?
    pub fn is_zero(e: &El) -> bool {
        e.iter().all(|c| c.num.is_zero())
    }

    /// The element of a polynomial in `t` (any degree) with rational-function coefficients.
    pub fn reduce(&self, mut p: Vec<RatFun>, m: &mut Meter) -> WorkResult<El> {
        let d = self.degree();
        for k in (d..p.len()).rev() {
            m.step(1)?;
            let c = p[k].clone();
            if c.num.is_zero() {
                continue;
            }
            // t^k = t^(k-d) t^d and t^d = -(g_0 + ... + g_(d-1) t^(d-1))
            for j in 0..d {
                if self.g[j].num.is_zero() {
                    continue;
                }
                p[k - d + j] = rsub(&p[k - d + j], &c.mul(&self.g[j], m)?, m)?;
            }
            p[k] = rzero(self.nv);
        }
        p.resize(d.max(p.len()), rzero(self.nv));
        p.truncate(d);
        Ok(p)
    }

    /// `a + b`.
    pub fn add(&self, a: &El, b: &El, m: &mut Meter) -> WorkResult<El> {
        a.iter().zip(b).map(|(x, y)| if y.num.is_zero() { Ok(x.clone()) } else { x.add(y, m) }).collect()
    }

    /// `a - b`.
    pub fn sub(&self, a: &El, b: &El, m: &mut Meter) -> WorkResult<El> {
        a.iter().zip(b).map(|(x, y)| rsub(x, y, m)).collect()
    }

    /// `-a`.
    pub fn neg(a: &El) -> El {
        a.iter().map(rneg).collect()
    }

    /// `a b`.
    pub fn mul(&self, a: &El, b: &El, m: &mut Meter) -> WorkResult<El> {
        let mut p = vec![rzero(self.nv); a.len() + b.len()];
        for (i, x) in a.iter().enumerate() {
            if x.num.is_zero() {
                continue;
            }
            for (j, y) in b.iter().enumerate() {
                if y.num.is_zero() {
                    continue;
                }
                p[i + j] = p[i + j].add(&x.mul(y, m)?, m)?;
            }
        }
        self.reduce(p, m)
    }

    /// Quotient and remainder of polynomials in `t` (`b` nonzero).
    fn divrem(&self, a: &[RatFun], b: &[RatFun], m: &mut Meter) -> WorkResult<Option<(Vec<RatFun>, Vec<RatFun>)>> {
        let b = trim(b.to_vec());
        let Some(lb) = b.last() else { return Ok(None) };
        let mut r = trim(a.to_vec());
        let mut q = vec![rzero(self.nv); r.len().saturating_sub(b.len()) + 1];
        while r.len() >= b.len() {
            m.step(1)?;
            let Some(c) = rdiv(r.last().expect("nonempty"), lb, m)? else { return Ok(None) };
            let shift = r.len() - b.len();
            for (j, bj) in b.iter().enumerate() {
                if bj.num.is_zero() {
                    continue;
                }
                r[shift + j] = rsub(&r[shift + j], &c.mul(bj, m)?, m)?;
            }
            q[shift] = c;
            // the leading coefficient cancels exactly
            r.pop();
            r = trim(r);
        }
        Ok(Some((q, r)))
    }

    /// The inverse of a nonzero element; `None` when the modulus turns out not to be irreducible (a common factor
    /// with the element).
    pub fn inv(&self, a: &El, m: &mut Meter) -> WorkResult<Option<El>> {
        let mut r0 = self.g.clone();
        let mut r1 = trim(a.clone());
        if r1.is_empty() {
            return Ok(None);
        }
        // invariant: s_i a = r_i (mod g)
        let mut s0: Vec<RatFun> = vec![];
        let mut s1: Vec<RatFun> = vec![RatFun::constant(self.nv, Z::ONE, Z::ONE)];
        while !r1.is_empty() {
            m.step(1)?;
            let Some((q, r)) = self.divrem(&r0, &r1, m)? else { return Ok(None) };
            let qs = poly_mul(&q, &s1, self.nv, m)?;
            let mut s2 = s0.clone();
            s2.resize(s2.len().max(qs.len()), rzero(self.nv));
            for (k, c) in qs.iter().enumerate() {
                s2[k] = rsub(&s2[k], c, m)?;
            }
            (r0, r1) = (r1, r);
            (s0, s1) = (s1, trim(s2));
        }
        // r0 is the gcd of a and g: a nonzero constant when g is irreducible
        if r0.len() != 1 {
            return Ok(None);
        }
        let Some(c) = r0[0].inv(1, m)?.ok() else { return Ok(None) };
        let s: Vec<RatFun> = s0.iter().map(|x| x.mul(&c, m)).collect::<WorkResult<_>>()?;
        Ok(Some(self.reduce(s, m)?))
    }

    /// The reduced row echelon form of a `rows x cols` matrix (row-major) and its pivot columns.
    pub fn rref(&self, mut a: Vec<El>, rows: usize, cols: usize, m: &mut Meter) -> WorkResult<Option<(Vec<El>, Vec<usize>)>> {
        let mut pivots = vec![];
        let mut r = 0;
        for c in 0..cols {
            if r == rows {
                break;
            }
            m.step((rows * cols) as u64 + 1)?;
            let Some(p) = (r..rows).find(|&i| !Self::is_zero(&a[i * cols + c])) else { continue };
            if p != r {
                for j in 0..cols {
                    a.swap(r * cols + j, p * cols + j);
                }
            }
            let Some(inv) = self.inv(&a[r * cols + c], m)? else { return Ok(None) };
            for j in 0..cols {
                a[r * cols + j] = if j == c { self.one() } else { self.mul(&a[r * cols + j], &inv, m)? };
            }
            for i in (0..rows).filter(|&i| i != r) {
                let f = a[i * cols + c].clone();
                if Self::is_zero(&f) {
                    continue;
                }
                for j in 0..cols {
                    let y = a[r * cols + j].clone();
                    if Self::is_zero(&y) {
                        continue;
                    }
                    let fy = self.mul(&f, &y, m)?;
                    a[i * cols + j] = self.sub(&a[i * cols + j], &fy, m)?;
                }
            }
            pivots.push(c);
            r += 1;
        }
        Ok(Some((a, pivots)))
    }

    /// A null-space basis from the reduced row echelon form of an `n`-column matrix: one vector per free column
    /// (that entry one), from the last free column to the first.
    pub fn null_basis(&self, r: &[El], pivots: &[usize], n: usize) -> Vec<Vec<El>> {
        let mut free: Vec<usize> = (0..n).filter(|c| !pivots.contains(c)).collect();
        free.reverse();
        free.iter()
            .map(|&fc| {
                let mut v = vec![self.zero(); n];
                v[fc] = self.one();
                for (i, &p) in pivots.iter().enumerate() {
                    v[p] = Self::neg(&r[i * n + fc]);
                }
                v
            })
            .collect()
    }
}

/// The product of two polynomials in `t` (no reduction).
fn poly_mul(a: &[RatFun], b: &[RatFun], nv: usize, m: &mut Meter) -> WorkResult<Vec<RatFun>> {
    if a.is_empty() || b.is_empty() {
        return Ok(vec![]);
    }
    let mut p = vec![rzero(nv); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        if x.num.is_zero() {
            continue;
        }
        for (j, y) in b.iter().enumerate() {
            if y.num.is_zero() {
                continue;
            }
            p[i + j] = p[i + j].add(&x.mul(y, m)?, m)?;
        }
    }
    Ok(trim(p))
}

/// An element over one common denominator: `(sum_j n_j t^j) / den` with integer polynomials `n_j` and `den`
/// sharing no factor and `den` with a positive leading coefficient.
pub fn common_denominator(e: &El, m: &mut Meter) -> WorkResult<(Vec<Poly>, Poly)> {
    let nv = e.first().map_or(0, |c| c.num.nvars());
    let mut l = Poly::constant(nv, Z::ONE);
    for c in e {
        if c.num.is_zero() {
            continue;
        }
        let g = mc_poly::gcd::gcd(&l, &c.den, m)?;
        let q = mc_poly::work::exact(c.den.div_exact(&g, m)?, "a gcd divides")?;
        l = l.mul(&q, m)?;
    }
    let mut nums = Vec::with_capacity(e.len());
    for c in e {
        if c.num.is_zero() {
            nums.push(Poly::zero(nv));
            continue;
        }
        let q = mc_poly::work::exact(l.div_exact(&c.den, m)?, "the common denominator is a multiple")?;
        nums.push(c.num.mul(&q, m)?);
    }
    Ok((nums, l))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_poly::work::Unlimited;

    fn c(nv: usize, k: i64) -> Poly {
        Poly::constant(nv, Z::S(k))
    }

    #[test]
    fn inverse_in_a_field_over_a_parameter() {
        // K = Q(a), g = t^2 - a: (1 + t)^-1 = (t - 1)/(a - 1)
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let a = Poly::var(1, 0);
        let k = PField::new(&[a.neg(), c(1, 0), c(1, 1)], &mut m).unwrap().unwrap();
        let one_t = vec![RatFun::constant(1, Z::ONE, Z::ONE), RatFun::constant(1, Z::ONE, Z::ONE)];
        let inv = k.inv(&one_t, &mut m).unwrap().unwrap();
        let prod = k.mul(&one_t, &inv, &mut m).unwrap();
        assert_eq!(prod, k.one());
    }
}
