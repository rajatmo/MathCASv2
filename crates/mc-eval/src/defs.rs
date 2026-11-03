//! Definitions of all symbols: own values, attributes, rule sets and argument-head rule sets (semantics 6.4, 6.5,
//! 7.3), shared by every task of an evaluation and written by one writer at a time (the session between evaluations,
//! or the frontier task during one; I2.4, I2.5).
//!
//! An own value is a handle in an atomic slot ([`Raw::to_bits`]; [`HOLE`] when there is none): a redefinition stores
//! the new handle, and a reader keeps using the term it loaded, which stays valid until the next safe point.
use crate::conc::SymTable;
use crate::index::{Rule, RuleSet};
use mc_core::{Heap, Raw, Sym, HOLE, T};
use parking_lot::Mutex;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering::*};
use std::sync::OnceLock;

/// Argument modes (semantics 5) of the built-ins and of symbols given attributes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Hold {
    /// every argument is evaluated first (strict)
    #[default]
    None,
    /// the first argument is passed unevaluated
    First,
    /// every argument but the first is passed unevaluated
    Rest,
    /// no argument is evaluated
    All,
    /// argument `i` is held when bit `i` is set (`i < 31`); bit 31 holds every argument from position 31 on (the
    /// mode of `held` parameters, 04 section 6.5, and of `for`)
    Positions(u32),
}

impl Hold {
    /// Is argument `i` held?
    #[inline]
    pub fn holds(self, i: usize) -> bool {
        match self {
            Hold::None => false,
            Hold::First => i == 0,
            Hold::Rest => i > 0,
            Hold::All => true,
            Hold::Positions(m) => m >> i.min(31) & 1 != 0,
        }
    }
    /// The mode as a position mask (bit 31 standing for every later position).
    pub fn mask(self) -> u32 {
        match self {
            Hold::None => 0,
            Hold::First => 1,
            Hold::Rest => !1,
            Hold::All => !0,
            Hold::Positions(m) => m,
        }
    }
    /// The simplest mode holding exactly the positions of `mask`.
    pub fn from_mask(mask: u32) -> Hold {
        match mask {
            0 => Hold::None,
            1 => Hold::First,
            m if m == !1 => Hold::Rest,
            m if m == !0 => Hold::All,
            m => Hold::Positions(m),
        }
    }
    fn bits(self) -> u64 {
        match self {
            Hold::None => 0,
            Hold::First => 1,
            Hold::Rest => 2,
            Hold::All => 3,
            Hold::Positions(m) => 4 | (m as u64) << 32,
        }
    }
    fn from_bits(b: u64) -> Hold {
        match b & 7 {
            0 => Hold::None,
            1 => Hold::First,
            2 => Hold::Rest,
            3 => Hold::All,
            _ => Hold::Positions((b >> 32) as u32),
        }
    }
}

/// Symbol attributes (semantics 5, 7.2, 7.3, 9).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Attributes {
    /// argument modes
    pub hold: Hold,
    /// associative and commutative: arguments are flattened and sorted, and matching is modulo AC (6.2)
    pub ac: bool,
    /// the function performs effects (semantics 9): calls are never evaluated speculatively ahead of their turn
    /// (used by the granularity estimate)
    pub effect: bool,
    /// the function receives failures as arguments instead of returning the first one (semantics 10)
    pub handles_failures: bool,
    /// the function maps over list arguments of equal length, broadcasting the others (semantics 7.2)
    pub listable: bool,
}

const A_AC: u64 = 1 << 3;
const A_EFFECT: u64 = 1 << 4;
const A_FAILURES: u64 = 1 << 5;
const A_LISTABLE: u64 = 1 << 6;

