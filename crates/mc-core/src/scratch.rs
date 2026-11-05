//! Scratch scopes: the garbage of one evaluation inside a long-running native, dropped when the evaluation ends
//! (package G, decisions G.1 to G.6).
//!
//! A native that calls the evaluator many times inside one statement (an interpreted integrand, an ODE right-hand
//! side) makes terms at every call, and the collector runs only at safe points between statements (1b.7): a
//! statement that integrates ten thousand times keeps every intermediate term until it ends. A safe point inside
//! the native would need every live handle of every task as a root, which the borrow brand cannot name (handles of
//! the frames above the native and of other threads' tasks are plain values on Rust stacks). A **scratch scope**
//! instead keeps the evaluation's new terms apart from the shared heap and drops them when the native has read what
//! it needs (machine numbers) out of the result.
//!
//! # How it works
//!
//! [`Heap::scratch`] opens a scope on the calling thread: it takes one of [`SLOTS`] slots of the heap. While it is
//! open, a term this thread builds is looked up in the slot's private table, then in the shared old and young tables
//! (look-up only), and if it is found nowhere it is stored in the slot, not in the shared young region. A term with
//! a scratch part is always a scratch term. Scratch handles have bits 30 and 29 set (`YOUNG | SCR`), then the slot
//! (4 bits), a page (4 bits) and an index within the page (21 bits); pages advance from scope to scope, so a stale
//! handle from a recent scope is detected. Dropping the [`Scratch`] guard ends the scope: the slot's nodes, argument
//! slabs, payloads, private table, private memo and private unpacked arguments are released for the next scope.
//!
//! # Why it is safe (decision G.2)
//!
//! No scratch handle may be used once its scope has ended. The rules that ensure it:
//!
//! 1. **Only the owning thread sees its scope.** The scope is a thread-local pointer; a scratch handle is resolved
//!    only on the thread whose scope made it, and only while the scope is open (anything else panics with a clear
//!    message instead of reading reused memory: `node` checks slot, page and index). The evaluator does not split
//!    work into parallel tasks inside a scope, and every parallel job of this crate and of the evaluator runs
//!    [`detached`] (without the scope of whichever thread executes it), so a job stolen by a thread that is waiting
//!    inside a scope never builds terms into that scope.
//! 2. **No shared structure stores a scratch handle.** The shared tables never hold one (a term with a scratch part
//!    is stored in the slot); memo entries with a scratch key or value go to the slot's private memo; the unpacked
//!    arguments of packed arrays viewed in a scope are made privately; the old region does not change during an
//!    evaluation. In the evaluator, effects (definitions, output, compiled specialisations) are refused inside a
//!    scope (the native retries the evaluation without one), cells written in a scope are re-pointed at shared
//!    copies ([`Scratch::export`]) when it ends, and native caches keyed by handles skip scratch terms
//!    ([`Heap::is_scratch`]).
//! 3. **Results leave as plain data.** The closure a native runs in a scope returns values without handles
//!    (machine numbers, `Num`); a term it must keep is copied out with [`Scratch::export`].
//! 4. **Canonical handles stay canonical** (equal terms, equal handles, which `===`, rule indexes and the AC
//!    canonicaliser rely on). A term made in a scope was absent from the shared tables when it was made, and the
//!    private table is probed first, so this thread keeps getting its scratch handle for it. A shared term made by
//!    another thread *after* that could still reach the scope; the channels are the memo, unpacked arguments,
//!    definitions and cells. Memo hits whose value is young are accepted only when no part of it has a structural
//!    hash present in the private table (a bounded check; otherwise a miss); unpacked arguments are made privately;
//!    definitions and cells change concurrently only through the frontier, whose writes invalidate the speculative
//!    work that read them (semantics 18.3).
//!
//! The brand `'h` cannot enforce rule 1 statically (scratch handles share the heap's brand), so the checks are
//! dynamic; the stress tests run every interpreted solver with scopes and the collector's stress mode.
use crate::handle::{is_imm, is_old, IMM, YOUNG};
use crate::heap::{same, Heap};
use crate::memo::Item;
use crate::node::{for_each_child, Kind, Node, Region};
use crate::young::Body;
use mc_api::T;
use rustc_hash::FxHashMap;
use std::cell::{Cell, RefCell, UnsafeCell};
use std::marker::PhantomData;
use std::ptr::null;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::*};

