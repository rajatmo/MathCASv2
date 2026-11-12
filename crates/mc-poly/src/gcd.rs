//! Greatest common divisors of polynomials over the integers: Brown's dense modular algorithm (decision K.7).
//!
//! The gcd over Z is assembled from gcds modulo large primes by Chinese remaindering; each gcd mod p in k
//! variables is assembled from gcds in k - 1 variables at evaluation points of the last variable by Newton
//! interpolation, down to univariate Euclid. Images are normalised by the gcd of the leading coefficients (in Z,
//! and in `Z_p[x_last]` one level down), so that the images of the true gcd agree; an image of too high degree
//! (an unlucky prime or point) is discarded, one of lower degree discards what was gathered. When the
//! interpolant stops changing it is checked by division: a candidate that divides both inputs and has the
//! image degree is the gcd (the images' degree is never below the true degree), so the result is always
//! correct, whatever the luck.
use crate::modp::{self, Fp, Primes, UPoly};
use crate::poly::Poly;
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use mc_arith::num::{Int, Nat};

/// A sparse polynomial mod p in `nv` variables, terms in decreasing lexicographic order, nonzero coefficients.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Sp {
    nv: usize,
    exps: Vec<u32>,
    coefs: Vec<u64>,
}

impl Sp {
    fn zero(nv: usize) -> Sp {
        Sp { nv, exps: vec![], coefs: vec![] }
    }
    fn len(&self) -> usize {
        self.coefs.len()
    }
    fn is_zero(&self) -> bool {
        self.coefs.is_empty()
    }
    fn e(&self, i: usize) -> &[u32] {
        &self.exps[i * self.nv..(i + 1) * self.nv]
    }
    fn of_poly(p: &Poly, f: Fp) -> Sp {
        let mut s = Sp::zero(p.nv);
        for (e, c) in p.terms() {
            let r = c.rem_u64(f.p);
            if r != 0 {
                s.exps.extend_from_slice(e);
                s.coefs.push(r);
            }
        }
        s
    }
    /// The runs of terms sharing all exponents but the last: (start, end) index pairs.
    fn runs(&self) -> Vec<(usize, usize)> {
        let k = self.nv - 1;
        let mut out = vec![];
        let mut i = 0;
        while i < self.len() {
            let mut j = i + 1;
            while j < self.len() && self.e(j)[..k] == self.e(i)[..k] {
                j += 1;
            }
            out.push((i, j));
            i = j;
        }
        out
    }
    /// The univariate polynomial in the last variable of the run [i, j).
    fn run_poly(&self, i: usize, j: usize) -> UPoly {
        let k = self.nv - 1;
        let mut u = vec![0u64; self.e(i)[k] as usize + 1];
        for t in i..j {
            u[self.e(t)[k] as usize] = self.coefs[t];
        }
        u
    }
    /// The polynomial whose run for prefix `pre` is `u` (prefixes given in decreasing order).
    fn push_run(&mut self, pre: &[u32], u: &[u64]) {
        for d in (0..u.len()).rev() {
            if u[d] != 0 {
                self.exps.extend_from_slice(pre);
                self.exps.push(d as u32);
                self.coefs.push(u[d]);
            }
        }
    }
    /// The polynomial with the last variable replaced by `a` (one variable fewer).
    fn eval_last(&self, f: Fp, a: u64) -> Sp {
        let k = self.nv - 1;
        let mut out = Sp::zero(k);
        for (i, j) in self.runs() {
            let v = modp::eval(f, &self.run_poly(i, j), a);
            if v != 0 {
                out.exps.extend_from_slice(&self.e(i)[..k]);
                out.coefs.push(v);
            }
        }
        out
    }
    /// The gcd of the runs (the content in Z_p[x_last]), monic.
    fn content_last(&self, f: Fp, m: &mut Meter) -> WorkResult<UPoly> {
        let mut g: UPoly = vec![];
        for (i, j) in self.runs() {
            g = modp::gcd(f, &g, &self.run_poly(i, j), m)?;
            if g.len() == 1 {
                break;
            }
        }
        Ok(g)
    }
    /// Every run multiplied (`div` false) or exactly divided (`div` true) by u.
    fn map_runs(&self, f: Fp, u: &[u64], div: bool) -> Sp {
        let k = self.nv - 1;
        let mut out = Sp::zero(self.nv);
        for (i, j) in self.runs() {
            let r = self.run_poly(i, j);
            let v = if div { modp::divrem(f, &r, u).0 } else { modp::mul(f, &r, u) };
            out.push_run(&self.e(i)[..k], &v);
        }
        out
    }
    /// The leading run (the coefficient of the largest monomial in the other variables).
    fn lc_last(&self) -> UPoly {
        let runs = self.runs();
        let (i, j) = runs[0];
        self.run_poly(i, j)
    }
    fn scale(&self, f: Fp, c: u64) -> Sp {
        let mut s = self.clone();
        for x in s.coefs.iter_mut() {
            *x = f.mul(*x, c);
        }
        s.coefs.retain(|&x| x != 0);
        s
    }
    fn monic(&self, f: Fp) -> Sp {
        self.scale(f, f.inv(self.coefs[0]))
    }
    /// self - c x^e o
    fn sub_term_mul(&self, f: Fp, o: &Sp, e: &[u32], c: u64) -> Sp {
        let nv = self.nv;
        let shifted: Vec<u32> =
            o.exps.chunks(nv).flat_map(|t| t.iter().zip(e).map(|(a, b)| a + b).collect::<Vec<_>>()).collect();
        let mut out = Sp::zero(nv);
        let (mut i, mut j) = (0, 0);
        let se = |j: usize| &shifted[j * nv..(j + 1) * nv];
        while i < self.len() || j < o.len() {
            let ord = if i == self.len() {
                std::cmp::Ordering::Less
            } else if j == o.len() {
                std::cmp::Ordering::Greater
            } else {
                self.e(i).cmp(se(j))
            };
            match ord {
                std::cmp::Ordering::Greater => {
                    out.exps.extend_from_slice(self.e(i));
                    out.coefs.push(self.coefs[i]);
                    i += 1;
                }
                std::cmp::Ordering::Less => {
                    out.exps.extend_from_slice(se(j));
                    out.coefs.push(f.neg(f.mul(c, o.coefs[j])));
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    let v = f.sub(self.coefs[i], f.mul(c, o.coefs[j]));
                    if v != 0 {
                        out.exps.extend_from_slice(self.e(i));
                        out.coefs.push(v);
                    }
                    i += 1;
                    j += 1;
                }
            }
        }
        out
    }
    /// Does d divide self (mod p)?
    fn divides(&self, f: Fp, d: &Sp, m: &mut Meter) -> WorkResult<bool> {
        let lt = d.e(0).to_vec();
        let inv = f.inv(d.coefs[0]);
        let mut r = self.clone();
        while !r.is_zero() {
            m.step((r.len() + d.len()) as u64)?;
            let e = r.e(0);
            if e.iter().zip(&lt).any(|(a, b)| a < b) {
                return Ok(false);
            }
            let qe: Vec<u32> = e.iter().zip(&lt).map(|(a, b)| a - b).collect();
            let c = f.mul(r.coefs[0], inv);
            r = r.sub_term_mul(f, d, &qe, c);
        }
        Ok(true)
    }
    /// The leading monomial (for degree comparisons of images).
    fn lead(&self) -> &[u32] {
        self.e(0)
    }
}

