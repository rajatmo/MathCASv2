//! The work budget of one method: the evaluator's fuel (and its interrupt), `mc-arith`'s stop flag, and an
//! allowance of fuel of the method's own (the driver gives every method of the portfolio its share of work).
use mc_eval::{Abort, Allowance, NativeCx};
use mc_poly::work::{Halt, Work, WorkResult};

/// Fuel units per unit of `mc-poly` work (one unit is about 16k elementary steps).
const FUEL_PER_UNIT: u64 = 16;

/// Why a method stopped early.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// the evaluator aborted (interrupt, fuel): must be passed on
    Abort(Abort),
    /// the method's own allowance ran out, or it gave up
    GiveUp,
}

/// A budget over the evaluator's fuel with an allowance of the method's own.
pub struct Budget<'c, 'a, 'k, 'h> {
    cx: &'c mut NativeCx<'a, 'k, 'h>,
    allowance: Allowance,
    /// the abort seen, to be passed on
    pub abort: Option<Abort>,
    /// did the allowance run out?
    pub timed_out: bool,
}

impl<'c, 'a, 'k, 'h> Budget<'c, 'a, 'k, 'h> {
    /// A budget of `allowance` (made on the task of `cx`).
    pub fn new(cx: &'c mut NativeCx<'a, 'k, 'h>, allowance: Allowance) -> Self {
        Budget { cx, allowance, abort: None, timed_out: false }
    }
    /// The stop behind a halted kernel.
    pub fn stop(&mut self) -> Stop {
        match self.abort.take() {
            Some(a) => Stop::Abort(a),
            None => Stop::GiveUp,
        }
    }
    /// Has the allowance run out?
    pub fn expired(&self) -> bool {
        self.allowance.expired(self.cx)
    }
}

impl Work for Budget<'_, '_, '_, '_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        if let Err(a) = self.cx.tick(units.saturating_mul(FUEL_PER_UNIT)) {
            self.abort = Some(a);
            return Err(Halt::Stopped);
        }
        if mc_arith::num::stopped() {
            return Err(Halt::Stopped);
        }
        if self.allowance.expired(self.cx) {
            self.timed_out = true;
            return Err(Halt::Stopped);
        }
        Ok(())
    }
}
