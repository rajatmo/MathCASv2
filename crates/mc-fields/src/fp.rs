//! The prime field F_p for a prime p < 2^63, and dense polynomials over it (coefficient vectors, index = degree,
//! no trailing zeros; the zero polynomial is empty).
//!
//! The polynomial algorithms are the classical ones (von zur Gathen and Gerhard, Modern Computer Algebra, chapters
//! 2-4 and 14): schoolbook products, long division, Euclid's algorithm with cofactors, square-and-multiply powers
//! modulo a polynomial, and Ben-Or's irreducibility test (section 14.9): f of degree n is irreducible over F_p
//! exactly when gcd(x^(p^k) - x, f) = 1 for k = 1..n/2.
use mc_arith::num::Nat;

/// A prime p < 2^63 and its arithmetic on residues 0 <= a < p.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fp {
    /// the prime
    pub p: u64,
}

impl Fp {
    /// The field of the prime p (2 <= p < 2^63, p prime: the caller checks primality).
    pub fn new(p: u64) -> Fp {
        debug_assert!((2..1 << 63).contains(&p));
        Fp { p }
    }
    /// a + b
    #[inline]
    pub fn add(self, a: u64, b: u64) -> u64 {
        let s = a + b;
        if s >= self.p {
            s - self.p
        } else {
            s
        }
    }
    /// a - b
    #[inline]
    pub fn sub(self, a: u64, b: u64) -> u64 {
        if a >= b {
            a - b
        } else {
            a + self.p - b
        }
    }
    /// -a
    #[inline]
    pub fn neg(self, a: u64) -> u64 {
        if a == 0 {
            0
        } else {
            self.p - a
        }
    }
    /// a b
    #[inline]
    pub fn mul(self, a: u64, b: u64) -> u64 {
        if self.p < 1 << 32 {
            a * b % self.p
        } else {
            ((a as u128 * b as u128) % self.p as u128) as u64
        }
    }
    /// a^e
    pub fn pow(self, a: u64, mut e: u64) -> u64 {
        let (mut r, mut b) = (1 % self.p, a);
        while e > 0 {
            if e & 1 == 1 {
                r = self.mul(r, b);
            }
            b = self.mul(b, b);
            e >>= 1;
        }
        r
    }
    /// 1/a for a != 0 (Euclid on machine words)
    pub fn inv(self, a: u64) -> Option<u64> {
        if a == 0 {
            return None;
        }
        let (mut r0, mut r1) = (self.p as i128, a as i128);
        let (mut t0, mut t1) = (0i128, 1i128);
        while r1 != 0 {
            let q = r0 / r1;
            (r0, r1) = (r1, r0 - q * r1);
            (t0, t1) = (t1, t0 - q * t1);
        }
        if r0 != 1 {
            return None;
        }
        Some(t0.rem_euclid(self.p as i128) as u64)
    }
    /// The residue of a signed machine integer.
    pub fn of_i64(self, v: i64) -> u64 {
        (v as i128).rem_euclid(self.p as i128) as u64
    }
    /// The residue of a natural number.
    pub fn of_nat(self, v: &Nat) -> u64 {
        v.divrem_u64(self.p).1
    }
}

/// A dense polynomial over F_p.
pub type Poly = Vec<u64>;

/// Drop trailing zero coefficients.
pub fn trim(a: &mut Poly) {
    while a.last() == Some(&0) {
        a.pop();
    }
}

/// The degree (-1 for the zero polynomial).
pub fn deg(a: &[u64]) -> isize {
    a.len() as isize - 1
}

/// a + b
pub fn add(f: Fp, a: &[u64], b: &[u64]) -> Poly {
    let mut r: Poly = (0..a.len().max(b.len()))
        .map(|i| f.add(a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0)))
        .collect();
    trim(&mut r);
    r
}

/// a - b
pub fn sub(f: Fp, a: &[u64], b: &[u64]) -> Poly {
    let mut r: Poly = (0..a.len().max(b.len()))
        .map(|i| f.sub(a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0)))
        .collect();
    trim(&mut r);
    r
}

/// c a
pub fn scale(f: Fp, a: &[u64], c: u64) -> Poly {
    let mut r: Poly = a.iter().map(|&x| f.mul(x, c)).collect();
    trim(&mut r);
    r
}

