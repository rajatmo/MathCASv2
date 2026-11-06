//! Groebner bases over the field of rational functions in parameters (package 3B).
//!
//! A [`GPoly`] is a polynomial in `n` main variables whose coefficients are integer polynomials in `np`
//! parameters ([`Poly`] with `np` variables; `np = 0` for rational systems). Its Groebner basis is computed over the
//! coefficient field Q(parameters) without fractions: reductions are fraction-free (`lc(g) f - c x^q g` divided by
//! the gcd of the two leading coefficients) and every new element is made primitive over Z[parameters], so
//! coefficients stay polynomials of moderate size. This is the generic case: a leading coefficient is a nonzero
//! polynomial in the parameters, assumed not to vanish (the answer holds for all parameter values outside a proper
//! algebraic subset).
//!
//! The algorithm is Buchberger's with the **sugar** selection strategy (the pair of least sugar degree first, then
//! the least lcm) and the Gebauer–Möller criteria to discard useless pairs; the result is the **reduced** basis,
//! every element primitive with a positive leading numeric coefficient, sorted by increasing leading monomial.
//! Every loop charges the [`Meter`], so a long run stops on an interrupt (semantics 10).
use crate::gcd::gcd;
use crate::poly::Poly;
use crate::work::{exact, Meter, WorkResult};
use crate::z::Z;
use std::cmp::Ordering;

/// A monomial order on exponent vectors of the main variables (variable 0 is the most significant).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonomialOrder {
    /// pure lexicographic: eliminates variables from the first to the last
    Lex,
    /// total degree, ties broken by reverse lexicographic order
    DegRevLex,
}

impl MonomialOrder {
    /// The order of two exponent vectors.
    pub fn cmp(self, a: &[u32], b: &[u32]) -> Ordering {
        match self {
            MonomialOrder::Lex => a.cmp(b),
            MonomialOrder::DegRevLex => degree(a).cmp(&degree(b)).then_with(|| {
                for i in (0..a.len()).rev() {
                    if a[i] != b[i] {
                        return b[i].cmp(&a[i]);
                    }
                }
                Ordering::Equal
            }),
        }
    }
}

fn degree(e: &[u32]) -> u64 {
    e.iter().map(|&x| x as u64).sum()
}

fn divides(a: &[u32], b: &[u32]) -> bool {
    a.iter().zip(b).all(|(x, y)| x <= y)
}

fn lcm(a: &[u32], b: &[u32]) -> Vec<u32> {
    a.iter().zip(b).map(|(&x, &y)| x.max(y)).collect()
}

fn coprime(a: &[u32], b: &[u32]) -> bool {
    a.iter().zip(b).all(|(&x, &y)| x == 0 || y == 0)
}

fn minus(a: &[u32], b: &[u32]) -> Vec<u32> {
    a.iter().zip(b).map(|(&x, &y)| x - y).collect()
}

// ------------------------------------------------------------------------------------------ coefficients

/// a * b for coefficient polynomials (constants without the general multiplication).
fn cmul(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    match (a.as_constant(), b.as_constant()) {
        (Some(x), _) => Ok(b.scale(&x)),
        (_, Some(y)) => Ok(a.scale(&y)),
        _ => a.mul(b, m),
    }
}

/// a / b, exact.
fn cdiv(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    if let Some(y) = b.as_constant() {
        if y.is_one() {
            return Ok(a.clone());
        }
        if a.coefs().iter().all(|c| c.divisible(&y)) {
            return Ok(a.div_exact_scalar(&y));
        }
    }
    exact(a.div_exact(b, m)?, "a gcd of coefficients divides")
}

/// gcd(a, b) for coefficient polynomials (constants without the general gcd).
fn cgcd(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    let np = a.nvars();
    match (a.as_constant(), b.as_constant()) {
        (Some(x), Some(y)) => Ok(Poly::constant(np, x.gcd(&y))),
        (Some(x), None) => Ok(Poly::constant(np, x.gcd(&b.content()))),
        (None, Some(y)) => Ok(Poly::constant(np, y.gcd(&a.content()))),
        _ => gcd(a, b, m),
    }
}

