//! The Risch decision procedure for a tower of height one over `C(x)`: `f` in `C(x)(t)` with `t = exp(w)` or
//! `t = log(u)` (Bronstein, *Symbolic Integration I*, chapters 5 and 6).
//!
//! 1. The canonical splitting of `f`: a (Laurent) polynomial part and a proper part with a normal denominator.
//! 2. Hermite reduction of the proper part ([`crate::uni::hermite`]): a rational part and a simple part `a/d`.
//! 3. The residue criterion (Rothstein–Trager): `R(z) = res_t(d, a - z Dd)`. If a root of `R` is not a constant,
//!    `f` has no elementary integral (Bronstein, theorem 5.6.1); otherwise the logarithmic part is
//!    `sum z log(gcd(d, a - z Dd))` over the roots (computed for rational roots; a residue polynomial with an
//!    irreducible factor of degree 2 or more ends the procedure without a decision, and the other methods take over).
//! 4. The polynomial part:
//!    - primitive `t`: from the top coefficient down, a limited integration problem `a_k = D b + c Dt` in `C(x)`
//!      (`b` rational, `c` constant); one without a solution proves non-elementarity (theorem 5.8.1);
//!    - hyperexponential `t`: for each power `t^i`, `i != 0`, the Risch differential equation `D y + i Dw y = a_i`
//!      in `C(x)` ([`rde`]); one without a rational solution proves non-elementarity (theorem 5.9.1).
//!
//!    The coefficient of `t^0` is a rational function of `x`, returned to the caller (the rational integrator).
//!
//! Every solution of a Risch differential equation is checked by substitution; the answer as a whole is checked
//! by the driver. A "no solution" is only claimed by the exact algorithms, never after a time-out.
use crate::budget::Budget;
use crate::field::Fld;
use crate::rde::{rde, Rde};
use crate::tower::{Kind, Tower};
use crate::uni::{self, from_uni, hermite, to_uni, U};
use mc_poly::ratfun::RatFun;
use mc_poly::unipoly as up;
use mc_poly::work::{exact, Meter, WorkResult};
use mc_poly::{Poly, Z};

/// The result of the decision procedure.
#[derive(Clone, Debug)]
pub enum Decision {
    /// `int f = part + int rest`, `rest` a rational function of x and constants
    Found { part: Box<RatFun>, logs: Vec<(RatFun, RatFun)>, rest: Box<RatFun> },
    /// no elementary integral: the reason
    NonElementary(String),
    /// outside the procedure, or a step it does not do
    Unknown,
}

/// The decision procedure on a tower of height one; `z` is a spare variable for the resultant.
pub fn decide(tw: &Tower<'_>, bud: &mut Budget<'_, '_, '_, '_>) -> WorkResult<Decision> {
    let mut m = Meter::new(bud);
    run(tw, &mut m)
}

