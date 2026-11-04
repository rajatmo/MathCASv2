//! The generational collector (1b.1 with I1's chunk-order promotion and I3's payloads), run at safe points.
//!
//! **Minor collection.** Young survivors are found from the roots alone: old nodes never refer to young ones, and the
//! old memo never holds a young term. Per-thread id chunks break global creation order, so marking is a descending
//! pass over the young indices with an explicit stack only for out-of-order edges (a child above its parent, I1.2).
//! Survivors are renumbered by rank and appended to the old region, keeping their relative order; their argument
//! slots and arena payload words are copied after the old arrays, boxed payloads change owner without moving. The
//! young memo entries of survivors are written at their new ids, and the side memo (old keys, young values) is
//! swept (A.1). Boxed payloads of dead young nodes are freed from the per-thread box lists (A.3). Then the young
//! table, arena and memo are reset wholesale.
//!
//! **Major collection.** Sliding compaction of the old region once the bytes promoted since the last one exceed the
//! live size: the same marking over old indices, ranks as new ids, and one ascending pass that slides nodes,
//! argument slots, arena payload words and memo entries together, frees dead boxed payloads, and remaps children.
//! The table is rebuilt from its slots alone. When nothing died every rank is the identity and the pass is skipped.
//!
//! Copy phases (minor copy and table insertion, major table rebuild) run on the pool when the safe point does
//! (I1.7); marking and the slide are sequential.
use crate::handle::{is_old, is_young, LOW, YOUNG};
use crate::memo::Item;
use crate::node::{box_bytes, for_each_child, free_box, Kind, Node, Region, NODE};
use crate::old::OldRegion;
use crate::roots::{Raw, Trace};
use crate::Heap;
use rayon::prelude::*;
use std::time::{Duration, Instant};

/// Survivors from which a minor collection copies on the pool.
const PAR_MIN: usize = 4096;

/// Which collection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GcKind {
    /// promotion of the young survivors
    Minor,
    /// compaction of the old region
    Major,
}

/// One collection.
#[derive(Clone, Debug)]
pub struct GcEvent {
    /// which collection
    pub kind: GcKind,
    /// its duration
    pub pause: Duration,
    /// old nodes after the collection
    pub live: usize,
    /// minor: young nodes promoted; major: old nodes freed
    pub moved: usize,
    /// boxed payloads freed
    pub freed_boxes: usize,
    /// bytes of the boxed payloads freed
    pub freed_box_bytes: usize,
    /// marked edges whose child index is above the parent's (the ones that need the explicit stack, I1.2)
    pub out_of_order: usize,
}

/// Totals over a heap's life.
#[derive(Clone, Debug, Default)]
pub struct GcStats {
    /// minor collections
    pub minors: usize,
    /// major collections
    pub majors: usize,
    /// total minor pause
    pub minor_pause: Duration,
    /// total major pause
    pub major_pause: Duration,
    /// longest minor pause
    pub max_minor_pause: Duration,
    /// longest major pause
    pub max_major_pause: Duration,
    /// young nodes promoted
    pub promoted_nodes: usize,
    /// boxed payloads freed
    pub freed_boxes: usize,
    /// out-of-order edges met by marking (see [`GcEvent::out_of_order`])
    pub out_of_order: usize,
    /// the last collection
    pub last: Option<GcEvent>,
}

impl GcStats {
    fn record(&mut self, ev: GcEvent) {
        match ev.kind {
            GcKind::Minor => {
                self.minors += 1;
                self.minor_pause += ev.pause;
                self.max_minor_pause = self.max_minor_pause.max(ev.pause);
                self.promoted_nodes += ev.moved;
            }
            GcKind::Major => {
                self.majors += 1;
                self.major_pause += ev.pause;
                self.max_major_pause = self.max_major_pause.max(ev.pause);
            }
        }
        self.freed_boxes += ev.freed_boxes;
        self.out_of_order += ev.out_of_order;
        self.last = Some(ev);
    }
}

/// The collector's counters.
#[derive(Default)]
pub(crate) struct GcState {
    /// bytes live after the last major collection, and promoted since
    live: usize,
    promoted: usize,
    stats: GcStats,
}

fn bit(bits: &[u64], i: usize) -> bool {
    bits[i >> 6] & (1 << (i & 63)) != 0
}

/// Set bit `i`; true if it was clear.
fn set(bits: &mut [u64], i: usize) -> bool {
    let w = &mut bits[i >> 6];
    let m = 1 << (i & 63);
    let was = *w & m != 0;
    *w |= m;
    !was
}

/// Ranks in a bitmap: the prefix count of each 64-bit word.
fn prefix(bits: &[u64]) -> Vec<u32> {
    let mut p = Vec::with_capacity(bits.len());
    let mut acc = 0u32;
    for w in bits {
        p.push(acc);
        acc += w.count_ones();
    }
    p
}

