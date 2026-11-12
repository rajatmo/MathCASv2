//! Functional decomposition of univariate polynomials, `f = g_1 ∘ g_2 ∘ ... ∘ g_k`, over any field of
//! [`CoeffRing`] (roadmap phase F, item F2; decision F2.4).
//!
//! The right component of a given degree `r` (monic, zero constant term) is unique when the characteristic does not
//! divide `s = n / r` (the tame case), and it is read off the top coefficients of `f`: the reversed polynomial of
//! `f / lc(f)` agrees with the `s`-th power of the reversed `h` modulo `t^r`, so `h` is a truncated `s`-th root
//! (Kozen and Landau, "Polynomial decomposition algorithms", J. Symbolic Comput. 7 (1989); von zur Gathen, "Functional
//! decomposition of polynomials: the tame case", J. Symbolic Comput. 9 (1990)). The left component is the `h`-adic
//! expansion of `f`, whose digits must all be constants; so a candidate is accepted only when `f = g(h)` holds
//! exactly, and every returned decomposition is verified by construction (the expansion is an identity). In the wild
//! case (the characteristic divides `s`) over a small prime field the candidates are enumerated, each checked the same
//! way; beyond 4096 candidates the decomposition is reported as undecided rather than guessed.
//!
//! Order of the search: a polynomial in `x^k` (`k` the gcd of the exponents) is written `g(x^k)` first, with `x^k`
//! kept whole as one component; otherwise right components of increasing degree are tried, and the left component is
//! decomposed again. Components equal to `x` are dropped.
use crate::ring::CoeffRing;
use crate::unipoly::{self as up, deg};
use crate::work::{Meter, WorkResult};

/// The most candidates the wild case enumerates.
const MAX_CANDIDATES: u64 = 4096;

/// A complete decomposition of `f` (degree at least 1) over the field `r`: the components outermost first, every
/// inner one monic with zero constant term; `[f]` when `f` is indecomposable. `None` when a wild case could not be
/// decided. `elements` lists the field's elements when it is a small finite field (for the wild case).
pub fn decompose<R: CoeffRing>(
    r: &R,
    f: &[R::E],
    elements: Option<&[R::E]>,
    m: &mut Meter,
) -> WorkResult<Option<Vec<Vec<R::E>>>> {
    let mut out = vec![];
    let mut cur = f.to_vec();
    // the components found so far are inner ones: cur ∘ out[last] ∘ ... ∘ out[0]
    let mut inner: Vec<Vec<R::E>> = vec![];
    loop {
        m.step(cur.len() as u64)?;
        let n = deg(&cur).unwrap_or(0);
        if n <= 1 {
            break;
        }
        // a polynomial in x^k
        let k = exponent_gcd(r, &cur);
        if k >= 2 {
            let g: Vec<R::E> = cur.iter().step_by(k).cloned().collect();
            let mut xk = vec![r.zero(); k];
            xk.push(r.one());
            inner.push(xk);
            cur = g;
            continue;
        }
        let mut found = None;
        for rr in 2..n {
            if n % rr != 0 {
                continue;
            }
            match right_component(r, &cur, rr, elements, m)? {
                Some(Some(gh)) => {
                    found = Some(gh);
                    break;
                }
                Some(None) => {}
                None => return Ok(None),
            }
        }
        match found {
            Some((g, h)) => {
                inner.push(h);
                cur = g;
            }
            None => break,
        }
    }
    let is_x = |p: &[R::E]| p.len() == 2 && r.is_zero(&p[0]) && r.is_one(&p[1]);
    if !is_x(&cur) || inner.is_empty() {
        out.push(cur);
    }
    out.extend(inner.into_iter().rev().filter(|p| !is_x(p)));
    Ok(Some(out))
}

/// The gcd of the exponents of the nonconstant terms.
fn exponent_gcd<R: CoeffRing>(r: &R, f: &[R::E]) -> usize {
    let mut g = 0usize;
    for (i, c) in f.iter().enumerate().skip(1) {
        if !r.is_zero(c) {
            g = num_gcd(g, i);
            if g == 1 {
                return 1;
            }
        }
    }
    g
}

fn num_gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        num_gcd(b, a % b)
    }
}

