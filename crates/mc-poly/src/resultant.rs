//! Resultants and discriminants (decision K.9).
//!
//! In one variable over the integers the resultant is computed modulo enough word-sized primes to exceed twice
//! Hadamard's bound on the Sylvester determinant (Euclid over each prime field), then reassembled by Chinese
//! remaindering. With other variables present, the coefficients live in Z[other variables] and the subresultant
//! algorithm (Collins, Brown; Cohen's Algorithm 3.3.7) computes it with exact divisions only.
use crate::modp::{self, Fp, Primes};
use crate::poly::Poly;
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use mc_arith::num::{Int, Nat};

/// The resultant of a and b with respect to variable v (a polynomial in the other variables).
pub fn resultant(a: &Poly, b: &Poly, v: usize, m: &mut Meter) -> WorkResult<Poly> {
    let nv = a.nv;
    if a.is_zero() || b.is_zero() {
        return Ok(Poly::zero(nv));
    }
    let only_v = |p: &Poly| p.used_vars().iter().all(|&u| u == v);
    if only_v(a) && only_v(b) {
        let r = modular(&a.to_dense(v), &b.to_dense(v), m)?;
        return Ok(Poly::constant(nv, r));
    }
    subresultant(a, b, v, m)
}

/// Hadamard's bound on |res(a, b)| in bits: deg b log2 |a|_2 + deg a log2 |b|_2 (rounded up).
fn hadamard_bits(a: &[Z], b: &[Z]) -> u64 {
    let norm_bits = |p: &[Z]| {
        // log2 of the 2-norm: half the bits of the sum of squares
        let s = p.iter().fold(Z::ZERO, |acc, c| acc.add(&c.mul(c)));
        s.bits().div_ceil(2) + 1
    };
    let (n, k) = (a.len() as u64 - 1, b.len() as u64 - 1);
    k * norm_bits(a) + n * norm_bits(b) + 2
}

/// The resultant of two dense integer polynomials by primes and Chinese remaindering.
fn modular(a: &[Z], b: &[Z], m: &mut Meter) -> WorkResult<Z> {
    let (n, k) = (a.len() - 1, b.len() - 1);
    if n == 0 {
        return Ok(a[0].pow(k as u64));
    }
    if k == 0 {
        return Ok(b[0].pow(n as u64));
    }
    let need = hadamard_bits(a, b) + 1;
    let mut x = Int::default();
    let mut modulus = Nat::from_u64(1);
    for p in Primes::large() {
        if modulus.bits() > need {
            break;
        }
        // a prime dividing a leading coefficient would change the degrees
        if a[n].rem_u64(p) == 0 || b[k].rem_u64(p) == 0 {
            continue;
        }
        let f = Fp::new(p);
        let red = |c: &[Z]| c.iter().map(|z| z.rem_u64(p)).collect::<Vec<u64>>();
        let r = modp::resultant(f, &red(a), &red(b), m)?;
        x = crt(&x, &modulus, r, p);
        modulus = modulus.mul_u64(p);
    }
    Ok(Z::from_int(x))
}

/// The symmetric value of (x mod modulus, r mod p) mod modulus * p.
fn crt(x: &Int, modulus: &Nat, r: u64, p: u64) -> Int {
    let f = Fp::new(p);
    let xz = Z::from_int(x.clone());
    let xr = xz.rem_u64(p);
    if xr == r && !modulus.is_zero() {
        return x.clone();
    }
    let t = f.mul(f.sub(r, xr), f.inv(modulus.divrem_u64(p).1));
    let v = x.add(&Int::from_nat(modulus.mul_u64(t)));
    let new_mod = modulus.mul_u64(p);
    if !v.neg && v.mag > new_mod.shr(1) {
        v.sub(&Int::from_nat(new_mod))
    } else {
        v
    }
}

/// Coefficients in variable v: index = degree, each a polynomial in the other variables.
fn coeffs(p: &Poly, v: usize) -> Vec<Poly> {
    let mut out = vec![Poly::zero(p.nv); p.degree(v).map_or(0, |d| d as usize + 1)];
    for (k, c) in p.coeffs_in(v) {
        out[k as usize] = c;
    }
    out
}

fn trim(a: &mut Vec<Poly>) {
    while a.last().is_some_and(Poly::is_zero) {
        a.pop();
    }
}

/// The pseudo-remainder lc(b)^(deg a - deg b + 1) a mod b.
fn prem(a: &[Poly], b: &[Poly], m: &mut Meter) -> WorkResult<Vec<Poly>> {
    let (db, lb) = (b.len() - 1, &b[b.len() - 1]);
    let mut r = a.to_vec();
    let mut steps = (a.len() - b.len() + 1) as u64;
    while r.len() >= b.len() {
        let lr = r[r.len() - 1].clone();
        let shift = r.len() - 1 - db;
        for c in r.iter_mut() {
            *c = c.mul(lb, m)?;
        }
        for (j, bj) in b.iter().enumerate() {
            r[shift + j] = r[shift + j].sub(&bj.mul(&lr, m)?);
        }
        steps -= 1;
        trim(&mut r);
    }
    // the factors of lc(b) not spent by the loop
    if steps > 0 {
        let s = lb.pow(steps, m)?;
        for c in r.iter_mut() {
            *c = c.mul(&s, m)?;
        }
    }
    Ok(r)
}

fn content(a: &[Poly], m: &mut Meter) -> WorkResult<Poly> {
    let mut g = Poly::zero(a[0].nv);
    for c in a {
        g = crate::gcd::gcd(&g, c, m)?;
    }
    Ok(g)
}

