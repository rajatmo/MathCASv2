//! Dense polynomials in one variable over a finite field F_q (coefficient vectors of field elements, index =
//! degree, no trailing zeros), with their factorisation.
//!
//! Factorisation follows von zur Gathen and Gerhard, Modern Computer Algebra, chapter 14: the square-free
//! decomposition (Yun's algorithm, with the p-th root taken when the derivative vanishes: algorithm 14.21 and
//! exercise 14.30), distinct-degree factorisation (algorithm 14.3) and Cantor and Zassenhaus's equal-degree
//! splitting (algorithm 14.8; in characteristic 2 by the trace map, exercise 14.16). Over the prime field the
//! factorisation is `mc-poly`'s mod-p code, which implements the same algorithms on machine words.
use crate::field::{El, Field};
use mc_arith::num::Nat;
use mc_eval::Res;
use mc_numtheory::budget::Budget;

/// A polynomial over F_q.
pub type QPoly = Vec<El>;

/// Drop trailing zero coefficients.
pub fn trim(a: &mut QPoly) {
    while a.last().is_some_and(|c| c.is_empty()) {
        a.pop();
    }
}

/// a + b
pub fn add(k: &Field, a: &[El], b: &[El]) -> QPoly {
    let z = vec![];
    let mut r: QPoly = (0..a.len().max(b.len())).map(|i| k.add(a.get(i).unwrap_or(&z), b.get(i).unwrap_or(&z))).collect();
    trim(&mut r);
    r
}

/// a - b
pub fn sub(k: &Field, a: &[El], b: &[El]) -> QPoly {
    let z = vec![];
    let mut r: QPoly = (0..a.len().max(b.len())).map(|i| k.sub(a.get(i).unwrap_or(&z), b.get(i).unwrap_or(&z))).collect();
    trim(&mut r);
    r
}

/// c a
pub fn scale(k: &Field, a: &[El], c: &El) -> QPoly {
    let mut r: QPoly = a.iter().map(|x| k.mul(x, c)).collect();
    trim(&mut r);
    r
}

/// a b
pub fn mul(k: &Field, a: &[El], b: &[El]) -> QPoly {
    if a.is_empty() || b.is_empty() {
        return vec![];
    }
    let mut r: QPoly = vec![vec![]; a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        if x.is_empty() {
            continue;
        }
        for (j, y) in b.iter().enumerate() {
            if !y.is_empty() {
                r[i + j] = k.add(&r[i + j], &k.mul(x, y));
            }
        }
    }
    trim(&mut r);
    r
}

/// (q, r) with a = q b + r, deg r < deg b (b != 0)
pub fn divrem(k: &Field, a: &[El], b: &[El]) -> (QPoly, QPoly) {
    assert!(!b.is_empty(), "division by the zero polynomial");
    if a.len() < b.len() {
        return (vec![], a.to_vec());
    }
    // invariant: the leading coefficient of a trimmed nonzero polynomial is a unit
    let li = k.inv(b.last().unwrap()).expect("a nonzero leading coefficient");
    let db = b.len() - 1;
    let mut r = a.to_vec();
    let mut q: QPoly = vec![vec![]; a.len() - db];
    for i in (0..q.len()).rev() {
        let c = k.mul(&r[i + db], &li);
        if !c.is_empty() {
            for (j, y) in b.iter().enumerate() {
                r[i + j] = k.sub(&r[i + j], &k.mul(&c, y));
            }
        }
        q[i] = c;
    }
    r.truncate(db);
    trim(&mut r);
    trim(&mut q);
    (q, r)
}

/// a mod b
pub fn rem(k: &Field, a: &[El], b: &[El]) -> QPoly {
    divrem(k, a, b).1
}

/// a divided by its leading coefficient
pub fn monic(k: &Field, a: &[El]) -> QPoly {
    match a.last() {
        None => vec![],
        // invariant: nonzero leading coefficient
        Some(l) => scale(k, a, &k.inv(l).expect("a nonzero leading coefficient")),
    }
}

