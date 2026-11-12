//! The library registry: one crate that depends on every library crate, so that the engine, the language server
//! and the documentation do not each list them (round 34).
//!
//! - [`install`]: registers every library's natives in a kernel, in the order the engine always used;
//! - [`SOURCES`]: the libraries written in MathCAS, in load order (the engine evaluates them after the prelude);
//! - [`FUNCTIONS`]: every library's `FUNCTIONS` table (the names a session knows from the libraries);
//! - the library crates themselves, re-exported (`mc_libraries::mc_lists`, ...), for the topic tables of the
//!   catalogue and the language server.
#![warn(missing_docs)]

pub use mc_algebra;
pub use mc_assoc;
pub use mc_cad;
pub use mc_calculus;
pub use mc_combinat;
pub use mc_decide;
pub use mc_diffalg;
pub use mc_dsolve;
pub use mc_dist;
pub use mc_fields;
pub use mc_further_functions;
pub use mc_geom;
pub use mc_graphics_kernel;
pub use mc_groups;
pub use mc_integrals;
pub use mc_io;
pub use mc_jit;
pub use mc_langlib;
pub use mc_linalg;
pub use mc_limit;
pub use mc_lists;
pub use mc_mechanics;
pub use mc_numeric;
pub use mc_numtheory;
pub use mc_ode;
pub use mc_plot;
pub use mc_poly;
pub use mc_quad;
pub use mc_random;
pub use mc_roots;
pub use mc_simplify;
pub use mc_solve;
pub use mc_special;
pub use mc_strings;
pub use mc_sums;
pub use mc_transforms;

use mc_core::Heap;
use mc_eval::Kernel;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// A `FUNCTIONS` table: names and one-line descriptions.
pub type Functions = &'static [(&'static str, &'static str)];

/// Every library's `FUNCTIONS` table: the names the libraries give a meaning (the engine's name resolution knows
/// them like defined symbols).
pub const FUNCTIONS: &[Functions] = &[
    mc_lists::FUNCTIONS,
    mc_assoc::FUNCTIONS,
    mc_numeric::FUNCTIONS,
    mc_numtheory::FUNCTIONS,
    mc_poly::FUNCTIONS,
    mc_algebra::FUNCTIONS,
    mc_jit::FUNCTIONS,
    mc_calculus::FUNCTIONS,
    mc_sums::FUNCTIONS,
    mc_strings::FUNCTIONS,
    mc_roots::FUNCTIONS,
    mc_quad::FUNCTIONS,
    mc_quad::LIBRARY_FUNCTIONS,
    mc_ode::FUNCTIONS,
    mc_graphics_kernel::FUNCTIONS,
    mc_geom::FUNCTIONS,
    mc_plot::FUNCTIONS,
    mc_io::FUNCTIONS,
    mc_simplify::FUNCTIONS,
    mc_random::FUNCTIONS,
    mc_special::FUNCTIONS,
    mc_linalg::FUNCTIONS,
    mc_solve::FUNCTIONS,
    mc_decide::FUNCTIONS,
    mc_cad::FUNCTIONS,
    mc_dist::FUNCTIONS,
    mc_langlib::FUNCTIONS,
    mc_langlib::MORE_FUNCTIONS,
    mc_combinat::FUNCTIONS,
    mc_fields::FUNCTIONS,
    mc_further_functions::FUNCTIONS,
    mc_mechanics::FUNCTIONS,
    mc_groups::FUNCTIONS,
    mc_diffalg::FUNCTIONS,
];

/// A library written in MathCAS.
#[derive(Clone, Copy, Debug)]
pub struct Source {
    /// what the library is called in the message when it does not load ("the {name} library does not load")
    pub name: &'static str,
    /// its file, the origin of its definitions in messages
    pub origin: &'static str,
    /// its text
    pub text: &'static str,
}

/// The core-language library (round 30), written in MathCAS; its functions are listed in
/// the engine's `FUNCTIONS`.
const LANGUAGE_SOURCE: &str = include_str!("../../../lib/language.mc");

