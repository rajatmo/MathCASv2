//! The number field `Q[t]/(f)` of an irreducible polynomial `f`: the exact eigenvectors of an eigenvalue that is a
//! root of `f` are a null space over this field (decision LA.11). An element is its coefficient vector in the
//! power basis `1, t, ..., t^(d-1)`.
use crate::elim::{Field, Weight};
use mc_arith::num::Rat;
use std::sync::Arc;

/// An element of `Q[t]/(f)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Alg {
    /// coefficients of `1, t, ..., t^(d-1)`
    pub c: Vec<Rat>,
    /// the monic modulus `f`: coefficients of `1, t, ..., t^d`
    pub f: Arc<Vec<Rat>>,
}

fn zero() -> Rat {
    Rat::from_i64(0)
}

impl Alg {
    /// The rational `q` in the field of `f`.
    pub fn rational(q: Rat, f: &Arc<Vec<Rat>>) -> Alg {
        let d = f.len() - 1;
        let mut c = vec![zero(); d];
        c[0] = q;
        Alg { c, f: f.clone() }
    }
    /// The generator `t`.
    pub fn generator(f: &Arc<Vec<Rat>>) -> Alg {
        let d = f.len() - 1;
        let mut c = vec![zero(); d];
        if d == 1 {
            // t = -f0 in a linear field
            c[0] = f[0].neg();
        } else {
            c[1] = Rat::from_i64(1);
        }
        Alg { c, f: f.clone() }
    }
    fn is_zero(&self) -> bool {
        self.c.iter().all(|x| x.is_zero())
    }
    /// The reduction of the polynomial `p` (any degree) modulo `f`.
    fn reduce(mut p: Vec<Rat>, f: &Arc<Vec<Rat>>) -> Alg {
        let d = f.len() - 1;
        while p.len() > d {
            // invariant: the loop runs while `p` is longer than the modulus's degree, so it is nonempty
            let top = p.pop().expect("a coefficient");
            if top.is_zero() {
                continue;
            }
            let k = p.len() - d;
            for (i, fi) in f[..d].iter().enumerate() {
                p[k + i] = p[k + i].sub(&top.mul(fi));
            }
        }
        p.resize(d, zero());
        Alg { c: p, f: f.clone() }
    }
    /// The inverse by the extended Euclidean algorithm in `Q[t]` (`f` irreducible, so every nonzero element is a
    /// unit).
    fn inverse(&self) -> Option<Alg> {
        if self.is_zero() {
            return None;
        }
        // invariant: s * self == r (mod f)
        let (mut r0, mut r1) = (trim(self.f.to_vec()), trim(self.c.clone()));
        let (mut s0, mut s1) = (vec![], vec![Rat::from_i64(1)]);
        while r1.len() > 1 {
            let (q, r) = divrem(&r0, &r1);
            let s = sub(&s0, &mul(&q, &s1));
            (r0, r1) = (r1, r);
            (s0, s1) = (s1, s);
        }
        // r1 is a nonzero constant
        let c = r1.first()?.recip()?;
        Some(Alg::reduce(s1.iter().map(|x| x.mul(&c)).collect(), &self.f))
    }
}

fn trim(mut p: Vec<Rat>) -> Vec<Rat> {
    while p.last().is_some_and(|x| x.is_zero()) {
        p.pop();
    }
    p
}

fn mul(a: &[Rat], b: &[Rat]) -> Vec<Rat> {
    if a.is_empty() || b.is_empty() {
        return vec![];
    }
    let mut out = vec![zero(); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        if x.is_zero() {
            continue;
        }
        for (j, y) in b.iter().enumerate() {
            out[i + j] = out[i + j].add(&x.mul(y));
        }
    }
    trim(out)
}

fn sub(a: &[Rat], b: &[Rat]) -> Vec<Rat> {
    let n = a.len().max(b.len());
    trim((0..n).map(|i| a.get(i).cloned().unwrap_or_else(zero).sub(&b.get(i).cloned().unwrap_or_else(zero))).collect())
}

/// Quotient and remainder of `a` by the nonzero `b`.
fn divrem(a: &[Rat], b: &[Rat]) -> (Vec<Rat>, Vec<Rat>) {
    let mut r = trim(a.to_vec());
    // invariant: callers divide by a nonzero, trimmed polynomial: its last coefficient is nonzero
    let lead = b.last().expect("a nonzero divisor").recip().expect("a nonzero leading coefficient");
    if r.len() < b.len() {
        return (vec![], r);
    }
    let mut q = vec![zero(); r.len() + 1 - b.len()];
    while r.len() >= b.len() && !r.is_empty() {
        let k = r.len() - b.len();
        // invariant: the loop runs while the remainder is at least as long as the divisor
        let c = r.last().expect("nonempty").mul(&lead);
        for (i, bi) in b.iter().enumerate() {
            r[k + i] = r[k + i].sub(&c.mul(bi));
        }
        q[k] = c;
        r.pop();
        r = trim(r);
    }
    (trim(q), r)
}

impl Field for Alg {
    fn zero_like(&self) -> Alg {
        Alg { c: vec![zero(); self.c.len()], f: self.f.clone() }
    }
    fn one_like(&self) -> Alg {
        Alg::rational(Rat::from_i64(1), &self.f)
    }
    fn add(&self, o: &Alg) -> Alg {
        Alg { c: self.c.iter().zip(&o.c).map(|(a, b)| a.add(b)).collect(), f: self.f.clone() }
    }
    fn sub(&self, o: &Alg) -> Alg {
        Alg { c: self.c.iter().zip(&o.c).map(|(a, b)| a.sub(b)).collect(), f: self.f.clone() }
    }
    fn mul(&self, o: &Alg) -> Alg {
        Alg::reduce(mul(&self.c, &o.c), &self.f)
    }
    fn div(&self, o: &Alg) -> Option<Alg> {
        Some(self.mul(&o.inverse()?))
    }
    fn weight(&self, _tol: f64) -> Weight {
        if self.is_zero() {
            Weight::Zero
        } else {
            Weight::Pivot(1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_of_sqrt2() {
        // t^2 - 2
        let f = Arc::new(vec![Rat::from_i64(-2), zero(), Rat::from_i64(1)]);
        let t = Alg::generator(&f);
        let two = t.mul(&t);
        assert_eq!(two, Alg::rational(Rat::from_i64(2), &f));
        // (1 + t)^-1 = t - 1
        let one = t.one_like();
        let inv = one.div(&one.add(&t)).unwrap();
        assert_eq!(inv, t.sub(&one));
    }
}