/// The monic gcd.
pub fn gcd(k: &Field, a: &[El], b: &[El]) -> QPoly {
    let (mut x, mut y) = (a.to_vec(), b.to_vec());
    trim(&mut x);
    trim(&mut y);
    while !y.is_empty() {
        let r = rem(k, &x, &y);
        x = std::mem::replace(&mut y, r);
    }
    monic(k, &x)
}

/// (g, s, t) with g = s a + t b the monic gcd (all zero when a = b = 0).
pub fn xgcd(k: &Field, a: &[El], b: &[El]) -> (QPoly, QPoly, QPoly) {
    let (mut r0, mut r1) = (a.to_vec(), b.to_vec());
    trim(&mut r0);
    trim(&mut r1);
    let (mut s0, mut s1): (QPoly, QPoly) = (vec![k.one()], vec![]);
    let (mut t0, mut t1): (QPoly, QPoly) = (vec![], vec![k.one()]);
    while !r1.is_empty() {
        let (q, r) = divrem(k, &r0, &r1);
        r0 = std::mem::replace(&mut r1, r);
        let ns = sub(k, &s0, &mul(k, &q, &s1));
        s0 = std::mem::replace(&mut s1, ns);
        let nt = sub(k, &t0, &mul(k, &q, &t1));
        t0 = std::mem::replace(&mut t1, nt);
    }
    match r0.last() {
        None => (vec![], vec![], vec![]),
        Some(l) => {
            // invariant: nonzero leading coefficient
            let li = k.inv(l).expect("a nonzero leading coefficient");
            (scale(k, &r0, &li), scale(k, &s0, &li), scale(k, &t0, &li))
        }
    }
}

/// a'
pub fn derivative(k: &Field, a: &[El]) -> QPoly {
    let mut r: QPoly = a.iter().enumerate().skip(1).map(|(i, c)| k.scale(c, i as u64)).collect();
    trim(&mut r);
    r
}

/// a(v)
pub fn eval(k: &Field, a: &[El], v: &El) -> El {
    a.iter().rev().fold(vec![], |acc, c| k.add(&k.mul(&acc, v), c))
}

/// a^e mod m
pub fn powmod(k: &Field, a: &[El], e: &Nat, m: &[El], b: &mut dyn Budget) -> Res<QPoly> {
    let mut r = rem(k, &[k.one()], m);
    let base = rem(k, a, m);
    let w = (m.len() * m.len() * k.n * k.n) as u64 / 256 + 1;
    for i in (0..e.bits()).rev() {
        r = rem(k, &mul(k, &r, &r), m);
        if e.bit(i) {
            r = rem(k, &mul(k, &r, &base), m);
        }
        b.charge(w)?;
    }
    Ok(r)
}

/// The square-free decomposition of a monic polynomial: (g, e) with a = prod g^e, the g square-free and coprime.
pub fn square_free(k: &Field, a: &[El], b: &mut dyn Budget) -> Res<Vec<(QPoly, u32)>> {
    let mut out = vec![];
    sqf_into(k, &monic(k, a), 1, &mut out, b)?;
    out.sort_by_key(|x| x.1);
    Ok(out)
}