/// `f = g(h)` with `deg h = rr`, `h` monic with `h(0) = 0`: `Some(Some((g, h)))`, `Some(None)` when there is none,
/// `None` when the wild case could not be decided.
#[allow(clippy::type_complexity)]
pub fn right_component<R: CoeffRing>(
    r: &R,
    f: &[R::E],
    rr: usize,
    elements: Option<&[R::E]>,
    m: &mut Meter,
) -> WorkResult<Option<Option<(Vec<R::E>, Vec<R::E>)>>> {
    let n = f.len() - 1;
    let s = n / rr;
    let p = r.characteristic();
    let fm = up::monic(r, f, m)?;
    let lcf = up::lc(r, f);
    let finish = |g: Vec<R::E>, h: Vec<R::E>, m: &mut Meter| -> WorkResult<(Vec<R::E>, Vec<R::E>)> {
        Ok((up::scale(r, &g, &lcf, m)?, h))
    };
    if p == 0 || !(s as u64).is_multiple_of(p) {
        let h = approximate_root(r, &fm, rr, s, m)?;
        return Ok(Some(match h_adic(r, &fm, &h, m)? {
            Some(g) => Some(finish(g, h, m)?),
            None => None,
        }));
    }
    // wild: enumerate h = x^rr + c_(rr-1) x^(rr-1) + ... + c_1 x over a small field
    let Some(els) = elements else { return Ok(None) };
    let q = els.len() as u64;
    let count = q.checked_pow(rr as u32 - 1).filter(|&c| c <= MAX_CANDIDATES);
    let Some(count) = count else { return Ok(None) };
    for idx in 0..count {
        m.step(f.len() as u64 * 4)?;
        let mut h = vec![r.zero(); rr + 1];
        h[rr] = r.one();
        let mut v = idx;
        for c in h.iter_mut().take(rr).skip(1) {
            *c = els[(v % q) as usize].clone();
            v /= q;
        }
        if let Some(g) = h_adic(r, &fm, &h, m)? {
            return Ok(Some(Some(finish(g, h, m)?)));
        }
    }
    Ok(Some(None))
}

/// The monic `h` of degree `rr` with `h(0) = 0` whose `s`-th power agrees with the monic `f` in its `rr` top
/// coefficients (the characteristic does not divide `s`).
fn approximate_root<R: CoeffRing>(r: &R, f: &[R::E], rr: usize, s: usize, m: &mut Meter) -> WorkResult<Vec<R::E>> {
    let n = f.len() - 1;
    // F(t) = t^n f(1/t) = 1 + f_(n-1) t + ..., truncated mod t^rr
    let big_f: Vec<R::E> = (0..rr).map(|k| f[n - k].clone()).collect();
    let p = r.characteristic();
    // G = F^(1/s) mod t^rr
    let mut g: Vec<R::E> = vec![r.one()];
    let inv_s = r.inv(&r.of_i64(s as i64), m)?;
    // invariant: the caller checked that the characteristic does not divide s
    let inv_s = inv_s.expect("s is invertible");
    if p == 0 || p > rr as u64 {
        // J. C. P. Miller's recurrence for F^a, a = 1/s: k g_k = sum_(j=1..k) ((a + 1) j - k) f_j g_(k-j)
        for k in 1..rr {
            m.step(k as u64)?;
            let mut acc = r.zero();
            for (j, fj) in big_f.iter().enumerate().take(k + 1).skip(1) {
                // ((1 + s) j - s k) / s
                let w = r.mul(&r.of_i64(((1 + s) * j) as i64 - (s * k) as i64), &inv_s, m)?;
                let t = r.mul(&r.mul(&w, fj, m)?, &g[k - j], m)?;
                acc = r.add(&acc, &t);
            }
            // invariant: k < rr < p (or characteristic 0), so k is invertible
            let ik = r.inv(&r.of_i64(k as i64), m)?.expect("k is invertible");
            g.push(r.mul(&acc, &ik, m)?);
        }
    } else {
        // coefficient by coefficient: [t^k] G^s = s g_k + (terms in g_1 .. g_(k-1))
        for (k, fk) in big_f.iter().enumerate().skip(1) {
            m.step((rr * rr) as u64)?;
            let mut trial = g.clone();
            trial.push(r.zero());
            let pw = trunc_pow(r, &trial, s as u64, k + 1, m)?;
            let rest = up::coef(r, &pw, k);
            let d = r.sub(fk, &rest);
            g.push(r.mul(&d, &inv_s, m)?);
        }
    }
    // h = x^rr + g_1 x^(rr-1) + ... + g_(rr-1) x
    let mut h = vec![r.zero(); rr + 1];
    for (k, gk) in g.into_iter().enumerate() {
        h[rr - k] = gk;
    }
    Ok(up::trim(r, h))
}

/// `a^e mod t^len`.
fn trunc_pow<R: CoeffRing>(r: &R, a: &[R::E], e: u64, len: usize, m: &mut Meter) -> WorkResult<Vec<R::E>> {
    let trunc = |mut v: Vec<R::E>| {
        v.truncate(len);
        up::trim(r, v)
    };
    let mut acc = up::constant(r, r.one());
    let mut b = trunc(a.to_vec());
    let mut e = e;
    while e > 0 {
        if e & 1 == 1 {
            acc = trunc(up::mul(r, &acc, &b, m)?);
        }
        e >>= 1;
        if e > 0 {
            b = trunc(up::mul(r, &b, &b, m)?);
        }
    }
    Ok(acc)
}