impl Attributes {
    fn bits(self) -> u64 {
        self.hold.bits()
            | if self.ac { A_AC } else { 0 }
            | if self.effect { A_EFFECT } else { 0 }
            | if self.handles_failures { A_FAILURES } else { 0 }
            | if self.listable { A_LISTABLE } else { 0 }
    }
    pub(crate) fn from_bits(b: u64) -> Attributes {
        Attributes {
            hold: Hold::from_bits(b),
            ac: b & A_AC != 0,
            effect: b & A_EFFECT != 0,
            handles_failures: b & A_FAILURES != 0,
            listable: b & A_LISTABLE != 0,
        }
    }
}

/// Argument-head rules a symbol holds for one outer head: a prepend-only list.
struct ArgNode {
    outer: Sym,
    rs: RuleSet,
    next: *mut ArgNode,
}

/// One symbol's definitions.
pub(crate) struct SymDefs {
    /// the own value's handle bits, or [`HOLE`]
    own: AtomicU32,
    version: AtomicU32,
    attrs: AtomicU64,
    /// the identity element's handle bits (semantics 7.3), or [`HOLE`]
    identity: AtomicU32,
    /// index + 1 of the symbol's native implementation (0: none)
    pub(crate) native: AtomicU32,
    rules: OnceLock<Box<RuleSet>>,
    args: AtomicPtr<ArgNode>,
}

// SAFETY: the raw pointers are owned boxes published by release stores and freed only with `&mut self`.
unsafe impl Send for SymDefs {}
// SAFETY: as above; readers only load and dereference published, immutable boxes.
unsafe impl Sync for SymDefs {}

impl Default for SymDefs {
    fn default() -> Self {
        SymDefs {
            own: AtomicU32::new(HOLE),
            version: AtomicU32::new(0),
            attrs: AtomicU64::new(0),
            identity: AtomicU32::new(HOLE),
            native: AtomicU32::new(0),
            rules: OnceLock::new(),
            args: AtomicPtr::new(null_mut()),
        }
    }
}

impl SymDefs {
    /// The own value.
    #[inline]
    pub(crate) fn own<'h>(&self, h: &'h Heap) -> Option<T<'h>> {
        let bits = self.own.load(Acquire);
        (bits != HOLE).then(|| Raw::from_bits(bits).get(h))
    }

    #[inline]
    pub(crate) fn attrs(&self) -> Attributes {
        Attributes::from_bits(self.attrs.load(Relaxed))
    }

    /// The identity element, if one was declared.
    #[inline]
    pub(crate) fn identity<'h>(&self, h: &'h Heap) -> Option<T<'h>> {
        let bits = self.identity.load(Acquire);
        (bits != HOLE).then(|| Raw::from_bits(bits).get(h))
    }

    #[inline]
    pub(crate) fn rules(&self) -> Option<&RuleSet> {
        self.rules.get().map(|b| &**b)
    }

    /// Whether the symbol holds any argument-head rules.
    #[inline]
    pub(crate) fn has_arg_rules(&self) -> bool {
        !self.args.load(Acquire).is_null()
    }

    /// The argument-head rules the symbol holds for outer head `outer`.
    pub(crate) fn arg_rules(&self, outer: Sym) -> Option<&RuleSet> {
        let mut p = self.args.load(Acquire);
        while !p.is_null() {
            // SAFETY: nodes are published complete (release) and freed only at drop.
            let n = unsafe { &*p };
            if n.outer == outer {
                return Some(&n.rs);
            }
            p = n.next;
        }
        None
    }

    fn for_each_rule_set(&mut self, mut f: impl FnMut(&mut RuleSet)) {
        if let Some(rs) = self.rules.get_mut() {
            f(rs);
        }
        let mut p = *self.args.get_mut();
        while !p.is_null() {
            // SAFETY: `&mut self`: no reader; the node is live until drop.
            let node = unsafe { &mut *p };
            f(&mut node.rs);
            p = node.next;
        }
    }
}

impl Drop for SymDefs {
    fn drop(&mut self) {
        let mut p = *self.args.get_mut();
        while !p.is_null() {
            // SAFETY: every node came from `Box::into_raw` and is reachable once.
            let b = unsafe { Box::from_raw(p) };
            p = b.next;
        }
    }
}