/// The gcd of two polynomials mod p, monic (leading coefficient 1 in the lexicographic order).
fn pgcd(f: Fp, a: &Sp, b: &Sp, m: &mut Meter) -> WorkResult<Sp> {
    let nv = a.nv;
    if a.is_zero() {
        return Ok(if b.is_zero() { b.clone() } else { b.monic(f) });
    }
    if b.is_zero() {
        return Ok(a.monic(f));
    }
    if nv == 1 {
        let ua = Sp::run_poly(a, 0, a.len());
        let ub = Sp::run_poly(b, 0, b.len());
        let g = modp::gcd(f, &ua, &ub, m)?;
        let mut out = Sp::zero(1);
        out.push_run(&[], &g);
        return Ok(out);
    }
    let k = nv - 1;
    // the variable drops out when neither input has it
    let dega = (0..a.len()).map(|i| a.e(i)[k]).max().unwrap_or(0);
    let degb = (0..b.len()).map(|i| b.e(i)[k]).max().unwrap_or(0);
    if dega == 0 && degb == 0 {
        let g = pgcd(f, &a.eval_last(f, 0), &b.eval_last(f, 0), m)?;
        return Ok(add_last(&g));
    }
    // contents in Z_p[x_last]
    let ca = a.content_last(f, m)?;
    let cb = b.content_last(f, m)?;
    let a = a.map_runs(f, &ca, true);
    let b = b.map_runs(f, &cb, true);
    let c = modp::gcd(f, &ca, &cb, m)?;
    let g = modp::gcd(f, &a.lc_last(), &b.lc_last(), m)?;
    let bound = dega.min(degb) as usize + g.len();
    // the interpolant, the product of (x - alpha) so far, and the number of points used
    let mut h: Option<Sp> = None;
    let mut q: UPoly = vec![1];
    let mut points = 0usize;
    let mut alpha = 0u64;
    loop {
        alpha += 1;
        // the large primes in use have far more points than any degree bound needs
        // invariant: the large primes in use have far more evaluation points than any degree bound needs
        assert!(alpha < f.p, "evaluation points exhausted");
        let ga = modp::eval(f, &g, alpha);
        if ga == 0 {
            continue;
        }
        m.step(1)?;
        let img = pgcd(f, &a.eval_last(f, alpha), &b.eval_last(f, alpha), m)?;
        let img = img.scale(f, ga);
        if let Some(cur) = &h {
            match img.lead().cmp(&cur.e(0)[..k]) {
                std::cmp::Ordering::Greater => continue,
                std::cmp::Ordering::Less => {
                    h = None;
                    q = vec![1];
                    points = 0;
                }
                std::cmp::Ordering::Equal => {}
            }
        }
        // a constant image: the gcd is the content gcd
        if img.lead().iter().all(|&e| e == 0) {
            let mut one = Sp::zero(nv);
            one.push_run(&vec![0; k], &c);
            return Ok(one.monic(f));
        }
        let (next, unchanged) = match &h {
            None => (add_last(&img), false),
            Some(cur) => newton(f, cur, &img, &q, alpha),
        };
        h = Some(next);
        q = modp::mul(f, &q, &[f.neg(alpha), 1]);
        points += 1;
        if unchanged || points > bound {
            // invariant: `h` was set two lines above
            let cand = h.as_ref().expect("an interpolant");
            let cc = cand.content_last(f, m)?;
            let pp = cand.map_runs(f, &cc, true);
            if a.divides(f, &pp, m)? && b.divides(f, &pp, m)? {
                return Ok(pp.map_runs(f, &c, false).monic(f));
            }
        }
    }
}

