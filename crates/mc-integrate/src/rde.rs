//! The Risch differential equation `D y + f y = g` over `C(x)` (`D = d/dx`), for `f = i Dw` the logarithmic
//! derivative of `exp(i w)` (Bronstein, *Symbolic Integration I*, chapter 6, the base case):
//!
//! 1. `f` is weakly normalised (a derivative has no simple poles);
//! 2. the normal denominator: `y = q/h` with `h = gcd(e, de/dx) / gcd(p, dp/dx)`, `e` the denominator of `g`,
//!    `p = gcd(den f, e)`; there is no solution unless `e` divides `den(f) h^2` (6.1);
//! 3. the polynomial equation `a Dq + b q = c`, made coprime, its degree bound (6.3) and Rothstein's SPDE (6.4)
//!    down to `D h + b h = c` with `a = 1`, solved from the top coefficient (6.5, no cancellation: `deg b >= 0`).
//!
//! A solution is checked by substitution before it is returned.
use crate::tower::Tower;
use crate::uni::{self, from_uni, U};
use mc_poly::ratfun::RatFun;
use mc_poly::unipoly as up;
use mc_poly::work::{exact, Meter, WorkResult};

/// The outcome.
pub enum Rde {
    /// a rational solution
    Solution(RatFun),
    /// proven: no rational solution
    None,
    /// a step not done (or a failed check): no decision
    Unknown,
}

/// Solve `D y + f y = g` for `y` in `C(x)` (f, g free of every monomial but x).
pub fn rde(tw: &Tower<'_>, f: &RatFun, g: &RatFun, m: &mut Meter) -> WorkResult<Rde> {
    let fld = &tw.fld;
    let x = tw.x;
    if g.num.is_zero() {
        return Ok(Rde::Solution(fld.int(0)));
    }
    let dx = |p: &[RatFun], m: &mut Meter| up::derivative(fld, p, m);
    let (fnum, fden) = uni::num_den(fld, f, x, m)?;
    let (gnum, gden) = uni::num_den(fld, g, x, m)?;
    // normal denominator
    let dn = fden.clone();
    let en = gden.clone();
    let p = uni::gcd(fld, &dn, &en, m)?;
    let h1 = uni::gcd(fld, &en, &dx(&en, m)?, m)?;
    let h2 = uni::gcd(fld, &p, &dx(&p, m)?, m)?;
    let Some(h) = up::div_exact(fld, &h1, &h2, m)? else { return Ok(Rde::Unknown) };
    let dnh2 = up::mul(fld, &dn, &up::mul(fld, &h, &h, m)?, m)?;
    if up::div_exact(fld, &dnh2, &en, m)?.is_none() {
        return Ok(Rde::None);
    }
    // a = dn h, b = h fnum - dn Dh, c = dn h^2 gnum / gden
    let a = up::mul(fld, &dn, &h, m)?;
    let b = up::sub(fld, &up::mul(fld, &h, &fnum, m)?, &up::mul(fld, &dn, &dx(&h, m)?, m)?);
    let Some(c) = up::div_exact(fld, &up::mul(fld, &dnh2, &gnum, m)?, &en, m)? else { return Ok(Rde::Unknown) };
    let Some(q) = poly_rde(fld, &a, &b, &c, m)? else { return Ok(Rde::None) };
    let qn = from_uni(fld, &q, x, m)?;
    let hn = from_uni(fld, &h, x, m)?;
    let y = exact(fld.divm(&qn, &hn, m)?, "h is not zero")?;
    // check D y + f y = g
    let lhs = fld.addm(&tw.d(&y, m)?, &fld.mulm(f, &y, m)?, m)?;
    if fld.subm(&lhs, g, m)?.num.is_zero() {
        Ok(Rde::Solution(y))
    } else {
        Ok(Rde::Unknown)
    }
}

