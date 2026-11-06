//! The natives behind the notebook protocol (docs/design/08-notebook.md, 5.2): `out`, the history of numbered
//! statements, and `message`, a warning of library code that front ends show apart from `print` output; and
//! `timing`, the wall-clock time of an evaluation (for stage timings in scripts and notebooks).
//!
//! Both keep their state in own values of hidden symbols (names that are not identifiers, so no program can
//! write them): the number of the running statement in `out#line` and the value of statement `n` in `out#n`. The
//! evaluator records reads of own values, so a memoised `out()` is stale as soon as the line number changes.
use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, CallKind, Hold, Kernel, Native, NativeCx, Reply, Res};

/// The functions of this module with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "out",
        "out(n): the value of numbered statement n of a notebook session; out() the last value, out(-k) the k-th previous one",
    ),
    (
        "message",
        "message(tag, text): a warning from tag (a function's name); front ends show it apart from print output",
    ),
    (
        "eval",
        "eval(e): the value of e; inside an argument a function holds, it marks e to be evaluated first",
    ),
    (
        "attributes",
        "attributes(f): the list of the attributes of the symbol f (listable, hold_all, orderless, ...)",
    ),
    ("set_attributes", "set_attributes(f, a), set_attributes(f, [a, ...]): give the symbol f the attributes"),
    ("clear_attributes", "clear_attributes(f, a), clear_attributes(f, [a, ...]): remove the attributes from f"),
    ("composition", "composition(f, g, ...): the function x => f(g(...(x))); applied to arguments it calls them in turn"),
    ("right_composition", "right_composition(f, g, ...): the function x => ...(g(f(x))), the functions applied left to right"),
    ("through", "through(p(f, g, ...)(x)): p(f(x), g(x), ...); through(e, p) only when the inner head is p"),
    ("activate", "activate(e): every inert head inactive(f) in e replaced by f, and the result evaluated"),
    ("inactive", "inactive(f): an inert head: inactive(f)(x) stays a call until activate"),
    (
        "timing",
        "timing(e): [seconds, value]: e evaluated, with the wall-clock time the evaluation took in seconds; timing(e, breakdown: true) adds where the time went",
    ),
    (
        "profile",
        "profile(e): [seconds, value, rows]: e evaluated with every call measured; rows [f, kind, calls, own seconds, total seconds] by own time",
    ),
];

/// The hidden symbol holding the number of the running numbered statement.
pub(crate) const LINE: &str = "out#line";

/// The hidden symbol holding the value of numbered statement `n`.
pub(crate) fn entry(n: u64) -> String {
    format!("out#{n}")
}

/// Register `out`, `message` and `timing`.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    k.register(h.sym("out"), Attributes::default(), Box::new(Out));
    k.register(h.sym("message"), Attributes { effect: true, ..Attributes::default() }, Box::new(Message));
    // the argument is held (it is timed here, not before the call); reading the clock is an effect, so a timing is
    // never memoised or run speculatively
    k.register(h.sym("timing"), Attributes { hold: Hold::All, effect: true, ..Attributes::default() }, Box::new(Timing));
    k.register(h.sym("profile"), Attributes { hold: Hold::All, effect: true, ..Attributes::default() }, Box::new(Profile));
}

/// A warning in the session output: `message(tag, text)` with `text` a string. `None` for anything else.
pub(crate) fn as_message(h: &Heap, t: T<'_>) -> Option<(String, String)> {
    let View::App { head, args } = h.view(t) else { return None };
    let (View::Sym(s), [tag, text]) = (h.view(head), args) else { return None };
    if h.sym_name(s) != "message" {
        return None;
    }
    let tag = match h.view(*tag) {
        View::Sym(t) => h.sym_name(t),
        View::Str(t) => t.to_string(),
        _ => return None,
    };
    let View::Str(text) = h.view(*text) else { return None };
    Some((tag, text.to_string()))
}

/// `out(n)`, `out()`, `out(-k)`.
struct Out;

impl Native for Out {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let k = match args {
            [] => -1,
            [n] => match h.view(*n) {
                View::Int(n) if n != 0 => n,
                _ => return Ok(Reply::Done(failure(h, &[h.sym_term(h.sym("out")), h.int(1)], "argument"))),
            },
            _ => return Ok(Reply::Decline),
        };
        let line = cx.value(h.sym(LINE)).and_then(|t| match h.view(t) {
            View::Int(n) => Some(n),
            _ => None,
        });
        // relative to the running statement; outside a numbered evaluation only absolute numbers make sense
        let n = if k > 0 { Some(k) } else { line.map(|l| l + k).filter(|&n| n > 0) };
        let value = n.and_then(|n| cx.value(h.sym(&entry(n as u64))));
        Ok(Reply::Done(match value {
            Some(v) => v,
            None => failure(h, &[h.int(n.unwrap_or(k))], "history"),
        }))
    }
}

