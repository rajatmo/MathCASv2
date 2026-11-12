//! Helpers shared by the integration tests: a reproducible generator, a term printer and conversions to the
//! num-bigint model.
#![allow(dead_code)]
use mc_api::{Store, View, T};
use num_bigint::{BigInt, BigUint, Sign};

/// SplitMix64: reproducible test inputs.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
    pub fn shuffle<X>(&mut self, v: &mut [X]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            v.swap(i, j);
        }
    }
}

pub fn big_of_limbs(neg: bool, mag: &[u64]) -> BigInt {
    let mut v = vec![];
    for &l in mag {
        v.push(l as u32);
        v.push((l >> 32) as u32);
    }
    BigInt::from_biguint(if neg { Sign::Minus } else { Sign::Plus }, BigUint::new(v))
}

/// The integer value of a term, if it is an integer.
pub fn big_of<S: Store>(st: &S, t: T<'_>) -> Option<BigInt> {
    match st.view(t) {
        View::Int(v) => Some(BigInt::from(v)),
        View::BigInt { neg, mag } => Some(big_of_limbs(neg, mag)),
        _ => None,
    }
}

/// A term in the abstract notation of the semantics (test output only; recursion is fine on small terms).
pub fn show<S: Store>(st: &S, t: T<'_>) -> String {
    match st.view(t) {
        View::Int(v) => v.to_string(),
        View::BigInt { neg, mag } => big_of_limbs(neg, mag).to_string(),
        View::Rational { num, den } => format!("{}/{}", show(st, num), show(st, den)),
        View::Real(v) => format!("{v:?}"),
        View::Ball { neg, words } => format!("ball({}{words:?})", if neg { "-" } else { "" }),
        View::Str(s) => format!("{s:?}"),
        View::Sym(s) => st.sym_name(s),
        View::App { head, args } => {
            let a: Vec<String> = args.iter().map(|&x| show(st, x)).collect();
            format!("{}({})", show(st, head), a.join(", "))
        }
    }
}

/// The stores the tests run on: the model store of `mc-api` and the real store of `mc-core` (used from the test
/// thread, with no safe points, so nothing is collected).
pub enum TestStore {
    Ref(mc_api::RefStore),
    Core(Box<mc_core::Heap>),
}

impl TestStore {
    /// One store of each kind.
    pub fn all() -> Vec<TestStore> {
        vec![TestStore::Ref(mc_api::RefStore::new()), TestStore::Core(Box::new(mc_core::Heap::new(mc_core::Config::no_collect())))]
    }
    /// A new, empty store of the same kind.
    pub fn fresh(&self) -> TestStore {
        match self {
            TestStore::Ref(_) => TestStore::Ref(mc_api::RefStore::new()),
            TestStore::Core(_) => TestStore::Core(Box::new(mc_core::Heap::new(mc_core::Config::no_collect()))),
        }
    }
}

macro_rules! delegate {
    ($self:ident, $s:ident => $e:expr) => {
        match $self {
            TestStore::Ref($s) => $e,
            TestStore::Core($s) => $e,
        }
    };
}

impl Store for TestStore {
    fn sym(&self, name: &str) -> mc_api::Sym {
        delegate!(self, s => s.sym(name))
    }
    fn sym_name(&self, x: mc_api::Sym) -> String {
        delegate!(self, s => s.sym_name(x))
    }
    fn sym_term<'h>(&'h self, x: mc_api::Sym) -> T<'h> {
        delegate!(self, s => s.sym_term(x))
    }
    fn int<'h>(&'h self, v: i64) -> T<'h> {
        delegate!(self, s => s.int(v))
    }
    fn big_int<'h>(&'h self, neg: bool, mag: &[u64]) -> T<'h> {
        delegate!(self, s => s.big_int(neg, mag))
    }
    fn rational<'h>(&'h self, num: T<'h>, den: T<'h>) -> T<'h> {
        delegate!(self, s => s.rational(num, den))
    }
    fn real<'h>(&'h self, v: f64) -> T<'h> {
        delegate!(self, s => s.real(v))
    }
    fn ball<'h>(&'h self, neg: bool, words: &[u64]) -> T<'h> {
        delegate!(self, s => s.ball(neg, words))
    }
    fn string<'h>(&'h self, x: &str) -> T<'h> {
        delegate!(self, s => s.string(x))
    }
    fn app<'h>(&'h self, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        delegate!(self, s => s.app(head, args))
    }
    fn view<'h>(&'h self, t: T<'h>) -> View<'h> {
        delegate!(self, s => s.view(t))
    }
    fn structural_hash(&self, t: T<'_>) -> u64 {
        delegate!(self, s => s.structural_hash(t))
    }
}
