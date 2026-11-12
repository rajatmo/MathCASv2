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
    /// associative only (`flat`): nested calls are spliced in, order is kept, and a pattern argument matches a run of
    /// arguments wrapped in the head (R22-13); an AC head is flat as well, by `ac`
    pub flat: bool,
    /// the function performs effects (semantics 9): calls are never evaluated speculatively ahead of their turn
    /// (used by the granularity estimate)
    pub effect: bool,
    /// the function receives failures as arguments instead of returning the first one (semantics 10)
    pub handles_failures: bool,
    /// the function maps over list arguments of equal length, broadcasting the others (semantics 7.2)
    pub listable: bool,
    /// the function reads the assumption context (semantics 13): its calls depend on it (see [`crate::assume`])
    pub uses_assumptions: bool,
    /// the symbol's definitions may not be changed: assignments, rule definitions, `unset` and attribute changes
    /// other than `unprotect` give `failure(protected, f)` (decision r17fix.4)
    pub protected: bool,
}

const A_AC: u64 = 1 << 3;
const A_EFFECT: u64 = 1 << 4;
const A_FAILURES: u64 = 1 << 5;
const A_LISTABLE: u64 = 1 << 6;
const A_ASSUMPTIONS: u64 = 1 << 7;
const A_PROTECTED: u64 = 1 << 8;
const A_FLAT: u64 = 1 << 9;

impl Attributes {
    fn bits(self) -> u64 {
        self.hold.bits()
            | if self.ac { A_AC } else { 0 }
            | if self.flat { A_FLAT } else { 0 }
            | if self.effect { A_EFFECT } else { 0 }
            | if self.handles_failures { A_FAILURES } else { 0 }
            | if self.listable { A_LISTABLE } else { 0 }
            | if self.uses_assumptions { A_ASSUMPTIONS } else { 0 }
            | if self.protected { A_PROTECTED } else { 0 }
    }
    pub(crate) fn from_bits(b: u64) -> Attributes {
        Attributes {
            hold: Hold::from_bits(b),
            ac: b & A_AC != 0,
            flat: b & A_FLAT != 0,
            effect: b & A_EFFECT != 0,
            handles_failures: b & A_FAILURES != 0,
            listable: b & A_LISTABLE != 0,
            uses_assumptions: b & A_ASSUMPTIONS != 0,
            protected: b & A_PROTECTED != 0,
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
    /// index + 1 of the symbol's compiled specialisation in the kernel's table (0: none; [`crate::Compiled`])
    pub(crate) compiled: AtomicU32,
    /// the symbol's own rule set (null: none); replaced whole by [`Defs::unset`], the old one retired
    rules: AtomicPtr<RuleSet>,
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
            compiled: AtomicU32::new(0),
            rules: AtomicPtr::new(null_mut()),
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
        let p = self.rules.load(Acquire);
        // SAFETY: rule sets are published complete (release) and freed only with `&mut` (retired ones at a safe
        // point, the current one at drop)
        (!p.is_null()).then(|| unsafe { &*p })
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
            // SAFETY: nodes are published complete (release) and freed only with `&mut` (at drop, or at the safe point
            // after `unset` retired them)
            let n = unsafe { &*p };
            if n.outer == outer {
                return Some(&n.rs);
            }
            p = n.next;
        }
        None
    }

    /// The outer heads the symbol holds argument-head rules for, oldest first.
    pub(crate) fn arg_outers(&self) -> Vec<Sym> {
        let mut out = vec![];
        let mut p = self.args.load(Acquire);
        while !p.is_null() {
            // SAFETY: as in `arg_rules`
            let n = unsafe { &*p };
            out.push(n.outer);
            p = n.next;
        }
        out.reverse();
        out
    }

