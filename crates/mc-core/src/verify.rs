//! Heap invariants (I1's and I3's stress checks), verified before and after every collection in stress mode.
use crate::handle::{is_imm, is_old, is_sym, is_young, LOW, SMALL_MAX, SMALL_MIN, YOUNG};
use crate::heap::{is_small_int, same};
use crate::hash::packed_hash;
use mc_api::packed::MAX_RANK;
use mc_api::PACK_MIN;
use crate::young::Body;
use crate::node::{box_bytes, for_each_child, Kind, Node, Region};
use crate::roots::{Raw, Trace};
use crate::Heap;
use mc_api::{Store, View, T};
use rustc_hash::FxHashSet;

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl Heap {
    /// Check every invariant of the heap at a safe point; panics with a description on the first violation.
    ///
    /// - Both regions: every node is in exactly one interning table and is found again there by its content; its
    ///   children are valid handles; numbers are canonical (a `Word` never holds an immediate value, an integer
    ///   payload has at least two limbs and a nonzero top limb, a rational has a nonzero integer numerator and an
    ///   integer denominator above 1, reduced when both fit a word; strings are UTF-8 with zero padding; a packed
    ///   array has consistent dimensions, at least the packing threshold of leaves, and the hash and mask of the
    ///   compound term it stands for).
    /// - Old region: no child is young and every child id is in range; argument slots and arena payload words are
    ///   contiguous in id order with no slack; boxed bytes add up; the old memo is parallel to the nodes, each valid
    ///   entry keyed by its own id with an old or immediate value in range.
    /// - Young region: no young node is in the old table; the box lists hold exactly the young boxed nodes and their
    ///   bytes add up; young memo entries are keyed by their own id; side-memo entries have an old key and a valid
    ///   value.
    /// - Roots are valid handles.
    pub fn verify(&mut self, roots: &mut [&mut dyn Trace]) {
        self.verify_old();
        self.verify_young();
        for r in roots.iter_mut() {
            r.trace(&mut |x: &mut Raw| self.check_handle(x.0, "root"));
        }
    }

    fn check_handle(&self, b: u32, what: &str) {
        if is_imm(b) {
            assert!(!is_sym(b) || ((b & LOW) as usize) < self.syms.len(), "{what}: unknown symbol {b:#x}");
        } else if is_young(b) {
            assert!(self.young.valid(b & LOW), "{what}: young id {b:#x} outside the current cycle");
        } else {
            assert!((b as usize) < self.old.nodes.len(), "{what}: old id {b} freed or moved away");
        }
    }

    fn is_integer(&self, b: u32) -> bool {
        is_small_int(b) || (!is_imm(b) && matches!(self.view(T::from_raw(b)), View::Int(_) | View::BigInt { .. }))
    }

    fn check_canonical<R: Region>(&self, r: &R, n: &Node, at: u32) {
        match n.kind() {
            Kind::App | Kind::Real => {}
            Kind::Word => {
                let neg = n.head != 0;
                assert!(n.head <= 1, "word {at:#x}: sign flag");
                let small = if neg { n.data <= SMALL_MIN.unsigned_abs() } else { n.data <= SMALL_MAX as u64 };
                assert!(!small, "word {at:#x} holds an immediate value");
            }
            Kind::Int => {
                let w = r.words(n);
                assert!(n.head <= 1, "integer {at:#x}: sign flag");
                assert!(w.len() >= 2 && *w.last().unwrap() != 0, "integer payload {at:#x} not canonical");
            }
            Kind::Ball => assert!(n.len() >= 3 && n.head <= 1, "ball {at:#x}: header"),
            Kind::Str => {
                let w = r.words(n);
                let bytes = n.head as usize;
                assert_eq!(w.len(), bytes.div_ceil(8), "string {at:#x}: length");
                let all: Vec<u8> = w.iter().flat_map(|x| x.to_ne_bytes()).collect();
                assert!(std::str::from_utf8(&all[..bytes]).is_ok(), "string {at:#x}: not UTF-8");
                assert!(all[bytes..].iter().all(|&c| c == 0), "string {at:#x}: padding");
            }
            Kind::Packed => {
                let w = r.words(n);
                let (elem, rank) = crate::heap::packed_kind(n.head);
                assert!((1..=MAX_RANK).contains(&rank) && rank <= w.len(), "packed {at:#x}: rank {rank}");
                let (dims, data) = w.split_at(rank);
                assert!(dims.iter().all(|&d| d >= 1), "packed {at:#x}: an empty dimension");
                let leaves = data.len() / elem.words();
                assert_eq!(dims.iter().product::<u64>() as usize * elem.words(), data.len(), "packed {at:#x}: dimensions and leaves");
                assert!(leaves >= PACK_MIN, "packed {at:#x}: fewer leaves than the packing threshold");
                assert!(elem != mc_api::Elem::Complex || self.complex_sym() != crate::handle::HOLE, "packed {at:#x}: complex leaves without the symbol");
                let want = packed_hash(self.meta_bits(self.list()), self.complex_meta(elem), elem, dims, data);
                assert_eq!((n.hash, n.mask), want, "packed {at:#x}: hash and mask of the compound term");
            }
            Kind::Rat => {
                let (num, den) = (n.head, n.data as u32);
                assert!(self.is_integer(num) && self.is_integer(den), "rational {at:#x}: parts are not integers");
                let (vn, vd) = (self.view(T::from_raw(num)), self.view(T::from_raw(den)));
                assert!(!matches!(vn, View::Int(0)), "rational {at:#x}: zero numerator");
                match vd {
                    View::Int(d) => assert!(d > 1, "rational {at:#x}: denominator not above 1"),
                    View::BigInt { neg, .. } => assert!(!neg, "rational {at:#x}: negative denominator"),
                    _ => unreachable!(),
                }
                if let (View::Int(a), View::Int(d)) = (vn, vd) {
                    assert_eq!(gcd(a.unsigned_abs(), d as u64), 1, "rational {at:#x} not reduced");
                }
            }
        }
    }

    fn verify_old(&self) {
        let old = &self.old;
        let n = old.nodes.len();
        assert_eq!(old.table.count, n, "old table out of step with the old region");
        assert_eq!(self.omemo.0.len(), n, "old memo not parallel to the old region");
        let (mut next_arg, mut next_limb, mut boxed) = (0usize, 0usize, 0usize);
        for (i, node) in old.nodes.iter().enumerate() {
            let at = i as u32;
            match node.kind() {
                Kind::App => {
                    assert_eq!(node.data as usize, next_arg, "old arguments not contiguous at {i}");
                    next_arg += node.len();
                }
                _ if node.in_arena() => {
                    assert_eq!(node.data as usize, next_limb, "old payload words not contiguous at {i}");
                    next_limb += node.len();
                }
                _ if node.is_boxed() => boxed += box_bytes(node.len()),
                _ => {}
            }
            for_each_child(old, node, |c| {
                assert!(!is_young(c), "old node {i} refers to a young term");
                self.check_handle(c, "old child");
            });
            self.check_canonical(old, node, at);
            let body = body_of(old, node);
            let found = old.table.find(node.hash, |o| same(old, old.node(o), node, body));
            assert_eq!(found, Some(at), "old node {i} not found by its content");
            if let Some(it) = self.omemo.0[i].load() {
                assert_eq!(it.key, at, "old memo entry {i} keyed by another id");
                assert!(!is_young(it.val), "old memo entry {i} holds a young value");
                self.check_handle(it.val, "old memo value");
            }
        }
        assert_eq!(next_arg, old.args.len(), "old argument array has slack");
        assert_eq!(next_limb, old.limbs.len(), "old limb array has slack");
        assert_eq!(boxed, old.boxed, "old boxed bytes out of step");
    }

    fn verify_young(&mut self) {
        let mut ids = vec![];
        self.ytable.for_each(|id| ids.push(id));
        assert_eq!(ids.len(), self.ytable.len(), "young table count");
        let listed: FxHashSet<u32> = self.young.boxed_ids().into_iter().collect();
        let (mut boxed, mut boxed_n) = (0usize, 0usize);
        for &id in &ids {
            assert!(self.young.valid(id), "young table entry {id} outside the cycle");
            let node = *self.young.node(id);
            let at = YOUNG | id;
            for_each_child(&self.young, &node, |c| self.check_handle(c, "young child"));
            self.check_canonical(&self.young, &node, at);
            let body = body_of(&self.young, &node);
            let found = self.ytable.find(node.hash, |o| same(&self.young, self.young.node(o), &node, body));
            assert_eq!(found, Some(id), "young node {id} not found by its content");
            if self.old.table.count > 0 {
                let in_old = self.old.table.find(node.hash, |o| same(&self.old, self.old.node(o), &node, body));
                assert!(in_old.is_none(), "young node {id} is also in the old table");
            }
            if node.is_boxed() {
                assert!(listed.contains(&id), "young boxed node {id} missing from the box lists");
                boxed += box_bytes(node.len());
                boxed_n += 1;
            }
            if let Some(it) = self.ymemo.entry(id).and_then(|e| e.load()) {
                assert_eq!(it.key, at, "young memo entry {id} keyed by another id");
                self.check_handle(it.val, "young memo value");
            }
        }
        assert_eq!(listed.len(), boxed_n, "box lists hold nodes that are not young boxed nodes");
        assert_eq!(boxed, self.young.boxed_bytes(), "young boxed bytes out of step");
        self.side.for_each(|it| {
            assert!(is_old(it.key) && (it.key as usize) < self.old.nodes.len(), "side memo key not old");
            self.check_handle(it.val, "side memo value");
        });
    }
}

/// The lookup body of an existing node (for finding it again by content).
fn body_of<'a, R: Region>(r: &'a R, n: &Node) -> Body<'a> {
    match n.kind() {
        Kind::App => Body::Args(r.args(n)),
        _ if n.is_payload() => Body::Words(r.words(n)),
        _ => Body::Inline,
    }
}

