//! Library functions of numerical evaluation: functions of one or more real arguments that another
//! crate defines and registers here, so that `n`, comparisons, `sign` and `is_real` evaluate them wherever they
//! occur (`n(gamma(1/3) zeta(3), 30)`, `bessel_k(1, 2) < 1/2`), refined with the rest of their island.
//!
//! A registered function is a numeric node of [`crate::numeval`] when all its arguments are. Its ball method
//! receives exact arguments as exact rationals (so it can round them at its own working precision) and computed
//! subterms as balls, and must return an enclosure for every point of the argument balls, or `None`; a complex
//! result (an exact zero imaginary part for real values) makes the island continue with complex balls. Its machine
//! method gives NaN where there is no value. The registry is process-wide (functions are looked up by name), as the
//! constants' cache is; registering a name twice keeps the first.
use crate::cball::CBall;
use crate::fun::Mach;
use mc_arith::Real;
use std::sync::RwLock;

/// A function `n` can evaluate.
pub trait LibraryFn: Send + Sync {
    /// The symbol name.
    fn name(&self) -> &'static str;
    /// The number of arguments.
    fn arity(&self) -> usize;
    /// An enclosure of the value at precision `p` for exact or ball arguments (real), or `None` (no value on the
    /// region, a raised stop flag).
    fn ball(&self, args: &[Real], p: u64) -> Option<CBall>;
    /// The machine value (NaN where there is none).
    fn machine(&self, args: &[f64]) -> Mach;
    /// The machine value at complex arguments, where the function has one (`None`: no value).
    fn machine_complex(&self, args: &[crate::machine::C64]) -> Option<crate::machine::C64> {
        let _ = args;
        None
    }
    /// An enclosure at precision `p` for complex ball arguments, where the function has one.
    fn ball_complex(&self, args: &[CBall], p: u64) -> Option<CBall> {
        let _ = (args, p);
        None
    }
}

/// A registered function (a `'static` reference, compared by name).
#[derive(Clone, Copy)]
pub struct LibRef(pub &'static dyn LibraryFn);

impl std::fmt::Debug for LibRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.0.name(), self.0.arity())
    }
}

static REGISTRY: RwLock<Vec<LibRef>> = RwLock::new(Vec::new());

/// Registers `f` (idempotent: a name and arity already present is kept).
pub fn register(f: &'static dyn LibraryFn) {
    let mut r = REGISTRY.write().unwrap_or_else(|e| e.into_inner());
    if !r.iter().any(|g| g.0.name() == f.name() && g.0.arity() == f.arity()) {
        r.push(LibRef(f));
    }
}

/// The registered function `name` of `arity` arguments.
pub fn lookup(name: &str, arity: usize) -> Option<LibRef> {
    let r = REGISTRY.read().unwrap_or_else(|e| e.into_inner());
    r.iter().find(|g| g.0.arity() == arity && g.0.name() == name).copied()
}

/// Is any function registered? (Lets numerical evaluation skip the name lookup for unknown heads.)
pub fn any() -> bool {
    !REGISTRY.read().unwrap_or_else(|e| e.into_inner()).is_empty()
}
