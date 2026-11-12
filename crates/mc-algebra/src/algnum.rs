//! Algebraic numbers: an element of a number field whose generator has an embedding, so the element is one
//! complex number. Enclosures are certified balls, refined on demand; the sign of a real algebraic number and the
//! equality of two are decided exactly (an element that is not zero in the field is not zero as a number, and its
//! enclosure eventually excludes zero).
//!
//! Primitive elements of towers ([`adjoin`]): `Q(θ, β) = Q(γ)` with `γ = β + s θ` for the first integer `s` (0,
//! 1, -1, 2, ...) at which the norm `N_s(y) = Res_t(f(t), g(y - s t))` is square-free (Trager 1976; Cohen,
//! Algorithm 3.6.4 and section 4.5.4). `γ`'s minimal polynomial is the factor of `N_s` that vanishes at the
//! numeric value of `γ` (decided with balls), its embedding the root that value lies in, and `θ` is the root of the
//! linear `gcd(f(t), g(γ - s t))` over `Q(γ)`.
use crate::embed::{self, Embedding};
use crate::field::{Elem, NumberField};
use crate::kpoly;
use crate::qpoly::{self, q, QPoly};
use mc_arith::num::Rat;
use mc_numeric::cball::CBall;
use mc_poly::work::{Meter, WorkResult};

/// The highest precision tried for a sign or a choice between roots (bits).
const MAX_BITS: u64 = 1 << 15;

/// The rationals as a field of degree 1 (generator 0), with its embedding.
pub fn rationals() -> NumberField {
    NumberField::new(vec![q(0), q(1)], Some(Embedding { index: 1, real: true, re: q(0), im: q(0), radius: q(0) }))
}

/// An algebraic number: an element of a field with an embedding.
#[derive(Clone, Debug, PartialEq)]
pub struct AlgNum {
    /// the field (it has an embedding)
    pub k: NumberField,
    /// the element
    pub a: Elem,
}

impl AlgNum {
    /// A rational number.
    pub fn rational(c: &Rat) -> AlgNum {
        let k = rationals();
        let a = k.of(c);
        AlgNum { k, a }
    }

    /// The root `e` of the monic irreducible `f`.
    pub fn root(f: &[Rat], e: Embedding) -> AlgNum {
        let k = NumberField::new(f.to_vec(), Some(e));
        let a = k.gen();
        AlgNum { k, a }
    }

    /// Is the number rational? Its value then.
    pub fn as_rat(&self) -> Option<Rat> {
        self.k.as_rat(&self.a)
    }

    /// An enclosure at about `bits` bits of the generator.
    pub fn enclose(&self, bits: u64, m: &mut Meter) -> WorkResult<Option<CBall>> {
        if let Some(c) = self.as_rat() {
            return Ok(Some(CBall::real(embed::rat_ball(&c, bits.max(64)))));
        }
        let Some(t) = generator_enclosure(&self.k, bits, m)? else { return Ok(None) };
        Ok(Some(embed::eval_ball(&self.a, &t, bits.max(64) + 16)))
    }

    /// The sign of a real algebraic number (`-1`, `0`, `1`); `None` when the number is not known to be real or the
    /// precision limit was reached.
    pub fn sign(&self, m: &mut Meter) -> WorkResult<Option<i32>> {
        if self.a.is_empty() {
            return Ok(Some(0));
        }
        if let Some(c) = self.as_rat() {
            return Ok(Some(if c.num.neg { -1 } else { 1 }));
        }
        if !self.k.is_real() {
            // a real number in a field with a complex generator: its own field Q(a) has a real one
            return match simple(self, m)? {
                Some(s) if s.k.is_real() => s.sign(m),
                _ => Ok(None),
            };
        }
        let mut bits = 64;
        while bits <= MAX_BITS {
            let Some(z) = self.enclose(bits, m)? else { return Ok(None) };
            if !z.re.contains_zero() {
                return Ok(Some(if z.re.mid.neg { -1 } else { 1 }));
            }
            bits *= 2;
        }
        Ok(None)
    }

    /// The minimal polynomial over Q (monic).
    pub fn minpoly(&self, m: &mut Meter) -> WorkResult<QPoly> {
        self.k.minpoly(&self.a, m)
    }