/// Bit 29 of a young handle: a scratch term.
pub(crate) const SCR: u32 = 1 << 29;
/// Scratch slots per heap: the most scopes open at once (on different threads); more wait for none, they run
/// without a scope.
pub const SLOTS: usize = 16;
const SLOT_SHIFT: u32 = 25;
const PAGE_SHIFT: u32 = 21;
/// Pages a scope may use, cyclically per slot.
const PAGES: u32 = 16;
/// Indices of a page handed out; the two highest of the 21-bit field are never used (they include [`crate::HOLE`]).
const PAGE_LEN: usize = (1 << PAGE_SHIFT) - 2;
/// Nodes per storage segment.
const SEG_BITS: u32 = 12;
const SEG: usize = 1 << SEG_BITS;
/// Words per argument slab; larger requests get their own allocation.
const SLAB: usize = 1 << 12;
/// Storage a slot keeps between scopes (more is freed at the next safe point).
const KEEP_SEGS: usize = 16;
const KEEP_SLABS: usize = 16;
/// Initial capacity of a private table; a larger one shrinks back when its scope ends.
const TABLE_MIN: usize = 1 << 8;
const TABLE_KEEP: usize = 1 << 16;
/// Nodes a memo hit's value is checked through for parts made in the scope (decision G.4).
const TWIN_CHECK: usize = 64;

/// Is `b` a scratch handle?
#[inline]
pub(crate) fn is_scratch(b: u32) -> bool {
    b & (IMM | YOUNG | SCR) == YOUNG | SCR
}

/// Is `t` a scratch term ([`Heap::is_scratch`] without the heap)? Caches keyed by handles that outlive one
/// evaluation must not keep scratch terms: later scopes reuse their handles.
#[inline]
pub fn is_scratch_term(t: T<'_>) -> bool {
    is_scratch(t.raw())
}

thread_local! {
    /// The scope open on this thread (null: none).
    static ACTIVE: Cell<*const Slot> = const { Cell::new(null()) };
}

/// The slot of the scope open on this thread (null when none).
#[inline]
pub(crate) fn active_slot() -> *const Slot {
    ACTIVE.with(|a| a.get())
}

/// Whether a scratch scope (of any heap) is open on this thread. Parallel code checks it and stays sequential in a
/// scope (decision G.2, rule 1).
#[inline]
pub fn scratch_active() -> bool {
    !active_slot().is_null()
}

/// Run `f` with no scratch scope on this thread, restoring the scope afterwards. Every parallel job that may build
/// terms runs detached: rayon may execute a job on a thread that is waiting inside a scope, and the job's terms must
/// not land in that scope.
#[inline]
pub fn detached<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(*const Slot);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE.with(|a| a.set(self.0));
        }
    }
    let _restore = Restore(ACTIVE.with(|a| a.replace(null())));
    f()
}

/// A private open-addressing table of a slot: `(hash >> 32) << 32 | (position + 1)`, 0 when empty, at most half
/// full.
struct Table {
    s: Vec<u64>,
    count: usize,
}

impl Table {
    fn new() -> Table {
        Table { s: vec![0; TABLE_MIN], count: 0 }
    }

    /// The position of a stored node with this hash for which `eq` holds, or the empty index the probe ended at.
    #[inline]
    fn find(&self, hash: u64, eq: impl Fn(usize) -> bool) -> Result<usize, usize> {
        let mask = self.s.len() - 1;
        let tag = hash >> 32;
        let mut i = tag as usize & mask;
        loop {
            let e = self.s[i];
            if e == 0 {
                return Err(i);
            }
            if e >> 32 == tag && eq((e as u32 - 1) as usize) {
                return Ok((e as u32 - 1) as usize);
            }
            i = (i + 1) & mask;
        }
    }