/// The definitions of every symbol.
#[derive(Default)]
pub(crate) struct Defs {
    syms: SymTable<SymDefs>,
    /// Serialises writers (uncontended: only the frontier or the session defines).
    writer: Mutex<()>,
}

impl Defs {
    #[inline]
    pub(crate) fn get(&self, s: Sym) -> Option<&SymDefs> {
        self.syms.get(s.0)
    }

    #[inline]
    pub(crate) fn own<'h>(&self, h: &'h Heap, s: Sym) -> Option<T<'h>> {
        self.get(s)?.own(h)
    }

    #[inline]
    pub(crate) fn attrs(&self, s: Sym) -> Attributes {
        self.get(s).map_or_else(Attributes::default, SymDefs::attrs)
    }

    pub(crate) fn version(&self, s: Sym) -> u32 {
        self.get(s).map_or(0, |d| d.version.load(Relaxed))
    }

    pub(crate) fn set_own(&self, s: Sym, v: T<'_>) {
        let _w = self.writer.lock();
        let sd = self.syms.get_or_init(s.0);
        sd.own.store(Raw::new(v).to_bits(), Release);
        sd.version.fetch_add(1, Relaxed);
    }

    pub(crate) fn set_attrs(&self, s: Sym, a: Attributes) {
        let _w = self.writer.lock();
        let sd = self.syms.get_or_init(s.0);
        sd.attrs.store(a.bits(), Relaxed);
        sd.version.fetch_add(1, Relaxed);
    }

    pub(crate) fn set_identity(&self, s: Sym, e: Option<T<'_>>) {
        let _w = self.writer.lock();
        let sd = self.syms.get_or_init(s.0);
        sd.identity.store(e.map_or(HOLE, |e| Raw::new(e).to_bits()), Release);
        sd.version.fetch_add(1, Relaxed);
    }

    pub(crate) fn set_native(&self, s: Sym, index: u32) {
        let _w = self.writer.lock();
        let sd = self.syms.get_or_init(s.0);
        sd.native.store(index + 1, Release);
        sd.version.fetch_add(1, Relaxed);
    }

    /// Add `rule` to the rules `owner` holds: its own (`outer == None`) or its argument-head rules for `outer`.
    pub(crate) fn define(&self, h: &Heap, owner: Sym, outer: Option<Sym>, ac_top: bool, rule: Rule) {
        let _w = self.writer.lock();
        let sd = self.syms.get_or_init(owner.0);
        let rs = match outer {
            None => &**sd.rules.get_or_init(|| Box::new(RuleSet::new(ac_top))),
            Some(o) => match sd.arg_rules(o) {
                Some(rs) => rs,
                None => {
                    let n = Box::into_raw(Box::new(ArgNode { outer: o, rs: RuleSet::new(ac_top), next: sd.args.load(Relaxed) }));
                    sd.args.store(n, Release);
                    // SAFETY: just published; freed only at drop
                    &unsafe { &*n }.rs
                }
            },
        };
        rs.insert(h, rule);
        sd.version.fetch_add(1, Relaxed);
    }

    /// Visit every handle the definitions hold, and free what definitions retired (a safe point).
    pub(crate) fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.reclaim();
        self.syms.for_each_mut(|sd| {
            for bits in [sd.own.get_mut(), sd.identity.get_mut()] {
                if *bits != HOLE {
                    let mut r = Raw::from_bits(*bits);
                    visit(&mut r);
                    *bits = r.to_bits();
                }
            }
            sd.for_each_rule_set(|rs| rs.trace(visit));
        });
    }

    /// Free everything retired by definitions (a safe point). Returns how many allocations were freed.
    pub(crate) fn reclaim(&mut self) -> usize {
        let mut n = 0;
        self.syms.for_each_mut(|sd| sd.for_each_rule_set(|rs| n += rs.reclaim()));
        n
    }
}
