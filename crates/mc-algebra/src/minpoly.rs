//! Minimal polynomials of expressions by composed sums and products (item F2, open F1 gap): for `a + b`, `a b`
//! and `a^(p/q)` the minimal polynomial divides a resultant of the operands' minimal polynomials,
//! `Res_y(P(y), Q(z - y))`, `Res_y(P(y), y^n Q(z/y))` and `Res_x(P(x^q), z - x^p)` (Loos, "Computing in algebraic
//! extensions", in *Computer Algebra: Symbolic and Algebraic Computation*, 1982; Cohen, section 4.1), and the factor
//! is the one that vanishes on a ball enclosure of the value (`algnum::choose_factor`). No number field or primitive
//! element is built, which is what made `minimal_polynomial` of sums of radicals slow. Atoms (root objects, `im`,
//! roots of unity) come from [`crate::convert::to_alg`]. `None` sends the caller to the general method.
use crate::algnum::{choose_factor, AlgNum};
use crate::convert::{to_alg, Memo};
use crate::cx::Env;
use crate::qpoly::{self, q, QPoly};
use mc_api::{Store, View, T};
use mc_arith::num::Rat;
use mc_numeric::cball::{self, CBall};
use mc_poly::resultant::resultant;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};

/// The largest degree of an intermediate resultant.
const MAX_DEGREE: usize = 256;

/// An expression tree over atoms.
enum Node {
    Atom(AlgNum),
    Sum(Vec<Node>),
    Prod(Vec<Node>),
    Pow(Box<Node>, Rat),
}

fn build<'h>(env: &Env<'h>, t: T<'h>, memo: &mut Memo<'h>, m: &mut Meter) -> WorkResult<Option<Node>> {
    m.step(1)?;
    let h = env.h();
    if let View::App { head, args } = h.view(t) {
        if head == env.hd.add || head == env.hd.mul {
            let mut xs = Vec::with_capacity(args.len());
            for &a in args {
                let Some(n) = build(env, a, memo, m)? else { return Ok(None) };
                xs.push(n);
            }
            return Ok(Some(if head == env.hd.add { Node::Sum(xs) } else { Node::Prod(xs) }));
        }
        if head == env.hd.pow && args.len() == 2 && args[0] != env.sym("e") {
            if let Some(e) = env.ar.num(args[1]).and_then(|e| e.as_rat().cloned()) {
                let Some(b) = build(env, args[0], memo, m)? else { return Ok(None) };
                return Ok(Some(Node::Pow(Box::new(b), e)));
            }
        }
    }
    Ok(to_alg(env, t, memo, m)?.map(Node::Atom))
}

/// An enclosure of the value at about `bits` bits.
fn enclose(n: &Node, bits: u64, m: &mut Meter) -> WorkResult<Option<CBall>> {
    let w = bits + 16;
    Ok(match n {
        Node::Atom(a) => a.enclose(bits, m)?,
        Node::Sum(xs) | Node::Prod(xs) => {
            let sum = matches!(n, Node::Sum(_));
            let mut acc = CBall::from_i64(if sum { 0 } else { 1 });
            for x in xs {
                let Some(z) = enclose(x, w, m)? else { return Ok(None) };
                acc = if sum { acc.add(&z, w) } else { acc.mul(&z, w) };
            }
            Some(acc)
        }
        Node::Pow(b, e) => {
            let Some(z) = enclose(b, w + 16, m)? else { return Ok(None) };
            if z.contains_zero() {
                return Ok(None);
            }
            cball::pow_rat(&z, e, w)
        }
    })
}

fn int_poly(p: &[Rat]) -> Vec<Z> {
    qpoly::primitive(p).into_iter().map(Z::from_int).collect()
}

/// `sum_i c_i t^i` for a polynomial `t` of the two-variable ring.
fn compose2(c: &[Z], t: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    let mut acc = Poly::zero(2);
    for ci in c.iter().rev() {
        acc = acc.mul(t, m)?.add(&Poly::constant(2, ci.clone()));
    }
    Ok(acc)
}

/// `Res_y(a(y), b(y, z))` as a polynomial in z (y is variable 0, z variable 1).
fn res_y(a: &[Z], b: &Poly, m: &mut Meter) -> WorkResult<QPoly> {
    let pa = Poly::from_dense(2, 0, a);
    let r = resultant(&pa, b, 0, m)?;
    Ok(qpoly::trim(r.to_dense(1).iter().map(|c| Rat::from_int(c.to_int())).collect()))
}