    /// A machine approximation `(re, im)`.
    pub fn approx(&self, m: &mut Meter) -> WorkResult<Option<(f64, f64)>> {
        Ok(self.enclose(64, m)?.map(|z| (ball_f64(&z.re), ball_f64(&z.im))))
    }
}

fn ball_f64(b: &mc_arith::num::Ball) -> f64 {
    embed::rat_f64(&embed::float_rat(&b.mid))
}

/// An enclosure of the generator of a field with an embedding.
pub fn generator_enclosure(k: &NumberField, bits: u64, m: &mut Meter) -> WorkResult<Option<CBall>> {
    let Some(e) = k.embedding() else { return Ok(None) };
    embed::enclose(k.modulus(), e, bits, m)
}

/// `Q(θ, β)` as one field.
#[derive(Clone, Debug)]
pub struct Tower {
    /// the field `Q(γ)`, with its embedding
    pub field: NumberField,
    /// the old generator `θ` in the new field
    pub theta: Elem,
    /// the adjoined number `β` in the new field
    pub beta: Elem,
    /// `Some(s)` when the new generator is `γ = β + s θ`, `None` when the field did not change
    pub shift: Option<i64>,
}

/// The field `K(β)` for `β` the generator of `kb` (both fields with embeddings); `None` when a numeric choice
/// could not be made within the precision limit.
pub fn adjoin(k: &NumberField, kb: &NumberField, m: &mut Meter) -> WorkResult<Option<Tower>> {
    let g = kb.modulus().to_vec();
    if g.len() == 2 {
        return Ok(Some(Tower { field: k.clone(), theta: k.gen(), beta: k.of(&g[0].neg()), shift: None }));
    }
    if k.degree() == 1 {
        let theta = kb.of(&k.modulus()[0].neg());
        return Ok(Some(Tower { field: kb.clone(), theta, beta: kb.gen(), shift: Some(0) }));
    }
    let (Some(_), Some(_)) = (k.embedding(), kb.embedding()) else { return Ok(None) };
    let d = k.degree();
    let t = k.gen();
    for i in 0..=80i64 {
        let s = if i % 2 == 0 { -(i / 2) } else { i / 2 + 1 };
        if s == 0 {
            // g is irreducible over Q: the norm of g(y) is g^d, never square-free for d > 1
            continue;
        }
        // G(y) = g(y - s θ); its norm over K
        let gk = kpoly::shift(k, &kpoly::lift(&g), &k.scale(&t, &q(-s)), m)?;
        let n = kpoly::norm(k, &gk, m)?;
        if !qpoly::is_square_free(&n, m)? {
            continue;
        }
        let factors: Vec<QPoly> = qpoly::factor(&n, m)?.into_iter().map(|(f, _)| f).collect();
        // gamma = beta + s theta, enclosed
        let mut gamma = |bits: u64, m: &mut Meter| -> WorkResult<Option<CBall>> {
            let (Some(zb), Some(zt)) = (generator_enclosure(kb, bits, m)?, generator_enclosure(k, bits, m)?) else {
                return Ok(None);
            };
            let w = bits.max(64) + 16;
            let st = zt.mul(&CBall::from_i64(s), w);
            Ok(Some(zb.add(&st, w)))
        };
        let Some(nj) = choose_factor(&factors, &mut gamma, m)? else { return Ok(None) };
        if nj.len() - 1 == d {
            // beta lies in K: the linear gcd of g(y) and n_j(y + s theta)
            let shifted = kpoly::shift(k, &kpoly::lift(&nj), &k.scale(&t, &q(s)), m)?;
            let h = kpoly::gcd(k, &kpoly::lift(&g), &shifted, m)?;
            if h.len() != 2 {
                return Ok(None);
            }
            return Ok(Some(Tower { field: k.clone(), theta: t, beta: k.neg(&h[0]), shift: None }));
        }
        let Some(rs) = embed::roots(&nj, m)? else { return Ok(None) };
        let Some(e) = embed::locate(&nj, &rs, &mut gamma, m)? else { return Ok(None) };
        let l = NumberField::new(nj, Some(e));
        let gl = l.gen();
        // H(t) = g(gamma - s t) over L, as a polynomial in t
        let lin: kpoly::KPoly = kpoly::trim(vec![gl.clone(), l.of(&q(-s))]);
        let mut hpoly: kpoly::KPoly = vec![];
        for c in g.iter().rev() {
            hpoly = kpoly::add(&l, &kpoly::mul(&l, &hpoly, &lin, m)?, &[l.of(c)]);
        }
        let h = kpoly::gcd(&l, &kpoly::lift(k.modulus()), &hpoly, m)?;
        if h.len() != 2 {
            return Ok(None);
        }
        let theta = l.neg(&h[0]);
        let beta = l.sub(&gl, &l.scale(&theta, &q(s)));
        return Ok(Some(Tower { field: l, theta, beta, shift: Some(s) }));
    }
    Ok(None)
}