    fn insert(&mut self, at: usize, hash: u64, pos: usize) {
        self.s[at] = (hash >> 32) << 32 | (pos as u64 + 1);
        self.count += 1;
        if self.count * 2 > self.s.len() {
            let grown = vec![0; self.s.len() * 2];
            let old = std::mem::replace(&mut self.s, grown);
            let mask = self.s.len() - 1;
            for e in old.into_iter().filter(|&e| e != 0) {
                let mut i = (e >> 32) as usize & mask;
                while self.s[i] != 0 {
                    i = (i + 1) & mask;
                }
                self.s[i] = e;
            }
        }
    }

    fn clear(&mut self) {
        if self.s.len() > TABLE_KEEP {
            self.s = vec![0; TABLE_MIN];
        } else if self.count > 0 {
            self.s.fill(0);
        }
        self.count = 0;
    }
}

/// One scratch slot: the storage of the scope that holds it.
///
/// Every field but `busy` is touched only by the thread whose open scope holds the slot (it took `busy`, and its
/// thread-local [`ACTIVE`] points here), or with `&mut Heap`; that is what makes the `Sync` below sound.
pub(crate) struct Slot {
    no: u32,
    busy: AtomicBool,
    /// set while exporting: the owner builds shared terms
    sharing: Cell<bool>,
    /// the page of the scope's first node, and the pages it has used
    page0: Cell<u32>,
    pages: Cell<u32>,
    /// nodes stored in the scope
    len: Cell<usize>,
    /// node segments (`SEG` nodes each), kept for reuse
    segs: UnsafeCell<Vec<*mut Node>>,
    /// argument and payload slabs (`SLAB` words each), kept for reuse; the current one and its fill
    slabs: UnsafeCell<Vec<*mut u64>>,
    slab: Cell<usize>,
    off: Cell<usize>,
    /// allocations of the scope beyond the slabs (long argument lists and payloads), freed when it ends
    big: UnsafeCell<Vec<(*mut u64, usize)>>,
    table: RefCell<Table>,
    memo: RefCell<FxHashMap<u32, Item>>,
    /// unpacked arguments of packed arrays viewed in the scope
    unpacked: RefCell<FxHashMap<u32, *mut [u32]>>,
}

// SAFETY: see the type's documentation: the cells are used by one thread at a time, handed over through `busy`
// (acquire on taking, release on giving back).
unsafe impl Sync for Slot {}
// SAFETY: the raw pointers are allocations the slot owns.
unsafe impl Send for Slot {}

fn alloc_words(n: usize) -> *mut u64 {
    Box::into_raw(vec![0u64; n].into_boxed_slice()) as *mut u64
}

/// Free an allocation made by [`alloc_words`] (or a boxed slice of `n` words).
///
/// # Safety
/// `p` came from a boxed `[u64]` of length `n` and is freed once.
unsafe fn free_words(p: *mut u64, n: usize) {
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, n)) });
}

fn alloc_seg() -> *mut Node {
    Box::into_raw(Box::<[Node]>::new_uninit_slice(SEG)) as *mut Node
}

/// # Safety
/// `p` came from [`alloc_seg`] and is freed once.
unsafe fn free_seg(p: *mut Node) {
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(p as *mut std::mem::MaybeUninit<Node>, SEG)) });
}

impl Slot {
    pub(crate) fn new(no: u32) -> Slot {
        Slot {
            no,
            busy: AtomicBool::new(false),
            sharing: Cell::new(false),
            page0: Cell::new(0),
            pages: Cell::new(0),
            len: Cell::new(0),
            segs: UnsafeCell::new(vec![]),
            slabs: UnsafeCell::new(vec![]),
            slab: Cell::new(0),
            off: Cell::new(0),
            big: UnsafeCell::new(vec![]),
            table: RefCell::new(Table::new()),
            memo: RefCell::new(FxHashMap::default()),
            unpacked: RefCell::new(FxHashMap::default()),
        }
    }