fn is_one(c: &Poly) -> bool {
    c.as_constant().is_some_and(|x| x.is_one())
}

// ------------------------------------------------------------------------------------------ polynomials

/// A polynomial in the main variables with coefficients in Z[parameters]: nonzero terms sorted by decreasing
/// monomial in the order it was built for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GPoly {
    /// the number of main variables
    pub nv: usize,
    /// the number of parameters (variables of the coefficients)
    pub np: usize,
    /// (exponents of the main variables, coefficient), decreasing in the monomial order
    pub terms: Vec<(Vec<u32>, Poly)>,
}

impl GPoly {
    /// The zero polynomial.
    pub fn zero(nv: usize, np: usize) -> GPoly {
        GPoly { nv, np, terms: vec![] }
    }
    /// Is it zero?
    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }
    /// The leading monomial (the polynomial must not be zero).
    pub fn lm(&self) -> &[u32] {
        &self.terms[0].0
    }
    /// The leading coefficient (the polynomial must not be zero).
    pub fn lc(&self) -> &Poly {
        &self.terms[0].1
    }
    /// Is it a nonzero constant (a polynomial in the parameters only)?
    pub fn is_constant(&self) -> bool {
        self.terms.len() == 1 && self.terms[0].0.iter().all(|&e| e == 0)
    }

    /// Split a polynomial of a ring into main variables `main` and parameters `params` (indices of `p`'s
    /// variables; every variable of `p` must be in one of the two).
    pub fn from_poly(p: &Poly, main: &[usize], params: &[usize], order: MonomialOrder) -> GPoly {
        let mut groups: Vec<(Vec<u32>, Vec<crate::poly::Term>)> = vec![];
        let mut index: std::collections::HashMap<Vec<u32>, usize> = Default::default();
        for (e, c) in p.terms() {
            let me: Vec<u32> = main.iter().map(|&v| e[v]).collect();
            let pe: Vec<u32> = params.iter().map(|&v| e[v]).collect();
            let k = *index.entry(me.clone()).or_insert_with(|| {
                groups.push((me, vec![]));
                groups.len() - 1
            });
            groups[k].1.push((pe, c.clone()));
        }
        let mut terms: Vec<(Vec<u32>, Poly)> =
            groups.into_iter().map(|(e, ts)| (e, Poly::from_terms(params.len(), ts))).collect();
        terms.sort_by(|a, b| order.cmp(&b.0, &a.0));
        GPoly { nv: main.len(), np: params.len(), terms }
    }

    /// Back to a polynomial in `nv` ring variables, main variable i being ring variable `main[i]` and parameter j
    /// ring variable `params[j]`.
    pub fn to_poly(&self, nv: usize, main: &[usize], params: &[usize]) -> Poly {
        let mut out = vec![];
        for (me, c) in &self.terms {
            for (pe, x) in c.terms() {
                let mut e = vec![0u32; nv];
                for (i, &v) in main.iter().enumerate() {
                    e[v] += me[i];
                }
                for (j, &v) in params.iter().enumerate() {
                    e[v] += pe[j];
                }
                out.push((e, x.clone()));
            }
        }
        Poly::from_terms(nv, out)
    }

    /// `a * self - b * x^e * g`: the fraction-free linear combination (terms merged in `order`).
    fn combine(&self, a: &Poly, b: &Poly, e: &[u32], g: &GPoly, order: MonomialOrder, m: &mut Meter) -> WorkResult<GPoly> {
        let mut out = Vec::with_capacity(self.terms.len() + g.terms.len());
        let (mut i, mut j) = (0, 0);
        let shifted = |j: usize| -> Vec<u32> { g.terms[j].0.iter().zip(e).map(|(x, y)| x + y).collect() };
        let mut next_g = if g.terms.is_empty() { None } else { Some(shifted(0)) };
        while i < self.terms.len() || next_g.is_some() {
            m.step(1 + self.np as u64)?;
            let ord = match (&next_g, self.terms.get(i)) {
                (None, _) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(ge), Some((fe, _))) => order.cmp(fe, ge),
            };
            match ord {
                Ordering::Greater => {
                    let c = if is_one(a) { self.terms[i].1.clone() } else { cmul(a, &self.terms[i].1, m)? };
                    out.push((self.terms[i].0.clone(), c));
                    i += 1;
                }
                Ordering::Less => {
                    let c = cmul(b, &g.terms[j].1, m)?.neg();
                    // invariant: Less is chosen only while g has terms left, and `next_g` holds the current one
                    out.push((next_g.take().expect("a term of g"), c));
                    j += 1;
                    next_g = if j < g.terms.len() { Some(shifted(j)) } else { None };
                }
                Ordering::Equal => {
                    let c = cmul(a, &self.terms[i].1, m)?.sub(&cmul(b, &g.terms[j].1, m)?);
                    if !c.is_zero() {
                        out.push((self.terms[i].0.clone(), c));
                    }
                    i += 1;
                    j += 1;
                    next_g = if j < g.terms.len() { Some(shifted(j)) } else { None };
                }
            }
        }
        Ok(GPoly { nv: self.nv, np: self.np, terms: out })
    }

    /// The primitive part over Z[parameters] with a positive leading numeric coefficient.
    pub fn primitive(&self, m: &mut Meter) -> WorkResult<GPoly> {
        if self.is_zero() {
            return Ok(self.clone());
        }
        let mut c = self.terms[0].1.clone();
        for (_, x) in &self.terms[1..] {
            if c.as_constant().is_some_and(|z| z.is_one()) {
                break;
            }
            c = cgcd(&c, x, m)?;
        }
        if self.lc().lc().is_neg() {
            c = c.neg();
        }
        if is_one(&c) {
            return Ok(self.clone());
        }
        let mut terms = Vec::with_capacity(self.terms.len());
        for (e, x) in &self.terms {
            terms.push((e.clone(), cdiv(x, &c, m)?));
        }
        Ok(GPoly { nv: self.nv, np: self.np, terms })
    }

    /// The largest total degree of a term in the main variables.
    fn total_degree(&self) -> u64 {
        self.terms.iter().map(|(e, _)| degree(e)).max().unwrap_or(0)
    }
}

