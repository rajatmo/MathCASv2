//! The user functions of the generic polynomial layer (item F2): `subresultants`, `subresultant_polynomials`,
//! `subresultant_polynomial_remainders`, `decompose`, and the `modulus:` option shared with `resultant` and
//! `discriminant`.
//!
//! Subresultants are computed over `Z[other variables]` by the chain of [`crate::unipoly`]; with `modulus: p` the
//! inputs are reduced modulo `p` first (so the degrees are those over `F_p`) and the integer chain of the reduced
//! inputs is reduced again: subresultants are integer polynomials in the coefficients, so this equals the chain over
//! `F_p[other variables]` (decision F2.3).
use crate::convert::Ring;
use crate::decompose;
use crate::natives::{Env, Out};
use crate::poly::Poly;
use crate::qpoly::QPoly;
use crate::ratfun::{self, RatFun};
use crate::ring::{PolyRing, RatFunField, Zp};
use crate::unipoly::{self as up, Chain};
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use mc_api::{Store, View, T};

/// The largest prime modulus accepted.
const MAX_MODULUS: u64 = 1 << 62;

/// `(arguments without the options, modulus)`: the `modulus: p` option of the last argument (0 or absent: none);
/// `Err` with the failure for a modulus that is not a prime.
pub(crate) fn modulus_option<'a, 'h>(env: &Env<'h>, args: &'a [T<'h>]) -> Result<(&'a [T<'h>], Option<u64>), Out<'h>> {
    let h = env.h();
    let Some((&last, rest)) = args.split_last() else { return Ok((args, None)) };
    let View::App { head, args: rules } = h.view(last) else { return Ok((args, None)) };
    if head != env.sym("options") {
        return Ok((args, None));
    }
    let key = env.sym("modulus");
    let mut modulus = None;
    for &r in rules {
        match h.view(r) {
            View::App { args: &[k, v], .. } if k == key => modulus = Some(v),
            // another option: not this function's business
            _ => return Ok((args, None)),
        }
    }
    let Some(v) = modulus else { return Ok((args, None)) };
    match env.int(v) {
        Some(0) => Ok((rest, None)),
        Some(p) if p > 1 && (p as u64) < MAX_MODULUS && crate::modp::is_prime(p as u64) => Ok((rest, Some(p as u64))),
        _ => Err(env.arg_failure("the modulus must be a prime (or 0)")),
    }
}

/// The integer polynomial of `q` with coefficients reduced to `0..p`; `None` when `p` divides the denominator.
pub(crate) fn reduce_mod(q: &QPoly, p: u64) -> Option<Poly> {
    let f = Zp::new(p);
    let d = q.den.rem_u64(p);
    if d == 0 {
        return None;
    }
    let di = f.0.inv(d);
    let terms = q.num.terms().map(|(e, c)| (e.to_vec(), Z::S(f.0.mul(c.rem_u64(p), di) as i64))).filter(|t| !t.1.is_zero()).collect();
    Some(Poly::from_terms(q.num.nvars(), terms))
}

/// `p` with its coefficients reduced to `0..prime`.
pub(crate) fn mod_poly(p: &Poly, prime: u64) -> Poly {
    let terms = p.terms().map(|(e, c)| (e.to_vec(), Z::S(c.rem_u64(prime) as i64))).filter(|t| !t.1.is_zero()).collect();
    Poly::from_terms(p.nvars(), terms)
}

/// Does the ring hold an inexact number as a kernel?
fn has_inexact(env: &Env<'_>, ring: &Ring<'_>) -> bool {
    ring.kernels.iter().any(|&k| env.ar.num(k).is_some_and(|n| !n.is_exact()))
}

/// The coefficients in variable `v`: index = degree, each a polynomial in the other variables.
pub(crate) fn coeffs(p: &Poly, v: usize) -> Vec<Poly> {
    let mut out = vec![Poly::zero(p.nvars()); p.degree(v).map_or(0, |d| d as usize + 1)];
    for (k, c) in p.coeffs_in(v) {
        out[k as usize] = c;
    }
    out
}

/// `sum c_i x_v^i`.
pub(crate) fn from_coeffs(nv: usize, v: usize, c: &[Poly]) -> Poly {
    let mut acc = Poly::zero(nv);
    let mut e = vec![0u32; nv];
    for (i, ci) in c.iter().enumerate() {
        e[v] = i as u32;
        acc = acc.add(&ci.mul_term(&e, &Z::ONE));
    }
    acc
}

/// Which list of the chain a function returns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ChainKind {
    /// principal subresultant coefficients `psc_0 .. psc_q`
    Coefficients,
    /// subresultant polynomials `S_0 .. S_q`
    Polynomials,
    /// `p, q` and the subresultant remainder sequence
    Remainders,
}

/// `subresultants(p, q, x)`, `subresultant_polynomials(p, q, x)`, `subresultant_polynomial_remainders(p, q, x)`.
pub(crate) fn chain_fn<'h>(env: &Env<'h>, args: &[T<'h>], kind: ChainKind, m: &mut Meter) -> WorkResult<Out<'h>> {
    let (args, modulus) = match modulus_option(env, args) {
        Ok(x) => x,
        Err(f) => return Ok(f),
    };
    let [p, q, x] = args else {
        return Ok(env.arg_failure("takes two polynomials and a variable: f(p, q, x)"));
    };
    if !env.is_variable(*x) {
        return Ok(env.arg_failure("the variable must be a kernel"));
    }
    let (ring, ps) = env.read(&[*p, *q], &[*x], m)?;
    if has_inexact(env, &ring) {
        return Ok(Out::Decline);
    }
    // invariant: `read` puts the given variables in the ring
    let v = ring.var(*x).expect("x is in the ring");
    let nv = ring.nvars();
    let (mut a, mut b) = (ps[0].clone(), ps[1].clone());
    if let Some(pr) = modulus {
        let (Some(ra), Some(rb)) = (reduce_mod(&a, pr), reduce_mod(&b, pr)) else { return Ok(Out::Decline) };
        a = QPoly::from_poly(ra);
        b = QPoly::from_poly(rb);
    }
    if a.is_zero() || b.is_zero() {
        return Ok(Out::Decline);
    }
    // one coefficient per degree: degrees beyond the dense limit are a memory limit, not an allocation
    a.num.check_dense()?;
    b.num.check_dense()?;
    let (mut ca, mut cb) = (coeffs(&a.num, v), coeffs(&b.num, v));
    let (mut dp, mut dq) = (ca.len() - 1, cb.len() - 1);
    // the chain is defined for deg p >= deg q; for the other order the polynomial chains stay unevaluated
    // (decision F2.3), while the coefficients follow from S_j(q, p) = (-1)^((deg p - j)(deg q - j)) S_j(p, q)
    // (Basu, Pollack and Roy, Algorithms in Real Algebraic Geometry, section 8.3)
    let mut swapped = false;
    if dp < dq {
        if kind != ChainKind::Coefficients {
            return Ok(Out::Decline);
        }
        std::mem::swap(&mut a, &mut b);
        std::mem::swap(&mut ca, &mut cb);
        std::mem::swap(&mut dp, &mut dq);
        swapped = true;
    }
    let r = PolyRing { nv };
    let ch: Chain<Poly> = up::subresultant_chain(&r, &ca, &cb, m)?;
    let lo = dq;
    let finish = |c: &Poly| -> Poly {
        match modulus {
            Some(pr) => mod_poly(c, pr),
            None => c.clone(),
        }
    };
    // S_j(P / d1, Q / d2) = S_j(P, Q) / (d1^(deg Q - j) d2^(deg P - j))
    let scale_of = |j: usize| -> Z { a.den.pow((dq - j) as u64).mul(&b.den.pow((dp - j) as u64)) };
    let term = |num: Poly, den: Z, m: &mut Meter| -> WorkResult<T<'h>> {
        let num = finish(&num);
        let qp = if modulus.is_some() { QPoly::from_poly(num) } else { QPoly::new(num, den) };
        env.conv().to_term(&qp, &ring, m)
    };
    let mut out = vec![];
    match kind {
        ChainKind::Coefficients => {
            for j in 0..=lo {
                let c = ch.psc(&r, j);
                let c = if swapped && (dp - j) * (dq - j) % 2 == 1 { c.neg() } else { c };
                out.push(term(c, scale_of(j), m)?);
            }
        }
        ChainKind::Polynomials => {
            for j in 0..=lo {
                out.push(term(from_coeffs(nv, v, &ch.s[j]), scale_of(j), m)?);
            }
        }
        ChainKind::Remainders => {
            out.push(term(a.num.clone(), a.den.clone(), m)?);
            out.push(term(b.num.clone(), b.den.clone(), m)?);
            // the remainders of the chain of (P, Q) with deg P >= deg Q, in decreasing degree
            for s in ch.remainders() {
                let j = s.len() - 1;
                let s = from_coeffs(nv, v, &s);
                if finish(&s).is_zero() {
                    continue;
                }
                out.push(term(s, scale_of(j), m)?);
            }
        }
    }
    Ok(Out::Done(env.list(&out)))
}

/// The largest degree `decompose` works on (dense coefficient lists of that length).
const MAX_DECOMPOSE_DEGREE: u32 = 1 << 20;

/// `decompose(p, x)` and `decompose(p, x, modulus: p)`: the list of components, outermost first (decision F2.4).
pub(crate) fn decompose_fn<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let (args, modulus) = match modulus_option(env, args) {
        Ok(x) => x,
        Err(f) => return Ok(f),
    };
    let [p, x] = args else {
        return Ok(env.arg_failure("decompose(p, x) takes a polynomial and a variable"));
    };
    if !env.is_variable(*x) {
        return Ok(env.arg_failure("the variable must be a kernel"));
    }
    let c = crate::convert::Conv::rational(&env.ar, &env.hd);
    let (ring, _) = c.ring(&[*p], &[*x], m)?;
    if has_inexact(env, &ring) {
        return Ok(Out::Decline);
    }
    let Ok(f) = ratfun::read(&c, *p, &ring, m)? else { return Ok(Out::Decline) };
    // invariant: `ring` was built with x
    let v = ring.var(*x).expect("x is in the ring");
    let nv = ring.nvars();
    if f.den.degree(v).unwrap_or(0) > 0 {
        return Ok(Out::Decline);
    }
    if f.num.degree(v).unwrap_or(0) <= 1 {
        return Ok(Out::Done(env.list(&[*p])));
    }
    // the decomposition works on dense coefficient lists: x^(2^31) + ... would ask for a 16 GB list, and the
    // allocation failure aborts the process (found by the round 22 fuzz campaign; the same cap as MAX_TRIG_UNITS in
    // the trigonometric rewrites of mc-simplify)
    if f.num.degree(v).unwrap_or(0) > MAX_DECOMPOSE_DEGREE {
        return Ok(Out::Decline);
    }
    let comps: Vec<Vec<RatFun>> = match modulus {
        Some(pr) => {
            if f.num.used_vars().iter().any(|&u| u != v) || f.den.used_vars().iter().any(|&u| u != v) {
                return Ok(Out::Decline);
            }
            let zp = Zp::new(pr);
            // invariant: the denominator is a nonzero constant here
            let den = f.den.as_constant().expect("a constant denominator");
            let dmod = den.rem_u64(pr);
            if dmod == 0 {
                return Ok(Out::Decline);
            }
            let di = zp.0.inv(dmod);
            let dense: Vec<u64> = f.num.to_dense(v).iter().map(|c| zp.0.mul(c.rem_u64(pr), di)).collect();
            let dense = up::trim(&zp, dense);
            if dense.len() <= 2 {
                return Ok(Out::Done(env.list(&[env.conv().to_term(&QPoly::from_poly(Poly::from_dense(nv, v, &dense.iter().map(|&c| Z::S(c as i64)).collect::<Vec<_>>())), &ring, m)?])));
            }
            let els: Option<Vec<u64>> = (pr <= MAX_ENUMERATED).then(|| (0..pr).collect());
            let Some(d) = decompose::decompose(&zp, &dense, els.as_deref(), m)? else { return Ok(Out::Decline) };
            let one = Poly::constant(nv, Z::ONE);
            d.into_iter()
                .map(|g| g.iter().map(|&c| RatFun { num: Poly::constant(nv, Z::S(c as i64)), den: one.clone() }).collect())
                .collect()
        }
        None => {
            let field = RatFunField { nv };
            let mut dense = vec![];
            for cn in coeffs(&f.num, v) {
                dense.push(field.quotient(&cn, &f.den, m)?);
            }
            let dense = up::trim(&field, dense);
            let Some(d) = decompose::decompose(&field, &dense, None, m)? else { return Ok(Out::Decline) };
            d
        }
    };
    // each component as sum c_i x^i, a coefficient with a constant denominator expanded into rationals
    let mut terms = vec![];
    for g in &comps {
        let mut parts = vec![];
        let mut e = vec![0u32; nv];
        for (i, ci) in g.iter().enumerate() {
            if ci.num.is_zero() {
                continue;
            }
            e[v] = i as u32;
            let num = ci.num.mul_term(&e, &Z::ONE);
            parts.push(match ci.den.as_constant() {
                Some(d) => env.conv().to_term(&QPoly::new(num, d), &ring, m)?,
                None => ratfun::to_term(&c, &RatFun { num, den: ci.den.clone() }, &ring, m)?,
            });
        }
        terms.push(if parts.is_empty() { env.h().int(0) } else { env.ar.add(&parts) });
    }
    Ok(Out::Done(env.list(&terms)))
}

/// The largest prime field whose elements the wild case of `decompose` enumerates.
const MAX_ENUMERATED: u64 = 4096;

// ------------------------------------------------------------------------------------------- products

/// Factors with exponents.
type Factors<'h> = Vec<(T<'h>, u64)>;

