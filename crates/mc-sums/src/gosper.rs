//! Gosper's algorithm (Gosper 1978; *A = B*, chapter 5) and the polynomial solutions of linear recurrences it
//! rests on.
//!
//! For a hypergeometric `t(k)` with ratio `r(k) = t(k + 1) / t(k)`:
//!
//! 1. the **Gosper form** `r = a(k) / b(k) * c(k + 1) / c(k)` with `gcd(a(k), b(k + h)) = 1` for every integer
//!    `h >= 0`. The integers `h` where the gcd is not trivial (the dispersion set) are found from the irreducible
//!    factors of `a` and `b`: `p(k)` and `q(k + h)` are proportional only for the `h` read off their two leading
//!    coefficients, so no resultant in a new variable is needed;
//! 2. a polynomial `x` with `a(k) x(k + 1) - b(k - 1) x(k) = c(k)`, by undetermined coefficients under the degree
//!    bound of Abramov, Bronstein and Petkovšek (the operator written in powers of the difference `Δ`: either the
//!    degree of the right side minus the order of the operator, or a nonnegative integer root of its indicial
//!    polynomial);
//! 3. the antidifference `z(k) = R(k) t(k)`, `R = b(k - 1) x(k) / c(k)`, so that `z(k + 1) - z(k) = t(k)`.
//!
//! The certificate `R(k + 1) r(k) - R(k) = 1` is checked as an identity of rational functions before the
//! antidifference is used, so a wrong `R` is never returned.
use crate::rf::{self, coeff, deg, has, lcm, nullspace, shift, Rf};
use mc_poly::factor::factor;
use mc_poly::gcd::gcd;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};

/// The largest polynomial degree tried for a solution.
pub const MAX_DEGREE: i64 = 60;

/// The nonnegative integers `h` with `gcd(a(k), b(k + h))` not constant in `k` (ascending).
pub fn dispersion(a: &Poly, b: &Poly, v: usize, m: &mut Meter) -> WorkResult<Vec<i64>> {
    let nv = a.nvars();
    let fa = factor(a, false, m)?;
    let fb = factor(b, false, m)?;
    let mut hs = vec![];
    for (p, _) in fa.factors.iter().filter(|f| has(&f.0, v)) {
        for (q, _) in fb.factors.iter().filter(|f| has(&f.0, v)) {
            let d = deg(p, v).unwrap_or(0);
            if d != deg(q, v).unwrap_or(0) {
                continue;
            }
            let (pd, pd1) = (coeff(p, v, d), coeff(p, v, d - 1));
            let (qd, qd1) = (coeff(q, v, d), coeff(q, v, d - 1));
            // h = (p_{d-1} q_d - q_{d-1} p_d) / (d p_d q_d)
            let top = pd1.mul(&qd, m)?.sub(&qd1.mul(&pd, m)?);
            let bot = pd.mul(&qd, m)?.scale(&Z::from_i128(d as i128));
            let hq = Rf::new(top, bot, m)?;
            let Some(h) = hq.as_int() else { continue };
            if !(0..=10_000).contains(&h) || hs.contains(&h) {
                continue;
            }
            // p(k) q_d == q(k + h) p_d
            if p.mul(&qd, m)? == shift(q, v, h, m)?.mul(&pd, m)? {
                hs.push(h);
            }
        }
    }
    let _ = nv;
    hs.sort_unstable();
    Ok(hs)
}

/// The Gosper form `(a, b, c)` of a ratio `r` in variable `v`.
pub fn gosper_form(r: &Rf, v: usize, m: &mut Meter) -> WorkResult<(Poly, Poly, Poly)> {
    let nv = r.nv();
    let (mut a, mut b) = (r.num.clone(), r.den.clone());
    let mut c = rf::zc(nv, 1);
    for h in dispersion(&a, &b, v, m)? {
        loop {
            m.step(1)?;
            let g = gcd(&a, &shift(&b, v, h, m)?, m)?;
            if !has(&g, v) {
                break;
            }
            a = rf::div(&a, &g, m)?;
            b = rf::div(&b, &shift(&g, v, -h, m)?, m)?;
            for j in 1..=h {
                c = c.mul(&shift(&g, v, -j, m)?, m)?;
            }
        }
    }
    Ok((a, b, c))
}

