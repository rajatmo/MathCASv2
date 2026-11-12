//! Dense univariate polynomials over the rationals: a coefficient vector, index = degree, without trailing zeros (the
//! zero polynomial is empty). The arithmetic of number fields and of polynomials over them is built from these;
//! factorisation over Q and resultants go through `mc-poly`'s integer kernels.
use mc_arith::num::{gcd::gcd as nat_gcd, Int, Nat, Rat};
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};

/// A polynomial over Q, index = degree, no trailing zeros.
pub type QPoly = Vec<Rat>;

/// The rational `n`.
pub fn q(n: i64) -> Rat {
    Rat::from_i64(n)
}

/// The rational `n / d` (d nonzero).
pub fn qq(n: i64, d: i64) -> Rat {
    Rat::new(Int::from_i64(n), Int::from_i64(d))
}

/// `a / b` for a nonzero `b`.
pub fn rdiv(a: &Rat, b: &Rat) -> Rat {
    // invariant: callers divide by nonzero rationals only
    a.mul(&b.recip().expect("a nonzero divisor"))
}

/// Remove trailing zeros.
pub fn trim(mut p: QPoly) -> QPoly {
    while p.last().is_some_and(|c| c.is_zero()) {
        p.pop();
    }
    p
}

/// The degree (`None` for the zero polynomial).
pub fn deg(p: &[Rat]) -> Option<usize> {
    p.len().checked_sub(1)
}

/// `a + b`.
pub fn add(a: &[Rat], b: &[Rat]) -> QPoly {
    let n = a.len().max(b.len());
    let z = q(0);
    trim((0..n).map(|i| a.get(i).unwrap_or(&z).add(b.get(i).unwrap_or(&z))).collect())
}

/// `a - b`.
pub fn sub(a: &[Rat], b: &[Rat]) -> QPoly {
    let n = a.len().max(b.len());
    let z = q(0);
    trim((0..n).map(|i| a.get(i).unwrap_or(&z).sub(b.get(i).unwrap_or(&z))).collect())
}

/// `c a`.
pub fn scale(a: &[Rat], c: &Rat) -> QPoly {
    if c.is_zero() {
        return vec![];
    }
    a.iter().map(|x| x.mul(c)).collect()
}

/// `a b`.
pub fn mul(a: &[Rat], b: &[Rat], m: &mut Meter) -> WorkResult<QPoly> {
    if a.is_empty() || b.is_empty() {
        return Ok(vec![]);
    }
    m.step((a.len() * b.len()) as u64)?;
    let mut out = vec![q(0); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        if x.is_zero() {
            continue;
        }
        for (j, y) in b.iter().enumerate() {
            if !y.is_zero() {
                out[i + j] = out[i + j].add(&x.mul(y));
            }
        }
    }
    Ok(trim(out))
}

/// Quotient and remainder of `a` by a nonzero `b`.
pub fn divrem(a: &[Rat], b: &[Rat], m: &mut Meter) -> WorkResult<(QPoly, QPoly)> {
    let mut r = trim(a.to_vec());
    // invariant: callers divide by nonzero polynomials
    let lead = b.last().expect("a nonzero divisor").recip().expect("a trimmed divisor");
    if r.len() < b.len() {
        return Ok((vec![], r));
    }
    let mut quo = vec![q(0); r.len() + 1 - b.len()];
    while r.len() >= b.len() {
        m.step(b.len() as u64)?;
        let k = r.len() - b.len();
        // invariant: the loop runs while the remainder is at least as long as the divisor
        let c = r.last().expect("nonempty").mul(&lead);
        for (i, bi) in b.iter().enumerate() {
            if !bi.is_zero() {
                r[k + i] = r[k + i].sub(&c.mul(bi));
            }
        }
        quo[k] = c;
        r.pop();
        r = trim(r);
    }
    Ok((trim(quo), r))
}

/// The remainder of `a` by a nonzero `b`.
pub fn rem(a: &[Rat], b: &[Rat], m: &mut Meter) -> WorkResult<QPoly> {
    Ok(divrem(a, b, m)?.1)
}

/// `a` made monic (the zero polynomial stays).
pub fn monic(a: &[Rat]) -> QPoly {
    match a.last() {
        Some(l) => {
            let c = l.recip().expect("a trimmed polynomial");
            a.iter().map(|x| x.mul(&c)).collect()
        }
        None => vec![],
    }
}

/// The monic gcd (0 for two zeros).
pub fn gcd(a: &[Rat], b: &[Rat], m: &mut Meter) -> WorkResult<QPoly> {
    // through the integer modular gcd: the Euclidean algorithm over Q has coefficient growth
    if a.is_empty() {
        return Ok(monic(b));
    }
    if b.is_empty() {
        return Ok(monic(a));
    }
    let g = mc_poly::gcd::gcd(&to_poly(a), &to_poly(b), m)?;
    Ok(monic(&from_poly(&g)))
}

/// `(g, s, t)` with `s a + t b = g` monic.
pub fn xgcd(a: &[Rat], b: &[Rat], m: &mut Meter) -> WorkResult<(QPoly, QPoly, QPoly)> {
    let (mut r0, mut r1) = (trim(a.to_vec()), trim(b.to_vec()));
    let (mut s0, mut s1) = (vec![q(1)], vec![]);
    let (mut t0, mut t1) = (vec![], vec![q(1)]);
    while !r1.is_empty() {
        let (quo, r) = divrem(&r0, &r1, m)?;
        let s = sub(&s0, &mul(&quo, &s1, m)?);
        let t = sub(&t0, &mul(&quo, &t1, m)?);
        (r0, r1) = (r1, r);
        (s0, s1) = (s1, s);
        (t0, t1) = (t1, t);
    }
    let Some(l) = r0.last() else { return Ok((vec![], vec![], vec![])) };
    let c = l.recip().expect("a trimmed polynomial");
    Ok((scale(&r0, &c), scale(&s0, &c), scale(&t0, &c)))
}

