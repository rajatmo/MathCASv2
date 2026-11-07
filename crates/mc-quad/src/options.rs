//! The options of `n_integrate` (v2's option syntax: a trailing `options(name -> value, ...)` argument, written
//! `n_integrate(..., name: value)`), and the goals they set.
use mc_api::names;
use mc_arith::text::digits_to_bits;
use mc_arith::Arith;
use mc_core::{Heap, Store, Sym, View, T};
use mc_numfn::Policy;

/// Decimal digits of a machine real (53 bits).
pub(crate) const MACHINE_DIGITS: f64 = 15.954589770191003;

/// Extra decimal digits of working precision under `digits:` (as `mc-roots`' decision R.9).
pub(crate) const GUARD_DIGITS: u64 = 10;

/// Extra decimal digits of the result ball under `digits: d`: the result is computed to d + 3 digits and rounded
/// there, so that it prints at least d correct digits unless the value lies within 10^-(d+3) of a rounding
/// boundary (decision N2.9).
pub(crate) const RESULT_DIGITS: u64 = 3;

/// The digits of `rigorous: true` without `digits:` (a little above machine precision).
pub(crate) const RIGOROUS_DIGITS: u64 = 16;

/// The largest `digits:` accepted (the tanh-sinh rule needs about 3 d nodes of d digits each).
pub(crate) const MAX_DIGITS: u64 = 20_000;

/// The default limit of interval subdivisions of the adaptive Gauss–Kronrod method (per integral).
pub(crate) const MAX_SUBDIVISIONS: usize = 1 << 13;

/// The default limit of bisection depth (an interval of length 2^-50 of the range is not split further).
pub(crate) const MAX_RECURSION: u32 = 50;

/// A method named by `method:`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    /// adaptive Gauss–Kronrod (7-15 points, 30-61 points where the 15-point rule does not resolve the integrand)
    GaussKronrod,
    /// the double-exponential substitution (tanh-sinh, exp-sinh, sinh-sinh)
    DoubleExponential,
    /// nested one-dimensional rules, each level choosing as for one variable (decision N2.7)
    Nested,
    /// Genz–Malik adaptive cubature on hyper-rectangles
    Cubature,
    /// VEGAS+ adaptive Monte Carlo
    Vegas,
    /// randomised rank-1 lattice rule (quasi-Monte Carlo)
    Lattice,
    /// scrambled Sobol points (quasi-Monte Carlo)
    Sobol,
    /// plain Monte Carlo
    MonteCarlo,
}

/// The options of one call.
#[derive(Clone, Debug, Default)]
pub(crate) struct Options {
    /// `digits: d`: balls of d digits (plus guard digits)
    pub digits: Option<u64>,
    /// `accuracy_goal: a`: absolute error 10^-a
    pub accuracy_goal: Option<f64>,
    /// `precision_goal: p`: relative error 10^-p
    pub precision_goal: Option<f64>,
    /// `max_subdivisions: k` (or `max_steps: k`)
    pub max_subdivisions: Option<usize>,
    /// `max_recursion: k`
    pub max_recursion: Option<u32>,
    /// `method: m` (`None`: automatic)
    pub method: Option<Method>,
    /// `rigorous: true`: a ball that is guaranteed to contain the integral
    pub rigorous: bool,
    /// `compile: auto | true | false` (`None`: the default policy)
    pub compile: Option<Policy>,
    /// `max_points: n`: the most evaluations of the integrand
    pub max_points: Option<u64>,
    /// `points: n`: evaluations per VEGAS iteration, or in all for the other sampling methods
    pub points: Option<u64>,
    /// `iterations: k`: averaged VEGAS iterations
    pub iterations: Option<usize>,
    /// `seed: n`: the random seed of the sampling methods (default 0)
    pub seed: Option<u64>,
    /// `error_estimate: true`: the result as `around(value, error)` for every method
    pub error_estimate: bool,
}

/// Why the options were refused.
pub(crate) enum OptionError {
    /// an option this function does not know: `failure(unknown_option, name)`
    Unknown(Sym),
    /// a known option with a bad value: `failure(argument, n_integrate, name)`
    Bad(Sym),
}

