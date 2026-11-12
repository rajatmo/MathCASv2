//! Zeilberger's algorithm (creative telescoping; Zeilberger 1991, *A = B* chapter 6).
//!
//! For `F(n, k)` hypergeometric in both variables, find the smallest order `J` and rational functions `s_j(n)`,
//! not all zero, with `sum_j s_j(n) F(n + j, k) = G(n, k + 1) - G(n, k)` and `G = Rc(n, k) F(n, k)`. Writing
//! `F(n + j, k) = R_j(n, k) F(n, k)` and `D` for the common denominator of the `R_j`, the term
//! `t(k) = F(n, k) p(k) / D(k)`, `p = sum s_j R_j D`, is fed to Gosper's algorithm with the unknown `s_j` kept
//! in the polynomial part, so one linear system gives both the `s_j` and the certificate. The identity
//! `sum s_j R_j = Rc(k + 1) F(n, k + 1) / F(n, k) - Rc(k)` is checked exactly before it is used.
use crate::gosper::{assemble, gosper_form, poly_solutions};
use crate::rf::{self, lcm, shift, Rf};
use mc_poly::work::{Meter, WorkResult};
use mc_poly::Poly;

/// The highest order tried.
pub const MAX_ORDER: usize = 4;

/// A recurrence `sum_j coefs[j] S(n + j) = 0` (polynomials in `n`, free of `k`) and its certificate `Rc`.
pub struct Telescoper {
    /// the coefficients, lowest shift first
    pub coefs: Vec<Poly>,
    /// the certificate
    pub cert: Rf,
}

/// Zeilberger's algorithm for the ratios `rk = F(n, k + 1) / F(n, k)` and `rn = F(n + 1, k) / F(n, k)`
/// (variables `kv` and `nv_` of one ring).
pub fn zeilberger(rk: &Rf, rn: &Rf, kv: usize, nvar: usize, max_order: usize, m: &mut Meter) -> WorkResult<Option<Telescoper>> {
    let nv = rk.nv();
    let mut rs = vec![Rf::one(nv)];
    for j in 1..=max_order.min(MAX_ORDER) {
        m.step(1)?;
        let prev = rs[j - 1].clone();
        rs.push(prev.mul(&rn.shift(nvar, j as i64 - 1, m)?, m)?);
        // D = lcm of the denominators; p_j = R_j D
        let mut d = rf::zc(nv, 1);
        for r in &rs {
            d = lcm(&d, &r.den, m)?;
        }
        let mut ps = vec![];
        for r in &rs {
            ps.push(rf::div(&d, &r.den, m)?.mul(&r.num, m)?);
        }
        // the ratio of F / D in k
        let dk1 = shift(&d, kv, 1, m)?;
        let r0 = rk.mul(&Rf::new(d.clone(), dk1, m)?, m)?;
        let (a, b, c) = gosper_form(&r0, kv, m)?;
        let b1 = shift(&b, kv, -1, m)?;
        let fs: Vec<Poly> = ps.iter().map(|p| c.mul(p, m)).collect::<WorkResult<_>>()?;
        let p = [b1.neg(), a];
        let Some((deg, basis)) = poly_solutions(&p, &fs, kv, m)? else { continue };
        let Some(sol) = basis.into_iter().find(|s| s[deg + 1..].iter().any(|x| !x.is_zero())) else { continue };
        // clear the denominators of the s_j (free of k)
        let sig = &sol[deg + 1..];
        let mut den = rf::zc(nv, 1);
        for s in sig.iter().chain(sol[..=deg].iter()) {
            den = lcm(&den, &s.den, m)?;
        }
        let scale = Rf::poly(den);
        let mut coefs = vec![];
        for s in sig {
            let v = s.mul(&scale, m)?;
            coefs.push(v.num);
        }
        // a common factor of the coefficients is dropped
        let mut g = Poly::zero(nv);
        for c in &coefs {
            g = mc_poly::gcd::gcd(&g, c, m)?;
        }
        let xs: Vec<Rf> = sol[..=deg].iter().map(|x| x.mul(&scale, m)).collect::<WorkResult<_>>()?;
        let x = assemble(&xs, kv, m)?;
        let mut coefs: Vec<Poly> = coefs.iter().map(|c| rf::div(c, &g, m)).collect::<WorkResult<_>>()?;
        let x = x.div(&Rf::poly(g.clone()), m)?.expect("nonzero gcd");
        // Rc = b(k - 1) x(k) / (c(k) D(k))
        let cd = c.mul(&d, m)?;
        let Some(cert) = x.mul_poly(&b1, m)?.div(&Rf::poly(cd), m)? else { continue };
        // check: sum s_j R_j == Rc(k + 1) rk - Rc(k)
        let mut lhs = Rf::zero(nv);
        for (s, r) in coefs.iter().zip(&rs) {
            lhs = lhs.add(&r.mul_poly(s, m)?, m)?;
        }
        let rhs = cert.shift(kv, 1, m)?.mul(rk, m)?.sub(&cert, m)?;
        if lhs != rhs {
            continue;
        }
        while coefs.last().is_some_and(|c| c.is_zero()) {
            coefs.pop();
        }
        if coefs.len() < 2 {
            // a pure telescoping relation (the sum itself has no recurrence of this form)
            continue;
        }
        return Ok(Some(Telescoper { coefs, cert }));
    }
    Ok(None)
}
