//! Polynomials in one variable `t` of the tower with coefficients in the field of the other variables (`Vec` of
//! field elements, index = degree), and the algorithms on them the decision procedure needs: the extended
//! Euclidean algorithm, Hermite reduction with respect to a derivation, and the derivation itself.
use crate::field::Fld;
use crate::tower::Tower;
use mc_poly::ratfun::RatFun;
use mc_poly::unipoly as up;
use mc_poly::work::{exact, Meter, WorkResult};
use mc_poly::{Poly, Z};

/// A polynomial in one variable over the field.
pub type U = Vec<RatFun>;

/// The coefficients of `a` in variable `v` (`a`'s denominator must be free of `v`), or `None`.
pub fn to_uni(fld: &Fld, a: &RatFun, v: usize, m: &mut Meter) -> WorkResult<Option<U>> {
    if a.den.degree(v).unwrap_or(0) > 0 {
        return Ok(None);
    }
    let deg = a.num.degree(v).unwrap_or(0) as usize;
    let mut out = vec![fld.int(0); deg + 1];
    for (k, c) in a.num.coeffs_in(v) {
        out[k as usize] = fld.quo(c, a.den.clone(), m)?;
    }
    Ok(Some(up::trim(fld, out)))
}

/// `sum c_k v^k`.
pub fn from_uni(fld: &Fld, p: &[RatFun], v: usize, m: &mut Meter) -> WorkResult<RatFun> {
    let mut acc = fld.int(0);
    let x = fld.var(v);
    let mut pw = fld.int(1);
    for (k, c) in p.iter().enumerate() {
        if k > 0 {
            pw = fld.mulm(&pw, &x, m)?;
        }
        if !c.num.is_zero() {
            acc = fld.addm(&acc, &fld.mulm(c, &pw, m)?, m)?;
        }
    }
    Ok(acc)
}

/// Numerator and denominator of `a` as polynomials in `v`.
pub fn num_den(fld: &Fld, a: &RatFun, v: usize, m: &mut Meter) -> WorkResult<(U, U)> {
    let one = Poly::constant(fld.nv, Z::ONE);
    let n = exact(to_uni(fld, &RatFun { num: a.num.clone(), den: one.clone() }, v, m)?, "a polynomial")?;
    let d = exact(to_uni(fld, &RatFun { num: a.den.clone(), den: one }, v, m)?, "a polynomial")?;
    Ok((n, d))
}

/// A derivation on the polynomials in `v`: the tower's, through the multivariate form.
pub fn d_uni(tw: &Tower<'_>, p: &[RatFun], v: usize, m: &mut Meter) -> WorkResult<Option<U>> {
    let fld = &tw.fld;
    let a = from_uni(fld, p, v, m)?;
    let da = tw.d(&a, m)?;
    to_uni(fld, &da, v, m)
}

/// `(s, t)` with `s a + t b = c` and `deg s < deg b` (a and b coprime); `None` when they are not.
pub fn diophantine(fld: &Fld, a: &[RatFun], b: &[RatFun], c: &[RatFun], m: &mut Meter) -> WorkResult<Option<(U, U)>> {
    // extended Euclid: s0 a + t0 b = g
    let (mut r0, mut r1) = (a.to_vec(), b.to_vec());
    let (mut s0, mut s1): (U, U) = (vec![fld.int(1)], vec![]);
    while !r1.is_empty() {
        m.step(r0.len() as u64)?;
        let (q, r) = exact(up::divrem(fld, &r0, &r1, m)?, "a field divides")?;
        let s2 = up::sub(fld, &s0, &up::mul(fld, &q, &s1, m)?);
        r0 = r1;
        r1 = r;
        s0 = s1;
        s1 = s2;
    }
    // r0 = gcd up to a constant
    if up::deg(&r0) != Some(0) {
        return Ok(None);
    }
    let g = r0[0].clone();
    let Some(gi) = fld.divm(&fld.int(1), &g, m)? else { return Ok(None) };
    // s = s0 c / g mod b; t = (c - s a)/b
    let s = up::scale(fld, &up::mul(fld, &s0, c, m)?, &gi, m)?;
    let (_, s) = exact(up::divrem(fld, &s, b, m)?, "a field divides")?;
    let rest = up::sub(fld, c, &up::mul(fld, &s, a, m)?);
    let Some(t) = up::div_exact(fld, &rest, b, m)? else { return Ok(None) };
    Ok(Some((s, t)))
}

/// The monic gcd.
pub fn gcd(fld: &Fld, a: &[RatFun], b: &[RatFun], m: &mut Meter) -> WorkResult<U> {
    up::gcd(fld, a, b, m)
}

/// Hermite reduction of the proper fraction `a/d` (every factor of `d` normal) for the derivation of the tower on
/// polynomials in `v` (Bronstein, *Symbolic Integration I*, section 5.3, Mack's linear version): `(g, h_num, h_den)`
/// with `a/d = D g + h_num/h_den`, `h_den` square-free; `None` when a step is not possible (a special factor).
pub fn hermite(tw: &Tower<'_>, a: &[RatFun], d: &[RatFun], v: usize, m: &mut Meter) -> WorkResult<Option<(RatFun, U, U)>> {
    let fld = &tw.fld;
    let mut g = fld.int(0);
    let dd = up::derivative(fld, d, m)?;
    let mut dm = gcd(fld, d, &dd, m)?;
    let Some(ds) = up::div_exact(fld, d, &dm, m)? else { return Ok(None) };
    let mut a = a.to_vec();
    while up::deg(&dm).unwrap_or(0) > 0 {
        m.step(dm.len() as u64)?;
        let ddm = up::derivative(fld, &dm, m)?;
        let dm2 = gcd(fld, &dm, &ddm, m)?;
        let Some(dms) = up::div_exact(fld, &dm, &dm2, m)? else { return Ok(None) };
        let Some(ddm_t) = d_uni(tw, &dm, v, m)? else { return Ok(None) };
        // -d* D(d-) / d-
        let Some(p) = up::div_exact(fld, &up::mul(fld, &ds, &ddm_t, m)?, &dm, m)? else { return Ok(None) };
        let p = up::neg(fld, &p);
        let Some((b, c)) = diophantine(fld, &p, &dms, &a, m)? else { return Ok(None) };
        let Some(db) = d_uni(tw, &b, v, m)? else { return Ok(None) };
        let Some(q) = up::div_exact(fld, &up::mul(fld, &db, &ds, m)?, &dms, m)? else { return Ok(None) };
        a = up::sub(fld, &c, &q);
        let bn = from_uni(fld, &b, v, m)?;
        let bd = from_uni(fld, &dm, v, m)?;
        let Some(t) = fld.divm(&bn, &bd, m)? else { return Ok(None) };
        g = fld.addm(&g, &t, m)?;
        dm = dm2;
    }
    Ok(Some((g, a, ds)))
}
