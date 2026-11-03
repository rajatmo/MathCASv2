//! The MathCAS v2 numeric compiler (round 3, package 1b): typed functions to machine code through Cranelift, run as
//! *compiled specialisations* of the evaluator, so a call whose arguments have the declared machine types runs
//! compiled and every other call runs as before.
//!
//! ```text
//! @compiled fn collatz_steps(k: Int) = { var c = k; var s = 0; while c != 1 { ...; s = s + 1 }; s }
//! ```
//!
//! Pipeline (spike E, decisions E.1 to E.8): [`front`] turns the function's definition (the terms `mc-syntax`
//! lowers it to) into the typed tree [`ir::Func`], inlining the functions it calls; [`ir::lower`] if-converts and
//! lowers it to three-address code [`ir::Lir`]; [`jit`] (Cranelift 0.137) or [`interp`] (the reference back end, and
//! the fallback when Cranelift fails) runs it; [`rt`] is the run-time side shared by both. [`spec`] wraps the result
//! as a specialisation, registered with `mc-eval`'s `Kernel::register_compiled`.
//!
//! **Exactness** (decision 1b.3): a compiled call gives exactly the interpreter's result, or *declines* and the
//! interpreter evaluates the call. Integers are exact `i64`s whose overflow declines; an inexact integer quotient
//! declines; machine-real operations that are not one IEEE operation (powers, square roots, sums and products of
//! more than two operands or with exact rational parts) call the arithmetic's own code; elementary functions are
//! `mc-numeric`'s shared machine functions, declining outside their real domain.
//!
//! **Interrupts and fuel** (decision E.5): every loop back edge counts down; every 1024 iterations the code reads
//! the evaluator's control block and charges 1024 fuel to the calling task (a speculative task spends its own
//! share). A stopped call declines, and the interpreter then stops at its first poll with the usual failure.
//!
//! Decisions of this package are numbered 1b.1, 1b.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod front;
pub mod interp;
pub mod ir;
pub mod jit;
pub mod rt;
pub mod spec;

pub use spec::{build, install, Backend, Built, Info, Jit};

/// The functions this crate gives a meaning, with one-line descriptions.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "compile",
        "compile(f): give the typed function f (Int and Real parameters) a compiled specialisation; f, or failure(not_compilable, f, reason)",
    ),
    (
        "compiled_info",
        "compiled_info(f): f's compiled specialisation: parameter and result types, IR sizes, code size, back end, inlined functions, state, compiled runs and declines",
    ),
];