fn sqf_into(k: &Field, a: &[El], mult: u32, out: &mut Vec<(QPoly, u32)>, b: &mut dyn Budget) -> Res<()> {
    if a.len() <= 1 {
        return Ok(());
    }
    let d = derivative(k, a);
    if d.is_empty() {
        // a = c^p: the p-th root of each coefficient (c^(q/p)) at every p-th position
        let p = k.p() as usize;
        let root_e = k.q.div(&Nat::from_u64(k.p()));
        let r: QPoly = a.iter().step_by(p).map(|c| k.pow(c, &root_e)).collect();
        return sqf_into(k, &r, mult * k.p() as u32, out, b);
    }
    // Yun's algorithm on the part coprime to p, the rest is a p-th power
    let mut c = gcd(k, a, &d);
    let mut w = divrem(k, a, &c).0;
    let mut i = 1u32;
    while w.len() > 1 {
        let y = gcd(k, &w, &c);
        let z = divrem(k, &w, &y).0;
        if z.len() > 1 {
            out.push((monic(k, &z), i * mult));
        }
        w = y;
        c = divrem(k, &c, &w).0;
        i += 1;
        b.charge(1)?;
    }
    if c.len() > 1 {
        // c is a p-th power
        let p = k.p() as usize;
        let root_e = k.q.div(&Nat::from_u64(k.p()));
        let r: QPoly = c.iter().step_by(p).map(|x| k.pow(x, &root_e)).collect();
        sqf_into(k, &r, mult * k.p() as u32, out, b)?;
    }
    Ok(())
}

/// Distinct-degree factorisation of a monic square-free polynomial: (product of its irreducible factors of degree
/// d, d).
pub fn ddf(k: &Field, a: &[El], b: &mut dyn Budget) -> Res<Vec<(QPoly, usize)>> {
    let mut out = vec![];
    let mut f = monic(k, a);
    let x: QPoly = vec![vec![], k.one()];
    let mut h = rem(k, &x, &f);
    let mut d = 0;
    while f.len() > 1 {
        d += 1;
        if 2 * d > f.len() - 1 {
            let deg = f.len() - 1;
            out.push((f, deg));
            break;
        }
        h = powmod(k, &h, &k.q, &f, b)?;
        let g = gcd(k, &sub(k, &h, &x), &f);
        if g.len() > 1 {
            f = divrem(k, &f, &g).0;
            h = rem(k, &h, &f);
            out.push((g, d));
        }
    }
    Ok(out)
}

/// A deterministic sequence of trial polynomials for the equal-degree split (deg < n).
fn trial(k: &Field, i: u64, n: usize) -> QPoly {
    // the coefficients run through the field elements by index
    let mut v: QPoly = vec![];
    let mut s = i.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(i);
    for _ in 0..n.max(1) {
        s ^= s >> 33;
        s = s.wrapping_mul(0xff51_afd7_ed55_8ccd);
        s ^= s >> 29;
        let c = k.from_index(&Nat::from_u64(s).divrem(&k.q).1);
        v.push(c);
    }
    trim(&mut v);
    v
}

/// Split a monic square-free product of irreducible factors of degree d into those factors.
pub fn edf(k: &Field, a: &[El], d: usize, b: &mut dyn Budget) -> Res<Vec<QPoly>> {
    let n = a.len() - 1;
    if n == d {
        return Ok(vec![monic(k, a)]);
    }
    let qd = k.q.pow_u64(d as u64);
    let mut i = 1u64;
    loop {
        let t = trial(k, i, n);
        i += 1;
        if t.len() < 2 {
            continue;
        }
        let g = gcd(k, &t, a);
        let split = if g.len() > 1 && g.len() < a.len() {
            g
        } else if k.p() == 2 {
            // the trace map T(t) = t + t^2 + ... + t^(2^(m d - 1)), q = 2^m
            let steps = k.n * d;
            let mut s = rem(k, &t, a);
            let mut acc = s.clone();
            let w = (a.len() * a.len() * k.n * k.n) as u64 / 256 + 1;
            for _ in 1..steps {
                b.charge(w)?;
                s = rem(k, &mul(k, &s, &s), a);
                acc = add(k, &acc, &s);
            }
            gcd(k, &acc, a)
        } else {
            let e = qd.sub_u64(1).shr(1);
            let w = powmod(k, &t, &e, a, b)?;
            gcd(k, &sub(k, &w, &[k.one()]), a)
        };
        b.charge(1)?;
        if split.len() > 1 && split.len() < a.len() {
            let other = divrem(k, a, &split).0;
            let mut out = edf(k, &split, d, b)?;
            out.extend(edf(k, &monic(k, &other), d, b)?);
            return Ok(out);
        }
    }
}