/// a b (schoolbook; for p < 2^32 the products are summed in 128 bits and reduced once per coefficient)
pub fn mul(f: Fp, a: &[u64], b: &[u64]) -> Poly {
    if a.is_empty() || b.is_empty() {
        return vec![];
    }
    let mut acc = vec![0u128; a.len() + b.len() - 1];
    if f.p < 1 << 32 {
        for (i, &x) in a.iter().enumerate() {
            if x == 0 {
                continue;
            }
            for (j, &y) in b.iter().enumerate() {
                acc[i + j] += (x * y) as u128;
            }
        }
        let p = f.p as u128;
        let mut r: Poly = acc.into_iter().map(|v| (v % p) as u64).collect();
        trim(&mut r);
        return r;
    }
    let mut r = vec![0u64; a.len() + b.len() - 1];
    for (i, &x) in a.iter().enumerate() {
        for (j, &y) in b.iter().enumerate() {
            r[i + j] = f.add(r[i + j], f.mul(x, y));
        }
    }
    trim(&mut r);
    r
}

/// (q, r) with a = q b + r, deg r < deg b (b != 0)
pub fn divrem(f: Fp, a: &[u64], b: &[u64]) -> (Poly, Poly) {
    assert!(!b.is_empty(), "division by the zero polynomial");
    if a.len() < b.len() {
        return (vec![], a.to_vec());
    }
    // invariant: b is nonzero, so its leading coefficient is invertible modulo the prime
    let li = f.inv(*b.last().unwrap()).expect("a nonzero leading coefficient");
    let mut r = a.to_vec();
    let db = b.len() - 1;
    let mut q = vec![0u64; a.len() - db];
    for i in (0..q.len()).rev() {
        let c = f.mul(r[i + db], li);
        q[i] = c;
        if c != 0 {
            for (j, &y) in b.iter().enumerate() {
                r[i + j] = f.sub(r[i + j], f.mul(c, y));
            }
        }
    }
    r.truncate(db);
    trim(&mut r);
    trim(&mut q);
    (q, r)
}

/// a mod b
pub fn rem(f: Fp, a: &[u64], b: &[u64]) -> Poly {
    divrem(f, a, b).1
}

/// a divided by its leading coefficient
pub fn monic(f: Fp, a: &[u64]) -> Poly {
    match a.last() {
        None => vec![],
        // invariant: the leading coefficient of a trimmed polynomial is nonzero
        Some(&l) => scale(f, a, f.inv(l).expect("a nonzero leading coefficient")),
    }
}

/// The monic gcd.
pub fn gcd(f: Fp, a: &[u64], b: &[u64]) -> Poly {
    let (mut x, mut y) = (a.to_vec(), b.to_vec());
    trim(&mut x);
    trim(&mut y);
    while !y.is_empty() {
        let r = rem(f, &x, &y);
        x = std::mem::replace(&mut y, r);
    }
    monic(f, &x)
}

/// (g, s, t) with g = s a + t b the monic gcd.
pub fn xgcd(f: Fp, a: &[u64], b: &[u64]) -> (Poly, Poly, Poly) {
    let (mut r0, mut r1) = (a.to_vec(), b.to_vec());
    trim(&mut r0);
    trim(&mut r1);
    let (mut s0, mut s1) = (vec![1u64], vec![]);
    let (mut t0, mut t1): (Poly, Poly) = (vec![], vec![1u64]);
    while !r1.is_empty() {
        let (q, r) = divrem(f, &r0, &r1);
        r0 = std::mem::replace(&mut r1, r);
        let ns = sub(f, &s0, &mul(f, &q, &s1));
        s0 = std::mem::replace(&mut s1, ns);
        let nt = sub(f, &t0, &mul(f, &q, &t1));
        t0 = std::mem::replace(&mut t1, nt);
    }
    match r0.last() {
        None => (vec![], vec![], vec![]),
        Some(&l) => {
            // invariant: nonzero leading coefficient
            let li = f.inv(l).expect("a nonzero leading coefficient");
            (scale(f, &r0, li), scale(f, &s0, li), scale(f, &t0, li))
        }
    }
}

/// a(v)
pub fn eval(f: Fp, a: &[u64], v: u64) -> u64 {
    a.iter().rev().fold(0, |acc, &c| f.add(f.mul(acc, v), c))
}