/// A polynomial in k variables as one in k + 1 (the last of degree 0).
fn add_last(s: &Sp) -> Sp {
    let mut out = Sp::zero(s.nv + 1);
    for i in 0..s.len() {
        out.exps.extend_from_slice(s.e(i));
        out.exps.push(0);
        out.coefs.push(s.coefs[i]);
    }
    out
}

/// Newton step: the interpolant that agrees with `cur` where q vanishes and with `img` at alpha; also whether it
/// equals `cur` (the new value was predicted).
fn newton(f: Fp, cur: &Sp, img: &Sp, q: &[u64], alpha: u64) -> (Sp, bool) {
    let nv = cur.nv;
    let k = nv - 1;
    let scale = f.inv(modp::eval(f, q, alpha));
    let runs = cur.runs();
    let mut out = Sp::zero(nv);
    let mut unchanged = true;
    let (mut r, mut j) = (0, 0);
    // merge the prefixes of cur's runs and img's terms, both in decreasing order
    while r < runs.len() || j < img.len() {
        let ord = if r == runs.len() {
            std::cmp::Ordering::Less
        } else if j == img.len() {
            std::cmp::Ordering::Greater
        } else {
            cur.e(runs[r].0)[..k].cmp(img.e(j))
        };
        let (pre, u, v): (Vec<u32>, UPoly, u64) = match ord {
            std::cmp::Ordering::Greater => {
                let (s, e) = runs[r];
                r += 1;
                (cur.e(s)[..k].to_vec(), cur.run_poly(s, e), 0)
            }
            std::cmp::Ordering::Less => {
                j += 1;
                (img.e(j - 1).to_vec(), vec![], img.coefs[j - 1])
            }
            std::cmp::Ordering::Equal => {
                let (s, e) = runs[r];
                r += 1;
                j += 1;
                (cur.e(s)[..k].to_vec(), cur.run_poly(s, e), img.coefs[j - 1])
            }
        };
        let d = f.sub(v, modp::eval(f, &u, alpha));
        let w = if d == 0 {
            u
        } else {
            unchanged = false;
            modp::add(f, &u, &modp::scale(f, q, f.mul(d, scale)))
        };
        out.push_run(&pre, &w);
    }
    (out, unchanged)
}

