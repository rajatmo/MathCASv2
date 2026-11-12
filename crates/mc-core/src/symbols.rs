//! The symbol table. Symbols are immediates (1b.3): a symbol's handle is its index, so symbols never move and may be
//! held anywhere. Names are interned under a read-mostly lock; each symbol's structural hash sits in a lock-free
//! array, since every compound construction reads its head's and arguments' hashes.
use crate::hash::sym_hash;
use crate::segs::ZeroSegs;
use mc_api::Sym;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicU64, Ordering::*};

/// Symbol indices occupy the low 30 bits of an immediate.
const SYM_BITS: u32 = 30;

#[derive(Default)]
struct Names {
    by_name: FxHashMap<Box<str>, u32>,
    names: Vec<Box<str>>,
}

pub(crate) struct Symbols {
    names: RwLock<Names>,
    hashes: ZeroSegs<AtomicU64>,
}

impl Symbols {
    pub(crate) fn new() -> Symbols {
        Symbols { names: RwLock::new(Names::default()), hashes: ZeroSegs::new(14, SYM_BITS) }
    }

    /// The symbol with this name, created on first use.
    pub(crate) fn intern(&self, name: &str) -> Sym {
        if let Some(&i) = self.names.read().by_name.get(name) {
            return Sym(i);
        }
        let mut g = self.names.write();
        if let Some(&i) = g.by_name.get(name) {
            return Sym(i);
        }
        let i = g.names.len() as u32;
        // invariant: 2^30 symbols need tens of GB of names: memory runs out first
        assert!(i < (1 << SYM_BITS) - 1, "too many symbols");
        // published before the symbol is: a thread can only use the symbol after `intern` returned it
        self.hashes.get_or_alloc(i as usize).store(sym_hash(name), Release);
        g.by_name.insert(name.into(), i);
        g.names.push(name.into());
        Sym(i)
    }

    /// The name of a symbol. Panics on a symbol this table did not make.
    pub(crate) fn name(&self, s: Sym) -> String {
        self.names.read().names[s.0 as usize].to_string()
    }

    /// Number of symbols.
    pub(crate) fn len(&self) -> usize {
        self.names.read().names.len()
    }

    /// The structural hash of symbol index `i`.
    #[inline]
    pub(crate) fn hash(&self, i: u32) -> u64 {
        // invariant: symbol indices come from this table (`Sym` values are made by `sym`)
        self.hashes.get(i as usize).expect("unknown symbol").load(Acquire)
    }
}