    fn for_each_rule_set(&mut self, mut f: impl FnMut(&mut RuleSet)) {
        let rs = *self.rules.get_mut();
        if !rs.is_null() {
            // SAFETY: `&mut self`: no reader; the rule set is live until replaced or dropped.
            f(unsafe { &mut *rs });
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
        let rs = *self.rules.get_mut();
        if !rs.is_null() {
            // SAFETY: the current rule set came from `Box::into_raw` and is owned by this symbol.
            drop(unsafe { Box::from_raw(rs) });
        }
        let mut p = *self.args.get_mut();
        while !p.is_null() {
            // SAFETY: every node came from `Box::into_raw` and is reachable once.
            let b = unsafe { Box::from_raw(p) };
            p = b.next;
        }
    }
}

// SAFETY: a node is owned by one list (or one retired list); its `next` pointer is only followed by readers of
// published lists and by `&mut` owners.
unsafe impl Send for ArgNode {}

/// Rule sets and argument-head lists replaced during an evaluation ([`Defs::unset`]): readers may still use them
/// until the next safe point, which frees them.
// The boxes are the point: readers hold references into these allocations, which must not move until freed.
#[allow(clippy::vec_box)]
#[derive(Default)]
struct Retired {
    rules: Vec<Box<RuleSet>>,
    args: Vec<Box<ArgNode>>,
}

/// The definitions of every symbol.
#[derive(Default)]
pub(crate) struct Defs {
    syms: SymTable<SymDefs>,
    /// Serialises writers (uncontended: only the frontier or the session defines).
    writer: Mutex<()>,
    retired: Mutex<Retired>,
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

    /// Link symbol `s` to compiled specialisation `slot` (index + 1; 0 unlinks). Not a definition: no version.
    pub(crate) fn set_compiled(&self, s: Sym, slot: u32) {
        let _w = self.writer.lock();
        self.syms.get_or_init(s.0).compiled.store(slot, Release);
    }

    /// Whether `s` has any definition: a native, an own value, attributes, an identity element or rules.
    pub(crate) fn is_defined(&self, s: Sym) -> bool {
        self.get(s).is_some_and(|d| {
            d.native.load(Relaxed) != 0
                || d.own.load(Relaxed) != HOLE
                || d.attrs.load(Relaxed) != 0
                || d.identity.load(Relaxed) != HOLE
                || d.rules().is_some_and(|r| r.len() > 0)
                || d.has_arg_rules()
        })
    }

    /// Whether `s` has a meaning of its own that evaluating its calls would apply: a native, an own value, an
    /// identity element or rules. Attributes alone (the engine gives `list` `handles_failures`) do not count: a
    /// packed array stays its own normal form under them.
    pub(crate) fn has_meaning(&self, s: Sym) -> bool {
        self.get(s).is_some_and(|d| {
            d.native.load(Relaxed) != 0
                || d.own.load(Relaxed) != HOLE
                || d.identity.load(Relaxed) != HOLE
                || d.rules().is_some_and(|r| r.len() > 0)
                || d.has_arg_rules()
        })
    }

    /// Remove everything defined for `s` except its native: own value, attributes, identity element, its own rules
    /// and the argument-head rules it holds, and its compiled specialisation (compiled from the rules). The version
    /// counter keeps counting (it never goes back).
    pub(crate) fn clear(&mut self, s: Sym) {
        if let Some(sd) = self.syms.get_mut(s.0) {
            let version = *sd.version.get_mut() + 1;
            let native = *sd.native.get_mut();
            *sd = SymDefs::default();
            *sd.version.get_mut() = version;
            *sd.native.get_mut() = native;
        }
    }

    /// Remove the own value, identity element, rules and held argument-head rules of `s` while readers may be
    /// using them (`unset(s)` during an evaluation, an effect of the frontier): each slot is emptied with a release
    /// store and what it held is retired until the next safe point. The native and the attributes stay (the
    /// attributes are the symbol's signature; `clear_attributes` removes them). Returns whether there was anything
    /// to remove (then the version counts a definition).
    pub(crate) fn unset(&self, s: Sym) -> bool {
        let _w = self.writer.lock();
        let Some(sd) = self.syms.get(s.0) else { return false };
        let mut any = false;
        any |= sd.own.swap(HOLE, AcqRel) != HOLE;
        any |= sd.identity.swap(HOLE, AcqRel) != HOLE;
        sd.compiled.store(0, Release);
        let rs = sd.rules.swap(null_mut(), AcqRel);
        let args = sd.args.swap(null_mut(), AcqRel);
        let mut r = self.retired.lock();
        if !rs.is_null() {
            // SAFETY: the set came from `Box::into_raw`; it is no longer reachable from the slot
            let b = unsafe { Box::from_raw(rs) };
            any |= b.len() > 0;
            r.rules.push(b);
        }
        let mut p = args;
        while !p.is_null() {
            any = true;
            // SAFETY: every node came from `Box::into_raw` and is reachable once, from the list just unlinked;
            // readers that loaded the old head keep following `next` pointers into nodes freed only at a safe point
            let b = unsafe { Box::from_raw(p) };
            p = b.next;
            r.args.push(b);
        }
        if any {
            sd.version.fetch_add(1, Relaxed);
        }
        any
    }