/// The gcd of two integer polynomials: nonnegative content gcd times a primitive part with positive leading
/// coefficient (zero only for two zeros).
pub fn gcd(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    let nv = a.nv;
    if a.is_zero() {
        return Ok(b.primitive().1.scale(&b.content()));
    }
    if b.is_zero() {
        return Ok(a.primitive().1.scale(&a.content()));
    }
    let (ca, pa) = a.primitive();
    let (cb, pb) = b.primitive();
    let c = ca.gcd(&cb);
    if pa.is_constant() || pb.is_constant() {
        return Ok(Poly::constant(nv, c));
    }
    if pa == pb {
        return Ok(pa.scale(&c));
    }
    // compress to the variables that occur (the ring may hold others)
    let mut used = pa.used_vars();
    used.extend(pb.used_vars());
    used.sort_unstable();
    used.dedup();
    if used.len() < nv {
        let sub = |p: &Poly| {
            let terms = p.terms().map(|(e, c)| (used.iter().map(|&v| e[v]).collect(), c.clone())).collect();
            Poly::from_terms(used.len(), terms)
        };
        let g = gcd_primitive(&sub(&pa), &sub(&pb), m)?;
        return Ok(g.embed(nv, &used).scale(&c));
    }
    Ok(gcd_primitive(&pa, &pb, m)?.scale(&c))
}

/// The gcd of two primitive polynomials with positive leading coefficients (positive leading coefficient).
fn gcd_primitive(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    let nv = a.nv;
    // Brown's algorithm is dense in every variable
    a.check_dense()?;
    b.check_dense()?;
    let lg = a.lc().gcd(&b.lc());
    // the Chinese remainder image so far: terms (exponents, symmetric residue) and the modulus
    let mut h: Option<(Image, Nat)> = None;
    for p in Primes::large() {
        m.step(1)?;
        if lg.rem_u64(p) == 0 {
            continue;
        }
        let f = Fp::new(p);
        let (sa, sb) = (Sp::of_poly(a, f), Sp::of_poly(b, f));
        let g = pgcd(f, &sa, &sb, m)?;
        if g.lead().iter().all(|&e| e == 0) {
            return Ok(Poly::constant(nv, Z::ONE));
        }
        let g = g.scale(f, lg.rem_u64(p));
        if let Some((terms, _)) = &h {
            match g.lead().cmp(&terms[0].0) {
                std::cmp::Ordering::Greater => continue,
                std::cmp::Ordering::Less => h = None,
                std::cmp::Ordering::Equal => {}
            }
        }
        let changed = match h.take() {
            None => {
                let terms =
                    (0..g.len()).map(|i| (g.e(i).to_vec(), Z::from_residue_sym(g.coefs[i], p).to_int())).collect();
                h = Some((terms, Nat::from_u64(p)));
                true
            }
            Some((terms, modulus)) => {
                let (terms, modulus, changed) = crt_step(terms, &modulus, &g, p);
                h = Some((terms, modulus));
                changed
            }
        };
        if !changed {
            // invariant: `h` was set just above
            let (terms, _) = h.as_ref().expect("an image");
            let cand = Poly::from_terms(nv, terms.iter().map(|(e, c)| (e.clone(), Z::from_int(c.clone()))).collect());
            let pp = cand.primitive().1;
            if a.div_exact(&pp, m)?.is_some() && b.div_exact(&pp, m)?.is_some() {
                return Ok(pp);
            }
        }
    }
    // invariant: the loop over the primes below 2^62 returns long before they run out
    unreachable!("the primes below 2^62 do not run out")
}

/// An image being reassembled: (exponents, symmetric residue) by term.
type Image = Vec<(Vec<u32>, Int)>;

