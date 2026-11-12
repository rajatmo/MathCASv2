//! Library calls from compiled code (round 13, decisions LC.1 to LC.6): functions of machine reals that compiled
//! code calls by number, each giving exactly the value the interpreter's native gives at the same machine reals,
//! or nothing, and then the compiled call declines and the interpreter evaluates it.
//!
//! - **Special functions** ([`special`]): `bessel_k`, `gamma`, `erf`, `polylog`, ... of `mc-special` through the
//!   very machine path their native takes (its real-domain test, its machine value); the functions `mc-numeric`'s
//!   library registers by formula (`bessel_k_scaled`, `product_log`, `asinh`, ...) through their registered machine value,
//!   only those listed in [`FORMULA_FUNCTIONS`], whose interpreted values agree with it bit for bit (checked by the
//!   tests over grids of arguments).
//! - **Interpolations** ([`interpolation`]): the local scheme (`interpolation_order`) and derivatives of either
//!   scheme, through `mc_numeric::spline`'s shared evaluation.
//! - **Other crates' callables** ([`add_resolver`]): a crate registers a resolver that recognises its curried
//!   heads (the solutions of `n_dsolve` in `mc-ode`) and registers a callee for them.
//!
//! Callees are registered once per key (a string naming the function and its data) and never removed, so a number
//! stays valid for the life of the process (as `mc_numeric::spline`'s registry, decision LC.2). A call with a
//! non-finite argument, or whose value is not a finite machine real, declines (decision LC.3): the interpreter
//! then gives its own answer (an unevaluated call, a complex number, an infinity, a failure).
use mc_core::{Heap, Store, View, T};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// The most arguments of a library call.
pub const MAX_ARGS: usize = 4;

/// A function compiled code may call.
pub trait Callee: Send + Sync {
    /// The interpreter's value at these machine reals (as many as the callee takes), or `None` where the
    /// interpreter gives anything but a machine real.
    fn value(&self, args: &[f64]) -> Option<f64>;
}

struct Registry {
    all: Vec<&'static dyn Callee>,
    by_key: HashMap<String, u32>,
}

fn registry() -> &'static RwLock<Registry> {
    static R: OnceLock<RwLock<Registry>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(Registry { all: vec![], by_key: HashMap::new() }))
}

/// The number of the callee registered under `key`, registering `make()` on first use (decision LC.2).
pub fn register(key: &str, make: impl FnOnce() -> Box<dyn Callee>) -> u32 {
    if let Some(&id) = registry().read().unwrap_or_else(|e| e.into_inner()).by_key.get(key) {
        return id;
    }
    let mut r = registry().write().unwrap_or_else(|e| e.into_inner());
    if let Some(&id) = r.by_key.get(key) {
        return id;
    }
    let id = r.all.len() as u32;
    // a registered callee lives as long as the process (numbers are never reused)
    r.all.push(Box::leak(make()));
    r.by_key.insert(key.to_string(), id);
    id
}

/// The callee registered as `id`.
pub fn get(id: u32) -> Option<&'static dyn Callee> {
    registry().read().unwrap_or_else(|e| e.into_inner()).all.get(id as usize).copied()
}

/// The value of callee `id` at `args`: `None` (decline) for an unknown callee, a non-finite argument, or a value
/// that is not a finite machine real (decision LC.3).
pub fn value(id: u32, args: &[f64]) -> Option<f64> {
    if args.iter().any(|x| !x.is_finite()) {
        return None;
    }
    get(id)?.value(args).filter(|v| v.is_finite())
}

// ---------------------------------------------------------------------------------------------------------------
// special functions

/// The functions registered with `mc-numeric`'s library by formula whose interpreted machine values are the
/// library's machine values bit for bit (decision LC.4; the tests compare every one over a grid of arguments). Left
/// out because the interpreter computes them by another formula, which differs in the last bits: `dawson_f`,
/// `log10`, `log2`, and the reciprocal functions (`sec`, `csch`, ...).
pub const FORMULA_FUNCTIONS: &[&str] = &[
    "bessel_k_scaled",
    "bessel_i_scaled",
    "product_log",
    "fresnel_s",
    "fresnel_c",
    "asinh",
    "acosh",
    "atanh",
];

struct SpecialFn(mc_special::functions::Special);

impl Callee for SpecialFn {
    fn value(&self, args: &[f64]) -> Option<f64> {
        // the native's machine path (mc-special `Fun::call`): outside the real domain the call stays
        if !self.0.real_domain(args) {
            return None;
        }
        match self.0.machine_value(args) {
            mc_numeric::fun::Mach::Real(v) => Some(v),
            mc_numeric::fun::Mach::Complex(_) => None,
        }
    }
}

struct LibraryFn(mc_numeric::library::LibRef);

impl Callee for LibraryFn {
    fn value(&self, args: &[f64]) -> Option<f64> {
        match self.0 .0.machine(args) {
            mc_numeric::fun::Mach::Real(v) => Some(v),
            mc_numeric::fun::Mach::Complex(_) => None,
        }
    }
}