/// Does `t` contain `x` as a subterm?
fn contains<'h>(env: &Env<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = env.h();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// The factors of a product term with their positive integer exponents; `None` when a factor depending on `x`
/// is not a polynomial power (a negative or fractional power).
fn product_factors<'h>(env: &Env<'h>, t: T<'h>, x: T<'h>) -> Option<Vec<(T<'h>, u64)>> {
    let h = env.h();
    let items: Vec<T<'h>> = match h.view(t) {
        View::App { head, args } if head == env.hd.mul => args.to_vec(),
        _ => vec![t],
    };
    let mut out = vec![];
    for it in items {
        match h.view(it) {
            View::App { head, args: &[b, e] } if head == env.hd.pow && contains(env, b, x) => match env.int(e) {
                Some(n) if n > 0 => out.push((b, n as u64)),
                _ => return None,
            },
            _ => out.push((it, 1)),
        }
    }
    Some(out)
}

/// Is the product worth splitting: two factors depending on `x`, or one with an exponent above 1?
fn splits(env: &Env<'_>, fs: &[(T<'_>, u64)], x: T<'_>) -> bool {
    let dep: Vec<&(T<'_>, u64)> = fs.iter().filter(|(f, _)| contains(env, *f, x)).collect();
    dep.len() >= 2 || dep.iter().any(|(_, e)| *e > 1)
}