    /// The handle of storage position `pos`.
    #[inline]
    fn handle(&self, pos: usize) -> u32 {
        let page = ((self.page0.get() + (pos / PAGE_LEN) as u32) % PAGES) << PAGE_SHIFT;
        YOUNG | SCR | self.no << SLOT_SHIFT | page | (pos % PAGE_LEN) as u32
    }

    /// The storage position of handle `b` of this slot, if it names a node of the open scope.
    #[inline]
    fn pos(&self, b: u32) -> Option<usize> {
        let idx = (b & ((1 << PAGE_SHIFT) - 1)) as usize;
        let page = ((b >> PAGE_SHIFT) % PAGES + PAGES - self.page0.get()) % PAGES;
        let p = page as usize * PAGE_LEN + idx;
        (page < self.pages.get() && idx < PAGE_LEN && p < self.len.get()).then_some(p)
    }

    #[inline]
    fn at(&self, p: usize) -> &Node {
        // SAFETY: `p < len`, so its segment exists and the node was written; segments are never freed or moved
        // while the scope is open (only `trim`, with `&mut Heap`, frees them). The vector itself is only read here.
        unsafe {
            let seg = (&*self.segs.get())[p >> SEG_BITS];
            &*seg.add(p & (SEG - 1))
        }
    }

    /// Room for `n` words that live until the scope ends.
    fn words(&self, n: usize) -> *mut u64 {
        if n == 0 {
            return std::ptr::NonNull::dangling().as_ptr();
        }
        if n > SLAB / 8 {
            let p = alloc_words(n);
            // SAFETY: owner-only access (type docs).
            unsafe { (*self.big.get()).push((p, n)) };
            return p;
        }
        // SAFETY: owner-only access (type docs); slabs are never moved, and their used part is not written again
        // before the scope ends.
        unsafe {
            let slabs = &mut *self.slabs.get();
            if slabs.is_empty() {
                slabs.push(alloc_words(SLAB));
            }
            if self.off.get() + n > SLAB {
                self.slab.set(self.slab.get() + 1);
                self.off.set(0);
                if self.slab.get() == slabs.len() {
                    slabs.push(alloc_words(SLAB));
                }
            }
            let p = slabs[self.slab.get()].add(self.off.get());
            self.off.set(self.off.get() + n);
            p
        }
    }

    /// Store a node; returns its handle.
    fn push(&self, n: Node) -> u32 {
        let p = self.len.get();
        if p / PAGE_LEN >= self.pages.get() as usize {
            assert!(self.pages.get() < PAGES, "a scratch scope made more than {} terms", PAGES as usize * PAGE_LEN);
            self.pages.set(self.pages.get() + 1);
        }
        // SAFETY: owner-only access (type docs); the slot `p` is unpublished and written once before use.
        unsafe {
            let segs = &mut *self.segs.get();
            if p >> SEG_BITS == segs.len() {
                segs.push(alloc_seg());
            }
            segs[p >> SEG_BITS].add(p & (SEG - 1)).write(n);
        }
        self.len.set(p + 1);
        self.handle(p)
    }

