//! MathCAS v2 random numbers: the session generator (semantics 9) and the functions that draw from it.
//!
//! - [`philox`]: the counter-based generator Philox4x32-10 and [`Stream`], with [`Stream::split`] for the explicit
//!   parallel constructs of semantics 18.2 (5);
//! - [`draw`]: uniform reals, unbiased integers in any range (big ones too), normal reals;
//! - [`Generator`] / [`Rng`]: the session's state (seed and stream), shared by the natives of one engine;
//! - the natives `seed`, `random_seed`, `random_real`, `random_integer`, `random_normal`, `random_choice`,
//!   `random_sample`.
//!
//! Every draw is an effect: the natives are registered `effect` and call [`NativeCx::effect`] before touching the
//! state, so a draw happens only on the sequential frontier, outside conditions, in the sequential order of the
//! evaluation, and no result that involved a draw is memoised (semantics 4.4, 9, 18.1).
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! let a = e.eval("seed(11); random_integer([1, 6], 5)", &stop).unwrap().printed;
//! let b = e.eval("seed(11); random_integer([1, 6], 5)", &stop).unwrap().printed;
//! assert_eq!(a, b);
//! ```
//!
//! Decisions of this crate are numbered RNG.1, RNG.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod draw;
mod natives;
pub mod philox;

pub use philox::Stream;

use mc_api::Store;
use mc_arith::num::Int;
use mc_core::Heap;
use mc_eval::{Attributes, Kernel};
use std::sync::{Arc, Mutex, MutexGuard};

/// Every function of this crate with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("seed", "seed(n): reset the session's random generator to the state of the integer seed n; seed(): a fresh seed from the operating system"),
    ("random_seed", "random_seed(): the seed in use, so that seed(random_seed()) at the start of a run reproduces its draws"),
    (
        "random_real",
        "random_real(): a uniform machine real in [0, 1); random_real(b) in [0, b); random_real([a, b]) in [a, b); a second argument n or [n1, n2, ...] gives a list or nested lists",
    ),
    (
        "random_integer",
        "random_integer(n): a uniform integer in [0, n]; random_integer([a, b]) in [a, b] (any size); a second argument n or [n1, ...] gives a list or nested lists",
    ),
    (
        "random_normal",
        "random_normal(): a standard normal machine real; random_normal(mu, sigma) with mean mu and standard deviation sigma; random_normal(mu, sigma, n or [n1, ...]): a list",
    ),
    ("random_choice", "random_choice(xs): a uniformly chosen element of the list xs; random_choice(xs, n): n of them, with replacement"),
    (
        "random_sample",
        "random_sample(xs, n): n elements of xs at distinct positions, without replacement, in random order; random_sample(xs): a random permutation",
    ),
];

/// The session's generator state (decision RNG.2): the seed it was last reset to and the stream position.
#[derive(Clone, Debug)]
pub struct Generator {
    seed: Int,
    stream: Stream,
}

impl Generator {
    /// The generator of seed `n`: the stream whose key [`key_of`] gives, at its start.
    pub fn seeded(n: Int) -> Generator {
        let stream = Stream::from_key(key_of(&n));
        Generator { seed: n, stream }
    }

    /// A generator seeded from the operating system ([`os_seed`]).
    pub fn from_os() -> Generator {
        Generator::seeded(Int::from_u64(os_seed()))
    }

    /// The seed in use.
    pub fn seed(&self) -> &Int {
        &self.seed
    }

    /// The stream draws come from.
    pub fn stream(&mut self) -> &mut Stream {
        &mut self.stream
    }
}

/// The key of a seed (decision RNG.4): the sign, the limb count and each 64-bit limb of the integer folded
/// through SplitMix64's finaliser, so every integer, of any size, gives a key, and nearby seeds give unrelated keys.
pub fn key_of(n: &Int) -> u64 {
    let limbs = &n.mag.limbs;
    let mut h = philox::mix64(0x5EED_5EED_0000_0000 ^ ((limbs.len() as u64) << 1) ^ n.neg as u64);
    for &l in limbs {
        h = philox::mix64(h ^ l);
    }
    h
}

/// A seed from the operating system without a new dependency (decision RNG.4): a fresh
/// [`std::collections::hash_map::RandomState`] holds keys the standard library takes from the system's generator
/// (getrandom on Linux, ProcessPrng on Windows, ...); it hashes the time, the process id, a process-wide counter and
/// a stack address. Two calls give different seeds.
pub fn os_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(CALLS.fetch_add(1, Ordering::Relaxed));
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    h.write_u128(now);
    h.write_u32(std::process::id());
    let local = 0u8;
    h.write_usize(&local as *const u8 as usize);
    h.finish()
}

/// The generator of one session, shared by this crate's natives (and, later, by the parallel constructs and
/// compiled code: see NOTES.md). Cloning shares the state.
#[derive(Clone)]
pub struct Rng(Arc<Mutex<Generator>>);

impl Rng {
    /// A generator seeded from the operating system.
    pub fn new() -> Rng {
        Rng(Arc::new(Mutex::new(Generator::from_os())))
    }

    /// Lock the state. Callers inside an evaluation must have announced an effect first (only the frontier
    /// draws), so the lock is never contended in practice.
    pub fn lock(&self) -> MutexGuard<'_, Generator> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Reset to seed `n`.
    pub fn reseed(&self, n: Int) {
        *self.lock() = Generator::seeded(n);
    }

    /// The base stream of one explicit parallel construct (semantics 18.2 (5)): one word is drawn from the session
    /// stream (so the next construct gets another base) and element `j` then draws from `base.split(j)`. Call it on
    /// the frontier, after announcing an effect.
    pub fn fork(&self) -> Stream {
        self.lock().stream.fork()
    }
}

impl Default for Rng {
    fn default() -> Rng {
        Rng::new()
    }
}

/// Register the natives of this crate with `k`, with a new generator seeded from the operating system; the
/// generator is returned so that the engine can give it to other parts of the session. Every function is declared
/// `effect` (decision RNG.7).
pub fn install(k: &mut Kernel, h: &Heap) -> Rng {
    let rng = Rng::new();
    let a = Attributes { effect: true, ..Attributes::default() };
    for &(name, f) in natives::TABLE {
        k.register(h.sym(name), a, Box::new(natives::RandomNative { f, name, rng: rng.clone() }));
    }
    rng
}