/// The one polynomial among `fs` that vanishes on the enclosures `z(bits)`; `None` when none or several remain at
/// the precision limit.
pub fn choose_factor(
    fs: &[QPoly],
    z: &mut dyn FnMut(u64, &mut Meter) -> WorkResult<Option<CBall>>,
    m: &mut Meter,
) -> WorkResult<Option<QPoly>> {
    if let [f] = fs {
        return Ok(Some(f.clone()));
    }
    let mut cands: Vec<&QPoly> = fs.iter().collect();
    let mut bits = 64;
    while bits <= MAX_BITS {
        let Some(zb) = z(bits, m)? else { return Ok(None) };
        let w = bits.max(64) + 16;
        cands.retain(|f| embed::eval_ball(f, &zb, w).contains_zero());
        match cands.len() {
            0 => return Ok(None),
            1 => return Ok(Some(cands[0].clone())),
            _ => bits *= 2,
        }
    }
    Ok(None)
}

/// `a` (an element of `from`) in the field `to`, given the image of `from`'s generator in `to`.
pub fn map_elem(to: &NumberField, a: &Elem, gen_image: &Elem) -> Elem {
    to.compose(a, gen_image)
}

/// The algebraic numbers `xs` in one field: the field and each number's element in it.
pub fn common_field(xs: &[AlgNum], m: &mut Meter) -> WorkResult<Option<(NumberField, Vec<Elem>)>> {
    let mut k = rationals();
    // images of each input field's generator in k, updated as k grows
    let mut gens: Vec<Elem> = vec![];
    let mut fields: Vec<NumberField> = vec![];
    for x in xs {
        // a field already present (same modulus and embedding) adds nothing
        if let Some(i) = fields.iter().position(|f| *f == x.k) {
            gens.push(gens[i].clone());
            fields.push(x.k.clone());
            continue;
        }
        let Some(tw) = adjoin(&k, &x.k, m)? else { return Ok(None) };
        let new_gens: Vec<Elem> = gens.iter().map(|g| map_elem(&tw.field, g, &tw.theta)).collect();
        gens = new_gens;
        gens.push(tw.beta.clone());
        fields.push(x.k.clone());
        k = tw.field;
    }
    let elems = xs.iter().zip(&gens).map(|(x, g)| map_elem(&k, &x.a, g)).collect();
    Ok(Some((k, elems)))
}

/// Are the numbers equal? Decided exactly: in a common field the embedding is injective, so equal numbers are equal
/// elements. `None` when no common field was found within the precision limit.
pub fn equal(a: &AlgNum, b: &AlgNum, m: &mut Meter) -> WorkResult<Option<bool>> {
    let Some((_, es)) = common_field(&[a.clone(), b.clone()], m)? else { return Ok(None) };
    Ok(Some(es[0] == es[1]))
}

/// The sign of `a - b` for real algebraic numbers (`-1`, `0`, `1`), decided exactly.
pub fn compare(a: &AlgNum, b: &AlgNum, m: &mut Meter) -> WorkResult<Option<i32>> {
    let Some((k, es)) = common_field(&[a.clone(), b.clone()], m)? else { return Ok(None) };
    AlgNum { a: k.sub(&es[0], &es[1]), k }.sign(m)
}

