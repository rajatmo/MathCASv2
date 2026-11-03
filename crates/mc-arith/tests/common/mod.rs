//! Helpers shared by the integration tests: a reproducible generator, a term printer and conversions to the
//! num-bigint oracle.
#![allow(dead_code)]
use mc_api::{Store, View, T};
use num_bigint::{BigInt, BigUint, Sign};

/// SplitMix64: reproducible test inputs.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
    pub fn shuffle<X>(&mut self, v: &mut [X]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            v.swap(i, j);
        }
    }
}

pub fn big_of_limbs(neg: bool, mag: &[u64]) -> BigInt {
    let mut v = vec![];
    for &l in mag {
        v.push(l as u32);
        v.push((l >> 32) as u32);
    }
    BigInt::from_biguint(if neg { Sign::Minus } else { Sign::Plus }, BigUint::new(v))
}

/// The integer value of a term, if it is an integer.
pub fn big_of<S: Store>(st: &S, t: T<'_>) -> Option<BigInt> {
    match st.view(t) {
        View::Int(v) => Some(BigInt::from(v)),
        View::BigInt { neg, mag } => Some(big_of_limbs(neg, mag)),
        _ => None,
    }
}

/// A term in the abstract notation of the semantics (test output only; recursion is fine on small terms).
pub fn show<S: Store>(st: &S, t: T<'_>) -> String {
    match st.view(t) {
        View::Int(v) => v.to_string(),
        View::BigInt { neg, mag } => big_of_limbs(neg, mag).to_string(),
        View::Rational { num, den } => format!("{}/{}", show(st, num), show(st, den)),
        View::Real(v) => format!("{v:?}"),
        View::Ball { neg, words } => format!("ball({}{words:?})", if neg { "-" } else { "" }),
        View::Str(s) => format!("{s:?}"),
        View::Sym(s) => st.sym_name(s),
        View::App { head, args } => {
            let a: Vec<String> = args.iter().map(|&x| show(st, x)).collect();
            format!("{}({})", show(st, head), a.join(", "))
        }
    }
}
