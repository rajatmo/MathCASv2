//! MathCAS v2 group theory and combinatorics (roadmap section 3.4 track G).
//!
//! Permutations are terms `cycles([[1, 2, 3], [4, 5]])` (disjoint cycles, 1-based points, canonical form: each
//! cycle starts at its least point, cycles ordered by that point, no fixed points); products apply the left factor
//! first. Permutation groups are terms `permutation_group([g1, g2, ...])` of such generators; every algorithm runs
//! on a base and strong generating set computed by the deterministic Schreier-Sims algorithm ([`chain`]) and kept
//! in a small cache. Finitely presented groups are `finitely_presented_group([a, b, ...], [relators])` with words
//! as lists of letters (`a`, `a^-1`, `a^3`). Methods, literature and decisions: `NOTES.md` (decisions g31.N).
// matrix and table code reads best with index loops
#![allow(clippy::needless_range_loop)]
mod api;
pub mod backtrack;
pub mod chain;
pub mod classes;
pub mod comb;
pub mod fp;
pub mod groups;
pub mod named;
pub mod perm;

use mc_eval::{Abort, Kernel, Res};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Interrupt checks for the long-running algorithms: they poll the session's stop flag.
pub struct Ctl {
    stop: Option<Arc<AtomicBool>>,
}

impl Ctl {
    /// A control that never stops (for tests of the algorithms).
    pub fn none() -> Ctl {
        Ctl { stop: None }
    }

    pub(crate) fn of(stop: &Arc<AtomicBool>) -> Ctl {
        Ctl { stop: Some(stop.clone()) }
    }

    /// Abort when the session asked to stop.
    pub fn step(&self, _work: u64) -> Res<()> {
        match &self.stop {
            Some(s) if s.load(Ordering::Relaxed) => Err(Abort::Limit),
            _ => Ok(()),
        }
    }
}

/// The public functions of this crate with a one-line description each.
pub const FUNCTIONS: &[(&str, &str)] = api::FUNCTIONS;

/// Register the natives with `k`.
pub fn install(k: &mut Kernel, h: &mc_core::Heap) {
    api::install(k, h);
}
