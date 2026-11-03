//! The 32-byte stored node, shared by both regions (I3's layout, adapted to the concurrent store).
//!
//! `hash` (structural, stable), `mask` (symbol mask for normal-form stamps, semantics 4.3), `data` (one word),
//! `head` (one handle or flag word) and `meta` (kind in the top 3 bits, length in the low 29). By kind:
//!
//! | kind | `head` | `data` | length |
//! |---|---|---|---|
//! | `App` | head handle | arguments: pointer (young) or index into the old argument array | arguments |
//! | `Word` | sign (0/1) | magnitude (one limb, I3.1) | 0 |
//! | `Rat` | numerator handle | denominator handle | 0 |
//! | `Real` | 0 | IEEE bits | 0 |
//! | `Int`, `Ball` | sign (0/1) | payload words: see below | words |
//! | `Str` | byte length | UTF-8 bytes packed into words, zero padded | words |
//!
//! Payload placement is a function of the length (I3.2, A.4): at most [`ARENA_MAX`] words live in a limb arena
//! (young: a pointer into a per-thread chunk; old: an index into the old limb array, sliding with the nodes); longer
//! payloads are **boxed**, an allocation owned by the node (a pointer in both regions). A young boxed node is listed
//! in its thread's box list, which the minor collection walks to free the dead ones (A.3); an old one is freed in the
//! major slide when it dies.

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Node {
    pub hash: u64,
    pub mask: u64,
    pub data: u64,
    pub head: u32,
    meta: u32,
}

pub(crate) const NODE: usize = std::mem::size_of::<Node>();
const _: () = assert!(NODE == 32, "nodes are 32 bytes (I3)");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    App,
    Word,
    Rat,
    Real,
    Int,
    Ball,
    Str,
}

const KIND_SHIFT: u32 = 29;
/// The longest argument list or payload a node can hold.
pub(crate) const MAX_LEN: usize = (1 << KIND_SHIFT) - 1;
/// Payloads of at most this many words live in the limb arena; longer ones are boxed (I3.2).
pub(crate) const ARENA_MAX: usize = 64;

impl Node {
    pub(crate) fn new(kind: Kind, hash: u64, mask: u64, head: u32, data: u64, len: usize) -> Node {
        assert!(len <= MAX_LEN, "term too wide ({len} arguments or payload words)");
        Node { hash, mask, data, head, meta: (kind as u32) << KIND_SHIFT | len as u32 }
    }

    #[inline]
    pub(crate) fn kind(&self) -> Kind {
        match self.meta >> KIND_SHIFT {
            0 => Kind::App,
            1 => Kind::Word,
            2 => Kind::Rat,
            3 => Kind::Real,
            4 => Kind::Int,
            5 => Kind::Ball,
            _ => Kind::Str,
        }
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        (self.meta & ((1 << KIND_SHIFT) - 1)) as usize
    }

    /// Kind and length together (one comparison in lookups).
    #[inline]
    pub(crate) fn meta(&self) -> u32 {
        self.meta
    }

    #[inline]
    pub(crate) fn is_payload(&self) -> bool {
        matches!(self.kind(), Kind::Int | Kind::Ball | Kind::Str)
    }

    /// A payload in a box owned by the node.
    #[inline]
    pub(crate) fn is_boxed(&self) -> bool {
        self.is_payload() && self.len() > ARENA_MAX
    }

    /// A payload in the limb arena of its region.
    #[inline]
    pub(crate) fn in_arena(&self) -> bool {
        self.is_payload() && self.len() <= ARENA_MAX
    }

    /// The words of a payload addressed by pointer (young arena payloads, boxed payloads).
    ///
    /// # Safety
    /// `data` must point at `len` initialised words that outlive the returned borrow.
    #[inline]
    pub(crate) unsafe fn words_by_ptr<'a>(&self) -> &'a [u64] {
        unsafe { std::slice::from_raw_parts(self.data as *const u64, self.len()) }
    }

    /// The arguments of a compound addressed by pointer (young compounds).
    ///
    /// # Safety
    /// `data` must point at `len` initialised argument slots that outlive the returned borrow.
    #[inline]
    pub(crate) unsafe fn args_by_ptr<'a>(&self) -> &'a [u32] {
        unsafe { std::slice::from_raw_parts(self.data as *const u32, self.len()) }
    }
}

/// Move `words` into a new box; returns the pointer for `data`.
pub(crate) fn box_words(words: &[u64]) -> u64 {
    let b: Box<[u64]> = words.into();
    Box::into_raw(b) as *mut u64 as u64
}

/// Free the box of a boxed node.
///
/// # Safety
/// `n` is a boxed node whose box was made by [`box_words`] and is freed exactly once; no borrow of it remains.
pub(crate) unsafe fn free_box(n: &Node) {
    debug_assert!(n.is_boxed());
    let p = std::ptr::slice_from_raw_parts_mut(n.data as *mut u64, n.len());
    // SAFETY: by the caller's contract, `p` is the boxed slice made by `box_words`.
    drop(unsafe { Box::from_raw(p) });
}

/// Bytes a boxed payload of `len` words accounts for (I3.3).
#[inline]
pub(crate) fn box_bytes(len: usize) -> usize {
    8 * len
}

/// The arguments and payload words of a region's nodes (young nodes address them by pointer, old nodes by index).
pub(crate) trait Region {
    fn args<'a>(&'a self, n: &Node) -> &'a [u32];
    fn words<'a>(&'a self, n: &Node) -> &'a [u64];
}

/// Call `f` on every child handle of `n`: the head and arguments of a compound, the parts of a rational.
#[inline]
pub(crate) fn for_each_child<R: Region>(r: &R, n: &Node, mut f: impl FnMut(u32)) {
    match n.kind() {
        Kind::App => {
            f(n.head);
            for &a in r.args(n) {
                f(a);
            }
        }
        Kind::Rat => {
            f(n.head);
            f(n.data as u32);
        }
        _ => {}
    }
}