/// `k^e` falling-factorial helper: `d (d - 1) ... (d - j + 1)`.
fn falling(d: i64, j: usize) -> i64 {
    (0..j as i64).map(|i| d - i).product()
}

/// The binomial coefficient `C(i, j)` for small arguments.
fn binom(i: usize, j: usize) -> i64 {
    if j > i {
        return 0;
    }
    let mut r: i64 = 1;
    for t in 0..j {
        r = r * (i - t) as i64 / (t + 1) as i64;
    }
    r
}

/// The degree bound for polynomial solutions of `sum_i p_i(v) y(v + i) = f(v)` with `deg f <= df` (`None` for
/// `f = 0`): the largest candidate, or `None` when there is none.
pub fn degree_bound(p: &[Poly], df: Option<u32>, v: usize, m: &mut Meter) -> WorkResult<Option<i64>> {
    let nv = p[0].nvars();
    // q_j = sum_i p_i C(i, j): the operator in powers of Δ
    let order = p.len() - 1;
    let mut q = vec![Poly::zero(nv); order + 1];
    for (j, qj) in q.iter_mut().enumerate() {
        for (i, pi) in p.iter().enumerate() {
            let c = binom(i, j);
            if c != 0 {
                *qj = qj.add(&pi.scale(&Z::from_i128(c as i128)));
            }
        }
    }
    let b = q.iter().enumerate().filter_map(|(j, qj)| deg(qj, v).map(|d| d as i64 - j as i64)).max();
    let Some(b) = b else { return Ok(None) };
    let mut cands = vec![];
    if let Some(df) = df {
        cands.push(df as i64 - b);
    }
    // the indicial polynomial sum over j with deg q_j - j = b of lc(q_j) d^(falling j)
    let tops: Vec<(usize, Poly)> = q
        .iter()
        .enumerate()
        .filter(|(j, qj)| deg(qj, v).is_some_and(|d| d as i64 - *j as i64 == b))
        .map(|(j, qj)| (j, rf::lcoeff(qj, v)))
        .collect();
    for d in 0..=MAX_DEGREE {
        m.step(1)?;
        let mut s = Poly::zero(nv);
        for (j, l) in &tops {
            let f = falling(d, *j);
            if f != 0 {
                s = s.add(&l.scale(&Z::from_i128(f as i128)));
            }
        }
        if s.is_zero() {
            cands.push(d);
        }
    }
    Ok(cands.into_iter().filter(|&d| d >= 0).max())
}

/// The polynomial solutions of `sum_i p_i(v) y(v + i) = sum_j s_j f_j(v)` with unknown constants `s_j`: the degree
/// `D` and a basis of the solutions as vectors `[y_0, ..., y_D, s_0, ...]` (`y = sum y_i v^i`). `None` when there is
/// no degree bound (then the only solution has `y = 0`).
pub fn poly_solutions(p: &[Poly], f: &[Poly], v: usize, m: &mut Meter) -> WorkResult<Option<(usize, Vec<Vec<Rf>>)>> {
    let nv = p[0].nvars();
    let df = f.iter().filter_map(|fj| deg(fj, v)).max();
    let Some(dmax) = degree_bound(p, df, v, m)? else {
        if f.is_empty() {
            return Ok(None);
        }
        // y = 0: the s_j with sum s_j f_j = 0
        return Ok(Some((0, constants_only(f, v, nv, m)?)));
    };
    if dmax > MAX_DEGREE {
        return Ok(None);
    }
    let d = dmax as usize;
    // the images L(v^i)
    let x = Poly::var(nv, v);
    let mut cols: Vec<Poly> = vec![];
    for i in 0..=d {
        let mut li = Poly::zero(nv);
        for (s, ps) in p.iter().enumerate() {
            if ps.is_zero() {
                continue;
            }
            let base = x.add(&rf::zc(nv, s as i64)).pow(i as u64, m)?;
            li = li.add(&ps.mul(&base, m)?);
        }
        cols.push(li);
    }
    for fj in f {
        cols.push(fj.neg());
    }
    let top = cols.iter().filter_map(|c| deg(c, v)).max().unwrap_or(0);
    let mut rows = vec![];
    for e in 0..=top {
        let row: Vec<Rf> = cols.iter().map(|c| Rf::poly(coeff(c, v, e))).collect();
        rows.push(row);
    }
    let basis = nullspace(&rows, cols.len(), m)?;
    Ok(Some((d, basis)))
}

