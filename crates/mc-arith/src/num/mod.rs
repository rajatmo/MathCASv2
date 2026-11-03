//! The bare number layer: values outside the store (integration decision I3.6: kernels compute on bare values and
//! intern only results).
//!
//! - [`Nat`]: natural numbers on 64-bit limbs with schoolbook, Karatsuba and three-prime NTT multiplication, Knuth
//!   and Newton division, square and k-th roots, and divide-and-conquer decimal conversion (spike 3);
//! - [`Int`]: sign and magnitude;
//! - [`gcd`]: Lehmer's gcd (integration piece I3);
//! - [`Rat`]: canonical rationals with Knuth/Henrici sum and product (I3);
//! - [`Ball`], [`Float`], [`Mag`]: ball arithmetic with an own-mantissa midpoint and a rounded-up 32-bit radius
//!   (spike 3, decisions 3.4 and 3.5);
//! - [`float`]: correctly rounded conversion of exact values to machine reals.
//!
//! Long loops poll a cooperative stop flag (semantics 10) installed per thread by [`StopScope`]; a stopped kernel
//! returns an unspecified value, which the caller must discard.
mod ball;
pub mod float;
pub mod gcd;
mod int;
mod nat;
mod ntt;
mod rat;

pub use ball::{Ball, Float, Mag};
pub use int::Int;
pub use nat::{Divisor, Nat};
pub use rat::Rat;

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

thread_local! {
    static STOP: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// Has the stop flag of the current thread's scope been raised?
#[inline]
pub fn stopped() -> bool {
    STOP.with(|s| s.borrow().as_ref().is_some_and(|f| f.load(Ordering::Relaxed)))
}

/// Installs a stop flag for the current thread until dropped, restoring the previous one (scopes nest).
pub struct StopScope {
    prev: Option<Arc<AtomicBool>>,
}

impl StopScope {
    /// Polls `flag` (or nothing, for `None`) in the number kernels of this thread while the scope lives.
    pub fn enter(flag: Option<&Arc<AtomicBool>>) -> StopScope {
        let prev = STOP.with(|s| std::mem::replace(&mut *s.borrow_mut(), flag.cloned()));
        StopScope { prev }
    }
}

impl Drop for StopScope {
    fn drop(&mut self) {
        let prev = self.prev.take();
        STOP.with(|s| *s.borrow_mut() = prev);
    }
}

/// SplitMix64 (spike 3's generator): reproducible test inputs.
#[cfg(test)]
pub(crate) struct Rng(pub u64);

#[cfg(test)]
impl Rng {
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// Uniform in 0..n (n > 0); the modulo bias is irrelevant for test data.
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_scopes_nest() {
        assert!(!stopped());
        let flag = Arc::new(AtomicBool::new(true));
        {
            let _outer = StopScope::enter(Some(&flag));
            assert!(stopped());
            {
                let _inner = StopScope::enter(None);
                assert!(!stopped());
            }
            assert!(stopped());
        }
        assert!(!stopped());
    }
}
