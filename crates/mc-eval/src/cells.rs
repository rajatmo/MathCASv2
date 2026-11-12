//! Mutable lexical locals (semantics 6.7 amendment): the cells behind `var`.
//!
//! A `var` in a block allocates a cell and stands for the term `local_cell(id)` in the rest of the block (and in
//! closures created there); evaluating that term reads the cell, `assign(local_cell(id), v)` writes it. Cells are
//! never symbol definitions: they carry no version and do not enter normal-form stamps, but an evaluation that reads
//! or allocates a cell is never memoised (it is *volatile*).
//!
//! **Two id spaces** keep results deterministic under speculation (semantics 9, 18):
//!
//! - *Frontier cells* (positive ids) are allocated by the task on the sequential frontier, always the smallest free
//!   id. A block frees its cells when it ends unless they escape (the value or an effect may hold them). Blocks that
//!   do not let cells escape are balanced, and every block whose cells do escape runs on the frontier, in order, so
//!   the ids a result can show do not depend on how much work speculative tasks did.
//! - *Speculative cells* (negative ids) are allocated by speculative tasks and owned by the allocating task. They
//!   never leave it: a block on a speculative task whose value holds one leaves a hole (the frontier redoes it with
//!   frontier cells). They are forgotten when the evaluation ends.
//!
//! **Writes** are allowed to the owner only: the frontier writes frontier cells, a speculative task its own cells;
//! anything else leaves a hole. A frontier write to a cell that a speculative task may be reading (one allocated
//! before the frontier's latest split) is recorded as a definition of the read-set bit of `local_cell`, so
//! speculative work that read it is redone (the validation of decision F.16). Inside a condition only cells allocated
//! in that condition may be written (A.6); the others give `failure(effect_in_condition)`.
use crate::conc::SymTable;
use mc_core::{Heap, Raw, HOLE, T};
use parking_lot::Mutex;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::*};

/// One cell.
pub(crate) struct Slot {
    /// the value's handle bits, or [`HOLE`] when the cell is free
    value: AtomicU32,
    /// allocation number (see [`Cells::born`])
    pub(crate) born: AtomicU64,
    /// the allocating task (speculative cells)
    pub(crate) owner: AtomicU64,
}

impl Default for Slot {
    fn default() -> Slot {
        Slot { value: AtomicU32::new(HOLE), born: AtomicU64::new(0), owner: AtomicU64::new(0) }
    }
}

impl Slot {
    #[inline]
    pub(crate) fn get<'h>(&self, h: &'h Heap) -> Option<T<'h>> {
        let bits = self.value.load(Acquire);
        (bits != HOLE).then(|| Raw::from_bits(bits).get(h))
    }
    #[inline]
    pub(crate) fn set(&self, v: T<'_>) {
        self.value.store(Raw::new(v).to_bits(), Release);
    }
}

/// Free frontier ids.
#[derive(Default)]
struct FrontIds {
    /// one more than the largest id in use
    next: u32,
    /// free ids below `next`
    free: BTreeSet<u32>,
}

/// Every cell of a session.
#[derive(Default)]
pub(crate) struct Cells {
    front: SymTable<Slot>,
    ids: Mutex<FrontIds>,
    spec: SymTable<Slot>,
    spec_next: AtomicU32,
    born: AtomicU64,
    tasks: AtomicU64,
}

impl Cells {
    /// Start a top-level evaluation: speculative cells of earlier evaluations are gone.
    pub(crate) fn begin(&mut self) {
        *self.spec_next.get_mut() = 0;
    }

    /// The allocation counter: a cell allocated after this reading has a larger `born`.
    #[inline]
    pub(crate) fn born(&self) -> u64 {
        self.born.load(Acquire)
    }

    /// A new speculative task id (never 0, the frontier's).
    pub(crate) fn task(&self) -> u64 {
        self.tasks.fetch_add(1, Relaxed) + 1
    }

    /// A new cell holding `v`: a frontier cell, or a speculative cell owned by `task`. Returns its id.
    pub(crate) fn alloc(&self, front: bool, task: u64, v: T<'_>) -> i64 {
        let born = self.born.fetch_add(1, AcqRel) + 1;
        let (slot, id) = if front {
            let mut ids = self.ids.lock();
            let id = match ids.free.pop_first() {
                Some(id) => id,
                None => {
                    ids.next = ids.next.max(1) + 1;
                    ids.next - 1
                }
            };
            (self.front.get_or_init(id), id as i64)
        } else {
            let k = self.spec_next.fetch_add(1, Relaxed) + 1;
            (self.spec.get_or_init(k), -(k as i64))
        };
        slot.born.store(born, Relaxed);
        slot.owner.store(task, Relaxed);
        slot.set(v);
        id
    }

    /// The cell `id`, if it is allocated.
    #[inline]
    pub(crate) fn slot(&self, id: i64) -> Option<&Slot> {
        let s = if id > 0 {
            self.front.get(u32::try_from(id).ok()?)?
        } else {
            self.spec.get(u32::try_from(id.checked_neg()?).ok()?)?
        };
        (s.value.load(Acquire) != HOLE).then_some(s)
    }

    /// Free frontier cells (their block ended and they did not escape).
    pub(crate) fn free(&self, ids: &[i64]) {
        let mut guard = self.ids.lock();
        let fr = &mut *guard;
        for &id in ids {
            debug_assert!(id > 0, "only frontier cells are freed one by one");
            if let Some(s) = self.front.get(id as u32) {
                s.value.store(HOLE, Release);
            }
            fr.free.insert(id as u32);
        }
        while fr.next > 1 && fr.free.remove(&(fr.next - 1)) {
            fr.next -= 1;
        }
    }

    /// Visit the values of the live frontier cells (a safe point). Speculative cells are dead between evaluations.
    pub(crate) fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.front.for_each_mut(|s| {
            let bits = s.value.get_mut();
            if *bits != HOLE {
                let mut r = Raw::from_bits(*bits);
                visit(&mut r);
                *bits = r.to_bits();
            }
        });
    }
}
