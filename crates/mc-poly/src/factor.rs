//! Factorisation over the integers (decision K.8).
//!
//! Univariate: content and powers of x first, then square-free decomposition (Yun), then for each square-free
//! part Berlekamp–Zassenhaus in the Cantor–Zassenhaus form: a small prime p that keeps the polynomial
//! square-free (the one with the fewest modular factors among a few candidates), distinct-degree and
//! equal-degree factorisation mod p, quadratic Hensel lifting of all factors at once down a balanced factor tree
//! to p^(2^j) beyond twice the Mignotte bound, and recombination of subsets of increasing size with a
//! constant-term divisibility filter before the trial division.
//!
//! Multivariate: integer and monomial content, the content with respect to each variable (by multivariate gcd),
//! the square-free decomposition with respect to a variable, then evaluation and Hensel lifting for square-free
//! parts in several variables ([`crate::mfactor`], decision K.14), with Kronecker's substitution as the fallback
//! past its size limits (complete within its own limits; beyond them the part is left as it is).
use crate::gcd::gcd;
use crate::modp::{self, Fp, UPoly};
use crate::poly::Poly;
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use mc_arith::num::Nat;

/// A factorisation: `unit * prod f_i^e_i`, with the factors primitive and of positive leading coefficient.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Factored {
    /// the integer content with its sign
    pub unit: Z,
    /// (factor, multiplicity), constant factors excluded
    pub factors: Vec<(Poly, u32)>,
}

/// The factorisation of an integer polynomial; `square_free_only` stops after the square-free decomposition.
pub fn factor(p: &Poly, square_free_only: bool, m: &mut Meter) -> WorkResult<Factored> {
    let nv = p.nv;
    if p.is_zero() {
        return Ok(Factored { unit: Z::ZERO, factors: vec![] });
    }
    let (unit, mut rest) = p.primitive();
    let mut factors: Vec<(Poly, u32)> = vec![];
    // monomial content: x_v^k divides every term
    let low: Vec<u32> = (0..nv).map(|v| rest.low_degree(v).unwrap_or(0)).collect();
    if low.iter().any(|&k| k > 0) {
        for (v, &k) in low.iter().enumerate() {
            if k > 0 {
                factors.push((Poly::var(nv, v), k));
            }
        }
        rest = crate::work::exact(rest.div_exact(&Poly::monomial(&low, Z::ONE), m)?, "a monomial content divides")?;
    }
    // contents with respect to each variable split the polynomial into parts in fewer variables
    let mut work = vec![rest];
    let mut parts = vec![];
    while let Some(q) = work.pop() {
        m.step(1)?;
        let used = q.used_vars();
        if used.len() <= 1 {
            parts.push(q);
            continue;
        }
        let mut split = false;
        for &v in &used {
            let c = content_in(&q, v, m)?;
            if !c.is_constant() {
                let pp = crate::work::exact(q.div_exact(&c, m)?, "the content divides")?;
                work.push(c);
                work.push(pp);
                split = true;
                break;
            }
        }
        if !split {
            parts.push(q);
        }
    }
    let mut unit = unit;
    for q in parts {
        if q.is_constant() {
            // invariant: `is_constant` was checked just above
            unit = unit.mul(&q.as_constant().expect("a constant"));
            continue;
        }
        let v = q.used_vars()[0];
        for (s, e) in square_free(&q, v, m)? {
            if square_free_only {
                factors.push((s, e));
            } else if s.used_vars().len() > 1 {
                let gs = match crate::mfactor::factor_multivariate(&s, m)? {
                    Some(gs) => gs,
                    None => kronecker(&s, m)?,
                };
                for g in gs {
                    factors.push((g, e));
                }
            } else {
                for g in factor_univariate(&s, v, m)? {
                    factors.push((g, e));
                }
            }
        }
    }
    // a canonical order: by degree vector, then by terms
    factors.sort_by(|a, b| {
        let coefs = || {
            a.0.coefs
                .iter()
                .zip(&b.0.coefs)
                .map(|(x, y)| x.cmp_z(y))
                .find(|o| o.is_ne())
                .unwrap_or(std::cmp::Ordering::Equal)
        };
        a.0.degrees().cmp(&b.0.degrees()).then_with(|| a.0.exps.cmp(&b.0.exps)).then_with(coefs)
    });
    // the factors were normalised to positive leading coefficients; fix the unit's sign
    let mut prod = Poly::constant(nv, unit.clone());
    for (f, e) in &factors {
        prod = prod.mul(&f.pow(*e as u64, m)?, m)?;
    }
    if prod == p.neg() {
        unit = unit.neg();
    }
    Ok(Factored { unit, factors })
}

