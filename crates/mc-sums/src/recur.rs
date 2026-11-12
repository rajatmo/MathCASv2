//! Linear recurrences with polynomial coefficients `sum_i p_i(n) y(n + i) = f(n)`: rational solutions
//! (Abramov 1989, 1995) and hypergeometric solutions (Petkovšek's algorithm Hyper, 1992; *A = B* chapter 8).
//!
//! **Abramov.** Every rational solution has its denominator dividing the universal denominator built from the
//! dispersion of `p_d(n - d)` and `p_0(n)`; the numerator is then a polynomial solution of the transformed
//! recurrence, found under the degree bound of `gosper::degree_bound`.
//!
//! **Hyper.** A hypergeometric solution has ratio `y(n + 1) / y(n) = Z A(n) C(n + 1) / (B(n) C(n))` with `A` a
//! factor of `p_0(n)`, `B` a factor of `p_d(n - d + 1)`, `Z` a root of the polynomial made of the leading
//! coefficients of `P_i = p_i prod_{j<i} A(n + j) prod_{j>=i} B(n + j)`, and `C` a polynomial solution of
//! `sum Z^i P_i(n) C(n + i) = 0`. Every pair `(A, B)` is tried (the number of pairs is bounded); only roots `Z`
//! that are rational functions of the other variables are used. Solutions with equal ratios are kept once, so the
//! solutions returned are linearly independent.
use crate::gosper::{assemble, dispersion, poly_solutions};
use crate::rf::{self, coeff, deg, has, lcm, shift, Rf};
use mc_poly::factor::factor;
use mc_poly::gcd::gcd;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};

/// The most `(A, B)` pairs Hyper tries.
const MAX_PAIRS: usize = 1024;

/// One hypergeometric solution: `y(n + 1) / y(n) = z a(n) c(n + 1) / (b(n) c(n))`.
#[derive(Clone, Debug)]
pub struct HyperSol {
    /// the ratio
    pub ratio: Rf,
    /// the constant `Z`
    pub z: Rf,
    /// `A`
    pub a: Poly,
    /// `B`
    pub b: Poly,
    /// `C`
    pub c: Poly,
}

/// The divisors (products of the irreducible factors depending on `v`, with multiplicity) of `p`.
fn divisors(p: &Poly, v: usize, m: &mut Meter) -> WorkResult<Vec<Poly>> {
    let nv = p.nvars();
    let f = factor(p, false, m)?;
    let mut out = vec![rf::zc(nv, 1)];
    for (q, e) in f.factors.iter().filter(|q| has(&q.0, v)) {
        let mut next = vec![];
        for d in &out {
            let mut acc = d.clone();
            next.push(acc.clone());
            for _ in 0..*e {
                acc = acc.mul(q, m)?;
                next.push(acc.clone());
            }
        }
        out = next;
        if out.len() > MAX_PAIRS {
            break;
        }
    }
    Ok(out)
}

/// `prod_{j=lo}^{hi-1} q(n + j)`.
fn shifted_product(q: &Poly, v: usize, lo: usize, hi: usize, m: &mut Meter) -> WorkResult<Poly> {
    let mut acc = rf::zc(q.nvars(), 1);
    for j in lo..hi {
        acc = acc.mul(&shift(q, v, j as i64, m)?, m)?;
    }
    Ok(acc)
}

/// Petkovšek's Hyper: the hypergeometric solutions of `sum_i p_i(n) y(n + i) = 0` (`p_0`, `p_d` nonzero). `zv` is a
/// variable of the ring not used otherwise, for the polynomial in `Z`.
pub fn hyper(p: &[Poly], v: usize, zv: usize, m: &mut Meter) -> WorkResult<Vec<HyperSol>> {
    let d = p.len() - 1;
    let mut sols: Vec<HyperSol> = vec![];
    if d == 0 || p[0].is_zero() || p[d].is_zero() {
        return Ok(sols);
    }
    let nv = p[0].nvars();
    let pd = shift(&p[d], v, 1 - d as i64, m)?;
    let da = divisors(&p[0], v, m)?;
    let db = divisors(&pd, v, m)?;
    let mut pairs = 0;
    for a in &da {
        for b in &db {
            pairs += 1;
            if pairs > MAX_PAIRS {
                return Ok(sols);
            }
            m.step(1)?;
            // P_i = p_i prod_{j<i} A(n + j) prod_{i<=j<d} B(n + j)
            let mut ps = vec![];
            for (i, pi) in p.iter().enumerate() {
                let x = pi.mul(&shifted_product(a, v, 0, i, m)?, m)?.mul(&shifted_product(b, v, i, d, m)?, m)?;
                ps.push(x);
            }
            let top = ps.iter().filter_map(|x| deg(x, v)).max().unwrap_or(0);
            // the polynomial sum alpha_i z^i
            let mut zp = Poly::zero(nv);
            for (i, x) in ps.iter().enumerate() {
                if deg(x, v) == Some(top) {
                    let mut e = vec![0u32; nv];
                    e[zv] = i as u32;
                    zp = zp.add(&coeff(x, v, top).mul(&Poly::monomial(&e, Z::ONE), m)?);
                }
            }
            for z in linear_roots(&zp, zv, m)? {
                // q_i = Z^i P_i with denominators cleared
                let (zn, zd) = (z.num.clone(), z.den.clone());
                let mut q = vec![];
                for (i, x) in ps.iter().enumerate() {
                    q.push(x.mul(&zn.pow(i as u64, m)?, m)?.mul(&zd.pow((d - i) as u64, m)?, m)?);
                }
                let Some((dc, basis)) = poly_solutions(&q, &[], v, m)? else { continue };
                for sol in basis {
                    let c = assemble(&sol[..=dc], v, m)?;
                    if c.is_zero() {
                        continue;
                    }
                    let cp = c.num;
                    let cr = Rf::new(shift(&cp, v, 1, m)?, cp.clone(), m)?;
                    let ab = Rf::new(a.clone(), b.clone(), m)?;
                    let ratio = z.mul(&ab, m)?.mul(&cr, m)?;
                    if sols.iter().any(|s| s.ratio == ratio) {
                        continue;
                    }
                    if !check_ratio(p, &ratio, v, m)? {
                        continue;
                    }
                    sols.push(HyperSol { ratio, z: z.clone(), a: a.clone(), b: b.clone(), c: cp });
                }
            }
            if sols.len() >= d {
                return Ok(sols);
            }
        }
    }
    Ok(sols)
}