fn div_all(a: &[Poly], d: &Poly, m: &mut Meter) -> WorkResult<Vec<Poly>> {
    a.iter().map(|c| crate::work::exact(c.div_exact(d, m)?, "an exact division in the subresultant algorithm")).collect()
}

/// The subresultant algorithm over Z[other variables].
fn subresultant(a: &Poly, b: &Poly, v: usize, m: &mut Meter) -> WorkResult<Poly> {
    let nv = a.nv;
    let one = Poly::constant(nv, Z::ONE);
    let (mut a, mut b) = (coeffs(a, v), coeffs(b, v));
    let mut s = 1i32;
    if a.len() < b.len() {
        if (a.len() - 1) % 2 == 1 && (b.len() - 1) % 2 == 1 {
            s = -s;
        }
        std::mem::swap(&mut a, &mut b);
    }
    if b.len() == 1 {
        return b[0].pow((a.len() - 1) as u64, m);
    }
    let (ca, cb) = (content(&a, m)?, content(&b, m)?);
    let t = ca.pow((b.len() - 1) as u64, m)?.mul(&cb.pow((a.len() - 1) as u64, m)?, m)?;
    a = div_all(&a, &ca, m)?;
    b = div_all(&b, &cb, m)?;
    let (mut g, mut h) = (one.clone(), one.clone());
    loop {
        m.step(1)?;
        let delta = (a.len() - b.len()) as u64;
        if (a.len() - 1) % 2 == 1 && (b.len() - 1) % 2 == 1 {
            s = -s;
        }
        let r = prem(&a, &b, m)?;
        if r.is_empty() {
            return Ok(Poly::zero(nv));
        }
        a = b;
        let d = g.mul(&h.pow(delta, m)?, m)?;
        b = div_all(&r, &d, m)?;
        g = a[a.len() - 1].clone();
        // h = g^delta / h^(delta - 1)
        h = if delta == 0 { h } else { crate::work::exact(g.pow(delta, m)?.div_exact(&h.pow(delta - 1, m)?, m)?, "the subresultant h divides")? };
        if b.len() == 1 {
            let da = (a.len() - 1) as u64;
            // h = lc(b)^deg(a) / h^(deg(a) - 1)
            let hh = crate::work::exact(b[0].pow(da, m)?.div_exact(&h.pow(da - 1, m)?, m)?, "the subresultant h divides")?;
            let r = hh.mul(&t, m)?;
            return Ok(if s < 0 { r.neg() } else { r });
        }
    }
}

/// The discriminant of a with respect to v: (-1)^(n(n-1)/2) res(a, a') / lc(a).
pub fn discriminant(a: &Poly, v: usize, m: &mut Meter) -> WorkResult<Poly> {
    let nv = a.nv;
    let n = a.degree(v).unwrap_or(0) as u64;
    if n == 0 {
        return Ok(Poly::zero(nv));
    }
    if n == 1 {
        return Ok(Poly::constant(nv, Z::ONE));
    }
    let r = resultant(a, &a.derivative(v), v, m)?;
    let lc = coeffs(a, v).pop().expect("positive degree");
    let q = r.div_exact(&lc, m)?.expect("the leading coefficient divides the resultant");
    Ok(if (n * (n - 1) / 2) % 2 == 1 { q.neg() } else { q })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::Unlimited;

    #[test]
    fn small_resultants() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let d = |c: &[i64]| Poly::from_dense(1, 0, &c.iter().map(|&x| Z::S(x)).collect::<Vec<_>>());
        // res(x^2 - 2, x - 1) = -1, res(x^2 + 1, x^2 - 1) = 4
        assert_eq!(resultant(&d(&[-2, 0, 1]), &d(&[-1, 1]), 0, &mut m).unwrap(), d(&[-1]));
        assert_eq!(resultant(&d(&[1, 0, 1]), &d(&[-1, 0, 1]), 0, &mut m).unwrap(), d(&[4]));
        // the discriminant of a x^2 + b x + c is b^2 - 4 a c
        assert_eq!(discriminant(&d(&[3, 5, 2]), 0, &mut m).unwrap(), d(&[25 - 24]));
        // the subresultant path agrees with the modular one: res_x(x^2 + y, x - y) = y^2 + y
        let x = Poly::var(2, 0);
        let y = Poly::var(2, 1);
        let a = x.mul(&x, &mut m).unwrap().add(&y);
        let b = x.sub(&y);
        let want = y.mul(&y, &mut m).unwrap().add(&y);
        assert_eq!(resultant(&a, &b, 0, &mut m).unwrap(), want);
        // with y = 3: res(x^2 + 3, x - 3) = 12
        let a3 = d(&[3, 0, 1]);
        let b3 = d(&[-3, 1]);
        assert_eq!(resultant(&a3, &b3, 0, &mut m).unwrap(), d(&[12]));
        // degrees 3 and 4 with a parameter, against substitution
        let p = x.pow(3, &mut m).unwrap().add(&x.mul(&y, &mut m).unwrap()).add(&Poly::constant(2, Z::S(1)));
        let q = x.pow(4, &mut m).unwrap().sub(&y.mul(&y, &mut m).unwrap()).add(&x);
        let r = resultant(&p, &q, 0, &mut m).unwrap();
        for k in -3..4i64 {
            let rk = r.eval_var(1, &Z::S(k)).as_constant().unwrap();
            let pk = p.eval_var(1, &Z::S(k)).to_dense(0);
            let qk = q.eval_var(1, &Z::S(k)).to_dense(0);
            assert_eq!(rk, modular(&pk, &qk, &mut m).unwrap(), "y = {k}");
        }
    }
}
