//! Compiled specialisations (the hook of the numeric compiler, item 1b): a head may carry an object that decides,
//! from a call's evaluated arguments, whether compiled code applies, and computes the value if it does.
//!
//! The evaluator consults a head's specialisation after the arguments are evaluated (by the head's argument modes,
//! failures returned, AC heads canonicalised) and after the head's native, if any, declined, and before any rule
//! (argument-head rules included). A head without one costs one atomic load per call. The consultation records the
//! head in the read set, as a rule lookup does.
//!
//! **Contract.** [`Compiled::call`] returns `Some(value)` only when the value is the normal form the rules would
//! give (so registering or removing a specialisation is not a definition, and memoised results stay valid), and
//! `None` to decline: argument types it was not compiled for, an integer overflow, or a stop or fuel exhaustion it
//! noticed in [`crate::Control`] (the interpreter then runs the call and reports the stop at its next poll). It may
//! evaluate, charge fuel and read through the [`NativeCx`] like a native; it keeps no terms across calls (a safe
//! point may move them; symbols are fine).
use crate::conc::AppendVec;
use crate::defs::Defs;
use crate::native::NativeCx;
use mc_core::{Sym, T};
use std::sync::Arc;

/// A compiled body for calls of one head (see the module documentation).
pub trait Compiled: Send + Sync {
    /// The value of the call with these evaluated arguments, or `None` to let the rules evaluate it.
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Option<T<'h>>;
}

/// Install (or with `None` remove) the specialisation of `head`. Entries of the table are never removed (a reader
/// may hold one), only unlinked from their head.
pub(crate) fn set_compiled(defs: &Defs, table: &AppendVec<Arc<dyn Compiled>>, head: Sym, c: Option<Arc<dyn Compiled>>) {
    let slot = c.map_or(0, |c| table.push(c) as u32 + 1);
    defs.set_compiled(head, slot);
}

/// The specialisation of `head`.
#[inline]
pub(crate) fn compiled_of<'a>(defs: &Defs, table: &'a AppendVec<Arc<dyn Compiled>>, head: Sym) -> Option<&'a Arc<dyn Compiled>> {
    let slot = defs.get(head)?.compiled.load(std::sync::atomic::Ordering::Acquire);
    if slot == 0 {
        return None;
    }
    table.get(slot as usize - 1)
}