/// Combine the image `terms` mod `modulus` with the image `g` mod p (missing terms are zero residues): the new
/// terms in symmetric range, the new modulus, and whether any value changed.
fn crt_step(terms: Image, modulus: &Nat, g: &Sp, p: u64) -> (Image, Nat, bool) {
    let f = Fp::new(p);
    let inv = f.inv(modulus.divrem_u64(p).1);
    let new_mod = modulus.mul_u64(p);
    let half = new_mod.shr(1);
    let mut changed = false;
    let mut combine = |c: Int, r: u64| -> Int {
        let cr = Z::from_int(c.clone()).rem_u64(p);
        if cr == r {
            return c;
        }
        changed = true;
        // c + modulus * ((r - c) / modulus mod p), then into the symmetric range
        let t = f.mul(f.sub(r, cr), inv);
        let v = c.add(&Int::from_nat(modulus.mul_u64(t)));
        if !v.neg && v.mag > half {
            v.sub(&Int::from_nat(new_mod.clone()))
        } else if v.neg && v.mag > half {
            v.add(&Int::from_nat(new_mod.clone()))
        } else {
            v
        }
    };
    let mut out = Vec::with_capacity(terms.len().max(g.len()));
    let mut it = terms.into_iter().peekable();
    let mut j = 0;
    loop {
        let ord = match (it.peek(), j < g.len()) {
            (None, false) => break,
            (None, true) => std::cmp::Ordering::Less,
            (Some(_), false) => std::cmp::Ordering::Greater,
            (Some((e, _)), true) => e.as_slice().cmp(g.e(j)),
        };
        let (e, v) = match ord {
            std::cmp::Ordering::Greater => {
                // invariant: `peek` returned a term
                let (e, c) = it.next().expect("peeked");
                (e, combine(c, 0))
            }
            std::cmp::Ordering::Less => {
                j += 1;
                (g.e(j - 1).to_vec(), combine(Int::default(), g.coefs[j - 1]))
            }
            std::cmp::Ordering::Equal => {
                // invariant: `peek` returned a term
                let (e, c) = it.next().expect("peeked");
                j += 1;
                (e, combine(c, g.coefs[j - 1]))
            }
        };
        if !v.is_zero() {
            out.push((e, v));
        }
    }
    (out, new_mod, changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::Unlimited;

    fn p(nv: usize, t: &[(&[u32], i64)]) -> Poly {
        Poly::from_terms(nv, t.iter().map(|(e, c)| (e.to_vec(), Z::S(*c))).collect())
    }

    #[test]
    fn univariate_and_multivariate() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // (x + 1)(x - 2) and (x + 1)(x + 5)
        let g = p(1, &[(&[1], 1), (&[0], 1)]);
        let a = g.mul(&p(1, &[(&[1], 1), (&[0], -2)]), &mut m).unwrap();
        let b = g.mul(&p(1, &[(&[1], 1), (&[0], 5)]), &mut m).unwrap();
        assert_eq!(gcd(&a, &b, &mut m).unwrap(), g);
        assert_eq!(gcd(&a.scale(&Z::S(6)), &b.scale(&Z::S(-4)), &mut m).unwrap(), g.scale(&Z::S(2)));
        // (x y + z^2 + 3)^2 times coprime cofactors in three variables
        let g = p(3, &[(&[1, 1, 0], 1), (&[0, 0, 2], 1), (&[0, 0, 0], 3)]);
        let g2 = g.mul(&g, &mut m).unwrap();
        let u = p(3, &[(&[2, 0, 0], 1), (&[0, 1, 1], -1), (&[0, 0, 0], 7)]);
        let v = p(3, &[(&[0, 2, 0], 2), (&[1, 0, 1], 1), (&[0, 0, 0], -1)]);
        let a = g2.mul(&u, &mut m).unwrap();
        let b = g2.mul(&v, &mut m).unwrap();
        assert_eq!(gcd(&a, &b, &mut m).unwrap(), g2);
        assert_eq!(gcd(&u, &v, &mut m).unwrap(), Poly::constant(3, Z::ONE));
        // a gcd that only involves some variables
        let x = p(3, &[(&[1, 0, 0], 1), (&[0, 0, 0], 1)]);
        let a = x.mul(&p(3, &[(&[0, 1, 0], 1), (&[0, 0, 0], 1)]), &mut m).unwrap();
        let b = x.mul(&p(3, &[(&[0, 1, 0], 1), (&[0, 0, 0], 2)]), &mut m).unwrap();
        assert_eq!(gcd(&a, &b, &mut m).unwrap(), x);
        // big coefficients force several primes
        let big = Z::S(i64::MAX).pow(3);
        let g = p(1, &[(&[1], 1), (&[0], 0)]).add(&Poly::constant(1, big));
        let a = g.mul(&p(1, &[(&[2], 1), (&[0], 1)]), &mut m).unwrap();
        let b = g.mul(&p(1, &[(&[2], 1), (&[0], 3)]), &mut m).unwrap();
        assert_eq!(gcd(&a, &b, &mut m).unwrap(), g);
    }
}
