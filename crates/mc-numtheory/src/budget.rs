//! The work budget of the algorithms: how a long computation charges fuel and notices an interrupt.
//!
//! The algorithms of this crate are plain functions over bare numbers; they take a `&mut dyn Budget` and call
//! [`Budget::charge`] in every loop whose trip count depends on the input. Inside a native the budget is the
//! evaluator's [`NativeCx`] (fuel and the interrupt flag, semantics 10); tests use [`Unlimited`] or [`Limited`].
//!
//! The evaluator polls its control block once per block of 1024 fuel units, so charges are scaled to the work done
//! (decision J.2): one unit is roughly a thousand machine operations, which polls the interrupt about every
//! millisecond of arithmetic.
use mc_eval::{Abort, NativeCx, Res};

/// Fuel and interrupt polling for a long computation.
pub trait Budget {
    /// Charge `units` of fuel (one unit is about a thousand machine operations); an error stops the computation.
    fn charge(&mut self, units: u64) -> Res<()>;
}

impl Budget for NativeCx<'_, '_, '_> {
    fn charge(&mut self, units: u64) -> Res<()> {
        self.tick(units)
    }
}

/// No limit (tests and internal precomputation).
pub struct Unlimited;

impl Budget for Unlimited {
    fn charge(&mut self, _units: u64) -> Res<()> {
        Ok(())
    }
}

/// A limit of `left` units, then [`Abort::Limit`] (tests of interruptibility).
pub struct Limited {
    /// the units still available
    pub left: u64,
}

impl Budget for Limited {
    fn charge(&mut self, units: u64) -> Res<()> {
        if units > self.left {
            self.left = 0;
            return Err(Abort::Limit);
        }
        self.left -= units;
        Ok(())
    }
}

/// The fuel for work on a number of `limbs` 64-bit limbs repeated `times` times: limbs^2 / 1024 per repetition for
/// schoolbook-sized numbers, at least one unit.
pub fn limb_work(limbs: usize, times: u64) -> u64 {
    let l = limbs as u64;
    ((l * l) / 1024).max(1).saturating_mul(times)
}
