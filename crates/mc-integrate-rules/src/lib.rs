//! Rule-based indefinite integration (roadmap phase 14, item 14C).
//!
//! A table of integration rules for the forms the tower-based engine (`mc-integrate`) does not reach: powers and
//! products of trigonometric and hyperbolic functions (including fractional powers, with elliptic and
//! hypergeometric answers), products with powers of `x`, inverse trigonometric and inverse hyperbolic functions
//! times algebraic factors, binomials `x^m (a + b x^n)^p` with hypergeometric answers, and logarithms of
//! trigonometric functions with polylogarithm answers.
//!
//! The rules are translated from Rubi, Rich's rule-based integrator (Albert D. Rich, *Rule-based Mathematics:
//! Symbolic Integration Rules*, rulebasedintegration.org; MIT licence, see `THIRD-PARTY-NOTICES`) into v2's pattern
//! notation (`rules/*.rules`, see [`syntax`]); the reduction formulas themselves are the classical ones (Gradshteyn
//! and Ryzhik, sections 2.5-2.8; Abramowitz and Stegun 15.3 for the hypergeometric forms).
//!
//! - [`syntax`]: the notation of the rule files and its parser (owned nodes; parsed once per process);
//! - [`table`]: the table and its dispatch (a bit per function; a rule is a candidate only when every function its
//!   left side names occurs in the integrand);
//! - [`run`]: the interpreter (patterns lowered to the evaluator's matcher, with optional arguments and their unit
//!   defaults; conditions decided natively on the bound terms; right sides built by canonical arithmetic; `int`
//!   recursion bounded by depth, steps and time);
//! - [`cond`], [`util`]: the predicates and term helpers.
//!
//! Answers are not checked here: the caller (`mc-calculus`'s integration portfolio) differentiates every answer
//! before returning it.
#![warn(missing_docs)]

pub mod cond;
mod methods;
pub mod run;
pub mod syntax;
pub mod table;
pub mod util;

pub use run::{Fallback, Limits, Run, Stats};

use mc_api::T;
use mc_eval::{NativeCx, Res};

/// The outcome of [`integrate`].
pub struct Outcome<'h> {
    /// the antiderivative, if found (not yet checked)
    pub answer: Option<T<'h>>,
    /// the rules applied, outermost first: (file, line, section)
    pub fired: Vec<(&'static str, usize, &'static str)>,
    /// counters
    pub stats: Stats,
}

/// An antiderivative of `f` in `x` by the rule table, trying `fallback` (the caller's quick methods) on every
/// integrand first. `Ok(Outcome { answer: None, .. })` when no rule applies or a limit is reached; an error of the
/// evaluator (interrupt, fuel) is passed on.
pub fn integrate<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    x: T<'h>,
    limits: Limits,
    fallback: &mut Fallback<'_, 'h>,
) -> Res<Outcome<'h>> {
    let mut run = match Run::new(cx, x, limits, fallback) {
        Ok(r) => r,
        // a bad rule file is a build error the table tests catch; at run time the table is simply absent
        Err(_) => return Ok(Outcome { answer: None, fired: vec![], stats: Stats::default() }),
    };
    let start = std::time::Instant::now();
    let answer = run.int(f)?;
    if std::env::var_os("MC_RULES_STATS").is_some() {
        let s = &run.stats;
        eprintln!(
            "rules-stats found={} integrands={} candidates={} matched={} dispatch_us={} matching_us={} total_us={} exhausted={}",
            answer.is_some(),
            s.integrands,
            s.candidates,
            s.matched,
            s.dispatch.as_micros(),
            s.matching.as_micros(),
            start.elapsed().as_micros(),
            s.exhausted
        );
    }
    let fired = run
        .fired
        .iter()
        .map(|&id| {
            let r = run.rule(id);
            (r.file, r.text.line, r.text.section.as_str())
        })
        .collect();
    Ok(Outcome { answer, fired, stats: run.stats })
}