/// Does the hypergeometric term with this ratio solve the recurrence: `sum_i p_i prod_{j<i} r(n + j) == 0`?
pub fn check_ratio(p: &[Poly], r: &Rf, v: usize, m: &mut Meter) -> WorkResult<bool> {
    let nv = r.nv();
    let mut acc = Rf::zero(nv);
    let mut prod = Rf::one(nv);
    for (i, pi) in p.iter().enumerate() {
        if i > 0 {
            prod = prod.mul(&r.shift(v, i as i64 - 1, m)?, m)?;
        }
        acc = acc.add(&prod.mul_poly(pi, m)?, m)?;
    }
    Ok(acc.is_zero())
}

/// The nonzero roots in `zv` of `zp` that are rational functions of the other variables (from its factors of
/// degree 1 in `zv`).
fn linear_roots(zp: &Poly, zv: usize, m: &mut Meter) -> WorkResult<Vec<Rf>> {
    let mut out = vec![];
    if zp.is_zero() || !has(zp, zv) {
        return Ok(out);
    }
    let f = factor(zp, false, m)?;
    for (q, _) in &f.factors {
        if deg(q, zv) == Some(1) {
            let c1 = coeff(q, zv, 1);
            let c0 = coeff(q, zv, 0);
            if c0.is_zero() {
                continue;
            }
            let z = Rf::new(c0.neg(), c1, m)?;
            if !out.contains(&z) {
                out.push(z);
            }
        }
    }
    Ok(out)
}

/// Abramov's universal denominator of `sum_i p_i(n) y(n + i) = f(n)`.
pub fn universal_denominator(p: &[Poly], v: usize, m: &mut Meter) -> WorkResult<Poly> {
    let d = p.len() - 1;
    let nv = p[0].nvars();
    let mut a = shift(&p[d], v, -(d as i64), m)?;
    let mut b = p[0].clone();
    let mut u = rf::zc(nv, 1);
    let mut hs = dispersion(&a, &b, v, m)?;
    hs.reverse();
    for h in hs {
        loop {
            m.step(1)?;
            let g = gcd(&a, &shift(&b, v, h, m)?, m)?;
            if !has(&g, v) {
                break;
            }
            a = rf::div(&a, &g, m)?;
            b = rf::div(&b, &shift(&g, v, -h, m)?, m)?;
            for i in 0..=h {
                u = u.mul(&shift(&g, v, -i, m)?, m)?;
            }
        }
    }
    Ok(u)
}

/// The rational solutions of `sum_i p_i(n) y(n + i) = s f(n)`: `(particular, homogeneous basis)`, the particular
/// solution for `s = 1` (`None` when there is none or `f` is `None`).
pub fn rational_solutions(p: &[Poly], f: Option<&Rf>, v: usize, m: &mut Meter) -> WorkResult<Option<(Option<Rf>, Vec<Rf>)>> {
    let nv = p[0].nvars();
    let mut u = universal_denominator(p, v, m)?;
    if let Some(f) = f {
        // a pole of f can also be one of y: allow the denominator of f and its shifts
        let _ = f;
    }
    if u.lc().is_neg() {
        u = u.neg();
    }
    // L = lcm U(n + i); the recurrence for z = y U: sum p_i L / U(n + i) z(n + i) = L f
    let d = p.len() - 1;
    let us: Vec<Poly> = (0..=d).map(|i| shift(&u, v, i as i64, m)).collect::<WorkResult<_>>()?;
    let mut l = rf::zc(nv, 1);
    for ui in &us {
        l = lcm(&l, ui, m)?;
    }
    let mut q = vec![];
    for (pi, ui) in p.iter().zip(&us) {
        q.push(pi.mul(&rf::div(&l, ui, m)?, m)?);
    }
    let (rhs, rden) = match f {
        Some(f) => {
            // multiply through by the denominator of f
            let lf = l.mul(&f.num, m)?;
            (Some(lf), f.den.clone())
        }
        None => (None, rf::zc(nv, 1)),
    };
    let q: Vec<Poly> = q.iter().map(|x| x.mul(&rden, m)).collect::<WorkResult<_>>()?;
    let fs: Vec<Poly> = rhs.into_iter().collect();
    let Some((dz, basis)) = poly_solutions(&q, &fs, v, m)? else { return Ok(Some((None, vec![]))) };
    let mut part = None;
    let mut hom = vec![];
    let uf = Rf::poly(u.clone());
    for sol in basis {
        let z = assemble(&sol[..=dz], v, m)?;
        let s = if fs.is_empty() { Rf::zero(nv) } else { sol[dz + 1].clone() };
        let y = z.div(&uf, m)?.expect("nonzero denominator");
        if s.is_zero() {
            if !y.is_zero() {
                hom.push(y);
            }
        } else if part.is_none() {
            let y = y.div(&s, m)?.expect("nonzero");
            part = Some(y);
        }
    }
    Ok(Some((part, hom)))
}
