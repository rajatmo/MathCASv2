//! Property tests against num-bigint: random operands through source text, and the algorithms directly.
mod common;
use common::Nt;
use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{One, Signed, Zero};

/// SplitMix64: reproducible operands.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// A random integer of up to `limbs` limbs, of either sign.
    fn int(&mut self, limbs: usize) -> BigInt {
        let l = 1 + self.below(limbs as u64) as usize;
        let digits: Vec<u32> = (0..2 * l).map(|_| self.next() as u32).collect();
        let mag = BigUint::from_slice(&digits) >> self.below(64);
        BigInt::from_biguint(if self.next() & 1 == 1 { Sign::Minus } else { Sign::Plus }, mag)
    }
}

fn floor_div(a: &BigInt, b: &BigInt) -> BigInt {
    let (q, r) = (a / b, a % b);
    if !r.is_zero() && (r.sign() != b.sign()) {
        q - 1
    } else {
        q
    }
}

fn gcd(a: &BigUint, b: &BigUint) -> BigUint {
    let (mut a, mut b) = (a.clone(), b.clone());
    while !b.is_zero() {
        let r = &a % &b;
        a = std::mem::replace(&mut b, r);
    }
    a
}

#[test]
fn integer_division_gcd_and_lcm_agree_with_num_bigint() {
    let mut nt = Nt::new();
    let mut rng = Rng(2024);
    for i in 0..300 {
        let a = rng.int(1 + i % 12);
        let mut b = rng.int(1 + i % 7);
        if b.is_zero() {
            b = BigInt::one();
        }
        let q = floor_div(&a, &b);
        let m = &a - &b * &q;
        assert_eq!(nt.eval(&format!("quotient({a}, {b})")), q.to_string(), "quotient({a}, {b})");
        assert_eq!(nt.eval(&format!("mod({a}, {b})")), m.to_string(), "mod({a}, {b})");
        let g = gcd(&a.magnitude().clone(), &b.magnitude().clone());
        assert_eq!(nt.eval(&format!("gcd({a}, {b})")), g.to_string(), "gcd({a}, {b})");
        let l = if a.is_zero() { BigUint::zero() } else { a.magnitude() / &g * b.magnitude() };
        assert_eq!(nt.eval(&format!("lcm({a}, {b})")), l.to_string(), "lcm({a}, {b})");
        // floor of a rational a/b
        assert_eq!(nt.eval(&format!("floor({a}/{b})")), q.to_string());
    }
}

#[test]
fn powers_digits_and_factorials_agree_with_num_bigint() {
    let mut nt = Nt::new();
    let mut rng = Rng(77);
    for i in 0..120 {
        let a = rng.int(6).abs();
        let e = rng.int(3).abs();
        let mut m = rng.int(1 + i % 5).abs();
        if m.is_zero() {
            m = BigInt::from(7);
        }
        let want = a.magnitude().modpow(e.magnitude(), m.magnitude());
        assert_eq!(nt.eval(&format!("power_mod({a}, {e}, {m})")), want.to_string(), "power_mod({a}, {e}, {m})");
        let base = 2 + rng.below(255) as u32;
        let ds = a.magnitude().to_radix_be(base);
        let shown: Vec<String> =
            if a.is_zero() { vec!["0".into()] } else { ds.iter().map(|d| d.to_string()).collect() };
        assert_eq!(nt.eval(&format!("integer_digits({a}, {base})")), format!("[{}]", shown.join(", ")));
        assert_eq!(
            nt.eval(&format!("integer_length({a}, {base})")),
            if a.is_zero() { 0 } else { ds.len() }.to_string()
        );
    }
    let mut f = BigUint::one();
    for n in 1..=600u32 {
        f *= n;
        if n % 37 == 0 {
            assert_eq!(nt.eval(&format!("factorial({n})")), f.to_string());
        }
    }
}

/// Miller-Rabin by num-bigint with the prime bases below 100.
fn mr_prime(n: &BigUint) -> bool {
    let small = [2u32, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97];
    if *n < BigUint::from(2u32) {
        return false;
    }
    for p in small {
        if *n == BigUint::from(p) {
            return true;
        }
        if (n % p).is_zero() {
            return false;
        }
    }
    let nm1 = n - 1u32;
    let s = nm1.trailing_zeros().unwrap();
    let d = &nm1 >> s;
    small.iter().all(|&a| {
        let mut x = BigUint::from(a).modpow(&d, n);
        if x.is_one() || x == nm1 {
            return true;
        }
        for _ in 1..s {
            x = &x * &x % n;
            if x == nm1 {
                return true;
            }
        }
        false
    })
}

#[test]
fn factorisations_multiply_back_to_primes() {
    let mut nt = Nt::new();
    let mut rng = Rng(5);
    for i in 0..60 {
        let n = rng.int(1 + i % 3).abs() + 2u32;
        let out = nt.eval(&format!("factor_integer({n})"));
        // [[p, e], ...]
        let inner = out.trim_start_matches("[[").trim_end_matches("]]");
        let mut prod = BigUint::one();
        let mut last = BigUint::zero();
        for pair in inner.split("], [") {
            let (p, e) = pair.split_once(", ").unwrap();
            let p: BigUint = p.parse().unwrap();
            let e: u32 = e.parse().unwrap();
            assert!(mr_prime(&p), "{p} in the factorisation of {n}");
            assert!(p > last);
            prod *= p.pow(e);
            last = p;
        }
        assert_eq!(prod, *n.magnitude(), "{n}");
    }
}

#[test]
fn primality_and_next_primes_agree_with_miller_rabin() {
    let mut nt = Nt::new();
    let mut rng = Rng(99);
    for i in 0..40 {
        let n = rng.int(1 + i % 4).abs();
        let p: BigUint = nt.eval(&format!("next_prime({n})")).parse().unwrap();
        assert!(mr_prime(&p) && p > *n.magnitude());
        let mut c = n.magnitude() + 1u32;
        while c < p {
            assert!(!mr_prime(&c));
            c += 1u32;
        }
        for k in [n.magnitude().clone(), p.clone(), &p * &p, &p + 2u32] {
            assert_eq!(nt.eval(&format!("is_prime({k})")), mr_prime(&k).to_string(), "is_prime({k})");
        }
    }
}
