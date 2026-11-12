//! MathCAS v2 real algebraic geometry (roadmap items 11A and 11B): cylindrical algebraic decomposition (CAD),
//! quantifier elimination over the reals, and `reduce` in several real variables.
//!
//! - [`cad`]: preprocessing (Groebner bases of equations, factorisation, equational constraints), projection
//!   ([`project`]: McCallum's operator with Brown's improvement by default, Lazard's as the fallback), lifting
//!   with certified sample points (rationals in sectors, real algebraic numbers over number fields in sections,
//!   `mc_algebra::realroot`), and the truth of every cell, with partial lifting for quantified variables;
//! - [`vs`]: linear virtual substitution, the fast path for quantified variables that occur linearly;
//! - [`sos`]: a sum-of-squares certificate for `for_all` of one polynomial inequality;
//! - [`output`]: solution formulas, cylindrical (adjacent cells combined) or by sign conditions;
//! - [`complex`]: complex quantifier elimination (several equations and `!=` per variable) and closed sentences
//!   by Groebner bases, inequalities over the real and imaginary parts;
//! - [`optimize`]: `minimize`, `maximize` and the others through the decomposition (item 11D, first part);
//! - [`integers`]: `reduce`, `find_instance` and `resolve` over the integers (Pell families, bounded problems);
//! - the natives `resolve`, `exists`, `for_all`, `cylindrical_decomposition`, the several-variable path of
//!   `find_instance`, [`natives::reduce_reals`] for `mc-solve`'s `reduce`, and [`reals::CadReals`], the decision
//!   service's last tier.
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("resolve(for_all(x, x^2 + 1 > 0))", &stop).unwrap().printed, "true");
//! ```
//!
//! Literature: Collins (1975); Collins and Hong (1991); McCallum (1988, 1999); Brown (2001); Lazard (1994) and
//! McCallum, Parusiński and Paunescu (2019); Loos and Weispfenning (1993); Weispfenning (1997) for the quadratic
//! sign rules; Kapur, Sun and Wang (2010) and Gianni (1989), Kalkbrener (1991) for the complex case splits; Basu,
//! Pollack and Roy, *Algorithms in Real Algebraic Geometry*. Decisions are numbered CAD.1, CAD.2, ... in
//! `NOTES.md` and where they apply.
#![warn(missing_docs)]
pub mod cad;
pub mod complex;
pub mod formula;
pub mod front;
pub mod integers;
pub mod natives;
pub mod optimize;
pub mod output;
mod par;
pub mod poly;
pub mod project;
pub mod reals;
pub mod sos;
pub mod vs;

pub use natives::reduce_reals;

use mc_core::{Heap, Store};
use mc_eval::{Attributes, Kernel};
use natives::{CadNative, FindInstance, Kind};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Every function of this crate with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "resolve",
        "resolve(formula), resolve(formula, reals | complexes): the formula with its quantifiers (exists, for_all) eliminated: true or false for a sentence, conditions on the free variables otherwise (real polynomial formulas by cylindrical algebraic decomposition)",
    ),
    ("exists", "exists(x, formula), exists([x, y], formula), exists(x, cond, formula): there is an x with formula; resolve and reduce eliminate it"),
    ("for_all", "for_all(x, formula), for_all([x, y], formula), for_all(x, cond, formula): formula holds for every x; resolve and reduce eliminate it"),
    (
        "cylindrical_decomposition",
        "cylindrical_decomposition(formula, [x, y, ...]): the solution set of a real polynomial formula as cylindrical cells, x first: a < x < b and f(x) < y < g(x) or ...",
    ),
];

/// Register the natives of this crate with `k` (after `mc-calculus`, whose `find_instance` it wraps, and
/// `mc-decide`, whose last tier it registers). `stop` is the flag `mc-arith`'s number kernels poll.
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let fns = [
        ("resolve", Kind::Resolve),
        ("exists", Kind::Exists),
        ("for_all", Kind::ForAll),
        ("cylindrical_decomposition", Kind::Cylindrical),
    ];
    for (name, kind) in fns {
        k.register(h.sym(name), Attributes::default(), Box::new(CadNative { kind, stop: stop.clone() }));
    }
    k.register(h.sym("cad$stats"), Attributes::default(), Box::new(CadNative { kind: Kind::Stats, stop: stop.clone() }));
    let st = stop.clone();
    k.wrap_native(h.sym("find_instance"), move |inner| Box::new(FindInstance { inner, stop: st }));
    let goals = [
        ("minimize", optimize::Goal::Minimize),
        ("maximize", optimize::Goal::Maximize),
        ("min_value", optimize::Goal::MinValue),
        ("max_value", optimize::Goal::MaxValue),
        ("arg_min", optimize::Goal::ArgMin),
        ("arg_max", optimize::Goal::ArgMax),
    ];
    for (name, goal) in goals {
        let st = stop.clone();
        k.wrap_native(h.sym(name), move |inner| Box::new(optimize::Optimize { inner, goal, stop: st }));
    }
    k.wrap_native(h.sym("not"), |inner| Box::new(natives::NotQuantifier { inner }));
    mc_decide::register_reals(Box::new(reals::Factory));
}