/// The `h`-adic digits of `f` when they are all constants: `g` with `f = g(h)`; `None` otherwise. `h` monic.
fn h_adic<R: CoeffRing>(r: &R, f: &[R::E], h: &[R::E], m: &mut Meter) -> WorkResult<Option<Vec<R::E>>> {
    let mut g = vec![];
    let mut cur = f.to_vec();
    while !cur.is_empty() {
        m.step(cur.len() as u64)?;
        // invariant: h is monic, so the division is exact at every step
        let (q, rem) = crate::work::exact(up::divrem(r, &cur, h, m)?, "division by a monic polynomial")?;
        if rem.len() > 1 {
            return Ok(None);
        }
        g.push(up::coef(r, &rem, 0));
        cur = q;
    }
    Ok(Some(up::trim(r, g)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring::{Rationals, Zp};
    use crate::work::Unlimited;
    use mc_arith::num::Rat;

    fn q(v: &[i64]) -> Vec<Rat> {
        up::trim(&Rationals, v.iter().map(|&x| Rat::from_i64(x)).collect())
    }

    fn check(f: &[Rat], comps: &[Vec<Rat>], m: &mut Meter) {
        let r = Rationals;
        let mut acc = vec![r.zero(), r.one()];
        for c in comps.iter().rev() {
            acc = up::compose(&r, c, &acc, m).unwrap();
        }
        assert_eq!(acc, f.to_vec());
    }

    #[test]
    fn decompositions() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let r = Rationals;
        // x^2 + 1 -> [1 + x, x^2]
        let d = decompose(&r, &q(&[1, 0, 1]), None, &mut m).unwrap().unwrap();
        assert_eq!(d, vec![q(&[1, 1]), q(&[0, 0, 1])]);
        // (x^2 + x)^4 + 1 -> [1 + x, x^4, x + x^2]
        let h = q(&[0, 1, 1]);
        let f = up::add(&r, &up::pow(&r, &h, 4, &mut m).unwrap(), &q(&[1]));
        let d = decompose(&r, &f, None, &mut m).unwrap().unwrap();
        assert_eq!(d, vec![q(&[1, 1]), q(&[0, 0, 0, 0, 1]), q(&[0, 1, 1])]);
        check(&f, &d, &mut m);
        // (x^2 + 1)^4 + 3 -> [4 + 2x + x^2, 2x + x^2, x^2]
        let f = up::add(&r, &up::pow(&r, &q(&[1, 0, 1]), 4, &mut m).unwrap(), &q(&[3]));
        let d = decompose(&r, &f, None, &mut m).unwrap().unwrap();
        assert_eq!(d, vec![q(&[4, 2, 1]), q(&[0, 2, 1]), q(&[0, 0, 1])]);
        // a degree-8 polynomial in three quadratic steps
        let f = q(&[8, -4, 2, 8, -5, -8, 2, 4, 1]);
        let d = decompose(&r, &f, None, &mut m).unwrap().unwrap();
        assert_eq!(d, vec![q(&[8, 2, 1]), q(&[0, -2, 1]), q(&[0, 1, 1])]);
        // indecomposable: prime degree, and a degree-4 one
        assert_eq!(decompose(&r, &q(&[2, 3, 3, 1]), None, &mut m).unwrap().unwrap().len(), 1);
        assert_eq!(decompose(&r, &q(&[1, 1, 0, 0, 1]), None, &mut m).unwrap().unwrap().len(), 1);
        // random compositions of degree 2 x 3 x 2 come back verified
        let g1 = q(&[3, -1, 2]);
        let g2 = q(&[0, 5, -2, 1]);
        let g3 = q(&[0, 1, 1]);
        let f = up::compose(&r, &g1, &up::compose(&r, &g2, &g3, &mut m).unwrap(), &mut m).unwrap();
        let d = decompose(&r, &f, None, &mut m).unwrap().unwrap();
        assert_eq!(d.len(), 3);
        check(&f, &d, &mut m);
        // modulo 3: x^4 + 4x^3 + 4x^2 + 6x + 24 = x^4 + x^3 + x^2 -> [x^2 ..., ...]
        let f3 = Zp::new(3);
        let els: Vec<u64> = vec![0, 1, 2];
        let f: Vec<u64> = vec![0, 0, 1, 1, 1];
        let d = decompose(&f3, &f, Some(&els), &mut m).unwrap().unwrap();
        assert_eq!(d.len(), 2);
        let back = up::compose(&f3, &d[0], &d[1], &mut m).unwrap();
        assert_eq!(back, f);
        // wild: modulo 2, (x^2 + x)^2 + x^2 + x = x^4 + x (degree 4 = 2 x 2, s = 2)
        let f2 = Zp::new(2);
        let h: Vec<u64> = vec![0, 1, 1];
        let f = up::add(&f2, &up::pow(&f2, &h, 2, &mut m).unwrap(), &h);
        let d = decompose(&f2, &f, Some(&[0, 1]), &mut m).unwrap().unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(up::compose(&f2, &d[0], &d[1], &mut m).unwrap(), f);
    }
}
