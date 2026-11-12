//! The process-wide cache of compiled code (item N1): one compilation per *family* of expressions.
//!
//! A solver called once per parameter value (`find_root(x - e*sin(x) == m, [x, 1.0])` for 10^5 values of `e` and
//! `m`) prepares a new body each time, and compiling each costs more than it saves. The bodies differ only in their
//! numeric constants, so the cache compiles the body with its machine reals **lifted** to extra parameters
//! (decision N1.1):
//!
//! ```text
//! x - 0.3*sin(x) - 0.006   ->   (x, c1, c2) => x + c1*sin(x) + c2      called with (x, -0.3, -0.006)
//! ```
//!
//! and keys the code by the lifted body's **shape**: its structure with variables by position, lifted constants by
//! position, every other symbol by name and every other atom by value (decision N1.2). Calls pass the instance's
//! constants after its inputs. The compiled code gives the interpreter's result for every value of a `Real`
//! parameter (decision 1b.3), and substituting the constants back gives the instance's body term for term, so a
//! lifted call gives bitwise what the instance's own compiled code (and the interpreter) gives.
//!
//! **Scope** (decision N1.3): code that inlined no function depends on its shape alone, so it serves every kernel
//! of the process; code that inlined user functions, and refusals, belong to the kernel they were made in
//! ([`Owner`]). A scoped entry is checked before every use: code by its inlined functions' version counters
//! (decision 1b.6: a redefinition invalidates it), a refusal by the versions of the symbols of the body.
//!
//! **Counting** (decision N1.4): a family counts the evaluations of all its instances, so [`crate::Policy::Family`]
//! (the default) compiles once the family, not one instance, has been evaluated often enough; on a hit an instance
//! runs compiled from its first call.
//!
//! The cache holds at most [`CAPACITY`] families and evicts the least recently used one. Statistics are process-wide
//! ([`cache_stats`]); each [`crate::Auto`] also reports its own hit or miss in [`crate::Stats`].
use crate::{Arg, Var};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::NativeCx;
use mc_jit::ExprFn;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

/// The most families the cache holds; beyond it the least recently used one is evicted.
pub const CAPACITY: usize = 512;

/// The largest body (in distinct nodes) whose shape is computed: larger bodies compile per instance, as without
/// the cache (their compilation dominates anyway, and the key would be as large as the body).
const MAX_NODES: usize = 1 << 16;

/// Process-wide counters of the cache.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CacheStats {
    /// lookups that found compiled code (or a refusal) for the family
    pub hits: u64,
    /// lookups that found nothing usable
    pub misses: u64,
    /// families compiled (lifted bodies given to the compiler)
    pub compiles: u64,
    /// families evicted to stay within [`CAPACITY`]
    pub evictions: u64,
    /// entries dropped because a function they inlined, or a symbol of a refused body, was redefined
    pub invalidations: u64,
    /// families held now
    pub families: u64,
}

static HITS: AtomicU64 = AtomicU64::new(0);
static MISSES: AtomicU64 = AtomicU64::new(0);
static COMPILES: AtomicU64 = AtomicU64::new(0);
static EVICTIONS: AtomicU64 = AtomicU64::new(0);
static INVALIDATIONS: AtomicU64 = AtomicU64::new(0);

/// The counters of the process-wide cache.
pub fn cache_stats() -> CacheStats {
    CacheStats {
        hits: HITS.load(Relaxed),
        misses: MISSES.load(Relaxed),
        compiles: COMPILES.load(Relaxed),
        evictions: EVICTIONS.load(Relaxed),
        invalidations: INVALIDATIONS.load(Relaxed),
        families: cache().map.len() as u64,
    }
}

/// Drop every cached family (code already handed out stays valid).
pub fn clear_cache() {
    cache().map.clear();
}

/// The kernel an entry belongs to: the heap's address and the kernel's stop flag. The weak reference keeps the
/// flag's allocation, so its address is not reused while the entry exists; a kernel whose flag is gone has ended.
#[derive(Clone)]
pub(crate) struct Owner {
    heap: usize,
    flag: Weak<AtomicBool>,
}

impl Owner {
    /// The kernel `cx` evaluates in.
    pub(crate) fn of(cx: &NativeCx<'_, '_, '_>) -> Owner {
        Owner { heap: cx.heap() as *const Heap as usize, flag: Arc::downgrade(cx.stop_flag()) }
    }