/// The monic minimal polynomial of the node's value; `None` when it could not be chosen.
fn minpoly(n: &Node, m: &mut Meter) -> WorkResult<Option<QPoly>> {
    let y = Poly::var(2, 0);
    let z = Poly::var(2, 1);
    let candidate: QPoly = match n {
        Node::Atom(a) => return Ok(Some(qpoly::monic(&a.minpoly(m)?))),
        Node::Sum(xs) | Node::Prod(xs) => {
            let sum = matches!(n, Node::Sum(_));
            // fold left, choosing a factor after every step
            let mut acc = match minpoly(&xs[0], m)? {
                Some(p) => p,
                None => return Ok(None),
            };
            for (i, x) in xs.iter().enumerate().skip(1) {
                let Some(qx) = minpoly(x, m)? else { return Ok(None) };
                if (acc.len() - 1) * (qx.len() - 1) > MAX_DEGREE {
                    return Ok(None);
                }
                let (pa, qz) = (int_poly(&acc), int_poly(&qx));
                let r = if sum {
                    // Q(z - y)
                    res_y(&pa, &compose2(&qz, &z.sub(&y), m)?, m)?
                } else {
                    if acc.len() == 2 && acc[0].is_zero() || qx.len() == 2 && qx[0].is_zero() {
                        // a zero factor
                        acc = vec![q(0), q(1)];
                        continue;
                    }
                    // y^n Q(z / y) = sum q_i z^i y^(n - i)
                    let nq = qz.len() - 1;
                    let terms = qz.iter().enumerate().filter(|(_, c)| !c.is_zero()).map(|(i, c)| (vec![(nq - i) as u32, i as u32], c.clone())).collect();
                    res_y(&pa, &Poly::from_terms(2, terms), m)?
                };
                let part = |bits: u64, m: &mut Meter| -> WorkResult<Option<CBall>> {
                    let w = bits + 16;
                    let mut accb = CBall::from_i64(if sum { 0 } else { 1 });
                    for x in &xs[..=i] {
                        let Some(zb) = enclose(x, w, m)? else { return Ok(None) };
                        accb = if sum { accb.add(&zb, w) } else { accb.mul(&zb, w) };
                    }
                    Ok(Some(accb))
                };
                let Some(f) = choose(&r, part, m)? else { return Ok(None) };
                acc = f;
            }
            return Ok(Some(acc));
        }
        Node::Pow(b, e) => {
            let Some(pb) = minpoly(b, m)? else { return Ok(None) };
            if pb.len() == 2 && pb[0].is_zero() {
                // 0^e: zero for e > 0
                return Ok(if e.num.neg || e.is_zero() { None } else { Some(vec![q(0), q(1)]) });
            }
            let qd = e.den.to_u128().and_then(|v| usize::try_from(v).ok()).filter(|&v| v <= 64);
            let pn = e.num.to_i64().filter(|v| v.unsigned_abs() <= 64);
            let (Some(qd), Some(pn)) = (qd, pn) else { return Ok(None) };
            if (pb.len() - 1) * qd * pn.unsigned_abs() as usize > MAX_DEGREE {
                return Ok(None);
            }
            // 1/b: the reversed polynomial
            let base: QPoly = if pn < 0 { qpoly::monic(&pb.iter().rev().cloned().collect::<Vec<_>>()) } else { pb };
            let p = pn.unsigned_abs() as usize;
            // x = b^(1/qd) is a root of base(x^qd); the value x^p a root of Res_x(base(x^qd), z - x^p)
            let mut stretched = vec![q(0); (base.len() - 1) * qd + 1];
            for (i, c) in base.iter().enumerate() {
                stretched[i * qd] = c.clone();
            }
            if p == 1 {
                stretched
            } else {
                let mut xp = vec![0u32, 0];
                xp[0] = p as u32;
                let lin = z.sub(&Poly::monomial(&xp, Z::ONE));
                res_y(&int_poly(&stretched), &lin, m)?
            }
        }
    };
    let whole = |bits: u64, m: &mut Meter| enclose(n, bits, m);
    choose(&candidate, whole, m)
}

/// The irreducible factor of `r` that vanishes on the enclosures.
fn choose(
    r: &[Rat],
    mut z: impl FnMut(u64, &mut Meter) -> WorkResult<Option<CBall>>,
    m: &mut Meter,
) -> WorkResult<Option<QPoly>> {
    if r.len() <= 1 {
        return Ok(None);
    }
    let fs: Vec<QPoly> = qpoly::factor(r, m)?.into_iter().map(|(f, _)| f).collect();
    choose_factor(&fs, &mut z, m)
}

/// The monic minimal polynomial of `t` and a 256-bit enclosure of its value, by composed sums and products; `None`
/// when the term is not such an expression or a choice could not be made.
pub(crate) fn fast_minpoly<'h>(env: &Env<'h>, t: T<'h>, m: &mut Meter) -> WorkResult<Option<(QPoly, CBall)>> {
    let mut memo = Memo::default();
    let Some(n) = build(env, t, &mut memo, m)? else { return Ok(None) };
    if matches!(n, Node::Atom(_)) {
        // the general method handles a single atom directly
        return Ok(None);
    }
    let Some(p) = minpoly(&n, m)? else { return Ok(None) };
    let Some(z) = enclose(&n, 256, m)? else { return Ok(None) };
    Ok(Some((p, z)))
}