/// The number as the generator of its own field `Q(x)` (modulus its minimal polynomial, with the embedding its
/// value lies in): the smallest canonical representation. `None` when the root could not be located.
pub fn simple(x: &AlgNum, m: &mut Meter) -> WorkResult<Option<AlgNum>> {
    if x.as_rat().is_some() || (x.a == x.k.gen() && x.k.embedding().is_some()) {
        return Ok(Some(x.clone()));
    }
    let mp = x.minpoly(m)?;
    if mp.len() == 2 {
        return Ok(Some(AlgNum::rational(&mp[0].neg())));
    }
    let Some(rs) = embed::roots(&mp, m)? else { return Ok(None) };
    let mut z = |bits: u64, m: &mut Meter| x.enclose(bits, m);
    let Some(e) = embed::locate(&mp, &rs, &mut z, m)? else { return Ok(None) };
    Ok(Some(AlgNum::root(&mp, e)))
}

/// The roots of a nonzero polynomial over Q as algebraic numbers, in the order of root objects, with multiplicity
/// left out; `None` when the roots could not be isolated.
pub fn roots_of(p: &[Rat], m: &mut Meter) -> WorkResult<Option<Vec<AlgNum>>> {
    let sf = qpoly::square_free(p, m)?;
    let Some(rs) = embed::roots(&sf, m)? else { return Ok(None) };
    let fs: Vec<QPoly> = qpoly::factor(&sf, m)?.into_iter().map(|(f, _)| f).collect();
    let mut out = Vec::with_capacity(rs.len());
    for e in rs {
        // the irreducible factor the root belongs to, and its index there
        let mut zf = |bits: u64, m: &mut Meter| embed::enclose(&sf, &e, bits, m);
        let Some(f) = choose_factor(&fs, &mut zf, m)? else { return Ok(None) };
        let Some(frs) = embed::roots(&f, m)? else { return Ok(None) };
        let Some(fe) = embed::locate(&f, &frs, &mut zf, m)? else { return Ok(None) };
        out.push(AlgNum::root(&f, fe));
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qpoly::of_i64;
    use mc_poly::work::Unlimited;

    fn sqrt(n: i64, m: &mut Meter) -> AlgNum {
        let f = of_i64(&[-n, 0, 1]);
        let rs = embed::roots(&f, m).unwrap().unwrap();
        AlgNum::root(&f, rs[1].clone())
    }

    #[test]
    fn sqrt2_plus_sqrt3() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let a = sqrt(2, &mut m);
        let b = sqrt(3, &mut m);
        let (k, es) = common_field(&[a, b], &mut m).unwrap().unwrap();
        assert_eq!(k.degree(), 4);
        let s = k.add(&es[0], &es[1]);
        assert_eq!(k.minpoly(&s, &mut m).unwrap(), of_i64(&[1, 0, -10, 0, 1]));
        // sqrt2 * sqrt3 = sqrt6; (sqrt2 sqrt3)^2 = 6
        let p = k.mul(&es[0], &es[1]);
        assert_eq!(k.mul(&p, &p), k.of(&q(6)));
        let x = AlgNum { k: k.clone(), a: k.sub(&es[1], &es[0]) };
        assert_eq!(x.sign(&mut m).unwrap(), Some(1));
        let y = AlgNum { k: k.clone(), a: k.sub(&es[0], &es[1]) };
        assert_eq!(y.sign(&mut m).unwrap(), Some(-1));
    }

    #[test]
    fn sqrt2_in_fourth_root_field() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // 2^(1/4) is the positive real root of t^4 - 2: the second real root
        let f = of_i64(&[-2, 0, 0, 0, 1]);
        let rs = embed::roots(&f, &mut m).unwrap().unwrap();
        let r4 = AlgNum::root(&f, rs[1].clone());
        let s2 = sqrt(2, &mut m);
        let (k, es) = common_field(&[r4, s2], &mut m).unwrap().unwrap();
        assert_eq!(k.degree(), 4);
        // sqrt 2 = (2^(1/4))^2
        assert_eq!(k.mul(&es[0], &es[0]), es[1]);
    }

    #[test]
    fn close_roots_sign() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // sqrt(10^12 + 1) - 10^6 > 0, tiny
        let f = vec![q(-1_000_000_000_001), q(0), q(1)];
        let rs = embed::roots(&f, &mut m).unwrap().unwrap();
        let r = AlgNum::root(&f, rs[1].clone());
        let x = AlgNum { k: r.k.clone(), a: r.k.sub(&r.a, &r.k.of(&q(1_000_000))) };
        assert_eq!(x.sign(&mut m).unwrap(), Some(1));
    }
}
