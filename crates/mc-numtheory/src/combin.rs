//! Factorials and binomial coefficients, exact (benchmark N5: 10^6!).
//!
//! n! is computed by Luschny's split recursion (decision J.8): n! = 2^(n - s_2(n)) prod_{i >= 0} P_i, where L_i is
//! the product of the odd numbers in (n / 2^(i+1), n / 2^i] and P_i = L_i L_(i+1) ...; the odd ranges are
//! multiplied by balanced product trees, so the big products are of similar sizes and use the NTT. Binomials with a
//! large lower index use Kummer's theorem: the exponent of a prime p in C(n, k) is the number of carries when k and
//! n - k are added in base p, and the prime powers are multiplied by a product tree.
use crate::budget::Budget;
use crate::sieve::primes_in;
use mc_arith::num::Nat;
use mc_eval::Res;

/// Product of a list of naturals by a balanced tree.
pub fn product(xs: Vec<Nat>, b: &mut dyn Budget) -> Res<Nat> {
    let mut level = xs;
    if level.is_empty() {
        return Ok(Nat::from_u64(1));
    }
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut it = level.into_iter();
        while let Some(a) = it.next() {
            match it.next() {
                Some(c) => {
                    b.charge((a.len() + c.len()) as u64 / 16 + 1)?;
                    next.push(a.mul(&c));
                }
                None => next.push(a),
            }
        }
        level = next;
    }
    Ok(level.pop().unwrap())
}

/// The odd numbers in (lo, hi], packed into words (products below 2^64), as naturals.
fn odd_words(lo: u64, hi: u64) -> Vec<Nat> {
    let mut out = vec![];
    let mut acc = 1u64;
    let mut m = (lo + 1) | 1;
    while m <= hi {
        match acc.checked_mul(m) {
            Some(v) => acc = v,
            None => {
                out.push(Nat::from_u64(acc));
                acc = m;
            }
        }
        m += 2;
    }
    if acc > 1 {
        out.push(Nat::from_u64(acc));
    }
    out
}

/// log2(n!) by Stirling's series (for size checks).
pub fn log2_factorial(n: u64) -> f64 {
    if n < 2 {
        return 0.0;
    }
    let x = n as f64;
    (x * x.ln() - x + 0.5 * (2.0 * std::f64::consts::PI * x).ln() + 1.0 / (12.0 * x)) / std::f64::consts::LN_2
}

/// n!.
pub fn factorial(n: u64, b: &mut dyn Budget) -> Res<Nat> {
    if n < 21 {
        return Ok(Nat::from_u64((1..=n).product()));
    }
    let top = 63 - n.leading_zeros();
    let (mut p, mut r) = (Nat::from_u64(1), Nat::from_u64(1));
    for i in (0..=top).rev() {
        let (lo, hi) = (n >> (i + 1), n >> i);
        let l = product(odd_words(lo, hi), b)?;
        if l != Nat::from_u64(1) {
            p = p.mul(&l);
        }
        if p != Nat::from_u64(1) {
            b.charge((p.len() + r.len()) as u64 / 16 + 1)?;
            r = r.mul(&p);
        }
    }
    Ok(r.shl(n - n.count_ones() as u64))
}

/// log2 C(n, k) approximately (0 <= k <= n).
pub fn log2_binomial(n: u64, k: u64) -> f64 {
    log2_factorial(n) - log2_factorial(k) - log2_factorial(n - k)
}

/// C(n, k) for 0 <= k <= n.
pub fn binomial(n: u64, k: u64, b: &mut dyn Budget) -> Res<Nat> {
    let k = k.min(n - k);
    if k == 0 {
        return Ok(Nat::from_u64(1));
    }
    if k < 64 {
        // C(n, i + 1) = C(n, i) (n - i) / (i + 1), each step exact
        let mut r = Nat::from_u64(1);
        for i in 0..k {
            r = r.mul_u64(n - i).divrem_u64(i + 1).0;
        }
        return Ok(r);
    }
    if n <= 1 << 34 {
        // Kummer: the prime powers in C(n, k)
        let mut factors = vec![];
        let mut lo = 2;
        while lo <= n {
            let hi = (lo + (1 << 24)).min(n + 1);
            for p in primes_in(lo, hi, b)? {
                let mut e = 0u32;
                let (mut a, mut c, mut carry) = (k, n - k, 0u64);
                while a > 0 || c > 0 || carry > 0 {
                    let s = a % p + c % p + carry;
                    carry = (s >= p) as u64;
                    e += carry as u32;
                    a /= p;
                    c /= p;
                }
                if e > 0 {
                    factors.push(Nat::from_u64(p).pow_u64(e as u64));
                }
            }
            lo = hi;
        }
        return product(factors, b);
    }
    // the falling factorial over k!
    let mut num = vec![];
    let mut acc = 1u64;
    for i in 0..k {
        let m = n - i;
        match acc.checked_mul(m) {
            Some(v) => acc = v,
            None => {
                num.push(Nat::from_u64(acc));
                acc = m;
            }
        }
    }
    num.push(Nat::from_u64(acc));
    let top = product(num, b)?;
    Ok(top.div(&factorial(k, b)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;
    use num_bigint::BigUint;

    fn big(x: &Nat) -> BigUint {
        BigUint::parse_bytes(x.to_decimal().as_bytes(), 10).unwrap()
    }

    #[test]
    fn factorials_against_a_loop() {
        let mut f = BigUint::from(1u32);
        for n in 0..=1500u64 {
            if n > 0 {
                f *= n;
            }
            if n % 97 == 0 || n < 30 {
                assert_eq!(big(&factorial(n, &mut Unlimited).unwrap()), f, "{n}!");
            }
        }
        // the number of decimal digits of 10^4!
        assert_eq!(factorial(10_000, &mut Unlimited).unwrap().to_decimal().len(), 35660);
    }

    #[test]
    fn binomials() {
        let u = &mut Unlimited;
        assert_eq!(binomial(10, 3, u).unwrap(), Nat::from_u64(120));
        assert_eq!(binomial(10, 0, u).unwrap(), Nat::from_u64(1));
        assert_eq!(binomial(10, 10, u).unwrap(), Nat::from_u64(1));
        for (n, k) in [(200u64, 100u64), (1000, 333), (5000, 70), (3000, 2990)] {
            let want =
                big(&factorial(n, u).unwrap()) / (big(&factorial(k, u).unwrap()) * big(&factorial(n - k, u).unwrap()));
            assert_eq!(big(&binomial(n, k, u).unwrap()), want, "C({n}, {k})");
        }
        // the falling-factorial path
        let n = (1u64 << 35) + 11;
        let got = binomial(n, 70, u).unwrap();
        let mut want = BigUint::from(1u32);
        for i in 0..70u64 {
            want = want * (n - i) / (i + 1);
        }
        assert_eq!(big(&got), want);
        let got = binomial(n, 64, u).unwrap();
        let mut want = BigUint::from(1u32);
        for i in 0..64u64 {
            want = want * (n - i) / (i + 1);
        }
        assert_eq!(big(&got), want);
    }
}
