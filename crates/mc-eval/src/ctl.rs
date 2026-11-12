//! The control block of an evaluation: one fuel budget, one stop flag and one failure status, shared by every task
//! and laid out as the compiled-code ABI (spike E, decisions E.5 and E.6), so compiled functions can later poll the
//! very same atomics.
//!
//! **Fuel** is drawn in blocks of [`FUEL_BLOCK`] units: a task spends from the block in hand and polls the stop flag
//! and the status when it draws the next one, so the shared cache line is touched once per block (E.5 found that
//! polling a shared counter every iteration contends at 4 threads). Only the frontier draws from the budget
//! (decision F.3): a speculative task spends a private share and its kept work is charged to the budget when the
//! frontier keeps it, so discarded speculative work never consumes the budget (semantics 18.4, A.5).
//!
//! **Status**: the first failure is recorded by compare-and-swap; once it is set, every task stops at its next poll.
//! [`Status`] maps to the failure terms of semantics 10 (decision F.4).
use mc_api::names::failure as tag;
use std::mem::{offset_of, size_of};
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering::*};

/// Fuel units drawn from the budget at a time; also the polling interval (decision F.3, E.5's 1024).
pub const FUEL_BLOCK: u64 = 1024;

/// The control block. Its layout is part of the compiled-code ABI: `fuel` at offset 0, `stop` at 8, `status` at 12.
#[repr(C)]
#[derive(Debug)]
pub struct Control {
    /// Remaining fuel of the running evaluation.
    pub fuel: AtomicI64,
    /// Nonzero: an external interrupt was requested.
    pub stop: AtomicU32,
    /// 0 while running; otherwise the [`Status`] of the first failure.
    pub status: AtomicU32,
}

/// Byte offset of [`Control::fuel`] (ABI).
pub const CTL_FUEL: usize = offset_of!(Control, fuel);
/// Byte offset of [`Control::stop`] (ABI).
pub const CTL_STOP: usize = offset_of!(Control, stop);
/// Byte offset of [`Control::status`] (ABI).
pub const CTL_STATUS: usize = offset_of!(Control, status);
// invariant: checked at compile time
const _: () = assert!(CTL_FUEL == 0 && CTL_STOP == 8 && CTL_STATUS == 12 && size_of::<Control>() == 16);

/// Why an evaluation stopped. The numbering is shared with compiled code (spike E's `Status`), which also uses
/// `Index` and `DivZero`; the evaluator adds `Depth`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Status {
    /// Not stopped.
    Running = 0,
    /// The stop flag was raised: `failure(interrupted)`.
    Interrupted = 1,
    /// The fuel budget is exhausted: `failure(limit, fuel)`.
    Fuel = 2,
    /// Compiled code: an index out of bounds (an ordinary failure, not a limit).
    Index = 3,
    /// Compiled code: an integer division by zero (an ordinary failure, not a limit).
    DivZero = 4,
    /// The evaluation nested deeper than the configured depth: `failure(limit, depth)` (decision F.11).
    Depth = 5,
    /// The young region outgrew the evaluation's memory budget: `failure(limit, memory)` (decision p6core.11).
    Memory = 6,
    /// One evaluation level was rewritten more times than the iteration limit: `failure(limit, iterations)`
    /// (decision p6core.11).
    Iterations = 7,
}

impl Status {
    /// The status with this code; unknown codes read as `Interrupted` (a stop of unknown cause).
    pub fn from_u32(v: u32) -> Status {
        match v {
            0 => Status::Running,
            2 => Status::Fuel,
            3 => Status::Index,
            4 => Status::DivZero,
            5 => Status::Depth,
            6 => Status::Memory,
            7 => Status::Iterations,
            _ => Status::Interrupted,
        }
    }