/// The arguments before a trailing `options(...)` argument, and the options it gives. The last occurrence of a
/// repeated option wins.
pub(crate) fn split<'a, 'h>(
    h: &'h Heap,
    ar: &Arith<'h, Heap>,
    args: &'a [T<'h>],
) -> Result<(&'a [T<'h>], Options), OptionError> {
    let mut o = Options::default();
    let Some((&last, rest)) = args.split_last() else { return Ok((args, o)) };
    let View::App { head, args: rules } = h.view(last) else { return Ok((args, o)) };
    if head != h.sym_term(h.sym("options")) {
        return Ok((args, o));
    }
    let rule = h.sym_term(h.sym(names::RULE));
    for &r in rules {
        let View::App { head, args: &[k, v] } = h.view(r) else { return Ok((args, o)) };
        if head != rule {
            return Ok((args, o));
        }
        let View::Sym(name) = h.view(k) else { return Ok((args, o)) };
        let bad = || OptionError::Bad(name);
        let symbol = || match h.view(v) {
            View::Sym(s) => Some(h.sym_name(s)),
            _ => None,
        };
        match h.sym_name(name).as_str() {
            "digits" => match h.view(v) {
                View::Int(d @ 1..) if d as u64 <= MAX_DIGITS => o.digits = Some(d as u64),
                _ => return Err(bad()),
            },
            "accuracy_goal" => o.accuracy_goal = Some(goal(h, ar, v).ok_or_else(bad)?),
            "precision_goal" => o.precision_goal = Some(goal(h, ar, v).ok_or_else(bad)?),
            "max_subdivisions" | "max_steps" => match h.view(v) {
                View::Int(k @ 1..=10_000_000) => o.max_subdivisions = Some(k as usize),
                _ => return Err(bad()),
            },
            "max_recursion" => match h.view(v) {
                View::Int(k @ 1..=1000) => o.max_recursion = Some(k as u32),
                _ => return Err(bad()),
            },
            "method" => {
                o.method = match symbol().as_deref() {
                    Some("auto" | "automatic") => None,
                    Some("gauss_kronrod") => Some(Method::GaussKronrod),
                    Some("double_exponential") => Some(Method::DoubleExponential),
                    Some("nested") => Some(Method::Nested),
                    Some("cubature") => Some(Method::Cubature),
                    Some("vegas") => Some(Method::Vegas),
                    Some("lattice") => Some(Method::Lattice),
                    Some("sobol") => Some(Method::Sobol),
                    Some("monte_carlo") => Some(Method::MonteCarlo),
                    _ => return Err(bad()),
                }
            }
            "max_points" => match h.view(v) {
                View::Int(k @ 1..) => o.max_points = Some(k as u64),
                _ => return Err(bad()),
            },
            "points" => match h.view(v) {
                View::Int(k @ 16..) => o.points = Some(k as u64),
                _ => return Err(bad()),
            },
            "iterations" => match h.view(v) {
                View::Int(k @ 1..=100_000) => o.iterations = Some(k as usize),
                _ => return Err(bad()),
            },
            "seed" => match h.view(v) {
                View::Int(k) => o.seed = Some(k as u64),
                _ => return Err(bad()),
            },
            "error_estimate" => {
                o.error_estimate = match symbol().as_deref() {
                    Some(names::TRUE) => true,
                    Some(names::FALSE) => false,
                    _ => return Err(bad()),
                }
            }
            "rigorous" => {
                o.rigorous = match symbol().as_deref() {
                    Some(names::TRUE) => true,
                    Some(names::FALSE) => false,
                    _ => return Err(bad()),
                }
            }
            "compile" => {
                o.compile = match symbol().as_deref() {
                    Some("auto" | "automatic") => Some(Policy::Auto { after: mc_numfn::DEFAULT_AFTER }),
                    Some(names::TRUE) => Some(Policy::Compiled),
                    Some(names::FALSE) => Some(Policy::Interpreted),
                    _ => return Err(bad()),
                }
            }
            _ => return Err(OptionError::Unknown(name)),
        }
    }
    Ok((rest, o))
}

/// A goal in digits: a non-negative real number, or `inf`.
fn goal<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, v: T<'h>) -> Option<f64> {
    if let View::Sym(s) = h.view(v) {
        return (h.sym_name(s) == "inf").then_some(f64::INFINITY);
    }
    let g = ar.num(v)?.as_real()?.to_f64();
    (g >= 0.0 && !g.is_nan()).then_some(g)
}

/// The error goals of an integration: converged when the error estimate is within the absolute or the relative
/// goal.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Goals {
    /// absolute tolerance (0 for `accuracy_goal: inf`)
    pub abs: f64,
    /// relative tolerance
    pub rel: f64,
}

impl Goals {
    /// The error allowed for an integral of magnitude `v`.
    pub fn tol(&self, v: f64) -> f64 {
        self.abs.max(self.rel * v)
    }
}

impl Options {
    /// The goals at machine precision (decision N2.6): relative error 10^-precision_goal (default: half the
    /// machine digits), absolute 10^-accuracy_goal (default `inf`: none, as customary for integrals, whose scale
    /// the absolute goal does not know).
    pub fn machine_goals(&self) -> Goals {
        let pg = self.precision_goal.unwrap_or(MACHINE_DIGITS / 2.0);
        Goals { abs: pow10(self.accuracy_goal.unwrap_or(f64::INFINITY)), rel: pow10(pg) }
    }

    /// The goals under `digits: d` in log2 terms (10^-goal may underflow a machine real): relative error
    /// 10^-(precision_goal + RESULT_DIGITS) (default precision goal d: the digits asked for, decision N2.9),
    /// absolute 10^-accuracy_goal (default none).
    pub fn ball_goals_log2(&self, d: u64) -> (f64, f64) {
        let l = std::f64::consts::LOG2_10;
        let pg = self.precision_goal.map_or(d as f64, |g| g.min(d as f64)) + RESULT_DIGITS as f64;
        (-self.accuracy_goal.unwrap_or(f64::INFINITY) * l, -pg * l)
    }

    /// The working precision in bits under `digits: d` (d plus guard digits) and the precision the result is
    /// rounded to.
    pub fn ball_precision(&self, d: u64) -> (u64, u64) {
        (digits_to_bits(d + GUARD_DIGITS), digits_to_bits(d + RESULT_DIGITS))
    }

    /// The subdivision limit.
    pub fn max_subdivisions(&self) -> usize {
        self.max_subdivisions.unwrap_or(MAX_SUBDIVISIONS)
    }

    /// The bisection depth limit.
    pub fn max_recursion(&self) -> u32 {
        self.max_recursion.unwrap_or(MAX_RECURSION)
    }

    /// When the integrand is compiled.
    pub fn policy(&self) -> Policy {
        self.compile.unwrap_or_default()
    }
}

/// 10^-g as a machine real (0 for g = inf).
fn pow10(g: f64) -> f64 {
    if g.is_infinite() {
        0.0
    } else {
        10f64.powf(-g)
    }
}
