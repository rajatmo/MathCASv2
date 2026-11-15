//! Monitors: options run during a computation (decision of 15 November 2026: options are values, none is held).
//!
//! `evaluation_monitor: f` is called at every evaluation of the user's function, `step_monitor: f` at every step a
//! method takes, each time with the current values of the variables as its arguments, one argument per variable in
//! the order of the variable specification (`(x, y) => sow([x, y])`; a vector variable gives a list). Its value is
//! discarded, except a failure (`throw(...)`, `break()`, any other failure): that ends the computation and is its
//! result, so `catch(find_root(..., evaluation_monitor: x => if x < 0 then throw(x)))` gives the thrown value. An
//! option value that is not a function is a failure that shows the right form.
//!
//! A monitor runs on the sequential frontier like any effect (`sow`, `print`, an assignment to a `var`), in the
//! order of the evaluations; a function with a monitor is evaluated by the interpreter (compiled code has no place
//! to call it).
use mc_core::{Heap, Raw, Store, Sym, View, T};
use mc_eval::{Abort, NativeCx, Res};
use std::sync::{Arc, Mutex};

/// The names of the monitor options.
pub const MONITOR_OPTIONS: [&str; 2] = ["evaluation_monitor", "step_monitor"];

/// The monitors of one call and the failure one of them gave (shared by every copy, so the native that set them
/// up reads what the function's evaluations met). The terms are kept unbranded, valid during the call that took
/// them from its options (no safe point falls inside an evaluation).
#[derive(Clone, Default, Debug)]
pub struct Monitors {
    /// `evaluation_monitor: f`
    evaluation: Option<Raw>,
    /// `step_monitor: f`
    step: Option<Raw>,
    stopped: Arc<Mutex<Option<Raw>>>,
    /// the monitors see only the first this many values (`n_dsolve`: the independent variable)
    first: Option<usize>,
}

impl Monitors {
    /// The monitors called with the first `k` values of the function's variables only.
    pub fn with_first(mut self, k: usize) -> Monitors {
        self.first = Some(k);
        self
    }

    /// Whether any monitor is set.
    pub fn any(&self) -> bool {
        self.evaluation.is_some() || self.step.is_some()
    }

    /// The evaluation monitor's function, for a caller that calls it itself.
    pub fn evaluation_fn<'h>(&self, h: &'h Heap) -> Option<T<'h>> {
        self.evaluation.map(|r| r.get(h))
    }

    /// Whether a step monitor is set.
    pub fn steps(&self) -> bool {
        self.step.is_some()
    }

    /// Whether a monitor has failed (the computation is to stop).
    pub fn stopped(&self) -> bool {
        self.stopped.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// The failure a monitor gave, which ends the computation and is its result.
    pub fn failure<'h>(&self, h: &'h Heap) -> Option<T<'h>> {
        self.stopped.lock().unwrap_or_else(|e| e.into_inner()).map(|r| r.get(h))
    }

    /// Call the evaluation monitor with `args` (one term per variable); `false` once a monitor has failed (the
    /// computation is to stop: the caller reports the evaluation as not numeric).
    pub fn evaluated<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<bool> {
        self.run(cx, self.evaluation, args)
    }

    /// Call the step monitor with `args`; `false` once a monitor has failed.
    pub fn stepped<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<bool> {
        self.run(cx, self.step, args)
    }

    fn run<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, f: Option<Raw>, args: &[T<'h>]) -> Res<bool> {
        if self.stopped() {
            return Ok(false);
        }
        let Some(f) = f else { return Ok(true) };
        let h = cx.heap();
        let args = &args[..self.first.map_or(args.len(), |k| k.min(args.len()))];
        let v = cx.eval(h.app(f.get(h), args))?;
        if !is_failure(h, v) {
            return Ok(true);
        }
        // a failure made inside a scratch scope does not outlive it: the native that opened the scope runs again
        // without one (the monitor's first effect would have asked for that already)
        if h.is_scratch(v) {
            return Err(Abort::Scratch);
        }
        *self.stopped.lock().unwrap_or_else(|e| e.into_inner()) = Some(Raw::new(v));
        Ok(false)
    }
}

fn is_failure(h: &Heap, t: T<'_>) -> bool {
    h.packed(t).is_none() && matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym(mc_api::names::FAILURE)))
}

/// Whether `v` is a function value: a closure (`x => e`, `fn(...)`), or a symbol with a definition (a native, rules,
/// an own value), such as `print`.
fn is_function<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>) -> bool {
    let h = cx.heap();
    if h.packed(v).is_some() {
        return false;
    }
    match h.view(v) {
        View::Sym(s) => !matches!(h.sym_name(s).as_str(), "null" | "true" | "false" | "none" | "automatic" | "auto") && cx.is_defined(s),
        View::App { head, .. } => head == h.sym_term(h.sym("fn")),
        _ => false,
    }
}

/// `failure(not_a_function, name, "...")`: the option `name` needs a function, with the form to write.
pub fn not_a_function<'h>(h: &'h Heap, name: Sym) -> T<'h> {
    let n = h.sym_name(name);
    let text = format!(
        "{n} takes a function of the current values, called at each {}: {n}: (x => sow(x)); with several variables one argument each, {n}: ((x, y) => print([x, y]))",
        if n == "step_monitor" { "step" } else { "evaluation" }
    );
    h.call(h.sym(mc_api::names::FAILURE), &[h.sym_term(h.sym("not_a_function")), h.sym_term(name), h.string(&text)])
}

/// The arguments without the monitor options of a trailing `options(...)` argument (the option list dropped when
/// nothing else is left in it), and the monitors they set; `Err` with the failure when a monitor's value is not a
/// function. `accepted` names the monitors the caller implements: another monitor option is left in place, for the
/// caller's own option check to refuse.
pub fn take_monitors<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    args: &[T<'h>],
    accepted: &[&str],
) -> Result<(Vec<T<'h>>, Monitors), T<'h>> {
    let h = cx.heap();
    let mut m = Monitors::default();
    let Some((&last, rest)) = args.split_last() else { return Ok((args.to_vec(), m)) };
    if h.packed(last).is_some() {
        return Ok((args.to_vec(), m));
    }
    let View::App { head, args: rules } = h.view(last) else { return Ok((args.to_vec(), m)) };
    if head != h.sym_term(h.sym("options")) {
        return Ok((args.to_vec(), m));
    }
    let mut kept = Vec::with_capacity(rules.len());
    for &r in rules {
        let named = match h.view(r) {
            View::App { args: &[k, v], .. } => match h.view(k) {
                View::Sym(s) if accepted.contains(&h.sym_name(s).as_str()) => Some((s, v)),
                _ => None,
            },
            _ => None,
        };
        let Some((s, v)) = named else {
            kept.push(r);
            continue;
        };
        if !is_function(cx, v) {
            return Err(not_a_function(h, s));
        }
        if h.sym_name(s) == "step_monitor" {
            m.step = Some(Raw::new(v));
        } else {
            m.evaluation = Some(Raw::new(v));
        }
    }
    let mut out = rest.to_vec();
    if !kept.is_empty() {
        out.push(h.app(head, &kept));
    }
    Ok((out, m))
}