    /// The tags of the failure term this status gives: `failure(tags...)` (semantics 10).
    pub fn failure_tags(self) -> &'static [&'static str] {
        match self {
            Status::Running => &[],
            Status::Interrupted => &[tag::INTERRUPTED],
            Status::Fuel => &[tag::LIMIT, tag::FUEL],
            Status::Index => &["index"],
            Status::DivZero => &[tag::DIVISION_BY_ZERO],
            Status::Depth => &[tag::LIMIT, tag::DEPTH],
            Status::Memory => &[tag::LIMIT, tag::MEMORY],
            Status::Iterations => &[tag::LIMIT, "iterations"],
        }
    }
}

impl Default for Control {
    fn default() -> Control {
        Control { fuel: AtomicI64::new(0), stop: AtomicU32::new(0), status: AtomicU32::new(0) }
    }
}

impl Control {
    /// Ask the running evaluation to stop (any thread; every task stops at its next poll). A request made before an
    /// evaluation begins stops that evaluation at its first poll: requests are kept until
    /// [`Control::clear_interrupt`].
    pub fn interrupt(&self) {
        self.stop.store(1, Release);
    }

    /// Withdraw an interrupt request (between evaluations, by the session's owner).
    pub fn clear_interrupt(&self) {
        self.stop.store(0, Release);
    }

    /// Withdraw status `s` if it is the current one (a limit a native set up for part of an evaluation fired and
    /// was handled there, [`crate::constrained`]).
    pub(crate) fn clear_status(&self, s: Status) {
        let _ = self.status.compare_exchange(s as u32, 0, AcqRel, Relaxed);
    }

    /// The current status.
    pub fn status(&self) -> Status {
        Status::from_u32(self.status.load(Acquire))
    }

    /// Prepare for a new evaluation with this budget. A pending interrupt request is kept, so that one raised just
    /// before the evaluation begins is not lost.
    pub(crate) fn begin(&self, fuel: i64) {
        self.fuel.store(fuel.max(0), Relaxed);
        self.status.store(0, Release);
    }

    /// Record the first failure (later ones are ignored).
    pub(crate) fn fail(&self, s: Status) {
        let _ = self.status.compare_exchange(0, s as u32, AcqRel, Relaxed);
    }

    /// The poll: has the evaluation been stopped? Turns a raised stop flag into the `Interrupted` status.
    #[inline]
    pub(crate) fn halted(&self) -> bool {
        if self.stop.load(Relaxed) != 0 {
            self.fail(Status::Interrupted);
            return true;
        }
        self.status.load(Relaxed) != 0
    }

    /// Take up to `want` units of fuel from the budget; returns what was taken (0 when it is exhausted). A
    /// compare-and-swap loop, so a draw never takes the budget below zero and never fails spuriously.
    pub(crate) fn draw(&self, want: u64) -> u64 {
        let mut cur = self.fuel.load(Relaxed);
        loop {
            if cur <= 0 {
                return 0;
            }
            let take = (want.min(i64::MAX as u64) as i64).min(cur);
            match self.fuel.compare_exchange_weak(cur, cur - take, AcqRel, Relaxed) {
                Ok(_) => return take as u64,
                Err(c) => cur = c,
            }
        }
    }

    /// Fuel left in the budget.
    pub(crate) fn remaining(&self) -> u64 {
        self.fuel.load(Relaxed).max(0) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_never_overdraw() {
        let c = Control::default();
        c.begin(2500);
        assert_eq!(c.draw(1024), 1024);
        assert_eq!(c.draw(1024), 1024);
        assert_eq!(c.draw(1024), 452);
        assert_eq!(c.draw(1024), 0);
        assert_eq!(c.remaining(), 0);
    }

    #[test]
    fn first_failure_wins() {
        let c = Control::default();
        c.begin(10);
        assert!(!c.halted());
        c.fail(Status::Fuel);
        c.interrupt();
        assert!(c.halted());
        assert_eq!(c.status(), Status::Fuel);
        c.clear_interrupt();
        c.begin(10);
        assert!(!c.halted());
        c.interrupt();
        assert!(c.halted());
        assert_eq!(c.status(), Status::Interrupted);
        // a request made before an evaluation begins is kept
        c.begin(10);
        assert!(c.halted());
    }
}