/// The S-polynomial of f and g.
fn spoly(f: &GPoly, g: &GPoly, order: MonomialOrder, m: &mut Meter) -> WorkResult<GPoly> {
    let l = lcm(f.lm(), g.lm());
    let c = cgcd(f.lc(), g.lc(), m)?;
    let a = cdiv(g.lc(), &c, m)?;
    let b = cdiv(f.lc(), &c, m)?;
    // a x^(l - lm f) f - b x^(l - lm g) g: shift f first
    let ef = minus(&l, f.lm());
    let shifted = GPoly { nv: f.nv, np: f.np, terms: f.terms.iter().map(|(e, x)| (e.iter().zip(&ef).map(|(p, q)| p + q).collect(), x.clone())).collect() };
    shifted.combine(&a, &b, &minus(&l, g.lm()), g, order, m)
}

/// The full reduction of f by `basis` (the elements with `alive[i]`), made primitive.
fn reduce(f: GPoly, basis: &[GPoly], alive: &[bool], order: MonomialOrder, m: &mut Meter) -> WorkResult<GPoly> {
    let mut f = f;
    let mut i = 0;
    while i < f.terms.len() {
        m.step(1)?;
        let e = &f.terms[i].0;
        let found = basis.iter().enumerate().find(|(k, g)| alive[*k] && divides(g.lm(), e));
        match found {
            Some((_, g)) => {
                let q = minus(e, g.lm());
                let c = cgcd(&f.terms[i].1, g.lc(), m)?;
                let a = cdiv(g.lc(), &c, m)?;
                let b = cdiv(&f.terms[i].1, &c, m)?;
                // the terms before i are already irreducible; scaling by a keeps them so
                f = f.combine(&a, &b, &q, g, order, m)?;
                if f.terms.len() > 64 && i % 16 == 0 {
                    f = f.primitive(m)?;
                }
            }
            None => i += 1,
        }
    }
    f.primitive(m)
}

