//! The work budget of the algorithms: how a long computation charges fuel and notices an interrupt.
//!
//! The algorithms of this crate are plain functions over bare numbers; they take a `&mut dyn Budget` and call
//! [`Budget::charge`] in every loop whose trip count depends on the input. Inside a native the budget is the
//! evaluator's [`NativeCx`] (fuel and the interrupt flag, semantics 10); tests use [`Unlimited`] or [`Limited`].
//!
//! The evaluator polls its control block once per block of 1024 fuel units, so charges are scaled to the work done
//! (decision J.2): one unit is roughly a thousand machine operations, which polls the interrupt about every
//! millisecond of arithmetic.
use mc_eval::{Abort, Control, NativeCx, Res};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Fuel and interrupt polling for a long computation.
pub trait Budget {
    /// Charge `units` of fuel (one unit is about a thousand machine operations); an error stops the computation.
    fn charge(&mut self, units: u64) -> Res<()>;
    /// What an interrupt raises, for parallel workers that cannot charge (they poll it between pieces of work and
    /// end early; the caller then reports the stop). `None`: nothing to poll.
    fn stop_signal(&self) -> Option<StopSignal> {
        None
    }
}

/// The stop flag of the session and the evaluation's control block (an interrupt raises one or the other).
#[derive(Clone)]
pub struct StopSignal {
    flag: Arc<AtomicBool>,
    ctl: Arc<Control>,
}

impl Budget for NativeCx<'_, '_, '_> {
    fn charge(&mut self, units: u64) -> Res<()> {
        self.tick(units)
    }
    fn stop_signal(&self) -> Option<StopSignal> {
        Some(StopSignal { flag: self.stop_flag().clone(), ctl: self.control() })
    }
}

/// A poll of [`Budget::stop_signal`] that worker threads share.
#[derive(Clone)]
pub struct Halt(Option<StopSignal>);

impl Halt {
    /// The poll for `b`.
    pub fn of(b: &dyn Budget) -> Halt {
        Halt(b.stop_signal())
    }
    /// Whether a stop is requested.
    #[inline]
    pub fn up(&self) -> bool {
        self.0.as_ref().is_some_and(|s| s.flag.load(Ordering::Relaxed) || s.ctl.should_stop())
    }
    /// `Err` (the evaluator's stop) when a stop is requested: after a parallel round whose workers may have ended
    /// early. A charge of two fuel blocks first lets the evaluator report the stop in its own way.
    pub fn check(&self, b: &mut dyn Budget) -> Res<()> {
        if self.up() {
            b.charge(2048)?;
            Err(Abort::Limit)
        } else {
            Ok(())
        }
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
