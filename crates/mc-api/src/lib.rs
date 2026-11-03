//! The interface contract of the MathCAS v2 core (docs/design/05-assembly.md section 3).
//!
//! Generic crates (`mc-arith`, `mc-syntax`, `mc-bench`) are written against [`Store`] and tested against the simple
//! [`RefStore`] in this crate; `mc-core` implements the same trait with the concurrent, collected arena. The
//! [`conformance`] checks state what every implementation must satisfy.
//!
//! Changes to this crate go through the coordinator only.
use std::marker::PhantomData;

pub mod conformance;
mod refstore;
pub use refstore::RefStore;

/// A term handle: 32 bits, `Copy`, branded with the lifetime of the store borrow that produced it. Equal terms are
/// the same handle (hash-consing), so `==` is structural identity (semantics 1.1). A handle must not outlive its
/// borrow; terms kept across evaluations live in the store's root containers.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct T<'h> {
    raw: u32,
    _brand: PhantomData<&'h ()>,
}

impl<'h> T<'h> {
    /// For store implementations only.
    pub fn from_raw(raw: u32) -> T<'h> {
        T { raw, _brand: PhantomData }
    }
    /// For store implementations only.
    pub fn raw(self) -> u32 {
        self.raw
    }
}

/// A symbol. Symbols are immediates: they never move and may be held anywhere without rooting.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Sym(pub u32);

/// A read-only view of one term, borrowed from the store for `'h`.
#[derive(Clone, Copy, Debug)]
pub enum View<'h> {
    /// An integer that fits in `i64`.
    Int(i64),
    /// An integer that does not fit in `i64`: sign and magnitude limbs (64-bit, least significant first, top limb
    /// nonzero, at least two limbs or a magnitude above `i64::MAX`).
    BigInt { neg: bool, mag: &'h [u64] },
    /// A rational in canonical form: reduced, denominator greater than 1 (semantics 2.4).
    Rational { num: T<'h>, den: T<'h> },
    /// A machine real (IEEE binary64).
    Real(f64),
    /// A ball (semantics 2.3): sign and payload words `[exp, radius mantissa, radius exponent, mantissa limbs...]`
    /// (integration decision I3.5).
    Ball { neg: bool, words: &'h [u64] },
    Str(&'h str),
    Sym(Sym),
    /// A compound term: head (any term) and arguments.
    App { head: T<'h>, args: &'h [T<'h>] },
}

/// The store: construction and inspection of terms. All methods take `&self`, so terms can be built from many
/// threads at once (integration decision I2.1).
pub trait Store: Sync {
    /// The symbol with this name, created on first use.
    fn sym(&self, name: &str) -> Sym;
    fn sym_name(&self, s: Sym) -> String;
    /// The term of a symbol.
    fn sym_term<'h>(&'h self, s: Sym) -> T<'h>;

    fn int<'h>(&'h self, v: i64) -> T<'h>;
    /// An integer from sign and magnitude limbs (least significant first, leading zero limbs allowed). Canonical: an
    /// integer that fits in `i64` is always [`View::Int`] (integration decision I3.1).
    fn big_int<'h>(&'h self, neg: bool, mag: &[u64]) -> T<'h>;
    /// A rational from canonical parts. The caller (normally `mc-arith`) reduces first; implementations check this in
    /// debug builds.
    fn rational<'h>(&'h self, num: T<'h>, den: T<'h>) -> T<'h>;
    fn real<'h>(&'h self, v: f64) -> T<'h>;
    fn ball<'h>(&'h self, neg: bool, words: &[u64]) -> T<'h>;
    fn string<'h>(&'h self, s: &str) -> T<'h>;
    fn app<'h>(&'h self, head: T<'h>, args: &[T<'h>]) -> T<'h>;

    fn view<'h>(&'h self, t: T<'h>) -> View<'h>;
    /// A hash of the term's structure, consistent with identity and stable across collections.
    fn structural_hash(&self, t: T<'_>) -> u64;

    /// Convenience: `head(args)` with a symbol head.
    fn call<'h>(&'h self, head: Sym, args: &[T<'h>]) -> T<'h> {
        let h = self.sym_term(head);
        self.app(h, args)
    }
}

/// Symbol names fixed by the contract (semantics sections 1, 3, 10).
pub mod names {
    pub const LIST: &str = "list";
    pub const ADD: &str = "add";
    pub const MUL: &str = "mul";
    pub const POW: &str = "pow";
    pub const COMPLEX: &str = "complex";
    pub const FAILURE: &str = "failure";
    pub const RULE: &str = "rule";
    pub const RULE_DELAYED: &str = "rule_delayed";
    pub const TRUE: &str = "true";
    pub const FALSE: &str = "false";
    pub const NULL: &str = "null";
}