/// The libraries written in MathCAS, in the order the engine evaluates them after the prelude: a later library may
/// use (and replace) the definitions of an earlier one, so the order is part of their meaning. Round 34 named the
/// files by area and merged the calculus ones (`lib/calculus.mc`: the libraries of rounds 28, 29 and 31, in this order;
/// the round 31 definitions call the further functions' helpers only when they run).
pub const SOURCES: &[Source] = &[
    // the language library
    Source { name: "language", origin: mc_langlib::ORIGIN, text: mc_langlib::SOURCE },
    // the number-sequence library
    Source { name: "number-sequence", origin: mc_combinat::ORIGIN, text: mc_combinat::SOURCE },
    // the calculus library (rounds 28, 29 and 31)
    Source { name: "calculus", origin: mc_calculus::LIB_ORIGIN, text: mc_calculus::LIB_SOURCE },
    // the numerics library
    Source { name: "numerics", origin: mc_quad::LIBRARY_ORIGIN, text: mc_quad::LIBRARY_SOURCE },
    // the lists library
    Source { name: "lists", origin: mc_lists::LIB_ORIGIN, text: mc_lists::LIB_SOURCE },
    // the core-language library
    Source { name: "core-language", origin: "lib/language.mc", text: LANGUAGE_SOURCE },
    // the further functions
    Source { name: "further-functions", origin: mc_further_functions::ORIGIN, text: mc_further_functions::SOURCE },
    // the special-function library
    Source { name: "special-function", origin: mc_special::LIB_ORIGIN, text: mc_special::LIB_SOURCE },
    Source { name: "mechanics", origin: mc_mechanics::ORIGIN, text: mc_mechanics::SOURCE },
    Source { name: "differential algebra", origin: mc_diffalg::ORIGIN, text: mc_diffalg::SOURCE },
    // more of the language library (after the lists library, whose interval helpers it uses)
    Source { name: "more language", origin: mc_langlib::MORE_ORIGIN, text: mc_langlib::MORE_SOURCE },
];

/// Register every library's natives with `k`, and the exporters and importers of pictures with `mc-io`. `stop` is
/// the flag the libraries poll in long loops (the engine's arithmetic stop flag, decision G.5). Returns the numeric
/// compiler, whose queued specialisations the engine registers after each statement.
///
/// The engine installs its own arithmetic before this, and its attributes, compositions and packed-array natives
/// after it.
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) -> mc_jit::Jit {
    // the libraries poll the same stop flag as the arithmetic (decision G.5)
    mc_lists::install(k, h, stop);
    mc_assoc::install(k, h);
    mc_numeric::install_with_stop(k, h, stop);
    mc_numtheory::install(k, h);
    mc_combinat::install(k, h);
    // mc-poly owns `gcd` and `lcm` and sends exact numbers to mc-numtheory's half-gcd (decision 9.1): no two
    // libraries register one name, so the order of these calls does not matter
    mc_poly::install(k, h, stop);
    mc_algebra::install(k, h, stop);
    mc_calculus::install(k, h, stop);
    mc_integrals::install(k, h, stop);
    mc_dsolve::install(k, h, stop);
    mc_transforms::install(k, h, stop);
    mc_limit::install(k, h, stop);
    let jit = mc_jit::install(k, h);
    mc_strings::install(k, h);
    mc_roots::install(k, h, stop);
    mc_quad::install(k, h, stop);
    mc_ode::install(k, h, stop);
    mc_graphics_kernel::install(k, h);
    mc_geom::install(k, h);
    mc_plot::install(k, h);
    mc_io::install(k, h);
    mc_io::register_exporter("graphics", &["png", "svg", "pdf", "gif"], mc_plot::export);
    mc_io::register_exporter("graphics3d", &["png", "svg", "pdf", "gif"], mc_plot::export);
    // labeled and legended values around non-pictures: the value with its label
    mc_io::register_exporter("labeled", &["png", "svg", "pdf"], mc_plot::export);
    mc_io::register_exporter("legended", &["png", "svg", "pdf"], mc_plot::export);
    // a list of graphics or plots as an animated GIF
    mc_io::register_exporter("list", &["gif"], mc_plot::export);
    // pictures written by export_data read back as their terms (gap G61)
    mc_io::register_importer(&["svg", "png"], mc_plot::import_graphics);
    // functions of truncated power series compose through the series algebra (gap G59)
    mc_numeric::series_hook::register(mc_poly::series_arith);
    mc_simplify::install(k, h, stop);
    mc_special::install(k, h, stop);
    mc_linalg::install(k, h, stop);
    mc_graphics_kernel::install_late(k, h);
    mc_solve::install(k, h, stop);
    mc_decide::install(k, h, stop);
    mc_cad::install(k, h, stop);
    mc_sums::install(k, h, stop);
    // the session generator: fresh per engine, seeded from the operating system (decision RNG.2)
    let rng = mc_random::install(k, h);
    // compiled functions draw from the same generator (item N4b)
    jit.set_rng(rng.clone());
    // distributions draw their variates from the same generator (item 3D)
    mc_dist::install(k, h, rng);
    mc_fields::install(k, h);
    mc_further_functions::install(k, h);
    mc_groups::install(k, h);
    jit
}