#[inline]
fn rank(bits: &[u64], pre: &[u32], j: usize) -> u32 {
    pre[j >> 6] + (bits[j >> 6] & ((1u64 << (j & 63)) - 1)).count_ones()
}

/// Mark a region of `n` nodes from the bits already set (the roots): a descending pass, with an explicit stack for
/// out-of-order edges, whose targets the pass has already left behind (I1.2). `kids(i, push)` calls `push` on the
/// region index of every child of node `i` that lies in the region. Returns the number of out-of-order edges.
fn mark(bits: &mut [u64], n: usize, kids: impl Fn(usize, &mut dyn FnMut(usize))) -> usize {
    let mut out_of_order = 0;
    let mut stack: Vec<usize> = vec![];
    for i in (0..n).rev() {
        if !bit(bits, i) {
            continue;
        }
        kids(i, &mut |c| {
            if c < i {
                set(bits, c);
            } else {
                out_of_order += 1;
                if set(bits, c) {
                    stack.push(c);
                }
            }
        });
        // bounded: a node is pushed only when its bit goes from clear to set
        while let Some(x) = stack.pop() {
            kids(x, &mut |y| {
                if set(bits, y) {
                    stack.push(y);
                }
            });
        }
    }
    out_of_order
}

/// A raw pointer shared by the tasks of a parallel copy, each writing slots no other task writes.
#[derive(Clone, Copy)]
struct SyncPtr<X>(*mut X);
// SAFETY: used only for disjoint writes during a collection, joined before the arrays are read.
unsafe impl<X> Sync for SyncPtr<X> {}
unsafe impl<X> Send for SyncPtr<X> {}

impl<X> SyncPtr<X> {
    /// # Safety
    /// `i` is in bounds of the allocation and no other task accesses slot `i` meanwhile.
    #[inline]
    unsafe fn at(self, i: usize) -> *mut X {
        unsafe { self.0.add(i) }
    }
}

fn trace_all(roots: &mut [&mut dyn Trace], f: &mut dyn FnMut(&mut Raw)) {
    for r in roots.iter_mut() {
        r.trace(f);
    }
}

impl Heap {
    /// A safe point: `&mut self`, so no evaluation task is running and no branded handle is alive. Every handle the
    /// caller keeps is in one of `roots`, which the collector may rewrite. Collects when the triggers say so (or
    /// always, in stress mode).
    pub fn safe_point(&mut self, roots: &mut [&mut dyn Trace]) {
        if !self.cfg.collect {
            return;
        }
        let stress = self.cfg.stress;
        let young = self.young.young_bytes() + self.unpacked.bytes();
        let minor = !self.ytable.is_empty() && (stress || young >= self.cfg.nursery);
        if !minor {
            return;
        }
        if stress {
            self.verify(roots);
        }
        self.minor(roots);
        let major = if stress {
            self.gc.stats.minors.is_multiple_of(4)
        } else {
            self.gc.promoted >= self.cfg.min_major.max(self.gc.live)
        };
        if major {
            self.major(roots);
        }
        if stress {
            self.verify(roots);
        }
    }

    /// A minor collection now, if there are young terms (and collection is on).
    pub fn collect_minor(&mut self, roots: &mut [&mut dyn Trace]) {
        if self.cfg.collect && !self.ytable.is_empty() {
            self.minor(roots);
        }
    }

    /// A minor and a major collection now: afterwards the heap holds exactly the terms reachable from `roots`.
    pub fn collect_full(&mut self, roots: &mut [&mut dyn Trace]) {
        if self.cfg.collect {
            self.collect_minor(roots);
            self.major(roots);
        }
    }

    /// Totals of the collections so far.
    pub fn gc_stats(&self) -> &GcStats {
        &self.gc.stats
    }

    /// Run the copy phases on the pool? Only on a pool thread (outside one, rayon would start its global pool), and
    /// only for enough work, except in stress mode, which exercises the parallel path at every collection.
    fn par(&self, work: usize) -> bool {
        self.cfg.par_gc && (work >= PAR_MIN || self.cfg.stress) && rayon::current_thread_index().is_some()
    }