/// The factorisation of a nonzero polynomial: its leading coefficient and its monic irreducible factors with
/// multiplicities, sorted by degree and then by their coefficients' indices.
pub fn factor(k: &Field, a: &[El], b: &mut dyn Budget) -> Res<(El, Vec<(QPoly, u32)>)> {
    let mut a = a.to_vec();
    trim(&mut a);
    let Some(lc) = a.last().cloned() else { return Ok((vec![], vec![])) };
    let mut out = vec![];
    for (g, e) in square_free(k, &a, b)? {
        for (h, d) in ddf(k, &g, b)? {
            for f in edf(k, &h, d, b)? {
                out.push((f, e));
            }
        }
    }
    sort_factors(k, &mut out);
    Ok((lc, out))
}

/// The order of factors: by degree, then by the polynomial indices of the coefficients from the constant up.
pub fn sort_factors(k: &Field, fs: &mut [(QPoly, u32)]) {
    fs.sort_by(|(f, e), (g, e2)| {
        f.len().cmp(&g.len()).then_with(|| {
            let fi: Vec<Nat> = f.iter().map(|c| k.poly_index(c)).collect();
            let gi: Vec<Nat> = g.iter().map(|c| k.poly_index(c)).collect();
            fi.cmp(&gi)
        }).then(e.cmp(e2))
    });
}

/// The distinct roots in F_q of a nonzero polynomial, by increasing polynomial index.
pub fn roots(k: &Field, a: &[El], b: &mut dyn Budget) -> Res<Vec<El>> {
    let m = monic(k, a);
    if m.len() < 2 {
        return Ok(vec![]);
    }
    let x: QPoly = vec![vec![], k.one()];
    let h = powmod(k, &x, &k.q, &m, b)?;
    let g = gcd(k, &sub(k, &h, &x), &m);
    if g.len() < 2 {
        return Ok(vec![]);
    }
    let mut out: Vec<El> = edf(k, &g, 1, b)?.into_iter().map(|f| k.neg(&f[0])).collect();
    out.sort_by_key(|c| k.poly_index(c));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Repr;
    use mc_numtheory::budget::Unlimited;

    #[test]
    fn factors_over_an_extension() {
        let k = Field::get(17, &[14, 1, 0, 1], Repr::Polynomial);
        // x^3 + 5 x + 19 over F_17^3
        let f: QPoly = vec![k.constant(19), k.constant(5), vec![], k.one()];
        let (lc, fs) = factor(&k, &f, &mut Unlimited).unwrap();
        assert_eq!(lc, k.one());
        let mut prod = vec![lc];
        for (g, e) in &fs {
            for _ in 0..*e {
                prod = mul(&k, &prod, g);
            }
        }
        assert_eq!(prod, f);
        assert_eq!(fs.len(), 3);
    }

    #[test]
    fn square_free_parts_in_characteristic_two() {
        let k = Field::get(2, &[1, 1, 0, 1], Repr::Polynomial);
        // (x + 1)^4 = x^4 + 1
        let f: QPoly = vec![k.one(), vec![], vec![], vec![], k.one()];
        let (_, fs) = factor(&k, &f, &mut Unlimited).unwrap();
        assert_eq!(fs, vec![(vec![k.one(), k.one()], 4)]);
    }

    #[test]
    fn roots_of_the_defining_polynomial() {
        let k = Field::get(17, &[14, 1, 0, 1], Repr::Polynomial);
        let f: QPoly = vec![k.constant(14), k.constant(1), vec![], k.one()];
        let rs = roots(&k, &f, &mut Unlimited).unwrap();
        assert_eq!(rs.len(), 3);
        for r in &rs {
            assert!(eval(&k, &f, r).is_empty());
        }
        assert!(rs.contains(&k.gen()));
    }
}