    fn same(&self, o: &Owner) -> bool {
        self.heap == o.heap && Weak::ptr_eq(&self.flag, &o.flag)
    }

    fn alive(&self) -> bool {
        self.flag.strong_count() > 0
    }
}

/// What a kernel holds for a family.
enum Scoped {
    /// code that inlined user functions
    Code(Arc<ExprFn>),
    /// the lifted body was refused (why), valid while the symbols of `checks` keep their versions; `unlifted`:
    /// the instances' own bodies compile (constants in a position the compiler needs as literals)
    Refused { why: Arc<str>, checks: Vec<(Sym, u32)>, unlifted: bool },
}

/// One family: its counter, its code for every kernel, or per kernel.
struct Family {
    /// evaluations of all instances (decision N1.4)
    seen: Arc<AtomicU64>,
    /// code that inlined nothing: valid in every kernel
    global: Option<Arc<ExprFn>>,
    scoped: Vec<(Owner, Scoped)>,
    /// the cache's clock at the last use (least recently used eviction)
    stamp: u64,
}

struct Cache {
    map: HashMap<Arc<[u8]>, Family>,
    clock: u64,
}

fn cache() -> MutexGuard<'static, Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    // a panic while holding the lock leaves a consistent map (every update is a single insert or removal)
    C.get_or_init(|| Mutex::new(Cache { map: HashMap::new(), clock: 0 })).lock().unwrap_or_else(|e| e.into_inner())
}

impl Cache {
    /// The family of `key`, created if new (evicting the least recently used family at capacity), stamped now.
    fn family(&mut self, key: &Arc<[u8]>) -> &mut Family {
        self.clock += 1;
        if !self.map.contains_key(key) {
            if self.map.len() >= CAPACITY {
                if let Some(old) = self.map.iter().min_by_key(|(_, f)| f.stamp).map(|(k, _)| k.clone()) {
                    self.map.remove(&old);
                    EVICTIONS.fetch_add(1, Relaxed);
                }
            }
            let fam = Family { seen: Arc::new(AtomicU64::new(0)), global: None, scoped: vec![], stamp: 0 };
            self.map.insert(key.clone(), fam);
        }
        let clock = self.clock;
        let fam = self.map.get_mut(key).expect("inserted");
        fam.stamp = clock;
        fam
    }
}

/// A body with its machine reals lifted to parameters, and its family key.
pub(crate) struct Lifted<'h> {
    /// the lifted body
    pub body: T<'h>,
    /// the variables, then one scalar per lifted constant
    pub params: Vec<Var>,
    /// the values of the lifted constants, in parameter order
    pub values: Vec<f64>,
    /// the family key
    pub key: Arc<[u8]>,
}

/// What the cache holds for a family in a kernel.
pub(crate) enum Found {
    /// compiled code to call with the lifted constants appended
    Code(Arc<ExprFn>),
    /// the lifted body is refused; `unlifted`: each instance's own body compiles
    Refused {
        /// the compiler's reason
        why: Arc<str>,
        /// the instances' own bodies compile
        unlifted: bool,
    },
    /// nothing yet
    Miss,
}

/// Tokens of the key encoding.
mod tok {
    pub const VAR: u8 = 1;
    pub const CONST: u8 = 2;
    pub const SYM: u8 = 3;
    pub const INT: u8 = 4;
    pub const BIG: u8 = 5;
    pub const RAT: u8 = 6;
    pub const BALL: u8 = 7;
    pub const STR: u8 = 8;
    pub const APP: u8 = 9;
    pub const REAL: u8 = 10;
}

fn put_u64(k: &mut Vec<u8>, v: u64) {
    k.extend_from_slice(&v.to_le_bytes());
}

fn put_str(k: &mut Vec<u8>, s: &str) {
    put_u64(k, s.len() as u64);
    k.extend_from_slice(s.as_bytes());
}

/// An integer atom; `None` for anything else.
fn put_int(k: &mut Vec<u8>, v: View<'_>) -> Option<()> {
    match v {
        View::Int(i) => {
            k.push(tok::INT);
            put_u64(k, i as u64);
        }
        View::BigInt { neg, mag } => {
            k.push(tok::BIG);
            k.push(neg as u8);
            put_u64(k, mag.len() as u64);
            mag.iter().for_each(|&w| put_u64(k, w));
        }
        _ => return None,
    }
    Some(())
}

