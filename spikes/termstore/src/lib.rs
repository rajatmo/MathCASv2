//! Term-store spike (plan section 5, spike 1).
//!
//! Two representations of the same terms behind one interface, so that every workload runs unchanged on both:
//! - [`tree::TreeStore`]: a reference-counted tree in the style of v1 (symbols as shared strings, structural equality
//!   and hashing by traversal, no sharing between equal subterms);
//! - [`hc::HcStore`]: hash-consed terms in an arena (each distinct term stored once, O(1) equality and hash, per-term
//!   symbol masks, and normal-form stamps with memoisation, semantics 4.3 and 4.4).
//!
//! Terms are integers, symbols and compound terms whose head is a symbol (enough for the workloads; general heads
//! do not change the comparison).
pub mod hc;
pub mod tree;
pub mod work;

/// A read-only view of one term.
pub enum View<'a, T, S> {
    Int(i64),
    Sym(&'a S),
    App(&'a S, &'a [T]),
}

pub trait Store {
    /// A term handle.
    type T: Clone;
    /// A symbol handle (used as a head and to test heads).
    type S: Clone;

    fn name(&self) -> &'static str;
    fn sym(&mut self, name: &str) -> Self::S;
    fn sym_term(&mut self, s: &Self::S) -> Self::T;
    fn int(&mut self, v: i64) -> Self::T;
    fn app(&mut self, head: &Self::S, args: Vec<Self::T>) -> Self::T;
    fn view<'a>(&'a self, t: &'a Self::T) -> View<'a, Self::T, Self::S>;
    /// Structural identity (`===`).
    fn same(&self, a: &Self::T, b: &Self::T) -> bool;
    /// A hash consistent with `same`.
    fn hash_term(&self, t: &Self::T) -> u64;
    /// Is `s` this symbol?
    fn sym_is(&self, s: &Self::S, other: &Self::S) -> bool;
    fn sym_name(&self, s: &Self::S) -> String;

    // Normal-form stamps and memoisation (semantics 4.3, 4.4). Stores without them evaluate everything again.
    fn has_stamps(&self) -> bool {
        false
    }
    /// The memoised normal form of `t`, if still valid.
    fn memo_get(&self, _t: &Self::T) -> Option<Self::T> {
        None
    }
    fn memo_put(&mut self, _t: &Self::T, _nf: &Self::T) {}
    /// A definition of `s` changed: invalidate what depends on it.
    fn redefine(&mut self, _s: &Self::S) {}

    fn head_is(&self, t: &Self::T, s: &Self::S) -> bool {
        match self.view(t) {
            View::App(h, _) => self.sym_is(h, s),
            _ => false,
        }
    }
    fn as_int(&self, t: &Self::T) -> Option<i64> {
        match self.view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }
}

/// A structural fingerprint independent of the store, used to check that both stores compute the same terms.
/// Iterative, so deep terms do not overflow the stack.
pub fn fingerprint<St: Store>(st: &St, t: &St::T) -> u64 {
    const K: u64 = 0x9E37_79B9_7F4A_7C15;
    // post-order: (term, next child index) with a stack of partial hashes
    enum Frame<'a, T> {
        Enter(&'a T),
        Exit(u64, usize),
    }
    let mut work: Vec<Frame<St::T>> = vec![Frame::Enter(t)];
    let mut vals: Vec<u64> = vec![];
    while let Some(f) = work.pop() {
        match f {
            Frame::Enter(t) => match st.view(t) {
                View::Int(v) => vals.push((v as u64).wrapping_mul(K) ^ 1),
                View::Sym(s) => vals.push(str_hash(&st.sym_name(s)) ^ 2),
                View::App(h, args) => {
                    work.push(Frame::Exit(str_hash(&st.sym_name(h)), args.len()));
                    for a in args.iter().rev() {
                        work.push(Frame::Enter(a));
                    }
                }
            },
            Frame::Exit(hh, n) => {
                let start = vals.len() - n;
                let mut h = hh.wrapping_mul(K) ^ 3;
                for v in vals.drain(start..) {
                    h = (h.rotate_left(5) ^ v).wrapping_mul(K);
                }
                vals.push(h);
            }
        }
    }
    vals.pop().unwrap()
}

pub fn str_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Deterministic pseudo-random numbers for building workloads (xorshift64*).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}