fn run(tw: &Tower<'_>, m: &mut Meter) -> WorkResult<Decision> {
    let fld = &tw.fld;
    if !tw.reduced {
        return Ok(Decision::Unknown);
    }
    let mons: Vec<usize> = tw.monomials().into_iter().filter(|&v| v != tw.x).collect();
    let [t] = mons.as_slice() else { return Ok(Decision::Unknown) };
    let t = *t;
    let exp = match &tw.vars[t].kind {
        Kind::Exp { .. } => true,
        Kind::Log { .. } => false,
        _ => return Ok(Decision::Unknown),
    };
    // Dt / t (exp) or Dt (log), in C(x)
    let dt = &tw.deriv[t];
    let w = if exp { exact(fld.divm(dt, &fld.var(t), m)?, "t is not zero")? } else { dt.clone() };
    if !fld.free_of(&w, t) {
        return Ok(Decision::Unknown);
    }
    let z = tw.vars.len();
    let (a, d) = uni::num_den(fld, &tw.f, t, m)?;
    // split d = t^k dn (exp case)
    let k = if exp { d.iter().take_while(|c| c.num.is_zero()).count() } else { 0 };
    let dn: U = d[k..].to_vec();
    let tk: U = {
        let mut v = vec![fld.int(0); k];
        v.push(fld.int(1));
        v
    };
    // a = q d + r
    let (q, r) = exact(up::divrem(fld, &a, &d, m)?, "a field divides")?;
    // r / (t^k dn) = s / t^k + u / dn
    let (s, u) = if k > 0 && up::deg(&dn).unwrap_or(0) > 0 {
        match uni::diophantine(fld, &dn, &tk, &r, m)? {
            Some((s, u)) => (s, u),
            None => return Ok(Decision::Unknown),
        }
    } else if k > 0 {
        // dn constant: everything is Laurent
        let Some(s) = up::div_scalar(fld, &r, &dn[0], m)? else { return Ok(Decision::Unknown) };
        (s, vec![])
    } else {
        (vec![], r)
    };
    // the polynomial (Laurent) part so far, as a field element
    let mut poly = from_uni(fld, &q, t, m)?;
    if k > 0 {
        let sn = from_uni(fld, &s, t, m)?;
        let tkp = fld.powm(&fld.var(t), k as i64, m)?.expect("t is not zero");
        poly = fld.addm(&poly, &exact(fld.divm(&sn, &tkp, m)?, "t is not zero")?, m)?;
    }
    let mut part = fld.int(0);
    let mut logs: Vec<(RatFun, RatFun)> = vec![];
    if !u.is_empty() && up::deg(&dn).unwrap_or(0) > 0 {
        let Some((g, ha, hd)) = hermite(tw, &u, &dn, t, m)? else { return Ok(Decision::Unknown) };
        part = g;
        // the simple part ha/hd: proper by construction of Hermite's last step? divide to be sure
        let (hq, hr) = exact(up::divrem(fld, &ha, &hd, m)?, "a field divides")?;
        poly = fld.addm(&poly, &from_uni(fld, &hq, t, m)?, m)?;
        if !hr.is_empty() {
            match residues(tw, &hr, &hd, t, z, m)? {
                Res::Logs(ls, rem) => {
                    logs = ls;
                    poly = fld.addm(&poly, &rem, m)?;
                }
                Res::NotConstant => {
                    return Ok(Decision::NonElementary(
                        "a residue of the simple part is not a constant (Rothstein-Trager)".into(),
                    ))
                }
                Res::Unknown => return Ok(Decision::Unknown),
            }
        }
    }
    // the polynomial part: a Laurent polynomial in t with coefficients in C(x)
    if exp {
        hyperexponential(tw, &poly, t, &w, part, logs, m)
    } else {
        primitive(tw, &poly, t, &w, part, logs, m)
    }
}

enum Res {
    /// (coefficient, argument) of each logarithm, and the rest of the simple part (a Laurent polynomial)
    Logs(Vec<(RatFun, RatFun)>, RatFun),
    NotConstant,
    Unknown,
}

/// The residue criterion and the logarithmic part of `a/d` (d square-free and normal, deg a < deg d).
fn residues(tw: &Tower<'_>, a: &[RatFun], d: &[RatFun], t: usize, z: usize, m: &mut Meter) -> WorkResult<Res> {
    let fld = &tw.fld;
    let Some(dd) = uni::d_uni(tw, d, t, m)? else { return Ok(Res::Unknown) };
    let zv = fld.var(z);
    let zdd = up::scale(fld, &dd, &zv, m)?;
    let p = up::sub(fld, a, &zdd);
    let (big, small) = if d.len() >= p.len() { (d.to_vec(), p.clone()) } else { (p.clone(), d.to_vec()) };
    if small.is_empty() {
        return Ok(Res::Unknown);
    }
    let rz = up::resultant(fld, &big, &small, m)?;
    // R as a polynomial in z over C(x): monic, the coefficients must be constants
    let Some(ru) = to_uni(fld, &rz, z, m)? else { return Ok(Res::Unknown) };
    if up::deg(&ru).unwrap_or(0) == 0 {
        return Ok(Res::Unknown);
    }
    let ru = up::monic(fld, &ru, m)?;
    let x = tw.x;
    if ru.iter().any(|c| !fld.free_of(c, x) || !fld.free_of(c, t)) {
        return Ok(Res::NotConstant);
    }
    // rational roots of the monic constant polynomial: factor its numerator over Z[z, constants]
    let rp = from_uni(fld, &ru, z, m)?;
    let fac = mc_poly::factor::factor(&rp.num, false, m)?;
    let mut roots: Vec<RatFun> = vec![];
    for (p, _) in &fac.factors {
        match p.degree(z).unwrap_or(0) {
            0 => {}
            1 => {
                let cs = p.coeffs_in(z);
                let c1 = cs.iter().find(|(k, _)| *k == 1).map(|(_, c)| c.clone()).unwrap_or_else(|| Poly::zero(fld.nv));
                let c0 = cs.iter().find(|(k, _)| *k == 0).map(|(_, c)| c.clone()).unwrap_or_else(|| Poly::zero(fld.nv));
                roots.push(fld.quo(c0.neg(), c1, m)?);
            }
            _ => return Ok(Res::Unknown),
        }
    }
    let mut logs = vec![];
    // rest = a/d - sum z Dg/g
    let an = from_uni(fld, a, t, m)?;
    let dn = from_uni(fld, d, t, m)?;
    let mut rest = exact(fld.divm(&an, &dn, m)?, "d is not zero")?;
    for r in roots {
        let pz = up::sub(fld, a, &up::scale(fld, &dd, &r, m)?);
        let g = uni::gcd(fld, d, &pz, m)?;
        if up::deg(&g).unwrap_or(0) == 0 {
            continue;
        }
        // the argument as a primitive polynomial (denominators of the monic gcd cleared, the content in the other
        // variables removed): log(1 + x e^x), not log(e^x + 1/x) and a correction; the rest absorbs the difference
        let gm = from_uni(fld, &g, t, m)?;
        let gr = fld.poly(primitive_in(&gm.num, t, m)?);
        let dg = tw.d(&gr, m)?;
        let ld = exact(fld.divm(&dg, &gr, m)?, "g is not zero")?;
        rest = fld.subm(&rest, &fld.mulm(&r, &ld, m)?, m)?;
        logs.push((r, gr));
    }
    // the rest must be a Laurent polynomial in t
    let den_t = rest.den.degree(t).unwrap_or(0);
    if den_t > 0 {
        let low = rest.den.low_degree(t).unwrap_or(0);
        if low != den_t {
            return Ok(Res::Unknown);
        }
    }
    Ok(Res::Logs(logs, rest))
}