    /// Store a node with its body copied into the scope's storage.
    fn store(&self, mut n: Node, body: Body) -> u32 {
        match body {
            Body::Inline => {}
            Body::Args(a) => {
                let p = self.words(a.len().div_ceil(2)) as *mut u32;
                // SAFETY: room for `a.len()` slots was just reserved.
                unsafe { std::ptr::copy_nonoverlapping(a.as_ptr(), p, a.len()) };
                n.data = p as u64;
            }
            Body::Words(w) => {
                let p = self.words(w.len());
                // SAFETY: as above.
                unsafe { std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len()) };
                n.data = p as u64;
            }
        }
        self.push(n)
    }

    /// Store a payload node whose words come in their own box (kept until the scope ends).
    fn store_owned(&self, mut n: Node, words: Box<[u64]>) -> u32 {
        let len = words.len();
        let p = Box::into_raw(words) as *mut u64;
        // SAFETY: owner-only access (type docs).
        unsafe { (*self.big.get()).push((p, len)) };
        n.data = p as u64;
        self.push(n)
    }

    /// Does the private table hold a term with this structural hash?
    fn has_hash(&self, hash: u64) -> bool {
        self.table.borrow().find(hash, |p| self.at(p).hash == hash).is_ok()
    }

    /// Start a scope: pages move on, so handles of the previous scope are recognised as stale.
    fn begin(&self) {
        self.page0.set((self.page0.get() + self.pages.get().max(1)) % PAGES);
        self.pages.set(0);
        self.len.set(0);
        self.slab.set(0);
        self.off.set(0);
    }

    /// End a scope: everything it stored is released for the next one.
    fn end(&self) {
        self.len.set(0);
        self.slab.set(0);
        self.off.set(0);
        // SAFETY: owner-only access (type docs); the scope's handles are dead (G.2), so no borrow of these
        // allocations remains.
        unsafe {
            for (p, n) in (*self.big.get()).drain(..) {
                free_words(p, n);
            }
        }
        self.table.borrow_mut().clear();
        self.memo.borrow_mut().clear();
        for (_, p) in self.unpacked.borrow_mut().drain() {
            // SAFETY: made by `Box::into_raw` in `unpacked`, freed once.
            drop(unsafe { Box::from_raw(p) });
        }
    }

    /// Free storage kept beyond the reuse allowance (a safe point).
    fn trim(&mut self) {
        let segs = self.segs.get_mut();
        while segs.len() > KEEP_SEGS {
            // SAFETY: no scope is open (`&mut`); the segment came from `alloc_seg`.
            unsafe { free_seg(segs.pop().expect("nonempty")) };
        }
        let slabs = self.slabs.get_mut();
        while slabs.len() > KEEP_SLABS {
            // SAFETY: as above; slabs came from `alloc_words(SLAB)`.
            unsafe { free_words(slabs.pop().expect("nonempty"), SLAB) };
        }
    }

    /// Bytes the slot keeps.
    fn reserved(&mut self) -> usize {
        self.segs.get_mut().len() * SEG * std::mem::size_of::<Node>()
            + self.slabs.get_mut().len() * SLAB * 8
            + self.table.get_mut().s.len() * 8
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.end();
        for p in self.segs.get_mut().drain(..) {
            // SAFETY: from `alloc_seg`, freed once.
            unsafe { free_seg(p) };
        }
        for p in self.slabs.get_mut().drain(..) {
            // SAFETY: from `alloc_words(SLAB)`, freed once.
            unsafe { free_words(p, SLAB) };
        }
    }
}

/// The slots of a heap and their counters.
pub(crate) struct Slots {
    slots: Box<[Slot]>,
    scopes: AtomicU64,
    terms: AtomicU64,
    largest: AtomicU64,
    declined: AtomicU64,
}

impl Slots {
    pub(crate) fn new() -> Slots {
        Slots {
            slots: (0..SLOTS as u32).map(Slot::new).collect(),
            scopes: AtomicU64::new(0),
            terms: AtomicU64::new(0),
            largest: AtomicU64::new(0),
            declined: AtomicU64::new(0),
        }
    }

    /// Is `s` one of these slots?
    #[inline]
    pub(crate) fn owns(&self, s: *const Slot) -> bool {
        self.slots.as_ptr_range().contains(&s)
    }

    /// Free storage beyond the reuse allowance (a safe point).
    pub(crate) fn trim(&mut self) {
        self.slots.iter_mut().for_each(Slot::trim);
    }