/// The content of q with respect to variable v: the gcd of its coefficients as a polynomial in v.
fn content_in(q: &Poly, v: usize, m: &mut Meter) -> WorkResult<Poly> {
    let mut g = Poly::zero(q.nv);
    for (_, c) in q.coeffs_in(v) {
        g = gcd(&g, &c, m)?;
        if g.is_constant() {
            break;
        }
    }
    Ok(g)
}

/// Yun's square-free decomposition of a primitive q with respect to variable v (q has positive degree in v and
/// no content in v): (s_i, i) with q = prod s_i^i up to sign, the s_i square-free and pairwise coprime.
fn square_free(q: &Poly, v: usize, m: &mut Meter) -> WorkResult<Vec<(Poly, u32)>> {
    let div =
        |a: &Poly, b: &Poly, m: &mut Meter| -> WorkResult<Poly> { crate::work::exact(a.div_exact(b, m)?, "an exact division") };
    let mut out = vec![];
    let d = q.derivative(v);
    let a0 = gcd(q, &d, m)?.primitive().1;
    let mut b = div(q, &a0, m)?;
    let mut c = div(&d, &a0, m)?;
    let mut dd = c.sub(&b.derivative(v));
    let mut i = 1;
    loop {
        m.step(1)?;
        if b.is_constant() {
            break;
        }
        let a = gcd(&b, &dd, m)?.primitive().1;
        if !a.is_constant() {
            out.push((a.clone(), i));
        }
        let nb = div(&b, &a, m)?;
        c = div(&dd, &a, m)?;
        dd = c.sub(&nb.derivative(v));
        b = nb;
        i += 1;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------- multivariate

/// The largest univariate image Kronecker's method factors.
const KRONECKER_MAX_DEGREE: u64 = 4000;
/// The most univariate factors whose subsets it tries.
const KRONECKER_MAX_FACTORS: usize = 16;

/// The irreducible factors of a primitive square-free multivariate q by Kronecker's substitution: x_i = x^(w_i)
/// with mixed-radix weights from the degrees maps q to a univariate polynomial whose factorisation determines
/// every factor of q (the image of a factor decodes back exactly, its degrees being below the radices); subsets
/// of the univariate factors are tried by increasing size and kept when they decode to an exact divisor, so the
/// factors found are irreducible. When the image is too large or splits into too many factors, q is returned
/// as it is (decision K.13).
fn kronecker(q: &Poly, m: &mut Meter) -> WorkResult<Vec<Poly>> {
    let nv = q.nv;
    let radix: Vec<u64> = q.degrees().iter().map(|&d| d as u64 + 1).collect();
    let span = radix.iter().try_fold(1u64, |acc, &r| acc.checked_mul(r));
    if span.is_none_or(|s| s > KRONECKER_MAX_DEGREE) {
        return Ok(vec![q.clone()]);
    }
    let index = |e: &[u32]| e.iter().zip(&radix).fold(0u64, |acc, (&x, &r)| acc * r + x as u64);
    let image = Poly::from_terms(1, q.terms().map(|(e, c)| (vec![index(e) as u32], c.clone())).collect());
    // the univariate factors, with multiplicity
    let uf = factor(&image, false, m)?;
    let mut pieces: Vec<Poly> = vec![];
    for (g, e) in &uf.factors {
        for _ in 0..*e {
            pieces.push(g.clone());
        }
    }
    if pieces.len() > KRONECKER_MAX_FACTORS {
        return Ok(vec![q.clone()]);
    }
    let decode = |u: &Poly| -> Poly {
        let terms = u
            .terms()
            .map(|(e, c)| {
                let mut x = e[0] as u64;
                let mut f = vec![0u32; nv];
                for v in (0..nv).rev() {
                    f[v] = (x % radix[v]) as u32;
                    x /= radix[v];
                }
                (f, c.clone())
            })
            .collect();
        Poly::from_terms(nv, terms)
    };
    let mut rest = q.clone();
    let mut out = vec![];
    let mut size = 1;
    while 2 * size <= pieces.len() && !rest.is_constant() {
        let r = pieces.len();
        let mut found = false;
        let mut idx: Vec<usize> = (0..size).collect();
        loop {
            m.step(1)?;
            let mut u = Poly::constant(1, Z::ONE);
            for &i in &idx {
                u = u.mul(&pieces[i], m)?;
            }
            let cand = decode(&u).primitive().1;
            if !cand.is_constant() {
                if let Some(quot) = rest.div_exact(&cand, m)? {
                    out.push(cand);
                    rest = quot;
                    for &i in idx.iter().rev() {
                        pieces.remove(i);
                    }
                    found = true;
                    break;
                }
            }
            let mut i = size;
            while i > 0 && idx[i - 1] == r - size + i - 1 {
                i -= 1;
            }
            if i == 0 {
                break;
            }
            idx[i - 1] += 1;
            for j in i..size {
                idx[j] = idx[j - 1] + 1;
            }
        }
        if !found {
            size += 1;
        }
    }
    if !rest.is_constant() {
        out.push(rest.primitive().1);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------- univariate

/// The irreducible factors of a primitive, square-free q of positive degree in v (and no other variable), with
/// positive leading coefficients.
pub fn factor_univariate(q: &Poly, v: usize, m: &mut Meter) -> WorkResult<Vec<Poly>> {
    let nv = q.nv;
    q.check_dense()?;
    let f = q.to_dense(v);
    let found = zassenhaus(&f, m)?;
    Ok(found.iter().map(|g| Poly::from_dense(nv, v, g).primitive().1).collect())
}

/// Dense helpers over Z.
fn dense(p: &[Z]) -> Poly {
    Poly::from_dense(1, 0, p)
}

fn degree(p: &[Z]) -> usize {
    p.len() - 1
}

/// The small primes tried for the modular factorisation.
fn small_primes() -> impl Iterator<Item = u64> {
    (3u64..).filter(|&n| modp::is_prime(n))
}

/// Factor a primitive square-free polynomial of degree >= 1 with f(0) != 0 or degree 1.
fn zassenhaus(f: &[Z], m: &mut Meter) -> WorkResult<Vec<Vec<Z>>> {
    let n = degree(f);
    if n <= 1 {
        return Ok(vec![f.to_vec()]);
    }
    let lc = f[n].clone();
    // choose the prime with the fewest factors among the first few good ones
    let fd = derivative(f);
    // (prime, distinct-degree split, number of factors)
    let mut best: Option<(Fp, DistinctDegree, usize)> = None;
    let mut tried = 0;
    for p in small_primes() {
        m.step(1)?;
        if lc.rem_u64(p) == 0 {
            continue;
        }
        let fp = Fp::new(p);
        let a = reduce(f, fp);
        let g = modp::gcd(fp, &a, &reduce(&fd, fp), m)?;
        if g.len() != 1 {
            continue;
        }
        // the distinct-degree split tells the number of factors; splitting further waits for the chosen prime
        let parts = ddf(fp, &modp::monic(fp, &a), m)?;
        let count: usize = parts.iter().map(|(g, d)| (g.len() - 1) / d).sum();
        if best.as_ref().is_none_or(|(_, _, c)| count < *c) {
            best = Some((fp, parts, count));
        }
        tried += 1;
        if tried >= 6 || count == 1 {
            break;
        }
    }
    // invariant: there are infinitely many primes and only finitely many divide the leading coefficient or the discriminant; the meter bounds the search
    let (fp, parts, _) = best.expect("some prime keeps a square-free polynomial square-free");
    let facs = edf(fp, parts, m)?;
    if facs.len() == 1 {
        return Ok(vec![f.to_vec()]);
    }
    // the bound on the coefficients of a factor (Mignotte), times the leading coefficient
    let norm_bits = f.iter().map(Z::bits).max().unwrap_or(0);
    let bound_bits = n as u64 + norm_bits + lc.bits() + (64 - (n as u64 + 1).leading_zeros() as u64) / 2 + 2;
    let mut k = 1u64;
    let mut modulus = Nat::from_u64(fp.p);
    while modulus.bits() <= bound_bits {
        modulus = modulus.square();
        k *= 2;
    }
    let lifted = hensel(f, fp, &facs, k, m)?;
    recombine(f, lifted, &Z::from_nat(modulus), m)
}

/// The derivative of a dense polynomial.
fn derivative(f: &[Z]) -> Vec<Z> {
    f.iter().enumerate().skip(1).map(|(i, c)| c.mul_i64(i as i64)).collect()
}

/// f mod p
fn reduce(f: &[Z], fp: Fp) -> UPoly {
    let mut a: UPoly = f.iter().map(|c| c.rem_u64(fp.p)).collect();
    modp::trim(&mut a);
    a
}

/// Products of the irreducible factors of each degree: (product, degree).
type DistinctDegree = Vec<(UPoly, usize)>;

/// The distinct-degree factorisation of a monic square-free polynomial mod p: (product of the irreducible
/// factors of degree d, d). gcd(x^(p^d) - x, f) collects the factors of degree d.
fn ddf(fp: Fp, a: &[u64], m: &mut Meter) -> WorkResult<DistinctDegree> {
    let mut f = a.to_vec();
    let mut h: UPoly = vec![0, 1];
    let mut d = 0usize;
    let mut by_degree: Vec<(UPoly, usize)> = vec![];
    while modp::deg(&f) >= 2 * (d as isize + 1) {
        d += 1;
        h = modp::powmod(fp, &h, &[fp.p], &f, m)?;
        let g = modp::gcd(fp, &modp::sub(fp, &h, &[0, 1]), &f, m)?;
        if g.len() > 1 {
            f = modp::divrem(fp, &f, &g).0;
            h = modp::rem(fp, &h, &f);
            by_degree.push((g, d));
        }
    }
    if f.len() > 1 {
        let df = f.len() - 1;
        by_degree.push((f, df));
    }
    Ok(by_degree)
}

/// The monic irreducible factors from a distinct-degree factorisation (p odd).
fn edf(fp: Fp, by_degree: DistinctDegree, m: &mut Meter) -> WorkResult<Vec<UPoly>> {
    let mut out = vec![];
    // equal-degree splitting (Cantor–Zassenhaus) with a reproducible generator
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for (g, d) in by_degree {
        let mut stack = vec![g];
        // (p^d - 1) / 2
        let e = Nat::pow(fp.p, d as u64).sub_u64(1).shr(1);
        while let Some(g) = stack.pop() {
            m.step(1)?;
            if g.len() - 1 == d {
                out.push(g);
                continue;
            }
            loop {
                let mut r: UPoly = (0..g.len() - 1).map(|_| rnd() % fp.p).collect();
                modp::trim(&mut r);
                if r.len() < 2 {
                    continue;
                }
                let b = modp::powmod(fp, &r, &e.limbs, &g, m)?;
                let s = modp::gcd(fp, &modp::sub(fp, &b, &[1]), &g, m)?;
                if s.len() > 1 && s.len() < g.len() {
                    let other = modp::divrem(fp, &g, &s).0;
                    stack.push(s);
                    stack.push(other);
                    break;
                }
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------- Hensel lifting

/// Arithmetic of dense polynomials with coefficients mod an integer `md` (kept in [0, md)).
struct ZMod {
    md: Z,
}

impl ZMod {
    fn red(&self, c: &Z) -> Z {
        let r = c.divrem(&self.md).1;
        if r.is_neg() {
            r.add(&self.md)
        } else {
            r
        }
    }
    fn trim(&self, mut a: Vec<Z>) -> Vec<Z> {
        while a.last().is_some_and(Z::is_zero) {
            a.pop();
        }
        a
    }
    fn reduce(&self, a: &[Z]) -> Vec<Z> {
        self.trim(a.iter().map(|c| self.red(c)).collect())
    }
    fn add(&self, a: &[Z], b: &[Z]) -> Vec<Z> {
        let n = a.len().max(b.len());
        let z = Z::ZERO;
        self.trim((0..n).map(|i| self.red(&a.get(i).unwrap_or(&z).add(b.get(i).unwrap_or(&z)))).collect())
    }
    fn sub(&self, a: &[Z], b: &[Z]) -> Vec<Z> {
        let n = a.len().max(b.len());
        let z = Z::ZERO;
        self.trim((0..n).map(|i| self.red(&a.get(i).unwrap_or(&z).sub(b.get(i).unwrap_or(&z)))).collect())
    }
    fn mul(&self, a: &[Z], b: &[Z], m: &mut Meter) -> WorkResult<Vec<Z>> {
        if a.is_empty() || b.is_empty() {
            return Ok(vec![]);
        }
        // over Z (Kronecker or heap), then reduced
        let p = dense(a).mul(&dense(b), m)?;
        Ok(self.reduce(&p.to_dense(0)))
    }
    /// (q, r) of a by a monic b.
    fn divrem(&self, a: &[Z], b: &[Z], m: &mut Meter) -> WorkResult<(Vec<Z>, Vec<Z>)> {
        if a.len() < b.len() {
            return Ok((vec![], a.to_vec()));
        }
        let db = b.len() - 1;
        let mut r = a.to_vec();
        let mut q = vec![Z::ZERO; a.len() - db];
        for k in (0..q.len()).rev() {
            m.step(b.len() as u64)?;
            let c = self.red(&r[k + db]);
            if !c.is_zero() {
                for (j, y) in b.iter().enumerate() {
                    r[k + j] = self.red(&r[k + j].sub(&c.mul(y)));
                }
            }
            q[k] = c;
        }
        r.truncate(db);
        Ok((self.trim(q), self.trim(r)))
    }
}

/// The inverse of a mod md (a unit mod p), by Newton iteration from the inverse mod p.
fn inverse_mod(a: &Z, fp: Fp, md: &Z) -> Z {
    let zm = ZMod { md: md.clone() };
    let mut x = Z::S(fp.inv(a.rem_u64(fp.p)) as i64);
    let mut prec = Z::S(fp.p as i64);
    loop {
        // x <- x (2 - a x)
        x = zm.red(&x.mul(&Z::S(2).sub(&a.mul(&x))));
        if prec.cmp_abs(md) != std::cmp::Ordering::Less {
            return x;
        }
        prec = prec.mul(&prec);
    }
}

/// Lift the monic factors `facs` of f / lc(f) mod p to mod p^k (k a power of two): monic factors mod p^k.
fn hensel(f: &[Z], fp: Fp, facs: &[UPoly], k: u64, m: &mut Meter) -> WorkResult<Vec<Vec<Z>>> {
    let n = degree(f);
    let p = Z::S(fp.p as i64);
    let md = p.pow(k);
    let zm = ZMod { md: md.clone() };
    let inv = inverse_mod(&f[n], fp, &md);
    let monic: Vec<Z> = zm.reduce(&f.iter().map(|c| c.mul(&inv)).collect::<Vec<_>>());
    let lift_facs: Vec<Vec<Z>> = facs.iter().map(|u| u.iter().map(|&c| Z::S(c as i64)).collect()).collect();
    // balanced tree, by explicit stack: (target product mod p^k, factors below)
    let mut out = vec![];
    let mut stack = vec![(monic, lift_facs)];
    while let Some((target, fs)) = stack.pop() {
        m.step(1)?;
        if fs.len() == 1 {
            out.push(target);
            continue;
        }
        let (l, r) = fs.split_at(fs.len() / 2);
        let pm = ZMod { md: p.clone() };
        let mut g = vec![Z::ONE];
        for u in l {
            g = pm.mul(&g, u, m)?;
        }
        let mut h = vec![Z::ONE];
        for u in r {
            h = pm.mul(&h, u, m)?;
        }
        let (g2, h2) = lift_pair(&target, &g, &h, fp, k, m)?;
        stack.push((g2, l.to_vec()));
        stack.push((h2, r.to_vec()));
    }
    Ok(out)
}

/// Quadratic Hensel lifting of target = g h (mod p; all monic, g and h coprime) to mod p^k.
fn lift_pair(target: &[Z], g: &[Z], h: &[Z], fp: Fp, k: u64, m: &mut Meter) -> WorkResult<(Vec<Z>, Vec<Z>)> {
    let to_u = |a: &[Z]| -> UPoly { a.iter().map(|c| c.rem_u64(fp.p)).collect() };
    let (one, s, t) = modp::xgcd(fp, &to_u(g), &to_u(h));
    debug_assert_eq!(one, vec![1]);
    let back = |a: &[u64]| -> Vec<Z> { a.iter().map(|&c| Z::S(c as i64)).collect() };
    let (mut g, mut h, mut s, mut t) = (g.to_vec(), h.to_vec(), back(&s), back(&t));
    let p = Z::S(fp.p as i64);
    let mut e = 1u64;
    let mut md = p.clone();
    while e < k {
        e *= 2;
        md = md.mul(&md);
        let zm = ZMod { md: md.clone() };
        let f = zm.reduce(target);
        // gathen and Gerhard, Algorithm 15.10
        let err = zm.sub(&f, &zm.mul(&g, &h, m)?);
        let (q, r) = zm.divrem(&zm.mul(&s, &err, m)?, &h, m)?;
        let g2 = zm.add(&zm.add(&g, &zm.mul(&t, &err, m)?), &zm.mul(&q, &g, m)?);
        let h2 = zm.add(&h, &r);
        let b = zm.sub(&zm.add(&zm.mul(&s, &g2, m)?, &zm.mul(&t, &h2, m)?), &[Z::ONE]);
        let (c, d) = zm.divrem(&zm.mul(&s, &b, m)?, &h2, m)?;
        s = zm.sub(&s, &d);
        t = zm.sub(&zm.sub(&t, &zm.mul(&t, &b, m)?), &zm.mul(&c, &g2, m)?);
        g = g2;
        h = h2;
    }
    Ok((g, h))
}

// ---------------------------------------------------------------------------------------------- recombination

/// The symmetric representative of each coefficient mod md.
fn symmetric(a: &[Z], md: &Z) -> Vec<Z> {
    let half = md.divrem(&Z::S(2)).0;
    a.iter().map(|c| if c.cmp_z(&half) == std::cmp::Ordering::Greater { c.sub(md) } else { c.clone() }).collect()
}

/// Zassenhaus recombination of the lifted monic factors.
fn recombine(f: &[Z], mut lifted: Vec<Vec<Z>>, md: &Z, m: &mut Meter) -> WorkResult<Vec<Vec<Z>>> {
    let zm = ZMod { md: md.clone() };
    let mut f = f.to_vec();
    let mut out = vec![];
    let mut size = 1;
    while 2 * size <= lifted.len() {
        let r = lifted.len();
        let mut found = false;
        // subsets of `size` indices in lexicographic order
        let mut idx: Vec<usize> = (0..size).collect();
        loop {
            m.step(1)?;
            let b = f[degree(&f)].clone();
            // the constant term filter: b * prod u_i(0) must divide b * f(0)
            let mut c0 = b.clone();
            for &i in &idx {
                c0 = zm.red(&c0.mul(&lifted[i][0]));
            }
            let c0 = symmetric(&[c0], md).remove(0);
            let f0b = f[0].mul(&b);
            if (c0.is_zero() && f0b.is_zero()) || (!c0.is_zero() && f0b.divisible(&c0)) {
                let mut g = vec![b.clone()];
                for &i in &idx {
                    g = zm.mul(&g, &lifted[i], m)?;
                }
                let g = dense(&symmetric(&g, md)).primitive().1;
                if let Some(q) = dense(&f).div_exact(&g, m)? {
                    out.push(g.to_dense(0));
                    f = q.primitive().1.to_dense(0);
                    for &i in idx.iter().rev() {
                        lifted.remove(i);
                    }
                    found = true;
                    break;
                }
            }
            // the next subset
            let mut i = size;
            while i > 0 && idx[i - 1] == r - size + i - 1 {
                i -= 1;
            }
            if i == 0 {
                break;
            }
            idx[i - 1] += 1;
            for j in i..size {
                idx[j] = idx[j - 1] + 1;
            }
        }
        if !found {
            size += 1;
        }
    }
    if degree(&f) > 0 {
        out.push(f);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::Unlimited;

    fn d(c: &[i64]) -> Poly {
        Poly::from_dense(1, 0, &c.iter().map(|&x| Z::S(x)).collect::<Vec<_>>())
    }

    fn check(p: &Poly, m: &mut Meter) -> Factored {
        let f = factor(p, false, m).unwrap();
        let mut prod = Poly::constant(p.nv, f.unit.clone());
        for (g, e) in &f.factors {
            prod = prod.mul(&g.pow(*e as u64, m).unwrap(), m).unwrap();
        }
        assert_eq!(&prod, p);
        f
    }

    #[test]
    fn univariate_factorisations() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // x^4 - 1 = (x - 1)(x + 1)(x^2 + 1)
        assert_eq!(check(&d(&[-1, 0, 0, 0, 1]), &mut m).factors.len(), 3);
        // -6 (x + 1)^2 (2x - 3)
        let p = d(&[1, 1]).pow(2, &mut m).unwrap().mul(&d(&[-3, 2]), &mut m).unwrap().scale(&Z::S(-6));
        let f = check(&p, &mut m);
        assert_eq!(f.unit, Z::S(-6));
        assert_eq!(f.factors, vec![(d(&[1, 1]), 2), (d(&[-3, 2]), 1)]);
        // x^8 + 1 is irreducible but splits mod every prime
        assert_eq!(check(&d(&[1, 0, 0, 0, 0, 0, 0, 0, 1]), &mut m).factors.len(), 1);
        // a product of several irreducibles with big coefficients
        let a = d(&[7, 0, -3, 0, 0, 1]);
        let b = d(&[1, 2, 0, 0, 0, 0, 0, 1]);
        let c = d(&[1000003, 1, 0, 1]);
        let p = a.mul(&b, &mut m).unwrap().mul(&c, &mut m).unwrap().mul(&c, &mut m).unwrap();
        let f = check(&p, &mut m);
        assert_eq!(f.factors.len(), 3);
        // x^2 (x + 2)
        assert_eq!(check(&d(&[0, 0, 2, 1]), &mut m).factors.len(), 2);
    }

    #[test]
    fn multivariate_square_free() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let x = Poly::var(2, 0);
        let y = Poly::var(2, 1);
        let one = Poly::constant(2, Z::ONE);
        // (x + y)^2 (x y + 1) (y + 1)
        let s = x.add(&y);
        let p = s.mul(&s, &mut m).unwrap().mul(&x.mul(&y, &mut m).unwrap().add(&one), &mut m).unwrap();
        let p = p.mul(&y.add(&one), &mut m).unwrap();
        let f = check(&p, &mut m);
        assert!(f.factors.contains(&(s, 2)));
        assert!(f.factors.contains(&(y.add(&one), 1)));
    }
}