/// The coefficients `(i, a_i)` of a Laurent polynomial in `t` (a field element whose denominator is `t^j` times a
/// polynomial free of t); `None` otherwise.
fn laurent(fld: &Fld, p: &RatFun, t: usize, m: &mut Meter) -> WorkResult<Option<Vec<(i64, RatFun)>>> {
    let j = p.den.degree(t).unwrap_or(0);
    if p.den.low_degree(t).unwrap_or(0) != j {
        return Ok(None);
    }
    // den = t^j c
    let c: Poly = {
        let terms = p
            .den
            .terms()
            .map(|(e, k)| {
                let mut f = e.to_vec();
                f[t] = 0;
                (f, k.clone())
            })
            .collect();
        Poly::from_terms(fld.nv, terms)
    };
    let mut out = vec![];
    for (k, a) in p.num.coeffs_in(t) {
        out.push((k as i64 - j as i64, fld.quo(a, c.clone(), m)?));
    }
    out.sort_by_key(|(k, _)| -k);
    Ok(Some(out))
}

/// The polynomial part, `t = log(u)` primitive: `w = Dt`.
fn primitive(
    tw: &Tower<'_>,
    p: &RatFun,
    t: usize,
    w: &RatFun,
    mut part: RatFun,
    logs: Vec<(RatFun, RatFun)>,
    m: &mut Meter,
) -> WorkResult<Decision> {
    let fld = &tw.fld;
    let Some(pu) = to_uni(fld, p, t, m)? else { return Ok(Decision::Unknown) };
    let mut pu = pu;
    // from the top: a_k = D b + c w; q0 = c/(k+1) t^(k+1) + b t^k
    while up::deg(&pu).unwrap_or(0) >= 1 {
        let k = up::deg(&pu).unwrap_or(0);
        let a = pu[k].clone();
        let Some((b, c)) = limited(tw, &a, w, m)? else {
            return Ok(Decision::NonElementary(format!(
                "the coefficient of log^{k} is not D b + c D log for a rational b and a constant c (limited integration)"
            )));
        };
        let tt = fld.var(t);
        let tk = fld.powm(&tt, k as i64, m)?.expect("power");
        let tk1 = fld.mulm(&tk, &tt, m)?;
        let c1 = exact(fld.divm(&c, &fld.int(k as i64 + 1), m)?, "k + 1 is not zero")?;
        let q0 = fld.addm(&fld.mulm(&c1, &tk1, m)?, &fld.mulm(&b, &tk, m)?, m)?;
        part = fld.addm(&part, &q0, m)?;
        let dq0 = tw.d(&q0, m)?;
        let pr = from_uni(fld, &pu, t, m)?;
        let rest = fld.subm(&pr, &dq0, m)?;
        let Some(nu) = to_uni(fld, &rest, t, m)? else { return Ok(Decision::Unknown) };
        if up::deg(&nu).unwrap_or(0) >= k {
            return Ok(Decision::Unknown);
        }
        pu = nu;
    }
    let rest = pu.first().cloned().unwrap_or_else(|| fld.int(0));
    Ok(Decision::Found { part: Box::new(part), logs, rest: Box::new(rest) })
}