/// The vectors `[0, s_0, ...]` with `sum s_j f_j = 0` (no polynomial part).
fn constants_only(f: &[Poly], v: usize, nv: usize, m: &mut Meter) -> WorkResult<Vec<Vec<Rf>>> {
    let top = f.iter().filter_map(|c| deg(c, v)).max().unwrap_or(0);
    let mut rows = vec![];
    for e in 0..=top {
        rows.push(f.iter().map(|c| Rf::poly(coeff(c, v, e))).collect::<Vec<_>>());
    }
    let basis = nullspace(&rows, f.len(), m)?;
    Ok(basis
        .into_iter()
        .map(|b| {
            let mut v = vec![Rf::zero(nv)];
            v.extend(b);
            v
        })
        .collect())
}

/// The polynomial `sum c_i v^i` of rational-function coefficients, as a rational function.
pub fn assemble(cs: &[Rf], v: usize, m: &mut Meter) -> WorkResult<Rf> {
    let nv = cs.first().map(|c| c.nv()).unwrap_or(1);
    let mut den = rf::zc(nv, 1);
    for c in cs {
        den = lcm(&den, &c.den, m)?;
    }
    let mut num = Poly::zero(nv);
    for (i, c) in cs.iter().enumerate() {
        if c.is_zero() {
            continue;
        }
        let f = rf::div(&den, &c.den, m)?;
        let mut e = vec![0u32; nv];
        e[v] = i as u32;
        num = num.add(&c.num.mul(&f, m)?.mul(&Poly::monomial(&e, Z::ONE), m)?);
    }
    Rf::new(num, den, m)
}

/// Gosper's algorithm: the rational `R` with `R(k + 1) r(k) - R(k) = 1` (checked), or `None` when `t` has no
/// hypergeometric antidifference.
pub fn gosper(r: &Rf, v: usize, m: &mut Meter) -> WorkResult<Option<Rf>> {
    let (a, b, c) = gosper_form(r, v, m)?;
    let b1 = shift(&b, v, -1, m)?;
    let p = [b1.neg(), a];
    let Some((d, basis)) = poly_solutions(&p, std::slice::from_ref(&c), v, m)? else { return Ok(None) };
    let Some(sol) = basis.into_iter().find(|s| !s[d + 1].is_zero()) else { return Ok(None) };
    let inv = sol[d + 1].inv().expect("nonzero");
    let xs: Vec<Rf> = sol[..=d].iter().map(|x| x.mul(&inv, m)).collect::<WorkResult<_>>()?;
    let x = assemble(&xs, v, m)?;
    // R = b(k - 1) x(k) / c(k)
    let rr = x.mul_poly(&shift(&b, v, -1, m)?, m)?;
    let Some(rr) = rr.div(&Rf::poly(c), m)? else { return Ok(None) };
    if !certificate(&rr, r, v, m)? {
        return Ok(None);
    }
    Ok(Some(rr))
}

/// `R(k + 1) r(k) - R(k) == 1`?
pub fn certificate(rr: &Rf, r: &Rf, v: usize, m: &mut Meter) -> WorkResult<bool> {
    let lhs = rr.shift(v, 1, m)?.mul(r, m)?.sub(rr, m)?;
    Ok(lhs.is_one())
}