    pub(crate) fn reserved(&mut self) -> usize {
        self.slots.iter_mut().map(Slot::reserved).sum()
    }
}

/// Counters of the scratch scopes of a heap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScratchStats {
    /// scopes opened
    pub scopes: u64,
    /// terms made in scopes (all dropped when their scope ended)
    pub terms: u64,
    /// the most terms one scope made
    pub largest: u64,
    /// requests that got no scope (one was open on the thread, every slot was taken, or scopes are off)
    pub declined: u64,
}

/// An open scratch scope on this thread (see the module docs). Dropping it ends the scope: every scratch handle it
/// made is dead from then on. Not `Send`: a scope belongs to its thread.
pub struct Scratch<'h> {
    heap: &'h Heap,
    slot: &'h Slot,
    _thread: PhantomData<*const ()>,
}

impl Heap {
    /// Open a scratch scope on this thread: until the guard is dropped, terms this thread builds that are not
    /// already shared are kept apart and dropped with the scope (decision G.1). `None` when scopes are off
    /// ([`Config::scratch`](crate::Config::scratch)), a scope is already open on this thread (the outer one then
    /// holds the inner work), or every slot is in use.
    pub fn scratch(&self) -> Option<Scratch<'_>> {
        if !self.cfg.scratch || scratch_active() {
            self.slots.declined.fetch_add(1, Relaxed);
            return None;
        }
        for s in self.slots.slots.iter() {
            if !s.busy.load(Relaxed) && s.busy.compare_exchange(false, true, Acquire, Relaxed).is_ok() {
                s.begin();
                ACTIVE.with(|a| a.set(s));
                self.slots.scopes.fetch_add(1, Relaxed);
                return Some(Scratch { heap: self, slot: s, _thread: PhantomData });
            }
        }
        self.slots.declined.fetch_add(1, Relaxed);
        None
    }

    /// Is `t` a scratch term (made in a scope that must be open on this thread for `t` to be used)? Caches keyed by
    /// handles that outlive one evaluation must not keep scratch terms: their handles are reused by later scopes.
    #[inline]
    pub fn is_scratch(&self, t: T<'_>) -> bool {
        is_scratch(t.raw())
    }

    /// Counters of the scratch scopes so far.
    pub fn scratch_stats(&self) -> ScratchStats {
        let s = &self.slots;
        ScratchStats {
            scopes: s.scopes.load(Relaxed),
            terms: s.terms.load(Relaxed),
            largest: s.largest.load(Relaxed),
            declined: s.declined.load(Relaxed),
        }
    }

    /// The open scope of this heap on this thread, unless it is exporting.
    #[inline]
    pub(crate) fn scope(&self) -> Option<&Slot> {
        let s = active_slot();
        if s.is_null() || !self.slots.owns(s) {
            return None;
        }
        // SAFETY: `s` points into this heap's slots, which live as long as `&self`.
        let s = unsafe { &*s };
        (!s.sharing.get()).then_some(s)
    }

    /// The node of scratch handle `b`; panics unless it belongs to the scope open on this thread.
    #[cold]
    #[inline(never)]
    fn stale(&self, b: u32) -> ! {
        panic!(
            "use of the scratch term {b:#x} outside its scope (a handle made in a scratch scope was kept after the \
             scope ended, or passed to another thread)"
        )
    }

    #[inline]
    pub(crate) fn scratch_node(&self, b: u32) -> &Node {
        let s = active_slot();
        if s.is_null() || !self.slots.owns(s) {
            self.stale(b);
        }
        // SAFETY: as in `scope`.
        let s = unsafe { &*s };
        if (b >> SLOT_SHIFT) & (SLOTS as u32 - 1) != s.no {
            self.stale(b);
        }
        match s.pos(b) {
            Some(p) => s.at(p),
            None => self.stale(b),
        }
    }

    /// Intern a node inside the scope `s`: the private table first (this thread's handle for the term stays the
    /// same), then the shared tables (look-up only, and only when no part is scratch), else store it privately.
    pub(crate) fn intern_scratch(&self, s: &Slot, proto: Node, body: Body, may_be_old: bool) -> u32 {
        let young = &self.young;
        let found = s.table.borrow().find(proto.hash, |p| same(young, s.at(p), &proto, body));
        let at = match found {
            Ok(p) => return s.handle(p),
            Err(at) => at,
        };
        let scratch_part = match body {
            Body::Args(a) => is_scratch(proto.head) || a.iter().any(|&x| is_scratch(x)),
            Body::Inline if proto.kind() == Kind::Rat => is_scratch(proto.head) || is_scratch(proto.data as u32),
            _ => false,
        };
        if !scratch_part {
            if may_be_old && self.old.table.count > 0 {
                if let Some(id) = self.old.table.find(proto.hash, |id| same(&self.old, self.old.node(id), &proto, body)) {
                    return id;
                }
            }
            if let Some(id) = self.ytable.find(proto.hash, |id| same(young, young.node(id), &proto, body)) {
                return YOUNG | id;
            }
        }
        let b = s.store(proto, body);
        // the table did not change since the probe
        s.table.borrow_mut().insert(at, proto.hash, s.len.get() - 1);
        b
    }

    /// [`Heap::intern_scratch`] for a payload whose words come in their own box.
    pub(crate) fn intern_scratch_owned(&self, s: &Slot, proto: Node, words: Vec<u64>) -> u32 {
        let young = &self.young;
        let w: &[u64] = &words;
        let at = match s.table.borrow().find(proto.hash, |p| same(young, s.at(p), &proto, Body::Words(w))) {
            Ok(p) => return s.handle(p),
            Err(at) => at,
        };
        if self.old.table.count > 0 {
            if let Some(id) = self.old.table.find(proto.hash, |id| same(&self.old, self.old.node(id), &proto, Body::Words(w))) {
                return id;
            }
        }
        if let Some(id) = self.ytable.find(proto.hash, |id| same(young, young.node(id), &proto, Body::Words(w))) {
            return YOUNG | id;
        }
        let b = s.store_owned(proto, words.into_boxed_slice());
        s.table.borrow_mut().insert(at, proto.hash, s.len.get() - 1);
        b
    }

    /// A memo hit inside scope `s`, or `None`: the private memo, then the shared one (decision G.4).
    pub(crate) fn scratch_memo_get(&self, s: &Slot, b: u32) -> Option<Item> {
        if let Some(it) = s.memo.borrow().get(&b) {
            return Some(*it);
        }
        if is_scratch(b) {
            return None;
        }
        let it = self.shared_memo_get(b)?;
        self.twin_free(s, it.val).then_some(it)
    }

    /// Memoise inside scope `s`: an entry with a scratch key or value stays private.
    pub(crate) fn scratch_memo_put(&self, s: &Slot, it: Item) -> bool {
        if is_scratch(it.key) || is_scratch(it.val) {
            s.memo.borrow_mut().insert(it.key, it);
            return true;
        }
        false
    }

    /// May a shared term `v` meet the scope's terms without breaking canonical handles? Yes when no part of it has a
    /// structural hash present in the private table (so no part is equal to a scratch term). Immediates and old
    /// terms always may (an old term existed before the scope began, so the scope found it instead of making a
    /// twin); a young term is checked through at most [`TWIN_CHECK`] nodes, beyond which the answer is no.
    fn twin_free(&self, s: &Slot, v: u32) -> bool {
        if is_imm(v) || is_old(v) || s.table.borrow().count == 0 {
            return true;
        }
        let mut stack = vec![v];
        let mut seen = 0;
        while let Some(b) = stack.pop() {
            if is_imm(b) || is_old(b) {
                continue;
            }
            seen += 1;
            if seen > TWIN_CHECK {
                return false;
            }
            let n = self.young.node(b & crate::handle::LOW);
            if s.has_hash(n.hash) {
                return false;
            }
            for_each_child(&self.young, n, |c| stack.push(c));
        }
        true
    }

    /// The unpacked arguments of packed node `b` inside scope `s`, made privately (G.2 rule 2, G.4).
    pub(crate) fn scratch_unpacked(&self, s: &Slot, b: u32, make: impl FnOnce() -> Vec<u32>) -> &[u32] {
        if let Some(&p) = s.unpacked.borrow().get(&b) {
            // SAFETY: the box lives until the scope ends, and the scope's slices die with it.
            return unsafe { &*p };
        }
        let p = Box::into_raw(make().into_boxed_slice());
        s.unpacked.borrow_mut().insert(b, p);
        // SAFETY: as above.
        unsafe { &*p }
    }
}

