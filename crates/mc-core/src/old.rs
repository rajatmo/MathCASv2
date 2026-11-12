//! The old region (I1.1): dense arrays of nodes, argument slots and arena payload words, plus an open-addressing
//! interning table. It changes only at safe points; between them every thread reads it with plain loads.
use crate::node::{free_box, Node, Region};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// The interning table: open addressing, `(hash >> 32) << 32 | (id + 1)` per slot, at most half full.
pub(crate) struct OldTable {
    pub(crate) s: Vec<u64>,
    pub(crate) count: usize,
}

const MIN_TABLE: usize = 1 << 12;

impl OldTable {
    fn new() -> OldTable {
        OldTable { s: vec![0; MIN_TABLE], count: 0 }
    }

    #[inline]
    pub(crate) fn find(&self, hash: u64, eq: impl Fn(u32) -> bool) -> Option<u32> {
        let mask = self.s.len() - 1;
        let tag = hash >> 32;
        let mut i = tag as usize & mask;
        loop {
            let e = self.s[i];
            if e == 0 {
                return None;
            }
            if e >> 32 == tag && eq((e as u32) - 1) {
                return Some((e as u32) - 1);
            }
            i = (i + 1) & mask;
        }
    }

    fn place(s: &mut [u64], e: u64) {
        let mask = s.len() - 1;
        let mut i = (e >> 32) as usize & mask;
        while s[i] != 0 {
            i = (i + 1) & mask;
        }
        s[i] = e;
    }

    /// The slots viewed as atomics, for parallel placement (`&mut`: nothing else reads them meanwhile).
    fn atomic(s: &mut [u64]) -> &[AtomicU64] {
        // SAFETY: `AtomicU64` has the size and alignment of `u64`, and the exclusive borrow is held throughout.
        unsafe { std::slice::from_raw_parts(s.as_mut_ptr() as *const AtomicU64, s.len()) }
    }

    fn place_atomic(s: &[AtomicU64], e: u64) {
        let mask = s.len() - 1;
        let mut i = (e >> 32) as usize & mask;
        while s[i].compare_exchange(0, e, Relaxed, Relaxed).is_err() {
            i = (i + 1) & mask;
        }
    }

    /// Insert the nodes `ids` (just promoted), on the pool when `par`.
    pub(crate) fn insert_range(&mut self, ids: std::ops::Range<usize>, nodes: &[Node], par: bool) {
        self.reserve(ids.len());
        self.count += ids.len();
        if par {
            let s = Self::atomic(&mut self.s);
            ids.into_par_iter().with_min_len(4096).for_each(|i| Self::place_atomic(s, entry(nodes[i].hash, i as u32)));
        } else {
            for i in ids {
                Self::place(&mut self.s, entry(nodes[i].hash, i as u32));
            }
        }
    }

    /// Make room for `extra` more entries at once (one re-placement instead of several doublings).
    fn reserve(&mut self, extra: usize) {
        if (self.count + extra) * 2 > self.s.len() {
            self.rebuild(2 * (self.count + extra), Some, false);
        }
    }

    /// Re-place the surviving entries (renumbered by `remap`) into an array of about `cap` slots, from the slots
    /// alone (no node reads), on the pool when `par`.
    pub(crate) fn rebuild(&mut self, cap: usize, remap: impl Fn(u32) -> Option<u32> + Sync, par: bool) {
        let cap = cap.next_power_of_two().max(MIN_TABLE);
        let mut fresh = vec![0u64; cap];
        let moved = |e: u64| (e != 0).then(|| remap((e as u32) - 1)).flatten().map(|id| (e >> 32) << 32 | (id as u64 + 1));
        if par {
            let f = Self::atomic(&mut fresh);
            self.count = self
                .s
                .par_iter()
                .with_min_len(4096)
                .filter_map(|&e| moved(e))
                .map(|e| Self::place_atomic(f, e))
                .count();
        } else {
            self.count = 0;
            for &e in &self.s {
                if let Some(e) = moved(e) {
                    Self::place(&mut fresh, e);
                    self.count += 1;
                }
            }
        }
        self.s = fresh;
    }
}

#[inline]
fn entry(hash: u64, id: u32) -> u64 {
    (hash >> 32) << 32 | (id as u64 + 1)
}

pub(crate) struct OldRegion {
    pub(crate) nodes: Vec<Node>,
    pub(crate) args: Vec<u32>,
    pub(crate) limbs: Vec<u64>,
    pub(crate) table: OldTable,
    /// bytes of boxed payloads owned by old nodes
    pub(crate) boxed: usize,
}

impl OldRegion {
    pub(crate) fn new() -> OldRegion {
        OldRegion { nodes: vec![], args: vec![], limbs: vec![], table: OldTable::new(), boxed: 0 }
    }

    /// The node at an old index. Panics beyond the region (a handle that was not rooted across a major collection).
    #[inline]
    pub(crate) fn node(&self, id: u32) -> &Node {
        &self.nodes[id as usize]
    }

    /// Bytes reserved by the arrays and the table, plus boxed payloads.
    pub(crate) fn reserved_bytes(&self) -> usize {
        self.nodes.capacity() * std::mem::size_of::<Node>()
            + self.args.capacity() * 4
            + self.limbs.capacity() * 8
            + self.table.s.len() * 8
            + self.boxed
    }
}

impl Region for OldRegion {
    #[inline]
    fn args<'a>(&'a self, n: &Node) -> &'a [u32] {
        &self.args[n.data as usize..n.data as usize + n.len()]
    }
    #[inline]
    fn words<'a>(&'a self, n: &Node) -> &'a [u64] {
        if n.is_boxed() {
            // SAFETY: an old boxed node owns its box until the major collection frees the node (`&mut`).
            unsafe { n.words_by_ptr() }
        } else {
            &self.limbs[n.data as usize..n.data as usize + n.len()]
        }
    }
}

impl Drop for OldRegion {
    fn drop(&mut self) {
        for n in self.nodes.iter().filter(|n| n.is_boxed()) {
            // SAFETY: every old boxed node owns its box, freed once here.
            unsafe { free_box(n) };
        }
    }
}
