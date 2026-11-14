//! Strong Groebner bases over the integers.
//!
//! Over a principal ideal domain such as Z a Groebner basis must describe the leading coefficients as well as the
//! leading monomials: a set G of polynomials of an ideal I is a **strong** Groebner basis when every nonzero f of I
//! has an element g of G whose leading term divides the leading term of f, coefficients included (Adams and
//! Loustaunau, *An Introduction to Groebner Bases*, 4.5; Becker and Weispfenning, *Groebner Bases*, 10.1). Then
//! membership is decided by reduction and the elements of G free of the first variables generate the elimination
//! ideal over Z, as over a field.
//!
//! The method is Buchberger's algorithm for a Euclidean coefficient ring (Kandri-Rody and Kapur, "Computing a
//! Groebner basis of a polynomial ideal over a Euclidean domain", J. Symbolic Comput. 6 (1988); Lichtblau,
//! "Effective computation of strong Groebner bases over Euclidean domains", Illinois J. Math. 56 (2012)). For a
//! pair f, g with leading coefficients a, b and leading monomials whose lcm is L, two polynomials are formed:
//!
//! - the S-polynomial `(c/a) (L/lm f) f - (c/b) (L/lm g) g`, c = lcm(a, b), which cancels the leading terms;
//! - the G-polynomial `u (L/lm f) f + v (L/lm g) g` with `u a + v b = gcd(a, b)`, whose leading coefficient is the
//!   gcd (not needed when a divides b or b divides a).
//!
//! Reduction is Euclidean: a term `c x^e` with `lm g | x^e` becomes `(c mod lc g) x^e` (remainder in
//! `[0, lc g)`, leading coefficients positive). The S-polynomial of two elements whose leading monomials and
//! leading coefficients are both coprime reduces to zero (the product criterion over Z) and is skipped. The result
//! is the **reduced** strong basis: no leading term divides another, every coefficient of a tail reduced modulo the
//! leading coefficients of the elements whose leading monomial divides its monomial, leading coefficients positive,
//! sorted by increasing leading monomial; it depends only on the ideal and the order (Pauer, "Groebner bases with
//! coefficients in rings", J. Symbolic Comput. 42 (2007)). Every loop charges the [`Meter`].
use crate::groebner::MonomialOrder;
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use std::cmp::Ordering;

/// A polynomial over Z: terms `(exponents, coefficient)`, nonzero, sorted by decreasing monomial.
pub type ZPoly = Vec<(Vec<u32>, Z)>;

fn divides(a: &[u32], b: &[u32]) -> bool {
    a.iter().zip(b).all(|(x, y)| x <= y)
}

fn lcm_m(a: &[u32], b: &[u32]) -> Vec<u32> {
    a.iter().zip(b).map(|(x, y)| *x.max(y)).collect()
}

fn minus(a: &[u32], b: &[u32]) -> Vec<u32> {
    a.iter().zip(b).map(|(x, y)| x - y).collect()
}

/// The floor quotient of c by a positive d.
fn floor_div(c: &Z, d: &Z) -> Z {
    let (q, r) = c.divrem(d);
    if r.is_neg() {
        q.sub(&Z::ONE)
    } else {
        q
    }
}

/// `(g, u, v)` with `u a + v b = g = gcd(a, b) >= 0` (extended Euclid).
fn xgcd(a: &Z, b: &Z, m: &mut Meter) -> WorkResult<(Z, Z, Z)> {
    let (mut r0, mut r1) = (a.clone(), b.clone());
    let (mut s0, mut s1) = (Z::ONE, Z::ZERO);
    let (mut t0, mut t1) = (Z::ZERO, Z::ONE);
    while !r1.is_zero() {
        m.step(1)?;
        let (q, r) = r0.divrem(&r1);
        let s = s0.sub(&q.mul(&s1));
        let t = t0.sub(&q.mul(&t1));
        (r0, r1) = (r1, r);
        (s0, s1) = (s1, s);
        (t0, t1) = (t1, t);
    }
    if r0.is_neg() {
        Ok((r0.neg(), s0.neg(), t0.neg()))
    } else {
        Ok((r0, s0, t0))
    }
}

/// One side of a combination: the factor `c x^e` and the polynomial.
type Side<'a> = (&'a Z, &'a [u32], &'a ZPoly);

