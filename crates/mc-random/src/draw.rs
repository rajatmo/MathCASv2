//! Conversions from random words to values (decision RNG.5). Each conversion consumes a number of words that
//! depends only on the stream (never on the thread count or timing): one per uniform real, two per normal pair, and
//! for integers a rejection loop whose length is fixed by the words drawn.
use crate::philox::Stream;
use mc_arith::num::{Int, Nat};

const TWO_M53: f64 = 1.0 / (1u64 << 53) as f64;

impl Stream {
    /// A uniform real in [0, 1): the top 53 bits of one word, times 2^-53 (every multiple of 2^-53 equally likely).
    #[inline]
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * TWO_M53
    }

    /// A uniform real in (0, 1], for logarithms.
    #[inline]
    fn uniform_open0(&mut self) -> f64 {
        ((self.next_u64() >> 11) + 1) as f64 * TWO_M53
    }

    /// A uniform integer in [0, n) for `n >= 1`, without bias: Lemire's multiply-and-reject ("Fast random integer
    /// generation in an interval", ACM TOMACS 2019). One word, plus one more with probability below n / 2^64.
    #[inline]
    pub fn below(&mut self, n: u64) -> u64 {
        debug_assert!(n > 0);
        let mut m = self.next_u64() as u128 * n as u128;
        if (m as u64) < n {
            let t = n.wrapping_neg() % n;
            while (m as u64) < t {
                m = self.next_u64() as u128 * n as u128;
            }
        }
        (m >> 64) as u64
    }

    /// A uniform integer in [0, w] (w any natural number). Up to 2^64 - 1 by [`Stream::below`]; above, by drawing
    /// `bits(w)` bits (whole words, the top one masked) and rejecting values above w, fewer than 2 tries on average.
    pub fn up_to(&mut self, w: &Nat) -> Nat {
        match w.limbs.as_slice() {
            [] => Nat::zero(),
            [u64::MAX] => Nat::from_u64(self.next_u64()),
            [x] => Nat::from_u64(self.below(x + 1)),
            limbs => {
                let n = limbs.len();
                let top_bits = 64 - limbs[n - 1].leading_zeros();
                let mask = if top_bits == 64 { u64::MAX } else { (1u64 << top_bits) - 1 };
                loop {
                    let mut v: Vec<u64> = (0..n).map(|_| self.next_u64()).collect();
                    v[n - 1] &= mask;
                    let r = Nat::from_limbs(v);
                    if r <= *w {
                        return r;
                    }
                }
            }
        }
    }

    /// A uniform integer in [a, b] (a <= b).
    pub fn integer(&mut self, a: &Int, b: &Int) -> Int {
        let w = b.sub(a);
        debug_assert!(!w.neg);
        a.add(&Int::from_nat(self.up_to(&w.mag)))
    }

    /// Two independent standard normal reals by the Box–Muller transform (decision RNG.6): exactly two words.
    #[inline]
    pub fn normal_pair(&mut self) -> (f64, f64) {
        let u1 = self.uniform_open0();
        let u2 = self.uniform();
        let r = (-2.0 * u1.ln()).sqrt();
        let (s, c) = (std::f64::consts::TAU * u2).sin_cos();
        (r * c, r * s)
    }

    /// `n` standard normal reals: `ceil(n / 2)` pairs; an odd count drops the last pair's second value.
    pub fn normals(&mut self, n: usize, out: &mut Vec<f64>) {
        for _ in 0..n / 2 {
            let (x, y) = self.normal_pair();
            out.push(x);
            out.push(y);
        }
        if n % 2 == 1 {
            out.push(self.normal_pair().0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_range_and_ends() {
        let mut s = Stream::from_key(1);
        for _ in 0..10000 {
            let u = s.uniform();
            assert!((0.0..1.0).contains(&u));
        }
        assert_eq!((u64::MAX >> 11) as f64 * TWO_M53, 1.0 - f64::EPSILON / 2.0);
    }

    #[test]
    fn below_is_unbiased_on_a_small_range() {
        let mut s = Stream::from_key(2);
        let mut counts = [0u32; 3];
        for _ in 0..30000 {
            counts[s.below(3) as usize] += 1;
        }
        for c in counts {
            assert!((9500..10500).contains(&c), "{counts:?}");
        }
        // the largest range takes the whole word
        let mut t = Stream::from_key(2);
        let w = t.clone().next_u64();
        assert_eq!(t.up_to(&Nat::from_u64(u64::MAX)), Nat::from_u64(w));
    }

    #[test]
    fn big_ranges() {
        let mut s = Stream::from_key(3);
        let w = Nat::pow2(100).sub_u64(1);
        let mut high = 0;
        for _ in 0..1000 {
            let r = s.up_to(&w);
            assert!(r <= w);
            if r.bit(99) {
                high += 1;
            }
        }
        assert!((400..600).contains(&high), "{high}");
        // [2^64, 2^64 + 2]: three values, rejection keeps them in range
        let a = Int::from_nat(Nat::pow2(64));
        let b = a.add(&Int::from_u64(2));
        for _ in 0..100 {
            let r = s.integer(&a, &b);
            assert!(r.cmp_int(&a).is_ge() && r.cmp_int(&b).is_le());
        }
    }
}