/// `body` in `vars` with its machine reals lifted (decision N1.1), and its family key (decision N1.2); `None` when
/// the body is larger than [`MAX_NODES`] (the walks are bounded by it) or holds an atom the key cannot encode.
/// `consts` is [`crate::Compiled::new`]'s flag.
pub(crate) fn lift<'h>(h: &'h Heap, vars: &[Var], body: T<'h>, consts: bool) -> Option<Lifted<'h>> {
    // distinct nodes in post-order (children before parents), each numbered once: the DAG, not the tree, so shared
    // subterms cost once
    let mut ids: HashMap<T<'h>, u32> = HashMap::new();
    let mut order: Vec<T<'h>> = vec![];
    let mut stack: Vec<(T<'h>, bool)> = vec![(body, false)];
    while let Some((u, expanded)) = stack.pop() {
        if ids.contains_key(&u) {
            continue;
        }
        match h.view(u) {
            View::App { head, args } if !expanded => {
                stack.push((u, true));
                // reversed, so that the head and then the arguments are numbered left to right
                stack.extend(args.iter().rev().map(|&a| (a, false)));
                stack.push((head, false));
                continue;
            }
            _ => {}
        }
        ids.insert(u, order.len() as u32);
        order.push(u);
        if order.len() > MAX_NODES {
            return None;
        }
    }
    let var_pos = |s: Sym| vars.iter().position(|v| v.sym == s);
    let mut key: Vec<u8> = Vec::with_capacity(16 * order.len() + 16);
    key.push(consts as u8);
    put_u64(&mut key, vars.len() as u64);
    for v in vars {
        put_u64(&mut key, match v.arg {
            Arg::Scalar => u64::MAX,
            Arg::Vector(n) => n as u64,
        });
    }
    let mut values: Vec<f64> = vec![];
    let mut consts_of: HashMap<T<'h>, T<'h>> = HashMap::new();
    for &u in &order {
        match h.view(u) {
            View::Sym(s) => match var_pos(s) {
                Some(i) => {
                    key.push(tok::VAR);
                    put_u64(&mut key, i as u64);
                }
                None => {
                    key.push(tok::SYM);
                    put_str(&mut key, &h.sym_name(s));
                }
            },
            View::Real(v) => {
                if v.is_finite() {
                    key.push(tok::CONST);
                    put_u64(&mut key, values.len() as u64);
                    let c = h.sym_term(h.sym(&format!("numfn·const·{}", values.len() + 1)));
                    consts_of.insert(u, c);
                    values.push(v);
                } else {
                    // infinities and NaN stay literals (the compiler may treat them apart)
                    key.push(tok::REAL);
                    put_u64(&mut key, v.to_bits());
                }
            }
            View::Int(_) | View::BigInt { .. } => put_int(&mut key, h.view(u))?,
            View::Rational { num, den } => {
                // the parts are integers, not nodes of the walk: encoded in place
                key.push(tok::RAT);
                put_int(&mut key, h.view(num))?;
                put_int(&mut key, h.view(den))?;
            }
            View::Ball { neg, words } => {
                key.push(tok::BALL);
                key.push(neg as u8);
                put_u64(&mut key, words.len() as u64);
                words.iter().for_each(|&w| put_u64(&mut key, w));
            }
            View::Str(s) => {
                key.push(tok::STR);
                put_str(&mut key, s);
            }
            View::App { head, args } => {
                key.push(tok::APP);
                put_u64(&mut key, ids[&head] as u64);
                put_u64(&mut key, args.len() as u64);
                args.iter().for_each(|a| put_u64(&mut key, ids[a] as u64));
            }
            #[allow(unreachable_patterns)]
            _ => return None,
        }
    }
    let lifted = if consts_of.is_empty() { body } else { crate::prep::replace(h, body, &mut |u| consts_of.get(&u).copied()) };
    let mut params = vars.to_vec();
    params.extend((1..=values.len()).map(|k| Var::scalar(h.sym(&format!("numfn·const·{k}")))));
    Some(Lifted { body: lifted, params, values, key: key.into() })
}

/// The symbols of `body` with their versions: what a refusal depends on (the definitions the front end read are
/// those of symbols the body calls).
pub(crate) fn checks<'h>(cx: &mut NativeCx<'_, '_, 'h>, body: T<'h>) -> Vec<(Sym, u32)> {
    let h = cx.heap();
    let mut seen = std::collections::HashSet::new();
    let mut syms = vec![];
    let mut stack = vec![body];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        match h.view(u) {
            View::Sym(s) => syms.push(s),
            View::App { head, args } => {
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    syms.into_iter().map(|s| (s, cx.version(s))).collect()
}

/// What the cache holds for `key` in the kernel of `cx` (a stale scoped entry is dropped and reported a miss),
/// and the family's evaluation counter.
pub(crate) fn lookup(cx: &mut NativeCx<'_, '_, '_>, key: &Arc<[u8]>) -> (Found, Arc<AtomicU64>) {
    let owner = Owner::of(cx);
    let (found, checks, seen) = {
        let mut c = cache();
        let fam = c.family(key);
        fam.scoped.retain(|(o, _)| o.alive());
        let seen = fam.seen.clone();
        match (&fam.global, fam.scoped.iter().find(|(o, _)| o.same(&owner))) {
            (Some(code), _) => (Found::Code(code.clone()), vec![], seen),
            (None, Some((_, Scoped::Code(code)))) => (Found::Code(code.clone()), vec![], seen),
            (None, Some((_, Scoped::Refused { why, checks, unlifted }))) => {
                (Found::Refused { why: why.clone(), unlifted: *unlifted }, checks.clone(), seen)
            }
            (None, None) => (Found::Miss, vec![], seen),
        }
    };
    // the validity checks read version counters through `cx` (so they are recorded as reads), outside the lock
    let valid = match &found {
        Found::Code(code) => code.current(cx),
        Found::Refused { .. } => checks.iter().all(|&(s, v)| cx.version(s) == v),
        Found::Miss => true,
    };
    if !valid {
        invalidate(cx, key);
        MISSES.fetch_add(1, Relaxed);
        return (Found::Miss, seen);
    }
    match found {
        Found::Miss => MISSES.fetch_add(1, Relaxed),
        _ => HITS.fetch_add(1, Relaxed),
    };
    (found, seen)
}

/// Record compiled code for `key`: for every kernel when it inlined nothing, else for the kernel of `cx`.
pub(crate) fn insert_code(cx: &NativeCx<'_, '_, '_>, key: &Arc<[u8]>, code: Arc<ExprFn>) {
    COMPILES.fetch_add(1, Relaxed);
    let owner = Owner::of(cx);
    let mut c = cache();
    let fam = c.family(key);
    if code.info().inlined.is_empty() {
        fam.global = Some(code);
    } else {
        fam.scoped.retain(|(o, _)| o.alive() && !o.same(&owner));
        fam.scoped.push((owner, Scoped::Code(code)));
    }
}

/// Record that the kernel of `cx` refused the lifted body of `key`.
pub(crate) fn insert_refusal(cx: &NativeCx<'_, '_, '_>, key: &Arc<[u8]>, why: &str, checks: Vec<(Sym, u32)>, unlifted: bool) {
    COMPILES.fetch_add(1, Relaxed);
    let owner = Owner::of(cx);
    let mut c = cache();
    let fam = c.family(key);
    fam.scoped.retain(|(o, _)| o.alive() && !o.same(&owner));
    fam.scoped.push((owner, Scoped::Refused { why: why.into(), checks, unlifted }));
}

/// Drop what the kernel of `cx` holds for `key` (stale after a redefinition). Shared code inlined nothing, so no
/// redefinition affects it.
pub(crate) fn invalidate(cx: &NativeCx<'_, '_, '_>, key: &Arc<[u8]>) {
    INVALIDATIONS.fetch_add(1, Relaxed);
    let owner = Owner::of(cx);
    let mut c = cache();
    if let Some(fam) = c.map.get_mut(key) {
        fam.scoped.retain(|(o, _)| o.alive() && !o.same(&owner));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiled_code_is_shareable() {
        fn send_sync<X: Send + Sync>() {}
        send_sync::<mc_jit::ExprFn>();
    }
}
