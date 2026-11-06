//! The options of `n_dsolve` and `n_dsolve_value` (v2's option syntax: a trailing `options(name -> value, ...)`
//! argument, written `f(..., name: value)`), and the tolerances they set.
use mc_api::names;
use mc_arith::Arith;
use mc_core::{Heap, Store, Sym, View, T};
use mc_numfn::Policy;

/// Decimal digits of a machine real (53 bits).
pub(crate) const MACHINE_DIGITS: f64 = 15.954589770191003;

/// The largest `digits:` accepted (a Taylor step of order d costs about d^2 ball products per operation).
pub(crate) const MAX_DIGITS: u64 = 2000;

/// The default step limit (decision N3.9).
pub(crate) const MAX_STEPS: usize = 1_000_000;

/// The integration method named by `method:`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    /// Dormand–Prince 5(4), switching to `Stiff` when the step size control detects stiffness (the default)
    Auto,
    /// Dormand–Prince 5(4) only
    Rk45,
    /// the Rosenbrock method Rodas4 with the Jacobian from `d`
    Stiff,
    /// the Taylor series method on balls (the method under `digits:`)
    Taylor,
}

impl Method {
    fn from_name(s: &str) -> Option<Method> {
        Some(match s {
            "auto" => Method::Auto,
            "rk45" => Method::Rk45,
            "stiff" => Method::Stiff,
            "taylor" => Method::Taylor,
            _ => return None,
        })
    }
}

/// What a located event does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EventAction {
    /// end the integration at the first event
    Stop,
    /// record every event and go on
    Record,
}

/// The options of one call.
#[derive(Clone, Debug)]
pub(crate) struct Options<'h> {
    /// `digits: d`: d correct digits, by the Taylor method on balls when d is above machine precision
    pub digits: Option<u64>,
    /// `accuracy_goal: a`: absolute error 10^-a
    pub accuracy_goal: Option<f64>,
    /// `precision_goal: p`: relative error 10^-p
    pub precision_goal: Option<f64>,
    /// `max_steps: k`
    pub max_steps: Option<usize>,
    /// `max_step_size: h`
    pub max_step_size: Option<f64>,
    /// `max_step_fraction: f`: the largest step as a fraction of the range of the integration
    pub max_step_fraction: Option<f64>,
    /// `method: m`
    pub method: Method,
    /// `events: [g == 0, ...]` (the term as given)
    pub events: Option<T<'h>>,
    /// `event_action: stop | record`
    pub event_action: EventAction,
    /// `compile: auto | true | false`: when the right-hand side is compiled
    pub policy: Policy,
}

impl Options<'_> {
    /// The largest step: `max_step_size` and `max_step_fraction` of the range `width`, the smaller.
    pub fn hmax(&self, width: f64) -> Option<f64> {
        let f = self.max_step_fraction.map(|f| f * width.abs()).filter(|h| *h > 0.0);
        match (self.max_step_size, f) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
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
) -> Result<(&'a [T<'h>], Options<'h>), OptionError> {
    let mut o = Options {
        digits: None,
        accuracy_goal: None,
        precision_goal: None,
        max_steps: None,
        max_step_size: None,
        max_step_fraction: None,
        method: Method::Auto,
        events: None,
        event_action: EventAction::Stop,
        policy: Policy::default(),
    };
    let Some((&last, rest)) = args.split_last() else { return Ok((args, o)) };
    let View::App { head, args: rules } = h.view(last) else { return Ok((args, o)) };
    if head != h.sym_term(h.sym("options")) {
        return Ok((args, o));
    }
    let rule = h.sym_term(h.sym(names::RULE));
    let symbol = |v: T<'h>| match h.view(v) {
        View::Sym(s) => Some(h.sym_name(s)),
        _ => None,
    };
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
            "max_steps" => match h.view(v) {
                View::Int(k @ 1..=100_000_000) => o.max_steps = Some(k as usize),
                _ => return Err(bad()),
            },
            "max_step_size" => match ar.num(v).and_then(|n| n.as_real().map(|r| r.to_f64())) {
                Some(x) if x > 0.0 && x.is_finite() => o.max_step_size = Some(x),
                _ => return Err(bad()),
            },
            "max_step_fraction" => match ar.num(v).and_then(|n| n.as_real().map(|r| r.to_f64())) {
                Some(x) if x > 0.0 && x <= 1.0 => o.max_step_fraction = Some(x),
                _ => return Err(bad()),
            },
            "method" => o.method = symbol(v).and_then(|s| Method::from_name(&s)).ok_or_else(bad)?,
            "events" => o.events = Some(v),
            "event_action" => {
                o.event_action = match symbol(v).as_deref() {
                    Some("stop") => EventAction::Stop,
                    Some("record") => EventAction::Record,
                    _ => return Err(bad()),
                }
            }
            "compile" => {
                o.policy = match symbol(v).as_deref() {
                    Some("auto") => Policy::default(),
                    Some(names::TRUE) => Policy::Compiled,
                    Some(names::FALSE) => Policy::Interpreted,
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

/// The error tolerances of an integration: a step is accepted when its error estimate is within
/// `abs + rel * |y|` componentwise (in the root-mean-square norm).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tolerance {
    /// absolute tolerance 10^-accuracy_goal
    pub abs: f64,
    /// relative tolerance 10^-precision_goal
    pub rel: f64,
}

/// The smallest relative tolerance a machine integration accepts: below it rounding errors dominate the error
/// estimates and the step size control fails.
const MIN_REL: f64 = 1e-14;

impl Options<'_> {
    /// Whether the Taylor method on balls runs: under `digits:` above machine precision, or `method: taylor`.
    pub fn high_precision(&self) -> bool {
        self.method == Method::Taylor || self.digits.is_some_and(|d| d as f64 > MACHINE_DIGITS)
    }

    /// The machine tolerances (decision N3.3): the goals, by default half the machine digits (8), or the asked
    /// `digits:` at or below machine precision; the relative tolerance is clamped to about 100 units of rounding.
    pub fn tolerance(&self) -> Tolerance {
        let default = self.digits.map_or(8.0, |d| d as f64);
        let ag = self.accuracy_goal.unwrap_or(default);
        let pg = self.precision_goal.unwrap_or(default);
        Tolerance { abs: 10f64.powf(-ag), rel: 10f64.powf(-pg).max(MIN_REL) }
    }

    /// The digits the Taylor method aims for: `digits:`, or machine precision for `method: taylor` alone.
    pub fn taylor_digits(&self) -> u64 {
        self.digits.unwrap_or(16)
    }

    /// The step limit.
    pub fn step_limit(&self) -> usize {
        self.max_steps.unwrap_or(MAX_STEPS)
    }
}