/// A polynomial solution of `a q' + b q = c` (polynomials in x over the constants), or `None` (proven).
fn poly_rde(fld: &crate::field::Fld, a: &[RatFun], b: &[RatFun], c: &[RatFun], m: &mut Meter) -> WorkResult<Option<U>> {
    let dx = |p: &[RatFun], m: &mut Meter| up::derivative(fld, p, m);
    // coprime a, b
    let g = uni::gcd(fld, a, b, m)?;
    let Some(a) = up::div_exact(fld, a, &g, m)? else { return Ok(None) };
    let Some(b) = up::div_exact(fld, b, &g, m)? else { return Ok(None) };
    let Some(c) = up::div_exact(fld, c, &g, m)? else { return Ok(None) };
    // degree bound (base case)
    let da = up::deg(&a).unwrap_or(0) as i64;
    let db = up::deg(&b).map(|d| d as i64);
    let dc = up::deg(&c).map(|d| d as i64);
    let mut n: i64 = match (db, dc) {
        (_, None) => 0,
        (Some(db), Some(dc)) => (dc - db.max(da - 1)).max(0),
        (None, Some(dc)) => (dc - (da - 1)).max(0),
    };
    if let Some(db) = db {
        if db == da - 1 {
            let s = exact(fld.divm(&up::lc(fld, &b), &up::lc(fld, &a), m)?, "lc a is not zero")?;
            if let Some((num, den)) = fld.as_rational(&s) {
                if den.is_one() {
                    if let Some(v) = num.neg().as_i64() {
                        if v > 0 {
                            n = n.max(v);
                        }
                    }
                }
            }
        }
    }
    // SPDE: q = alpha h + beta, D h + bb h = cc, deg h <= n
    let (mut a, mut b, mut c) = (a, b, c);
    let mut alpha: U = vec![fld.int(1)];
    let mut beta: U = vec![];
    loop {
        m.step(1 + a.len() as u64)?;
        if n < 0 {
            if c.is_empty() {
                return Ok(Some(beta));
            }
            return Ok(None);
        }
        let g = uni::gcd(fld, &a, &b, m)?;
        let Some(c2) = up::div_exact(fld, &c, &g, m)? else { return Ok(None) };
        let Some(a2) = up::div_exact(fld, &a, &g, m)? else { return Ok(None) };
        let Some(b2) = up::div_exact(fld, &b, &g, m)? else { return Ok(None) };
        (a, b, c) = (a2, b2, c2);
        if up::deg(&a).unwrap_or(0) == 0 {
            let Some(bb) = up::div_scalar(fld, &b, &a[0], m)? else { return Ok(None) };
            let Some(cc) = up::div_scalar(fld, &c, &a[0], m)? else { return Ok(None) };
            let Some(hsol) = no_cancel(fld, &bb, &cc, n, m)? else { return Ok(None) };
            let q = up::add(fld, &up::mul(fld, &alpha, &hsol, m)?, &beta);
            return Ok(Some(q));
        }
        // b r + a z = c, deg r < deg a
        let Some((r, z)) = uni::diophantine(fld, &b, &a, &c, m)? else { return Ok(None) };
        let nb = up::add(fld, &b, &dx(&a, m)?);
        let nc = up::sub(fld, &z, &dx(&r, m)?);
        beta = up::add(fld, &up::mul(fld, &alpha, &r, m)?, &beta);
        alpha = up::mul(fld, &alpha, &a, m)?;
        n -= up::deg(&a).unwrap_or(0) as i64;
        b = nb;
        c = nc;
    }
}

/// `h' + b h = c` with `deg h <= n`: `b = 0` by antidifferentiation, else from the top coefficient.
fn no_cancel(fld: &crate::field::Fld, b: &[RatFun], c: &[RatFun], n: i64, m: &mut Meter) -> WorkResult<Option<U>> {
    if b.is_empty() {
        // h = int c
        let mut h: U = vec![fld.int(0)];
        for (k, ck) in c.iter().enumerate() {
            h.push(exact(fld.divm(ck, &fld.int(k as i64 + 1), m)?, "k + 1 is not zero")?);
        }
        let h = up::trim(fld, h);
        if up::deg(&h).is_some_and(|d| d as i64 > n) {
            return Ok(None);
        }
        return Ok(Some(h));
    }
    let db = up::deg(b).unwrap_or(0) as i64;
    let mut q: U = vec![];
    let mut c = c.to_vec();
    let mut n = n;
    while !c.is_empty() {
        m.step(c.len() as u64)?;
        let dc = up::deg(&c).unwrap_or(0) as i64;
        let k = dc - db;
        if n < 0 || k < 0 || k > n {
            return Ok(None);
        }
        let co = exact(fld.divm(&up::lc(fld, &c), &up::lc(fld, b), m)?, "lc b is not zero")?;
        let mut p: U = vec![fld.int(0); k as usize];
        p.push(co);
        q = up::add(fld, &q, &p);
        n = k - 1;
        let dp = up::derivative(fld, &p, m)?;
        c = up::sub(fld, &c, &up::add(fld, &dp, &up::mul(fld, b, &p, m)?));
    }
    Ok(Some(q))
}