/// `failure(tag, details...)`.
fn failure<'h>(h: &'h Heap, details: &[T<'h>], tag: &str) -> T<'h> {
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

/// `message(tag, text)`: puts the warning into the session output, as `print` does with its argument.
struct Message;

impl Native for Message {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [tag, text] = args else { return Ok(Reply::Decline) };
        if !matches!(h.view(*tag), View::Sym(_) | View::Str(_)) || !matches!(h.view(*text), View::Str(_)) {
            return Ok(Reply::Decline);
        }
        cx.emit(h.call(h.sym("message"), &[*tag, *text]))?;
        Ok(Reply::Done(cx.null()))
    }
}

/// `timing(e)`: `[seconds, value]`, the value of `e` and the wall-clock seconds its evaluation took.
struct Timing;

impl Native for Timing {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let (e, breakdown) = match *args {
            [e] => (e, false),
            [e, o] => match option_value(cx, o, "breakdown")? {
                Some(v) => (e, v == cx.heap().sym_term(cx.heap().sym(names::TRUE))),
                None => return Ok(Reply::Decline),
            },
            _ => return Ok(Reply::Decline),
        };
        // reading the clock is an effect: it keeps the result out of the memo (a second timing of the same term
        // measures again)
        cx.effect()?;
        if !breakdown {
            let start = std::time::Instant::now();
            let value = cx.eval(e)?;
            let seconds = start.elapsed().as_secs_f64();
            let h = cx.heap();
            return Ok(Reply::Done(cx.list(&[h.real(seconds), value])));
        }
        let young0 = cx.heap().young_bytes();
        let start = std::time::Instant::now();
        let (value, lines) = cx.profiled(e)?;
        let seconds = start.elapsed().as_secs_f64();
        let allocated = cx.heap().young_bytes().saturating_sub(young0);
        let h = cx.heap();
        let lines = lines.unwrap_or_default();
        let own = |kind: CallKind| lines.iter().filter(|l| l.kind == kind).fold(0.0, |a, l| a + l.own);
        let (compiled, library) = (own(CallKind::Compiled), own(CallKind::Native));
        let entry = |k: &str, v: T<'h>| h.call(h.sym(names::RULE), &[h.string(k), v]);
        // the definitions' own time and the evaluation outside every measured call are the interpreter's; no
        // collection runs inside an evaluation (collections run between statements), so `gc` is 0 by design
        let parts = h.call(
            h.sym("assoc"),
            &[
                entry("compiled", h.real(compiled)),
                entry("library", h.real(library)),
                entry("interpreter", h.real((seconds - compiled - library).max(0.0))),
                entry("gc", h.real(0.0)),
                entry("allocated_mb", h.real(allocated as f64 / (1u64 << 20) as f64)),
            ],
        );
        Ok(Reply::Done(cx.list(&[h.real(seconds), value, parts])))
    }
}

/// The value of option `name` in the held option list `o` (`options(name -> v)`, evaluated); `None` when `o` is no
/// such list.
fn option_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, o: T<'h>, name: &str) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let View::App { head, args: &[r] } = h.view(o) else { return Ok(None) };
    if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "options") {
        return Ok(None);
    }
    let View::App { head: rh, args: &[k, v] } = h.view(r) else { return Ok(None) };
    let rule = matches!(h.view(rh), View::Sym(s) if h.sym_name(s) == names::RULE || h.sym_name(s) == names::RULE_DELAYED);
    if !rule || !matches!(h.view(k), View::Sym(s) if h.sym_name(s) == name) {
        return Ok(None);
    }
    Ok(Some(cx.eval(v)?))
}

/// `profile(e)`: `[seconds, value, rows]`, the value of `e` and where the time of its evaluation went: one row
/// `[f, kind, calls, own, total]` per function `f` and kind of call (`"library"` for natives, `"compiled"` for
/// compiled code, `"definitions"` for functions defined by rules), sorted by own time (the time not spent in other
/// measured calls), largest first; then a row `[evaluation, "interpreter", 1, own, seconds]` for the evaluation
/// outside every measured call. `profile(e, top: n)` keeps the first n rows of functions (default 25).
struct Profile;

impl Native for Profile {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let (e, top) = match *args {
            [e] => (e, 25),
            [e, o] => match option_value(cx, o, "top")?.map(|v| cx.heap().view(v)) {
                Some(View::Int(n)) if n > 0 => (e, n as usize),
                _ => return Ok(Reply::Decline),
            },
            _ => return Ok(Reply::Decline),
        };
        cx.effect()?;
        let start = std::time::Instant::now();
        let (value, lines) = cx.profiled(e)?;
        let seconds = start.elapsed().as_secs_f64();
        let h = cx.heap();
        let Some(mut lines) = lines else {
            // inside another profile: that one measures
            return Ok(Reply::Done(cx.list(&[h.real(seconds), value, cx.list(&[])])));
        };
        lines.sort_by(|a, b| b.own.total_cmp(&a.own).then_with(|| b.total.total_cmp(&a.total)));
        let outside = (seconds - lines.iter().fold(0.0, |a, l| a + l.own)).max(0.0);
        let mut rows: Vec<T<'h>> = lines
            .iter()
            .take(top)
            .map(|l| {
                let row = [h.sym_term(l.sym), h.string(l.kind.name()), h.int(l.calls as i64), h.real(l.own), h.real(l.total)];
                cx.list(&row)
            })
            .collect();
        let rest = [h.sym_term(h.sym("evaluation")), h.string("interpreter"), h.int(1), h.real(outside), h.real(seconds)];
        rows.push(cx.list(&rest));
        let rows = cx.list(&rows);
        Ok(Reply::Done(cx.list(&[h.real(seconds), value, rows])))
    }
}