impl<'h> Scratch<'h> {
    /// The terms made in the scope so far.
    pub fn terms(&self) -> usize {
        self.slot.len.get()
    }

    /// A shared copy of `t`: `t` itself unless it has scratch parts, else the same term built in the shared heap
    /// (equal terms keep equal handles). What a native keeps from a scope must be exported before the scope ends.
    pub fn export(&self, t: T<'h>) -> T<'h> {
        let h = self.heap;
        let root = t.raw();
        if !is_scratch(root) {
            return t;
        }
        self.slot.sharing.set(true);
        struct Sharing<'a>(&'a Slot);
        impl Drop for Sharing<'_> {
            fn drop(&mut self) {
                self.0.sharing.set(false);
            }
        }
        let _sharing = Sharing(self.slot);
        let mut done: FxHashMap<u32, u32> = FxHashMap::default();
        let map = |done: &FxHashMap<u32, u32>, c: u32| if is_scratch(c) { done[&c] } else { c };
        // an explicit stack: post-order over the scratch parts (terms may be deep)
        let mut stack = vec![(root, false)];
        while let Some((b, expanded)) = stack.pop() {
            if done.contains_key(&b) {
                continue;
            }
            let n = h.scratch_node(b);
            if !expanded {
                stack.push((b, true));
                for_each_child(&h.young, n, |c| {
                    if is_scratch(c) && !done.contains_key(&c) {
                        stack.push((c, false));
                    }
                });
                continue;
            }
            let n = *n;
            let copy = match n.kind() {
                Kind::App => {
                    let head = map(&done, n.head);
                    let args: Vec<u32> = h.young.args(&n).iter().map(|&a| map(&done, a)).collect();
                    let shared = !crate::handle::is_young(head) && args.iter().all(|&a| !crate::handle::is_young(a));
                    let proto = Node::new(Kind::App, n.hash, n.mask, head, 0, args.len());
                    h.intern(proto, Body::Args(&args), shared)
                }
                Kind::Rat => {
                    let (num, den) = (map(&done, n.head), map(&done, n.data as u32));
                    let shared = !crate::handle::is_young(num) && !crate::handle::is_young(den);
                    h.intern(Node::new(Kind::Rat, n.hash, 0, num, den as u64, 0), Body::Inline, shared)
                }
                Kind::Word | Kind::Real => h.intern(n, Body::Inline, true),
                Kind::Packed => h.intern_owned(n, h.young.words(&n).to_vec()),
                Kind::Int | Kind::Ball | Kind::Str => h.intern(n, Body::Words(h.young.words(&n)), true),
            };
            done.insert(b, copy);
        }
        T::from_raw(done[&root])
    }
}

impl Drop for Scratch<'_> {
    fn drop(&mut self) {
        let made = self.slot.len.get() as u64;
        let st = &self.heap.slots;
        st.terms.fetch_add(made, Relaxed);
        st.largest.fetch_max(made, Relaxed);
        ACTIVE.with(|a| a.set(null()));
        self.slot.end();
        self.slot.busy.store(false, Release);
    }
}
