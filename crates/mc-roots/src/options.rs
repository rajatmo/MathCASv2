//! The options of `find_root`, `find_minimum` and `find_maximum` (v2's option syntax: a trailing
//! `options(name -> value, ...)` argument, written `f(..., name: value)`), and the goals they set.
use mc_api::names;
use mc_arith::text::digits_to_bits;
use mc_arith::Arith;
use mc_core::{Heap, Store, Sym, View, T};
use mc_numfn::Policy;

/// Decimal digits of a machine real (53 bits).
pub(crate) const MACHINE_DIGITS: f64 = 15.954589770191003;

/// Extra decimal digits of working precision under `digits:` (decision R.9).
pub(crate) const GUARD_DIGITS: u64 = 10;

/// The largest `digits:` accepted (beyond it the ball arithmetic of one Newton step alone takes minutes).
pub(crate) const MAX_DIGITS: u64 = 100_000;

/// An iteration method named by `method:`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    /// Newton's method with the symbolic derivative (the default for one start).
    Newton,
    /// the secant method (the default for two starts)
    Secant,
    /// Brent's method (the default for a bracket)
    Brent,
    /// Broyden's method: Newton with a finite-difference Jacobian updated by rank-one corrections
    Broyden,
    /// for minimisation: BFGS with the symbolic gradient
    Bfgs,
}

impl Method {
    fn from_name(s: &str) -> Option<Method> {
        Some(match s {
            "newton" => Method::Newton,
            "secant" => Method::Secant,
            "brent" => Method::Brent,
            "broyden" => Method::Broyden,
            "bfgs" => Method::Bfgs,
            _ => return None,
        })
    }
}

/// The options of one call.
#[derive(Clone, Debug)]
pub(crate) struct Options {
    /// `digits: d`: refine with balls at d digits (plus guard digits)
    pub digits: Option<u64>,
    /// `accuracy_goal: a`: absolute error 10^-a
    pub accuracy_goal: Option<f64>,
    /// `precision_goal: p`: relative error 10^-p
    pub precision_goal: Option<f64>,
    /// `max_iterations: k`
    pub max_iterations: Option<usize>,
    /// `method: m`
    pub method: Option<Method>,
    /// `verify: true`: an unverified root under `digits:` is a failure
    pub verify: bool,
    /// `compile: auto | true | false`: when the machine-real functions are compiled (decision N1.6)
    pub compile: Policy,
}

/// Why the options were refused.
pub(crate) enum OptionError {
    /// an option this function does not know: `failure(unknown_option, name)`
    Unknown(Sym),
    /// a known option with a bad value: `failure(argument, f, name)`
    Bad(Sym),
}

/// The arguments before a trailing `options(...)` argument, and the options it gives. The last occurrence of a
/// repeated option wins.
pub(crate) fn split<'a, 'h>(
    h: &'h Heap,
    ar: &Arith<'h, Heap>,
    args: &'a [T<'h>],
) -> Result<(&'a [T<'h>], Options), OptionError> {
    let mut o = Options {
        digits: None,
        accuracy_goal: None,
        precision_goal: None,
        max_iterations: None,
        method: None,
        verify: false,
        compile: Policy::default(),
    };
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
        match h.sym_name(name).as_str() {
            "digits" => match h.view(v) {
                View::Int(d @ 1..) if d as u64 <= MAX_DIGITS => o.digits = Some(d as u64),
                _ => return Err(bad()),
            },
            "accuracy_goal" => o.accuracy_goal = Some(goal(h, ar, v).ok_or_else(bad)?),
            "precision_goal" => o.precision_goal = Some(goal(h, ar, v).ok_or_else(bad)?),
            "max_iterations" | "max_steps" => match h.view(v) {
                View::Int(k @ 1..=1_000_000) => o.max_iterations = Some(k as usize),
                _ => return Err(bad()),
            },
            "method" => match h.view(v) {
                View::Sym(s) => o.method = Some(Method::from_name(&h.sym_name(s)).ok_or_else(bad)?),
                _ => return Err(bad()),
            },
            "verify" => match h.view(v) {
                View::Sym(s) if h.sym_name(s) == names::TRUE => o.verify = true,
                View::Sym(s) if h.sym_name(s) == names::FALSE => o.verify = false,
                _ => return Err(bad()),
            },
            "compile" => match h.view(v) {
                View::Sym(s) => {
                    o.compile = match h.sym_name(s).as_str() {
                        "auto" => Policy::default(),
                        names::TRUE => Policy::Compiled,
                        names::FALSE => Policy::Interpreted,
                        _ => return Err(bad()),
                    }
                }
                _ => return Err(bad()),
            },
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

/// The tolerances of an iteration: converged when the error estimate is within the absolute or the relative goal.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Goals {
    /// absolute tolerance 10^-accuracy_goal
    pub abs: f64,
    /// relative tolerance 10^-precision_goal
    pub rel: f64,
    /// the iteration limit
    pub max_iterations: usize,
}

impl Goals {
    /// The error allowed at a point of magnitude `x`.
    pub fn tol(&self, x: f64) -> f64 {
        self.abs.max(self.rel * x)
    }
}

/// The default iteration limit.
pub(crate) const MAX_ITERATIONS: usize = 100;

impl Options {
    /// The working precision in decimal digits: machine precision, or the `digits:` asked for.
    pub fn working_digits(&self) -> f64 {
        self.digits.map_or(MACHINE_DIGITS, |d| d as f64)
    }

    /// The goals: half the working precision unless given (decision R.4).
    pub fn goals(&self) -> Goals {
        let half = self.working_digits() / 2.0;
        let ag = self.accuracy_goal.unwrap_or(half);
        let pg = self.precision_goal.unwrap_or(half);
        Goals {
            abs: 10f64.powf(-ag),
            rel: 10f64.powf(-pg),
            max_iterations: self.max_iterations.unwrap_or(MAX_ITERATIONS),
        }
    }

    /// The goals of the machine iteration: the goals, or under `digits:` (where the goals apply to the ball
    /// refinement) the machine defaults.
    pub fn machine_goals(&self) -> Goals {
        if self.digits.is_none() {
            return self.goals();
        }
        let half = MACHINE_DIGITS / 2.0;
        Goals {
            abs: 10f64.powf(-half),
            rel: 10f64.powf(-half),
            max_iterations: self.max_iterations.unwrap_or(MAX_ITERATIONS),
        }
    }

    /// The goals in log2 terms for the ball refinement, where 10^-goal may underflow a machine real: the
    /// absolute and relative tolerances as exponents of 2.
    pub fn goals_log2(&self) -> (f64, f64) {
        let half = self.working_digits() / 2.0;
        let l = std::f64::consts::LOG2_10;
        (-self.accuracy_goal.unwrap_or(half) * l, -self.precision_goal.unwrap_or(half) * l)
    }

    /// The working precision of the ball refinement in bits (the asked digits plus guard digits) and the precision
    /// of the result, both for `digits: d`. The result keeps guard bits beyond d digits, as `n(x, d)` does
    /// (`mc-numeric`'s first precision): rounded to exactly d digits' worth of bits, the rounding error alone
    /// (half a unit in the last bit, up to half a unit in the d-th digit) often made the d-th digit uncertain, and
    /// the printer, which shows only correct digits, showed d - 1 of them. The refinement then caps the result at
    /// d digits (`refine::result_num`, decision N1.7).
    pub fn ball_precision(&self) -> Option<(u64, u64)> {
        self.digits.map(|d| (digits_to_bits(d + GUARD_DIGITS), mc_numeric::numeval::first_precision(d)))
    }
}