    fn minor(&mut self, roots: &mut [&mut dyn Trace]) {
        let t0 = Instant::now();
        // unpacked arguments hold young handles: the cache goes with the cycle (N4a.4)
        self.unpacked.clear();
        let (lo, hi) = self.young.range();
        let n = (hi - lo) as usize;
        let old0 = self.old.nodes.len();
        let local = move |c: u32| is_young(c).then(|| ((c & LOW) - lo) as usize);

        // marking
        let mut bits = vec![0u64; n.div_ceil(64)];
        trace_all(roots, &mut |x: &mut Raw| {
            if let Some(j) = local(x.0) {
                set(&mut bits, j);
            }
        });
        let young = &self.young;
        let out_of_order = mark(&mut bits, n, |i, push| {
            for_each_child(young, young.node(lo + i as u32), |c| {
                if let Some(j) = local(c) {
                    push(j)
                }
            })
        });
        let pre = prefix(&bits);
        let alive = |c: u32| local(c).is_none_or(|j| bit(&bits, j));
        let remap = |c: u32| local(c).map_or(c, |j| old0 as u32 + rank(&bits, &pre, j));

        // destinations: new ids are old0 + rank; argument and limb offsets are prefix sums in survivor order
        let order: Vec<u32> = (0..n as u32).filter(|&j| bit(&bits, j as usize)).collect();
        let k = order.len();
        assert!(old0 + k < YOUNG as usize, "old region full (2^30 terms)");
        let (a0, l0) = (self.old.args.len(), self.old.limbs.len());
        let (mut acc_a, mut acc_l) = (a0, l0);
        let offs: Vec<usize> = order
            .iter()
            .map(|&j| {
                let node = young.node(lo + j);
                let acc = match node.kind() {
                    Kind::App => &mut acc_a,
                    _ if node.in_arena() => &mut acc_l,
                    _ => return 0,
                };
                *acc += node.len();
                *acc - node.len()
            })
            .collect();
        let empty = Node::new(Kind::Real, 0, 0, 0, 0, 0);
        self.old.nodes.resize(old0 + k, empty);
        self.old.args.resize(acc_a, 0);
        self.old.limbs.resize(acc_l, 0);
        self.omemo.0.resize_with(old0 + k, Default::default);

        // copy: each survivor writes its own node, argument and limb slots, and memo entry
        let (np, ap, lp) =
            (SyncPtr(self.old.nodes.as_mut_ptr()), SyncPtr(self.old.args.as_mut_ptr()), SyncPtr(self.old.limbs.as_mut_ptr()));
        let (ymemo, omemo) = (&self.ymemo, &self.omemo.0);
        let copy = |r: usize| {
            let id = lo + order[r];
            let node = *young.node(id);
            let mut m = node;
            match node.kind() {
                Kind::App => {
                    for (x, &c) in young.args(&node).iter().enumerate() {
                        // SAFETY: slot offs[r] + x lies in the resized argument array and belongs to this survivor.
                        unsafe { *ap.at(offs[r] + x) = remap(c) };
                    }
                    m.head = remap(node.head);
                    m.data = offs[r] as u64;
                }
                Kind::Rat => {
                    m.head = remap(node.head);
                    m.data = remap(node.data as u32) as u64;
                }
                _ if node.in_arena() => {
                    let w = young.words(&node);
                    // SAFETY: as above, limb slots offs[r]..offs[r] + len.
                    unsafe { std::ptr::copy_nonoverlapping(w.as_ptr(), lp.at(offs[r]), w.len()) };
                    m.data = offs[r] as u64;
                }
                // words, reals, and boxed payloads (the box changes owner, it does not move)
                _ => {}
            }
            // SAFETY: as above, node slot old0 + r.
            unsafe { *np.at(old0 + r) = m };
            let it = ymemo.entry(id).and_then(|e| e.get(YOUNG | id)).filter(|it| alive(it.val));
            let key = (old0 + r) as u32;
            omemo[old0 + r].store(it.map(|it| Item { key, val: remap(it.val), ..it }));
        };
        let par = self.par(k);
        if par {
            (0..k).into_par_iter().with_min_len(1024).for_each(copy);
        } else {
            (0..k).for_each(copy);
        }
        self.old.table.insert_range(old0..old0 + k, &self.old.nodes, par);

        // the side memo: entries whose young value survives move to the old memo (unless it holds a newer one)
        {
            let omemo = &self.omemo.0;
            self.side.drain(|it| {
                if alive(it.val) {
                    let e = &omemo[it.key as usize];
                    if e.load().is_none_or(|cur| cur.epoch <= it.epoch) {
                        e.store(Some(Item { val: remap(it.val), ..it }));
                    }
                }
            });
        }
        trace_all(roots, &mut |x: &mut Raw| x.0 = remap(x.0));

        // boxed payloads of this cycle: free the dead ones, hand the survivors to the old region (A.3)
        let (mut freed, mut freed_bytes, mut kept_bytes) = (0, 0, 0);
        for id in self.young.boxed_ids() {
            let node = self.young.node(id);
            if bit(&bits, (id - lo) as usize) {
                kept_bytes += box_bytes(node.len());
            } else {
                // SAFETY: a dead young boxed node owns its box; nothing refers to the node any more.
                unsafe { free_box(node) };
                freed += 1;
                freed_bytes += box_bytes(node.len());
            }
        }
        self.old.boxed += kept_bytes;

        self.ytable.clear();
        self.young.reset();
        self.ymemo.clear();
        self.gc.promoted += k * NODE + (acc_a - a0) * 4 + (acc_l - l0) * 8 + kept_bytes;
        let live = self.old.nodes.len();
        self.gc.stats.record(GcEvent {
            kind: GcKind::Minor,
            pause: t0.elapsed(),
            live,
            moved: k,
            freed_boxes: freed,
            freed_box_bytes: freed_bytes,
            out_of_order,
        });
    }