/// Limited integration in `C(x)`: `a = D b + c w` with `b` rational, `c` constant (`w` a logarithmic derivative,
/// proper with a square-free denominator); `None` when there is no solution.
fn limited(tw: &Tower<'_>, a: &RatFun, w: &RatFun, m: &mut Meter) -> WorkResult<Option<(RatFun, RatFun)>> {
    let fld = &tw.fld;
    let x = tw.x;
    let (an, ad) = uni::num_den(fld, a, x, m)?;
    let (q, r) = exact(up::divrem(fld, &an, &ad, m)?, "a field divides")?;
    // the polynomial part integrates term by term
    let mut b = fld.int(0);
    for (k, c) in q.iter().enumerate() {
        let xk = fld.powm(&fld.var(x), k as i64 + 1, m)?.expect("power");
        let t = exact(fld.divm(&fld.mulm(c, &xk, m)?, &fld.int(k as i64 + 1), m)?, "k + 1 is not zero")?;
        b = fld.addm(&b, &t, m)?;
    }
    let mut h = fld.int(0);
    if !r.is_empty() {
        let Some((g, hn, hd)) = hermite(tw, &r, &ad, x, m)? else { return Ok(None) };
        b = fld.addm(&b, &g, m)?;
        let hn = from_uni(fld, &hn, x, m)?;
        let hd = from_uni(fld, &hd, x, m)?;
        h = exact(fld.divm(&hn, &hd, m)?, "d is not zero")?;
    }
    if h.num.is_zero() {
        return Ok(Some((b, fld.int(0))));
    }
    if w.num.is_zero() {
        return Ok(None);
    }
    let c = exact(fld.divm(&h, w, m)?, "w is not zero")?;
    if !fld.free_of(&c, x) {
        return Ok(None);
    }
    Ok(Some((b, c)))
}

/// The polynomial part, `t = exp(w0)` hyperexponential: `w = Dw0`.
fn hyperexponential(
    tw: &Tower<'_>,
    p: &RatFun,
    t: usize,
    w: &RatFun,
    mut part: RatFun,
    logs: Vec<(RatFun, RatFun)>,
    m: &mut Meter,
) -> WorkResult<Decision> {
    let fld = &tw.fld;
    let Some(cs) = laurent(fld, p, t, m)? else { return Ok(Decision::Unknown) };
    let mut rest = fld.int(0);
    for (i, a) in cs {
        if a.num.is_zero() {
            continue;
        }
        if i == 0 {
            rest = a;
            continue;
        }
        let f = fld.mulm(&fld.int(i), w, m)?;
        match rde(tw, &f, &a, m)? {
            Rde::Solution(y) => {
                let ti = fld.powm(&fld.var(t), i, m)?.expect("t is not zero");
                part = fld.addm(&part, &fld.mulm(&y, &ti, m)?, m)?;
            }
            Rde::None => {
                return Ok(Decision::NonElementary(format!(
                    "the Risch differential equation of exp^{i} has no rational solution"
                )))
            }
            Rde::Unknown => return Ok(Decision::Unknown),
        }
    }
    let _ = Z::ONE;
    Ok(Decision::Found { part: Box::new(part), logs, rest: Box::new(rest) })
}

/// The primitive part of `p` as a polynomial in variable `t`: `p` divided by the gcd of its coefficients in `t`
/// (polynomials in the other variables), with a positive leading coefficient.
fn primitive_in(p: &Poly, t: usize, m: &mut Meter) -> WorkResult<Poly> {
    let mut g: Option<Poly> = None;
    for (_, c) in p.coeffs_in(t) {
        g = Some(match g {
            None => c,
            Some(g) => mc_poly::gcd::gcd(&g, &c, m)?,
        });
    }
    let Some(g) = g else { return Ok(p.clone()) };
    let q = exact(p.div_exact(&g, m)?, "the content divides")?;
    Ok(if q.lc().is_neg() { q.neg() } else { q })
}