/// A critical pair (i, j), i < j, with its lcm and sugar degree.
struct Pair {
    i: usize,
    j: usize,
    lcm: Vec<u32>,
    sugar: u64,
}

/// The reduced Groebner basis of `input` in `order` over Q(parameters). The zero polynomials of the input are
/// ignored; a basis `[1]` (one constant) means the equations are inconsistent.
pub fn groebner(input: &[GPoly], order: MonomialOrder, m: &mut Meter) -> WorkResult<Vec<GPoly>> {
    let mut basis: Vec<GPoly> = vec![];
    let mut sugar: Vec<u64> = vec![];
    let mut alive: Vec<bool> = vec![];
    let mut pairs: Vec<Pair> = vec![];
    // the input, inter-reduced as it is added (small inputs: the order is not critical)
    let mut todo: Vec<GPoly> = input.iter().filter(|p| !p.is_zero()).cloned().collect();
    todo.sort_by(|a, b| order.cmp(b.lm(), a.lm()));
    for p in todo {
        let s = p.total_degree();
        let h = reduce(p, &basis, &alive, order, m)?;
        if !h.is_zero() {
            insert(h, s, &mut basis, &mut sugar, &mut alive, &mut pairs, m)?;
        }
    }
    while !pairs.is_empty() {
        m.step(1)?;
        if basis.iter().zip(&alive).any(|(g, &a)| a && g.is_constant()) {
            break;
        }
        // the pair of least sugar, then least lcm
        let k = (0..pairs.len())
            .min_by(|&a, &b| {
                pairs[a].sugar.cmp(&pairs[b].sugar).then_with(|| order.cmp(&pairs[a].lcm, &pairs[b].lcm))
            })
            // invariant: the loop runs while `pairs` is nonempty
            .expect("a pair");
        let p = pairs.swap_remove(k);
        let s = spoly(&basis[p.i], &basis[p.j], order, m)?;
        let h = reduce(s, &basis, &alive, order, m)?;
        if !h.is_zero() {
            insert(h, p.sugar, &mut basis, &mut sugar, &mut alive, &mut pairs, m)?;
        }
    }
    // a constant means the ideal is the whole ring
    if let Some(g) = basis.iter().zip(&alive).find(|(g, &a)| a && g.is_constant()) {
        let one = GPoly { nv: g.0.nv, np: g.0.np, terms: vec![(vec![0; g.0.nv], Poly::constant(g.0.np, Z::ONE))] };
        return Ok(vec![one]);
    }
    // the minimal basis, then each element reduced by the others
    let mut minimal: Vec<GPoly> = vec![];
    for (k, g) in basis.iter().enumerate() {
        if !alive[k] {
            continue;
        }
        let redundant = basis.iter().enumerate().any(|(l, h)| {
            alive[l] && l != k && divides(h.lm(), g.lm()) && (h.lm() != g.lm() || l < k)
        });
        if !redundant {
            minimal.push(g.clone());
        }
    }
    let mut reduced = Vec::with_capacity(minimal.len());
    for k in 0..minimal.len() {
        let others: Vec<GPoly> = minimal.iter().enumerate().filter(|(l, _)| *l != k).map(|(_, g)| g.clone()).collect();
        let ones = vec![true; others.len()];
        // the leading term is irreducible by the others; reduce the tail
        let g = &minimal[k];
        let head = GPoly { nv: g.nv, np: g.np, terms: vec![g.terms[0].clone()] };
        let tail = GPoly { nv: g.nv, np: g.np, terms: g.terms[1..].to_vec() };
        let r = reduce_keeping(head, tail, &others, &ones, order, m)?;
        reduced.push(r);
    }
    reduced.sort_by(|a, b| order.cmp(a.lm(), b.lm()));
    Ok(reduced)
}

