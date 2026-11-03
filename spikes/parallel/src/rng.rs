//! A splittable generator (SplitMix64). The stream of element `i` of a `ptable` or `pmap` is a function of the seed
//! and `i` only (semantics 18.2 item 5), so it does not depend on which thread evaluates the element or when.
#[derive(Clone, Copy, Debug)]
pub struct SplitMix(u64);

const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

use crate::finalize;

impl SplitMix {
    pub fn new(seed: u64) -> SplitMix {
        SplitMix(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(GOLDEN);
        finalize(self.0)
    }
    /// A draw in `0..n` (`n > 0`); the slight modulo bias is irrelevant here.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
    /// The stream of element `i` under `seed`: the state is a bijective mix of both, so distinct indices start far
    /// apart in the sequence and streams do not overlap in practice.
    pub fn split(seed: u64, i: u64) -> SplitMix {
        SplitMix(finalize(seed ^ finalize(i.wrapping_add(GOLDEN))))
    }
}