/// The callee of the special function `name` with `arity` machine-real arguments, if compiled code may call it.
pub fn special(name: &str, arity: usize) -> Option<u32> {
    if arity == 0 || arity > MAX_ARGS {
        return None;
    }
    if let Some(f) = mc_special::functions::Special::find(name, arity) {
        return Some(register(&format!("special:{name}/{arity}"), || Box::new(SpecialFn(f))));
    }
    if FORMULA_FUNCTIONS.contains(&name) {
        let f = mc_numeric::library::lookup(name, arity)?;
        return Some(register(&format!("library:{name}/{arity}"), || Box::new(LibraryFn(f))));
    }
    None
}

// ---------------------------------------------------------------------------------------------------------------
// interpolations

/// How an interpolation is evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// the cubic spline with these second derivatives at the nodes
    Spline,
    /// the local polynomial of this degree through the nearest nodes (`interpolation_order`)
    Local(usize),
}

struct Interp {
    xs: Vec<f64>,
    ys: Vec<f64>,
    ms: Vec<f64>,
    scheme: Scheme,
    order: u32,
}

impl Callee for Interp {
    fn value(&self, args: &[f64]) -> Option<f64> {
        // the evaluation the interpreter's `interpolated(...)(x)` shares (mc-io)
        match self.scheme {
            Scheme::Local(k) => mc_numeric::spline::local(&self.xs, &self.ys, k, args[0], self.order),
            Scheme::Spline => mc_numeric::spline::derivative(&self.xs, &self.ys, &self.ms, args[0], self.order),
        }
    }
}

fn content_key(parts: &[&[f64]], tag: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut s = std::collections::hash_map::DefaultHasher::new();
    for v in parts {
        v.len().hash(&mut s);
        for x in *v {
            x.to_bits().hash(&mut s);
        }
    }
    // the lengths and the end nodes are part of the key too, a guard against a hash collision
    let ends: Vec<String> = parts
        .iter()
        .map(|v| format!("{}:{:x}:{:x}", v.len(), v.first().map_or(0, |x| x.to_bits()), v.last().map_or(0, |x| x.to_bits())))
        .collect();
    format!("interp:{tag}:{:016x}:{}", s.finish(), ends.join(","))
}

/// The callee of an interpolation with nodes `xs`, values `ys`, the scheme's data (`ms` for the spline, empty for
/// the local scheme) and derivative order `order`.
pub fn interpolation(xs: &[f64], ys: &[f64], ms: &[f64], scheme: Scheme, order: u32) -> u32 {
    let tag = match scheme {
        Scheme::Spline => format!("s{order}"),
        Scheme::Local(k) => format!("l{k}:{order}"),
    };
    let key = content_key(&[xs, ys, ms], &tag);
    register(&key, || Box::new(Interp { xs: xs.to_vec(), ys: ys.to_vec(), ms: ms.to_vec(), scheme, order }))
}

// ---------------------------------------------------------------------------------------------------------------
// resolvers of other crates

/// A resolver: for the head `head` of a call with `nargs` arguments (a curried head such as
/// `interpolating_function(...)`), the number of a callee it registered, or `None` when the head is not its.
pub type Resolver = for<'h> fn(&'h Heap, T<'h>, usize) -> Option<u32>;

fn resolvers() -> &'static RwLock<Vec<Resolver>> {
    static R: OnceLock<RwLock<Vec<Resolver>>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(vec![]))
}

/// Adds a resolver (once: the same function added again is ignored).
pub fn add_resolver(r: Resolver) {
    let mut v = resolvers().write().unwrap_or_else(|e| e.into_inner());
    if !v.iter().any(|&x| x as usize == r as usize) {
        v.push(r);
    }
}

/// The callee some resolver gives the head `head` with `nargs` arguments.
pub fn resolve<'h>(h: &'h Heap, head: T<'h>, nargs: usize) -> Option<u32> {
    if !matches!(h.view(head), View::App { .. }) || nargs == 0 || nargs > MAX_ARGS {
        return None;
    }
    let rs: Vec<Resolver> = resolvers().read().unwrap_or_else(|e| e.into_inner()).clone();
    rs.into_iter().find_map(|r| r(h, head, nargs))
}

/// [`value`] for compiled code: arguments in order (unused ones ignored); declines when there is no value.
///
/// # Safety
/// `env` is the live environment of the running call.
pub unsafe extern "C" fn call(env: *mut crate::rt::Env, id: i64, n: i64, a: f64, b: f64, c: f64, d: f64) -> f64 {
    let args = [a, b, c, d];
    let n = (n as usize).min(MAX_ARGS);
    match value(id as u32, &args[..n]) {
        Some(v) => v,
        None => {
            // SAFETY: the caller's contract.
            unsafe { &mut *env }.decline();
            0.0
        }
    }
}
