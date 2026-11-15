//! Work done below the evaluator, counted so that it can be charged as fuel.
//!
//! The evaluator's fuel counts its own steps; the kernels it calls (big-number products and divisions, term
//! construction, the series of the special functions) do their work without a handle on the evaluation. They report
//! it here instead, in *operations* of roughly one nanosecond of work each (one limb product, a few floating-point
//! operations), into a counter of the running thread; the evaluator takes the counter after every native call and
//! whenever a native charges or reads its fuel, and charges one unit of fuel per [`OPS_PER_TICK`] operations to the
//! task that ran the kernel (the part of a unit left over stays with the task).
//!
//! The count depends only on the operations performed, never on the time they took, so a budget measured in fuel
//! gives the same answer on every machine (semantics 18.7). A kernel that hands work to other threads charges the
//! work on the thread that called it ([`isolated`] keeps a worker's own count apart).
use std::cell::Cell;

/// Operations per unit of fuel: one unit is about one step of the evaluator.
pub const OPS_PER_TICK: u64 = 256;

thread_local! {
    /// Operations reported on this thread and not yet taken.
    static PENDING: Cell<u64> = const { Cell::new(0) };
}

/// Report `ops` operations of work on this thread.
#[inline]
pub fn charge(ops: u64) {
    PENDING.with(|p| p.set(p.get().saturating_add(ops)));
}

/// The operations reported on this thread since the last take; the counter is left at zero. The evaluator takes it
/// at the end of every piece of work a task does on the thread (a native call, an element of a parallel group) and
/// keeps the part of a unit of fuel left over with the task, so that no part of one task's count stays on the thread
/// for the next task that runs there.
#[inline]
pub fn take_ops() -> u64 {
    PENDING.with(|p| p.replace(0))
}

/// The operations reported on this thread and not yet taken (a read that takes nothing).
#[inline]
pub fn pending_ops() -> u64 {
    PENDING.with(|p| p.get())
}

/// Run `f` with a counter of its own: the operations `f` reported, and its result. The thread's count from before
/// is restored afterwards, so work done on a worker thread can be handed to the caller's thread explicitly.
pub fn isolated<R>(f: impl FnOnce() -> R) -> (R, u64) {
    let saved = PENDING.with(|p| p.replace(0));
    let r = f();
    let ops = PENDING.with(|p| p.replace(saved));
    (r, ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_are_taken_whole() {
        PENDING.with(|p| p.set(0));
        charge(3 * OPS_PER_TICK + 5);
        assert_eq!(pending_ops(), 3 * OPS_PER_TICK + 5);
        assert_eq!(take_ops(), 3 * OPS_PER_TICK + 5);
        assert_eq!(take_ops(), 0);
    }

    #[test]
    fn an_isolated_count_is_kept_apart() {
        PENDING.with(|p| p.set(0));
        charge(7);
        let ((), ops) = isolated(|| charge(1000));
        assert_eq!(ops, 1000);
        assert_eq!(PENDING.with(|p| p.get()), 7);
    }
}