/// head + tail with the tail fully reduced (the head term scaled along), primitive.
fn reduce_keeping(
    head: GPoly,
    tail: GPoly,
    basis: &[GPoly],
    alive: &[bool],
    order: MonomialOrder,
    m: &mut Meter,
) -> WorkResult<GPoly> {
    let mut f = GPoly { nv: head.nv, np: head.np, terms: head.terms.into_iter().chain(tail.terms).collect() };
    let mut i = 1;
    while i < f.terms.len() {
        m.step(1)?;
        let e = &f.terms[i].0;
        match basis.iter().enumerate().find(|(k, g)| alive[*k] && divides(g.lm(), e)) {
            Some((_, g)) => {
                let q = minus(e, g.lm());
                let c = cgcd(&f.terms[i].1, g.lc(), m)?;
                let a = cdiv(g.lc(), &c, m)?;
                let b = cdiv(&f.terms[i].1, &c, m)?;
                f = f.combine(&a, &b, &q, g, order, m)?;
            }
            None => i += 1,
        }
    }
    f.primitive(m)
}

/// Add h to the basis with the Gebauer–Möller update of the pairs.
fn insert(
    h: GPoly,
    s: u64,
    basis: &mut Vec<GPoly>,
    sugar: &mut Vec<u64>,
    alive: &mut Vec<bool>,
    pairs: &mut Vec<Pair>,
    m: &mut Meter,
) -> WorkResult<()> {
    let t = basis.len();
    let lh = h.lm().to_vec();
    // candidate new pairs (k, t)
    let mut cands: Vec<(usize, Vec<u32>, bool)> = vec![];
    for k in 0..t {
        m.step(1)?;
        if alive[k] {
            cands.push((k, lcm(basis[k].lm(), &lh), coprime(basis[k].lm(), &lh)));
        }
    }
    // Gebauer–Möller: a new pair is kept when its leading monomials are coprime (to be dropped below by the
    // product criterion, but it still masks others) or when no other remaining or kept pair's lcm divides its lcm
    let mut c = cands;
    let mut d: Vec<(usize, Vec<u32>, bool)> = vec![];
    while let Some(p) = c.pop() {
        m.step(1)?;
        if p.2 || !c.iter().chain(d.iter()).any(|q| divides(&q.1, &p.1)) {
            d.push(p);
        }
    }
    // criterion B on the old pairs: lm(h) divides lcm(i, j) strictly in both directions
    pairs.retain(|p| {
        !(divides(&lh, &p.lcm)
            && lcm(basis[p.i].lm(), &lh) != p.lcm
            && lcm(basis[p.j].lm(), &lh) != p.lcm)
    });
    // the product criterion: coprime leading monomials need no pair
    for (k, l, cop) in d {
        if cop {
            continue;
        }
        let dl = degree(&l);
        let su = (sugar[k] + dl - degree(basis[k].lm())).max(s + dl - degree(&lh));
        pairs.push(Pair { i: k, j: t, lcm: l, sugar: su });
    }
    // elements whose leading monomial h's divides take no part in new pairs or reductions (their pending pairs
    // stay)
    for k in 0..t {
        if alive[k] && divides(&lh, basis[k].lm()) {
            alive[k] = false;
        }
    }
    basis.push(h);
    sugar.push(s);
    alive.push(true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::Unlimited;

    fn gp(nv: usize, t: &[(&[u32], i64)], order: MonomialOrder) -> GPoly {
        let p = Poly::from_terms(nv, t.iter().map(|(e, c)| (e.to_vec(), Z::S(*c))).collect());
        let main: Vec<usize> = (0..nv).collect();
        GPoly::from_poly(&p, &main, &[], order)
    }

    #[test]
    fn circle_and_line() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let o = MonomialOrder::Lex;
        // x^2 + y^2 - 1, x - y  ->  [2 y^2 - 1, x - y]
        let f = gp(2, &[(&[2, 0], 1), (&[0, 2], 1), (&[0, 0], -1)], o);
        let g = gp(2, &[(&[1, 0], 1), (&[0, 1], -1)], o);
        let b = groebner(&[f, g], o, &mut m).unwrap();
        assert_eq!(b, vec![gp(2, &[(&[0, 2], 2), (&[0, 0], -1)], o), gp(2, &[(&[1, 0], 1), (&[0, 1], -1)], o)]);
    }

    #[test]
    fn inconsistent_and_cyclic() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let o = MonomialOrder::Lex;
        let f = gp(1, &[(&[1], 1)], o);
        let g = gp(1, &[(&[1], 1), (&[0], -1)], o);
        let b = groebner(&[f, g], o, &mut m).unwrap();
        assert!(b.len() == 1 && b[0].is_constant());
        // cyclic 3: x+y+z, xy+yz+zx, xyz-1 -> lex basis [z^3 - 1, y^2 + y z + z^2, x + y + z]
        let c1 = gp(3, &[(&[1, 0, 0], 1), (&[0, 1, 0], 1), (&[0, 0, 1], 1)], o);
        let c2 = gp(3, &[(&[1, 1, 0], 1), (&[0, 1, 1], 1), (&[1, 0, 1], 1)], o);
        let c3 = gp(3, &[(&[1, 1, 1], 1), (&[0, 0, 0], -1)], o);
        let b = groebner(&[c1.clone(), c2, c3], o, &mut m).unwrap();
        assert_eq!(b.len(), 3);
        assert_eq!(b[0], gp(3, &[(&[0, 0, 3], 1), (&[0, 0, 0], -1)], o));
        assert_eq!(b[1], gp(3, &[(&[0, 2, 0], 1), (&[0, 1, 1], 1), (&[0, 0, 2], 1)], o));
        assert_eq!(b[2], c1);
    }

    #[test]
    fn parameters() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let o = MonomialOrder::Lex;
        // ring (x, a): a x - 1 with x main, a a parameter: the basis is a x - 1 itself
        let p = Poly::from_terms(2, vec![(vec![1, 1], Z::S(1)), (vec![0, 0], Z::S(-1))]);
        let g = GPoly::from_poly(&p, &[0], &[1], o);
        let b = groebner(std::slice::from_ref(&g), o, &mut m).unwrap();
        assert_eq!(b, vec![g]);
        assert_eq!(b[0].to_poly(2, &[0], &[1]), p);
    }
}

