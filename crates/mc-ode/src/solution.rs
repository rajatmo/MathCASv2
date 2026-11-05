//! Solutions and the `interpolating_function` terms that stand for them (decisions NUM.6 and N3.2).
//!
//! A solution's data (thousands of steps of dense output) is kept by the session in a [`Store`], keyed by a hash
//! of its content; the term is a short summary that names the key:
//!
//! ```text
//! interpolating_function("y", [0.0, 10.0], 123, "9c1f04e2b7a35d68")    // component, domain, steps, key
//! interpolating_function("y'", ...)                                     // a derivative
//! ```
//!
//! so results print compactly, compare and hash as terms, and evaluate without decoding lists. The key is a
//! function of the content, so equal solutions get equal terms at any thread count and memoised results stay
//! valid; the store keeps every solution for the session.
use crate::dense::Piecewise;
use crate::taylor::{self, Segment};
use mc_arith::num::Ball;
use mc_arith::Num;
use parking_lot::Mutex;
use rustc_hash::FxHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// An unknown of the solved system: its name, order and first state index.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Unknown {
    pub name: String,
    pub order: usize,
    pub at: usize,
}

/// The Taylor integrations under `digits:`: the fine one gives the values, the coarse one the error estimate.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Taylor {
    /// the working precision in bits
    pub p: u64,
    /// the precision of results in bits
    pub out: u64,
    pub fine: Vec<Segment>,
    pub coarse: Vec<Segment>,
}

/// The dense output.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Data {
    Machine(Piecewise),
    Taylor(Taylor),
}

/// A solution over `[lo, hi]`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Solution {
    pub unknowns: Vec<Unknown>,
    /// the size of the state
    pub n: usize,
    pub lo: f64,
    pub hi: f64,
    /// accepted steps
    pub steps: usize,
    pub data: Data,
}

/// A value of a solution.
pub(crate) enum Value {
    Machine(f64),
    Number(Num),
}

impl Solution {
    /// The state component and the further derivative order of a component name `y`, `y'`, `y''`, ...: a
    /// derivative below the unknown's order is a state component, a higher one the derivative of the dense
    /// output.
    pub fn component(&self, name: &str) -> Option<(usize, usize)> {
        let base = name.trim_end_matches('\'');
        let d = name.len() - base.len();
        let u = self.unknowns.iter().find(|u| u.name == base)?;
        Some(if d < u.order { (u.at + d, 0) } else { (u.at + u.order - 1, d + 1 - u.order) })
    }

    /// Component `i` (differentiated `order` times) at `x`; `None` outside the domain or for a non-real `x`.
    pub fn value(&self, i: usize, order: usize, x: &Num) -> Option<Value> {
        let xf = crate::util::real(x)?;
        if !(self.lo <= xf && xf <= self.hi) {
            return None;
        }
        match &self.data {
            Data::Machine(p) => p.value(i, order, xf).map(Value::Machine),
            Data::Taylor(tl) => {
                let xb = taylor::ball(x, tl.p)?;
                let f = taylor::eval(taylor::find(&tl.fine, xf)?, self.n, i, &xb, order, tl.p);
                let c = taylor::eval(taylor::find(&tl.coarse, xf)?, self.n, i, &xb, order, tl.p);
                let err = taylor::distance(&f, &c, tl.p);
                let b = Ball { rad: f.rad.add(&err), mid: f.mid };
                Some(Value::Number(taylor::num_of(b, tl.out)))
            }
        }
    }

    /// The content key.
    fn hash(&self, salt: u64) -> u64 {
        let mut s = FxHasher::default();
        salt.hash(&mut s);
        self.unknowns.hash(&mut s);
        (self.n, self.steps, self.lo.to_bits(), self.hi.to_bits()).hash(&mut s);
        match &self.data {
            Data::Machine(p) => p.words().for_each(|w| w.to_bits().hash(&mut s)),
            Data::Taylor(tl) => {
                (tl.p, tl.out).hash(&mut s);
                for seg in tl.fine.iter().chain(&tl.coarse) {
                    for b in std::iter::once(&seg.t0).chain([&seg.h]).chain(&seg.coefs) {
                        hash_ball(b, &mut s);
                    }
                }
            }
        }
        s.finish()
    }
}

fn hash_ball(b: &Ball, s: &mut FxHasher) {
    (b.mid.neg, &b.mid.man, b.mid.exp, b.rad.m, b.rad.e).hash(s);
}

/// The solutions of a session, by key.
#[derive(Default)]
pub(crate) struct Store {
    map: Mutex<HashMap<u64, Arc<Solution>>>,
}

impl Store {
    /// Keep `s`: the kept solution, its key (equal solutions share one; a hash collision with a different one takes
    /// the next salt) and whether it is new (no equal solution was kept before).
    pub fn insert(&self, s: Solution) -> (Arc<Solution>, u64, bool) {
        let mut map = self.map.lock();
        let mut salt = 0;
        loop {
            let key = s.hash(salt);
            match map.get(&key) {
                Some(old) if **old == s => return (old.clone(), key, false),
                Some(_) => salt += 1,
                None => {
                    let a = Arc::new(s);
                    map.insert(key, a.clone());
                    return (a, key, true);
                }
            }
        }
    }

    /// Forget the solution `sol` of `key` unless someone else holds it (a thread evaluating an equal solution
    /// got it from the store, under the lock, and still holds it).
    pub fn forget(&self, key: u64, sol: &Arc<Solution>) {
        let mut map = self.map.lock();
        if map.get(&key).is_some_and(|a| Arc::ptr_eq(a, sol) && Arc::strong_count(a) == 2) {
            map.remove(&key);
        }
    }

    /// The solution of a key.
    pub fn get(&self, key: u64) -> Option<Arc<Solution>> {
        self.map.lock().get(&key).cloned()
    }
}
