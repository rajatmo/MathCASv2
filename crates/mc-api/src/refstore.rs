//! A simple reference implementation of [`Store`]: append-only, one lock, no collection. It exists so that generic
//! crates can be built and tested before `mc-core`, and as the reference the real store is checked against. It is not
//! fast and never frees anything.
//!
//! It packs by the rule of [`crate::packed`]: a packable `list(...)` is stored as the compound term (so its view is
//! the stored arguments) with a copy of its leaves as the payload [`Store::packed`] returns.
use crate::packed::{pack_shape, words_ints, words_reals, Elem};
use crate::{names, Packed, Store, Sym, View, T};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Int(i64),
    BigInt(bool, Vec<u64>),
    Rational(u32, u32),
    /// bit pattern, so that -0.0, 0.0 and NaNs are distinct terms (structural identity of machine reals)
    Real(u64),
    Ball(bool, Vec<u64>),
    Str(String),
    Sym(u32),
    App(u32, Vec<u32>),
}

/// Node payloads live in boxes that never move once created, so views can borrow them for the store's lifetime.
struct Node {
    key: Box<Key>,
    args: Box<[u32]>,
    hash: u64,
    /// the packed payload of a packable list: leaf kind, dimensions, leaves
    packed: Option<Box<Payload>>,
}

type Payload = (Elem, Vec<u64>, Vec<u64>);

#[derive(Default)]
struct Inner {
    nodes: Vec<Node>,
    index: HashMap<Key, u32>,
    syms: Vec<String>,
    sym_index: HashMap<String, u32>,
}

/// The reference store.
#[derive(Default)]
pub struct RefStore {
    inner: Mutex<Inner>,
}

impl RefStore {
    pub fn new() -> RefStore {
        RefStore::default()
    }

    fn intern<'h>(&'h self, key: Key) -> T<'h> {
        self.intern_packed(key, None)
    }

    fn intern_packed<'h>(&'h self, key: Key, packed: Option<Payload>) -> T<'h> {
        let mut g = self.inner.lock();
        if let Some(&id) = g.index.get(&key) {
            return T::from_raw(id);
        }
        let hash = match &key {
            Key::App(h, args) => {
                let mut x = 0x9E37_79B9_7F4A_7C15u64 ^ g.nodes[*h as usize].hash;
                for a in args {
                    x = (x.rotate_left(5) ^ g.nodes[*a as usize].hash).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                }
                x
            }
            k => {
                let mut s = std::collections::hash_map::DefaultHasher::new();
                k.hash(&mut s);
                s.finish()
            }
        };
        let args: Box<[u32]> = match &key {
            Key::App(_, a) => a.clone().into_boxed_slice(),
            _ => Box::new([]),
        };
        let id = g.nodes.len() as u32;
        g.nodes.push(Node { key: Box::new(key.clone()), args, hash, packed: packed.map(Box::new) });
        g.index.insert(key, id);
        T::from_raw(id)
    }

    fn node<'h>(&'h self, t: T<'h>) -> (&'h Key, &'h [u32], u64) {
        let g = self.inner.lock();
        let n = &g.nodes[t.raw() as usize];
        let (k, a, h): (*const Key, *const [u32], u64) = (&*n.key, &*n.args, n.hash);
        // SAFETY: nodes are append-only and their boxes never move or drop while the store lives, so the pointees
        // outlive the lock guard for the store's lifetime 'h.
        unsafe { (&*k, &*a, h) }
    }
}

impl Store for RefStore {
    fn sym(&self, name: &str) -> Sym {
        let mut g = self.inner.lock();
        if let Some(&s) = g.sym_index.get(name) {
            return Sym(s);
        }
        let s = g.syms.len() as u32;
        g.syms.push(name.to_string());
        g.sym_index.insert(name.to_string(), s);
        Sym(s)
    }
    fn sym_name(&self, s: Sym) -> String {
        self.inner.lock().syms[s.0 as usize].clone()
    }
    fn sym_term<'h>(&'h self, s: Sym) -> T<'h> {
        self.intern(Key::Sym(s.0))
    }
    fn int<'h>(&'h self, v: i64) -> T<'h> {
        self.intern(Key::Int(v))
    }
    fn big_int<'h>(&'h self, neg: bool, mag: &[u64]) -> T<'h> {
        let mut m = mag.to_vec();
        while m.last() == Some(&0) {
            m.pop();
        }
        match m.len() {
            0 => self.int(0),
            1 if m[0] <= i64::MAX as u64 => self.int(if neg { -(m[0] as i64) } else { m[0] as i64 }),
            1 if neg && m[0] == 1u64 << 63 => self.int(i64::MIN),
            _ => self.intern(Key::BigInt(neg, m)),
        }
    }
    fn rational<'h>(&'h self, num: T<'h>, den: T<'h>) -> T<'h> {
        debug_assert!(matches!(self.view(den), View::Int(d) if d > 1) || matches!(self.view(den), View::BigInt { neg: false, .. }));
        self.intern(Key::Rational(num.raw(), den.raw()))
    }
    fn real<'h>(&'h self, v: f64) -> T<'h> {
        self.intern(Key::Real(v.to_bits()))
    }
    fn ball<'h>(&'h self, neg: bool, words: &[u64]) -> T<'h> {
        self.intern(Key::Ball(neg, words.to_vec()))
    }
    fn string<'h>(&'h self, s: &str) -> T<'h> {
        self.intern(Key::Str(s.to_string()))
    }
    fn app<'h>(&'h self, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        let packed = if head == self.sym_term(self.sym(names::LIST)) { pack_shape(self, args) } else { None };
        self.intern_packed(Key::App(head.raw(), args.iter().map(|a| a.raw()).collect()), packed)
    }
    fn view<'h>(&'h self, t: T<'h>) -> View<'h> {
        let (k, args, _) = self.node(t);
        match k {
            Key::Int(v) => View::Int(*v),
            Key::BigInt(neg, m) => View::BigInt { neg: *neg, mag: m },
            Key::Rational(n, d) => View::Rational { num: T::from_raw(*n), den: T::from_raw(*d) },
            Key::Real(b) => View::Real(f64::from_bits(*b)),
            Key::Ball(neg, w) => View::Ball { neg: *neg, words: w },
            Key::Str(s) => View::Str(s),
            Key::Sym(s) => View::Sym(Sym(*s)),
            Key::App(h, _) => {
                // SAFETY: T is a transparent wrapper of u32 plus a zero-sized brand.
                let args: &'h [T<'h>] = unsafe { std::slice::from_raw_parts(args.as_ptr() as *const T<'h>, args.len()) };
                View::App { head: T::from_raw(*h), args }
            }
        }
    }
    fn structural_hash(&self, t: T<'_>) -> u64 {
        let g = self.inner.lock();
        g.nodes[t.raw() as usize].hash
    }
    fn packed<'h>(&'h self, t: T<'h>) -> Option<Packed<'h>> {
        let g = self.inner.lock();
        let p: *const Payload = &**g.nodes[t.raw() as usize].packed.as_ref()?;
        drop(g);
        // SAFETY: as in `node`: the payload box is never moved or dropped while the store lives.
        let (elem, dims, data) = unsafe { &*p };
        Some(match elem {
            Elem::Real => Packed::Reals { dims, data: words_reals(data) },
            Elem::Int => Packed::Ints { dims, data: words_ints(data) },
            Elem::Complex => Packed::Complexes { dims, data: words_reals(data) },
        })
    }
}