/// a'
pub fn derivative(f: Fp, a: &[u64]) -> Poly {
    let mut r: Poly = a.iter().enumerate().skip(1).map(|(i, &c)| f.mul(c, (i as u64) % f.p)).collect();
    trim(&mut r);
    r
}

/// a b mod m
pub fn mulmod(f: Fp, a: &[u64], b: &[u64], m: &[u64]) -> Poly {
    rem(f, &mul(f, a, b), m)
}

/// a^e mod m for an exponent given as a natural number
pub fn powmod(f: Fp, a: &[u64], e: &Nat, m: &[u64]) -> Poly {
    let mut r: Poly = rem(f, &[1], m);
    let base = rem(f, a, m);
    for i in (0..e.bits()).rev() {
        r = mulmod(f, &r, &r, m);
        if e.bit(i) {
            r = mulmod(f, &r, &base, m);
        }
    }
    r
}

/// The primes dividing n (n >= 1, small: trial division)
pub fn prime_divisors(mut n: u64) -> Vec<u64> {
    let mut out = vec![];
    let mut d = 2;
    while d * d <= n {
        if n.is_multiple_of(d) {
            out.push(d);
            while n.is_multiple_of(d) {
                n /= d;
            }
        }
        d += 1;
    }
    if n > 1 {
        out.push(n);
    }
    out
}

/// x^(p^k) mod m, by k p-th powers
pub fn frobenius_x(f: Fp, k: usize, m: &[u64]) -> Poly {
    let p = Nat::from_u64(f.p);
    let mut r = rem(f, &[0, 1], m);
    for _ in 0..k {
        r = powmod(f, &r, &p, m);
    }
    r
}

/// Is the polynomial (degree >= 1) irreducible over F_p? Ben-Or's test: no factor of degree k <= n/2, that is
/// gcd(x^(p^k) - x, f) = 1 for k = 1..n/2 (von zur Gathen and Gerhard, section 14.9); most reducible polynomials
/// fail at a small k, so the search for irreducible polynomials stops early on them.
pub fn is_irreducible(f: Fp, a: &[u64]) -> bool {
    let mut a = a.to_vec();
    trim(&mut a);
    let n = a.len().saturating_sub(1);
    if n == 0 {
        return false;
    }
    if n == 1 {
        return true;
    }
    let m = monic(f, &a);
    let x = vec![0, 1];
    let p = Nat::from_u64(f.p);
    let mut r = rem(f, &x, &m);
    for _ in 0..n / 2 {
        r = powmod(f, &r, &p, &m);
        if gcd(f, &sub(f, &r, &x), &m).len() != 1 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residues_and_inverses() {
        let f = Fp::new(17);
        assert_eq!(f.mul(5, 7), 1);
        assert_eq!(f.inv(5), Some(7));
        assert_eq!(f.of_i64(-500), 10);
        let g = Fp::new((1 << 61) - 1);
        let a = (1 << 60) + 12345;
        assert_eq!(g.mul(a, g.inv(a).unwrap()), 1);
    }

    #[test]
    fn division_and_gcd() {
        let f = Fp::new(7);
        let a = mul(f, &[1, 1], &[3, 0, 1]);
        let (q, r) = divrem(f, &a, &[3, 0, 1]);
        assert_eq!(q, vec![1, 1]);
        assert!(r.is_empty());
        assert_eq!(gcd(f, &a, &mul(f, &[1, 1], &[3, 1])), vec![1, 1]);
        let (g, s, t) = xgcd(f, &[1, 2, 3], &[4, 1]);
        assert_eq!(add(f, &mul(f, &s, &[1, 2, 3]), &mul(f, &t, &[4, 1])), g);
    }

    #[test]
    fn rabin() {
        let f = Fp::new(2);
        assert!(is_irreducible(f, &[1, 1, 0, 1]));
        assert!(!is_irreducible(f, &[1, 0, 0, 1]));
        assert!(is_irreducible(f, &[1, 1, 0, 0, 1]));
        assert!(!is_irreducible(f, &[1, 0, 1, 0, 1]));
        let g = Fp::new(17);
        assert!(is_irreducible(g, &[14, 1, 0, 1]));
    }
}