/// `resultant(p, q, x)` of products, multiplicatively: `res(prod f_i^e_i, prod g_j^k_j) = prod res(f_i, g_j)^(e_i
/// k_j)`, each factor's resultant expanded and the product kept, as the inputs were (decision F2.9); `None` when
/// neither input is such a product.
pub(crate) fn product_resultant<'h>(env: &Env<'h>, p: T<'h>, q: T<'h>, x: T<'h>, m: &mut Meter) -> WorkResult<Option<T<'h>>> {
    let (Some(fp), Some(fq)) = (product_factors(env, p, x), product_factors(env, q, x)) else { return Ok(None) };
    if !splits(env, &fp, x) && !splits(env, &fq, x) {
        return Ok(None);
    }
    let mut parts = vec![];
    for &(f, e) in &fp {
        for &(g, k) in &fq {
            let Out::Done(r) = crate::functions::resultant(env, &[f, g, x], m)? else { return Ok(None) };
            if env.ar.num(r).is_some_and(|n| n.is_exact_zero()) {
                return Ok(Some(r));
            }
            parts.push(env.ar.pow(r, env.h().int((e * k) as i64)));
        }
    }
    Ok(Some(env.ar.mul(&parts)))
}

/// `discriminant(p, x)` of a product: `disc(c prod f_i) = c^(2n-2) prod disc(f_i) prod_(i<j) res(f_i, f_j)^2`, zero
/// with a repeated factor (decision F2.9); `None` when `p` is not such a product.
pub(crate) fn product_discriminant<'h>(env: &Env<'h>, p: T<'h>, x: T<'h>, m: &mut Meter) -> WorkResult<Option<T<'h>>> {
    let Some(fp) = product_factors(env, p, x) else { return Ok(None) };
    if !splits(env, &fp, x) {
        return Ok(None);
    }
    let zero = env.h().int(0);
    let (dep, cst): (Factors<'h>, Factors<'h>) = fp.into_iter().partition(|(f, _)| contains(env, *f, x));
    if dep.iter().any(|(_, e)| *e > 1) {
        return Ok(Some(zero));
    }
    // the degree of the whole polynomial in x
    let (ring, ps) = env.read(&[p], &[x], m)?;
    // invariant: `read` puts the given variables in the ring
    let v = ring.var(x).expect("x is in the ring");
    let n = ps[0].num.degree(v).unwrap_or(0) as i64;
    let mut parts = vec![];
    for (c, e) in cst {
        parts.push(env.ar.pow(c, env.h().int(e as i64 * (2 * n - 2))));
    }
    for (i, &(f, _)) in dep.iter().enumerate() {
        let Out::Done(d) = crate::functions::discriminant(env, &[f, x], m)? else { return Ok(None) };
        parts.push(d);
        for &(g, _) in &dep[i + 1..] {
            let Out::Done(r) = crate::functions::resultant(env, &[f, g, x], m)? else { return Ok(None) };
            parts.push(env.ar.pow(r, env.h().int(2)));
        }
    }
    Ok(Some(env.ar.mul(&parts)))
}