    /// Remove the rule of `s` whose left side is `lhs` up to renaming, while readers may be using the rule set
    /// (`unset(f(x_))`): a copy without it replaces the set, the old set is retired. Returns whether there was one.
    pub(crate) fn unset_rule(&self, h: &Heap, s: Sym, lhs: &crate::pat::Pat) -> bool {
        let _w = self.writer.lock();
        let Some(sd) = self.syms.get(s.0) else { return false };
        let Some(old) = sd.rules() else { return false };
        let Some(new) = old.without(h, lhs) else { return false };
        sd.compiled.store(0, Release);
        let p = sd.rules.swap(Box::into_raw(Box::new(new)), AcqRel);
        // SAFETY: as in `unset`
        self.retired.lock().rules.push(unsafe { Box::from_raw(p) });
        sd.version.fetch_add(1, Relaxed);
        true
    }

    /// Remove the argument-head rules `holder` holds for outer head `outer`, keeping its others in order. Returns
    /// whether there were any (then the version counter counts a definition).
    pub(crate) fn clear_arg_rules(&mut self, holder: Sym, outer: Sym) -> bool {
        let Some(sd) = self.syms.get_mut(holder.0) else { return false };
        let mut nodes = Vec::new();
        let mut p = *sd.args.get_mut();
        while !p.is_null() {
            // SAFETY: `&mut self`: no reader; every node came from `Box::into_raw` and is reachable once.
            let b = unsafe { Box::from_raw(p) };
            p = b.next;
            nodes.push(b);
        }
        let before = nodes.len();
        nodes.retain(|n| n.outer != outer);
        let found = nodes.len() < before;
        // relink in the original order
        let mut head = null_mut();
        for mut n in nodes.into_iter().rev() {
            n.next = head;
            head = Box::into_raw(n);
        }
        *sd.args.get_mut() = head;
        if found {
            *sd.version.get_mut() += 1;
        }
        found
    }

    /// Add `rule` to the rules `owner` holds: its own (`outer == None`) or its argument-head rules for `outer`.
    pub(crate) fn define(&self, h: &Heap, owner: Sym, outer: Option<Sym>, ac_top: bool, rule: Rule) {
        let _w = self.writer.lock();
        let sd = self.syms.get_or_init(owner.0);
        let rs = match outer {
            None => {
                let mut p = sd.rules.load(Relaxed);
                if p.is_null() {
                    p = Box::into_raw(Box::new(RuleSet::new(ac_top)));
                    sd.rules.store(p, Release);
                }
                // SAFETY: published above or earlier; freed only with `&mut self`
                unsafe { &*p }
            }
            Some(o) => match sd.arg_rules(o) {
                Some(rs) => rs,
                None => {
                    let n = Box::into_raw(Box::new(ArgNode { outer: o, rs: RuleSet::new(ac_top), next: sd.args.load(Relaxed) }));
                    sd.args.store(n, Release);
                    // SAFETY: just published; freed only with `&mut` (drop or a safe point)
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
        let r = self.retired.get_mut();
        let mut n = r.rules.len() + r.args.len();
        r.rules.clear();
        r.args.clear();
        self.syms.for_each_mut(|sd| sd.for_each_rule_set(|rs| n += rs.reclaim()));
        n
    }
}

impl crate::eval::Kernel {
    /// Remove the argument-head rules `holder` holds for the outer head `outer` (the rules `@on(holder) rule
    /// outer(...) := ...` attached), between evaluations; other definitions of both symbols stay. Used when a module
    /// that attached such rules to a symbol it does not own is loaded again, so that its old rules go
    /// ([`Kernel::clear`](crate::eval::Kernel::clear) covers only the module's own members). Counts as a definition
    /// of both symbols (memoised results that read them are stale).
    pub fn clear_attached(&mut self, holder: Sym, outer: Sym) {
        let e = self.epoch();
        self.defs.clear_arg_rules(holder, outer);
        self.defined(e, crate::eval::bit(holder) | crate::eval::bit(outer), || ());
    }
}
