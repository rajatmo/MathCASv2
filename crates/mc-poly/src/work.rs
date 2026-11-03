//! Cooperative interruption of long polynomial kernels (semantics 10, 18.4).
//!
//! Every loop whose trip count depends on the input charges its work to a [`Work`] budget. The natives charge the
//! evaluator's fuel through `NativeCx::tick` (which also polls the interrupt); tests and benchmarks use
//! [`Unlimited`]. Charges are batched: [`Meter`] counts elementary steps and forwards one charge per
//! [`Meter::BLOCK`] steps, so the inner loops stay cheap.
use std::sync::atomic::{AtomicBool, Ordering};

/// Why a kernel ended early; its partial results must be discarded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Halt {
    /// interrupt, fuel or the stop flag
    Stopped,
    /// the result would exceed the memory limit
    Memory,
}

/// The result of an interruptible kernel.
pub type WorkResult<X> = Result<X, Halt>;

/// A budget that a kernel charges its work to.
pub trait Work {
    /// Charge `units` of work (one unit is about [`Meter::BLOCK`] elementary steps); `Err` stops the kernel.
    fn charge(&mut self, units: u64) -> WorkResult<()>;
}

/// No limit (tests, benchmarks of the bare kernels).
pub struct Unlimited;

impl Work for Unlimited {
    fn charge(&mut self, _units: u64) -> WorkResult<()> {
        Ok(())
    }
}

/// A budget that stops when a flag is raised (used by parallel workers, which cannot reach the evaluator).
pub struct Flag<'a>(pub &'a AtomicBool);

impl Work for Flag<'_> {
    fn charge(&mut self, _units: u64) -> WorkResult<()> {
        if self.0.load(Ordering::Relaxed) {
            Err(Halt::Stopped)
        } else {
            Ok(())
        }
    }
}

/// Counts elementary steps and charges the budget once per [`Meter::BLOCK`] of them.
pub struct Meter<'w> {
    work: &'w mut dyn Work,
    steps: u64,
}

impl<'w> Meter<'w> {
    /// Elementary steps per unit charged.
    pub const BLOCK: u64 = 1 << 14;

    /// A meter over `work`.
    pub fn new(work: &'w mut dyn Work) -> Meter<'w> {
        Meter { work, steps: 0 }
    }
    /// Count `n` steps.
    #[inline]
    pub fn step(&mut self, n: u64) -> WorkResult<()> {
        self.steps += n;
        if self.steps >= Self::BLOCK {
            let units = self.steps / Self::BLOCK;
            self.steps %= Self::BLOCK;
            self.work.charge(units)?;
        }
        Ok(())
    }
    /// The underlying budget (for nested kernels).
    pub fn work(&mut self) -> &mut dyn Work {
        &mut *self.work
    }
}