    fn major(&mut self, roots: &mut [&mut dyn Trace]) {
        let t0 = Instant::now();
        self.unpacked.clear();
        let n = self.old.nodes.len();
        let mut bits = vec![0u64; n.div_ceil(64)];
        trace_all(roots, &mut |x: &mut Raw| {
            assert!(!is_young(x.0), "a young root at a major collection");
            if is_old(x.0) {
                set(&mut bits, x.0 as usize);
            }
        });
        let old = &self.old;
        let out_of_order = mark(&mut bits, n, |i, push| {
            for_each_child(old, &old.nodes[i], |c| {
                if is_old(c) {
                    push(c as usize)
                }
            })
        });
        let pre = prefix(&bits);
        let live = pre.last().map_or(0, |&p| p as usize + bits[bits.len() - 1].count_ones() as usize);
        let alive = |c: u32| !is_old(c) || bit(&bits, c as usize);
        let remap = |c: u32| if is_old(c) { rank(&bits, &pre, c as usize) } else { c };
        let (mut freed, mut freed_bytes) = (0, 0);
        if live < n {
            // slide nodes, argument slots, limbs and memo entries down in one ascending pass (each array was filled
            // in node order, so every destination is at or below its source)
            let OldRegion { nodes, args, limbs, boxed, .. } = &mut self.old;
            let memo = &mut self.omemo.0;
            let (mut dst, mut adst, mut ldst) = (0usize, 0usize, 0usize);
            for i in 0..n {
                let mut node = nodes[i];
                if !bit(&bits, i) {
                    if node.is_boxed() {
                        // SAFETY: a dead old boxed node owns its box, and nothing refers to the node any more.
                        unsafe { free_box(&node) };
                        freed += 1;
                        freed_bytes += box_bytes(node.len());
                    }
                    continue;
                }
                match node.kind() {
                    Kind::App => {
                        let (s, l) = (node.data as usize, node.len());
                        for x in 0..l {
                            args[adst + x] = remap(args[s + x]);
                        }
                        node.head = remap(node.head);
                        node.data = adst as u64;
                        adst += l;
                    }
                    Kind::Rat => {
                        node.head = remap(node.head);
                        node.data = remap(node.data as u32) as u64;
                    }
                    _ if node.in_arena() => {
                        let (s, l) = (node.data as usize, node.len());
                        if s != ldst {
                            limbs.copy_within(s..s + l, ldst);
                        }
                        node.data = ldst as u64;
                        ldst += l;
                    }
                    _ => {}
                }
                nodes[dst] = node;
                let it = memo[i].load().filter(|it| alive(it.val));
                memo[dst].store(it.map(|it| Item { key: dst as u32, val: remap(it.val), ..it }));
                dst += 1;
            }
            debug_assert_eq!(dst, live);
            nodes.truncate(dst);
            args.truncate(adst);
            limbs.truncate(ldst);
            memo.truncate(dst);
            *boxed -= freed_bytes;
            let cap = 2 * dst + self.cfg.min_major / NODE;
            let cap = cap.max(self.old.table.s.len() / 4);
            let par = self.par(n);
            self.old.table.rebuild(cap, |id| alive(id).then(|| remap(id)), par);
            trace_all(roots, &mut |x: &mut Raw| x.0 = remap(x.0));
        }
        self.gc.live = live * NODE + self.old.args.len() * 4 + self.old.limbs.len() * 8 + self.old.boxed;
        self.gc.promoted = 0;
        self.shrink();
        self.gc.stats.record(GcEvent {
            kind: GcKind::Major,
            pause: t0.elapsed(),
            live,
            moved: n - live,
            freed_boxes: freed,
            freed_box_bytes: freed_bytes,
            out_of_order,
        });
    }

    /// Keep about twice the live size reserved, plus the trigger slack (1b.6).
    fn shrink(&mut self) {
        fn fit<X>(v: &mut Vec<X>, slack: usize) {
            let n = v.len();
            if v.capacity() > 4 * n + slack {
                v.shrink_to(2 * n + slack);
            }
        }
        let k = self.cfg.min_major;
        fit(&mut self.old.nodes, k / NODE);
        fit(&mut self.old.args, k / 4);
        fit(&mut self.old.limbs, k / 8);
        fit(&mut self.omemo.0, k / NODE);
    }
}