#[cfg(test)]
mod timing {
    use super::*;
    use crate::work::Unlimited;

    #[test]
    #[ignore]
    fn katsura3() {
        let k = |t: &[(&[u32], i64)]| Poly::from_terms(4, t.iter().map(|(e, c)| (e.to_vec(), Z::S(*c))).collect());
        let ps = [
            k(&[(&[1, 0, 0, 0], 1), (&[0, 1, 0, 0], 2), (&[0, 0, 1, 0], 2), (&[0, 0, 0, 1], 2), (&[0, 0, 0, 0], -1)]),
            k(&[(&[2, 0, 0, 0], 1), (&[0, 2, 0, 0], 2), (&[0, 0, 2, 0], 2), (&[0, 0, 0, 2], 2), (&[1, 0, 0, 0], -1)]),
            k(&[(&[1, 1, 0, 0], 2), (&[0, 1, 1, 0], 2), (&[0, 0, 1, 1], 2), (&[0, 1, 0, 0], -1)]),
            k(&[(&[0, 2, 0, 0], 1), (&[1, 0, 1, 0], 2), (&[0, 1, 0, 1], 2), (&[0, 0, 1, 0], -1)]),
        ];
        for o in [MonomialOrder::DegRevLex, MonomialOrder::Lex] {
            let gs: Vec<GPoly> = ps.iter().map(|p| GPoly::from_poly(p, &[0, 1, 2, 3], &[], o)).collect();
            let mut w = Unlimited;
            let mut m = Meter::new(&mut w);
            let t = std::time::Instant::now();
            let b = groebner(&gs, o, &mut m).unwrap();
            eprintln!("{o:?}: {} elements, {:?}", b.len(), t.elapsed());
        }
    }
}