/// The derivative.
pub fn derivative(a: &[Rat]) -> QPoly {
    trim(a.iter().enumerate().skip(1).map(|(i, c)| c.mul(&q(i as i64))).collect())
}

/// `a(x)` at a rational.
pub fn eval(a: &[Rat], x: &Rat) -> Rat {
    let mut acc = q(0);
    for c in a.iter().rev() {
        acc = acc.mul(x).add(c);
    }
    acc
}

/// `a(x + c)` (Horner's shift).
pub fn shift(a: &[Rat], c: &Rat) -> QPoly {
    let mut out = a.to_vec();
    let n = out.len();
    for i in 0..n {
        for j in (i..n - 1).rev() {
            let t = out[j + 1].mul(c);
            out[j] = out[j].add(&t);
        }
    }
    trim(out)
}

/// The lcm of the denominators and the integer polynomial `lcm * a` (exact, not primitive).
pub fn clear(a: &[Rat]) -> (Nat, Vec<Int>) {
    let mut l = Nat::from_u64(1);
    for c in a {
        let g = nat_gcd(&l, &c.den);
        l = l.mul(&c.den.divrem(&g).0);
    }
    let li = Int::from_nat(l.clone());
    let out = a.iter().map(|c| c.num.mul(&Int::from_nat(li.mag.divrem(&c.den).0))).collect();
    (l, out)
}

/// The primitive integer polynomial with positive leading coefficient that is a rational multiple of `a`.
pub fn primitive(a: &[Rat]) -> Vec<Int> {
    let (_, ints) = clear(a);
    let mut g = Nat::zero();
    for c in &ints {
        g = nat_gcd(&g, &c.mag);
    }
    if g.is_zero() {
        return vec![];
    }
    let neg = ints.last().is_some_and(|c| c.neg);
    ints.iter()
        .map(|c| {
            let (qn, _) = c.mag.divrem(&g);
            Int::new(c.neg != neg && !qn.is_zero(), qn)
        })
        .collect()
}

/// `a` as a univariate `mc-poly` polynomial (a rational multiple: the primitive integer form).
pub fn to_poly(a: &[Rat]) -> Poly {
    let zs: Vec<Z> = primitive(a).into_iter().map(Z::from_int).collect();
    Poly::from_dense(1, 0, &zs)
}

/// A univariate `mc-poly` polynomial as a rational polynomial.
pub fn from_poly(p: &Poly) -> QPoly {
    trim(p.to_dense(0).iter().map(|z| Rat::from_int(z.to_int())).collect())
}

/// Integers as a rational polynomial.
pub fn from_ints(a: &[Int]) -> QPoly {
    trim(a.iter().map(|c| Rat::from_int(c.clone())).collect())
}

/// The distinct monic irreducible factors of positive degree with their multiplicities.
pub fn factor(a: &[Rat], m: &mut Meter) -> WorkResult<Vec<(QPoly, u32)>> {
    if deg(a).unwrap_or(0) == 0 {
        return Ok(vec![]);
    }
    let fs = mc_poly::factor::factor(&to_poly(a), false, m)?;
    Ok(fs.factors.iter().map(|(f, e)| (monic(&from_poly(f)), *e)).collect())
}

/// The monic square-free part.
pub fn square_free(a: &[Rat], m: &mut Meter) -> WorkResult<QPoly> {
    let g = gcd(a, &derivative(a), m)?;
    Ok(monic(&divrem(a, &g, m)?.0))
}

/// Is `a` square-free?
pub fn is_square_free(a: &[Rat], m: &mut Meter) -> WorkResult<bool> {
    Ok(deg(&gcd(a, &derivative(a), m)?).unwrap_or(0) == 0)
}

/// A machine-integer polynomial as a rational one.
pub fn of_i64(a: &[i64]) -> QPoly {
    trim(a.iter().map(|&c| q(c)).collect())
}

/// The total size of the coefficients in bits (a measure for choosing between representations).
pub fn bit_size(a: &[Rat]) -> u64 {
    a.iter().map(|c| c.num.mag.bits() + c.den.bits()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_poly::work::Unlimited;

    #[test]
    fn arithmetic() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let a = of_i64(&[-1, 0, 1]);
        let b = of_i64(&[1, 1]);
        let (qu, r) = divrem(&a, &b, &mut m).unwrap();
        assert_eq!(qu, of_i64(&[-1, 1]));
        assert!(r.is_empty());
        assert_eq!(gcd(&a, &of_i64(&[-1, 1]), &mut m).unwrap(), of_i64(&[-1, 1]));
        assert_eq!(shift(&of_i64(&[0, 0, 1]), &q(1)), of_i64(&[1, 2, 1]));
        let (g, s, t) = xgcd(&of_i64(&[1, 0, 1]), &of_i64(&[0, 1]), &mut m).unwrap();
        assert_eq!(g, of_i64(&[1]));
        let lhs = add(&mul(&s, &of_i64(&[1, 0, 1]), &mut m).unwrap(), &mul(&t, &of_i64(&[0, 1]), &mut m).unwrap());
        assert_eq!(lhs, g);
        assert_eq!(primitive(&[qq(1, 2), qq(-3, 4)]), vec![Int::from_i64(-2), Int::from_i64(3)]);
    }
}