/// `a x^ea f + b x^eb g` (either factor may be zero).
fn combine((a, ea, f): Side<'_>, (b, eb, g): Side<'_>, o: MonomialOrder, m: &mut Meter) -> WorkResult<ZPoly> {
    let shift = |e: &[u32], s: &[u32]| -> Vec<u32> { e.iter().zip(s).map(|(x, y)| x + y).collect() };
    let fs: Vec<(Vec<u32>, Z)> =
        if a.is_zero() { vec![] } else { f.iter().map(|(e, c)| (shift(e, ea), c.mul(a))).collect() };
    let gs: Vec<(Vec<u32>, Z)> =
        if b.is_zero() { vec![] } else { g.iter().map(|(e, c)| (shift(e, eb), c.mul(b))).collect() };
    let mut out = Vec::with_capacity(fs.len() + gs.len());
    let (mut i, mut j) = (0, 0);
    while i < fs.len() || j < gs.len() {
        m.step(1)?;
        let ord = match (fs.get(i), gs.get(j)) {
            (Some(x), Some(y)) => o.cmp(&x.0, &y.0),
            (Some(_), None) => Ordering::Greater,
            _ => Ordering::Less,
        };
        match ord {
            Ordering::Greater => {
                out.push(fs[i].clone());
                i += 1;
            }
            Ordering::Less => {
                out.push(gs[j].clone());
                j += 1;
            }
            Ordering::Equal => {
                let c = fs[i].1.add(&gs[j].1);
                if !c.is_zero() {
                    out.push((fs[i].0.clone(), c));
                }
                i += 1;
                j += 1;
            }
        }
    }
    Ok(out)
}

/// The Euclidean normal form of f by `basis` (every element with a positive leading coefficient): each term in
/// turn, from the largest, has its coefficient reduced modulo the leading coefficient of every element whose
/// leading monomial divides its monomial, until no reduction changes it. `from` terms at the front are kept as they
/// are (the leading term of a basis element during inter-reduction).
fn normal_form(f: ZPoly, basis: &[&ZPoly], from: usize, o: MonomialOrder, m: &mut Meter) -> WorkResult<ZPoly> {
    let mut done: ZPoly = f[..from.min(f.len())].to_vec();
    let mut rest: ZPoly = f[from.min(f.len())..].to_vec();
    while let Some((e, c)) = rest.first().cloned() {
        m.step(1)?;
        let mut changed = false;
        for g in basis {
            let (ge, gc) = &g[0];
            if !divides(ge, &e) {
                continue;
            }
            let q = floor_div(&c, gc);
            if q.is_zero() {
                continue;
            }
            let zero = vec![0; e.len()];
            rest = combine((&Z::ONE, &zero, &rest), (&q.neg(), &minus(&e, ge), g), o, m)?;
            changed = true;
            break;
        }
        if !changed {
            done.push(rest.remove(0));
        }
    }
    Ok(done)
}

/// Make the leading coefficient positive.
fn positive(mut f: ZPoly) -> ZPoly {
    if f.first().is_some_and(|t| t.1.is_neg()) {
        for t in &mut f {
            t.1 = t.1.neg();
        }
    }
    f
}

/// Does the leading term of g divide that of f (monomial and coefficient)?
fn term_divides(g: &ZPoly, f: &ZPoly) -> bool {
    divides(&g[0].0, &f[0].0) && f[0].1.divisible(&g[0].1)
}

/// The reduced strong Groebner basis over Z of the ideal generated by `input` (terms in any order; zero
/// polynomials ignored) in the monomial order `o`. `[[(0, 1)]]`-like output `[1]` means the unit ideal; an empty
/// output the zero ideal.
pub fn strong_basis(input: &[ZPoly], o: MonomialOrder, m: &mut Meter) -> WorkResult<Vec<ZPoly>> {
    let mut basis: Vec<ZPoly> = vec![];
    let mut alive: Vec<bool> = vec![];
    let mut pairs: Vec<(usize, usize)> = vec![];
    let add = |h: ZPoly, basis: &mut Vec<ZPoly>, alive: &mut Vec<bool>, pairs: &mut Vec<(usize, usize)>| {
        let k = basis.len();
        for (i, _) in alive.iter().enumerate().filter(|(_, &a)| a) {
            pairs.push((i, k));
        }
        basis.push(h);
        alive.push(true);
    };
    for f in input {
        let mut f = f.clone();
        f.retain(|t| !t.1.is_zero());
        f.sort_by(|a, b| o.cmp(&b.0, &a.0));
        let refs: Vec<&ZPoly> = basis.iter().zip(&alive).filter(|(_, &a)| a).map(|(g, _)| g).collect();
        let h = positive(normal_form(f, &refs, 0, o, m)?);
        if !h.is_empty() {
            add(h, &mut basis, &mut alive, &mut pairs);
        }
    }
    while let Some(k) = (0..pairs.len()).min_by(|&a, &b| {
        let la = lcm_m(&basis[pairs[a].0][0].0, &basis[pairs[a].1][0].0);
        let lb = lcm_m(&basis[pairs[b].0][0].0, &basis[pairs[b].1][0].0);
        o.cmp(&la, &lb)
    }) {
        m.step(1)?;
        let (i, j) = pairs.swap_remove(k);
        if !alive[i] || !alive[j] {
            continue;
        }
        let (f, g) = (basis[i].clone(), basis[j].clone());
        let ((ef, a), (eg, b)) = (&f[0], &g[0]);
        let l = lcm_m(ef, eg);
        let (tf, tg) = (minus(&l, ef), minus(&l, eg));
        let (d, u, v) = xgcd(a, b, m)?;
        let mut new = vec![];
        let coprime_m = ef.iter().zip(eg).all(|(x, y)| *x == 0 || *y == 0);
        if !(coprime_m && d.is_one()) {
            // c/a = b/d and c/b = a/d
            new.push(combine((&b.div_exact(&d), &tf, &f), (&a.div_exact(&d).neg(), &tg, &g), o, m)?);
        }
        if !a.divisible(b) && !b.divisible(a) {
            new.push(combine((&u, &tf, &f), (&v, &tg, &g), o, m)?);
        }
        for s in new {
            let refs: Vec<&ZPoly> = basis.iter().zip(&alive).filter(|(_, &a)| a).map(|(g, _)| g).collect();
            let h = positive(normal_form(s, &refs, 0, o, m)?);
            if h.is_empty() {
                continue;
            }
            // elements whose leading term the new one divides are no longer needed for the leading terms; they
            // stay members of the ideal through the new element, which came from them
            add(h, &mut basis, &mut alive, &mut pairs);
        }
    }
    // the minimal basis: drop an element whose leading term another one divides (of two equal ones the later)
    let live: Vec<ZPoly> = basis.into_iter().zip(alive).filter(|(_, a)| *a).map(|(g, _)| g).collect();
    let mut minimal: Vec<ZPoly> = vec![];
    for (k, g) in live.iter().enumerate() {
        let redundant = live.iter().enumerate().any(|(l, h)| {
            l != k && term_divides(h, g) && (!term_divides(g, h) || l < k)
        });
        if !redundant {
            minimal.push(g.clone());
        }
    }
    // a unit constant: the whole ring
    if let Some(g) = minimal.iter().find(|g| g.len() == 1 && g[0].0.iter().all(|&e| e == 0) && g[0].1.is_one()) {
        return Ok(vec![g.clone()]);
    }
    let mut reduced = Vec::with_capacity(minimal.len());
    for k in 0..minimal.len() {
        let others: Vec<&ZPoly> = minimal.iter().enumerate().filter(|(l, _)| *l != k).map(|(_, g)| g).collect();
        reduced.push(normal_form(minimal[k].clone(), &others, 1, o, m)?);
    }
    reduced.sort_by(|a, b| o.cmp(&a[0].0, &b[0].0).then_with(|| a[0].1.cmp_z(&b[0].1)));
    Ok(reduced)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::Unlimited;

    fn p(ts: &[(&[u32], i64)]) -> ZPoly {
        ts.iter().map(|(e, c)| (e.to_vec(), Z::S(*c))).collect()
    }

    #[test]
    fn two_coprime_constants_give_one() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let b = strong_basis(&[p(&[(&[0], 2)]), p(&[(&[0], 3)])], MonomialOrder::Lex, &mut m).unwrap();
        assert_eq!(b, vec![p(&[(&[0], 1)])]);
    }

    #[test]
    fn linear_forms_with_coefficients() {
        // <2x, 3y> over Z contains xy (= -y 2x + x 3y... as x y = 3y x - 2x y), not x or y
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let b = strong_basis(&[p(&[(&[1, 0], 2)]), p(&[(&[0, 1], 3)])], MonomialOrder::Lex, &mut m).unwrap();
        assert_eq!(b, vec![p(&[(&[0, 1], 3)]), p(&[(&[1, 0], 2)]), p(&[(&[1, 1], 1)])]);
    }
}
