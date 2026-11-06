//! The natives behind the notebook protocol (docs/design/08-notebook.md, 5.2): `out`, the history of numbered
//! statements, and `message`, a warning of library code that front ends show apart from `print` output; and
//! `timing`, the wall-clock time of an evaluation (for stage timings in scripts and notebooks).
//!
//! Both keep their state in own values of hidden symbols (names that are not identifiers, so no program can
//! write them): the number of the running statement in `out#line` and the value of statement `n` in `out#n`. The
//! evaluator records reads of own values, so a memoised `out()` is stale as soon as the line number changes.
use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Hold, Kernel, Native, NativeCx, Reply, Res};

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
    (
        "timing",
        "timing(e): [seconds, value]: e evaluated, with the wall-clock time the evaluation took in seconds",
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
        let [e] = args else { return Ok(Reply::Decline) };
        // reading the clock is an effect: it keeps the result out of the memo (a second timing of the same term
        // measures again)
        cx.effect()?;
        let start = std::time::Instant::now();
        let value = cx.eval(*e)?;
        let seconds = start.elapsed().as_secs_f64();
        let h = cx.heap();
        Ok(Reply::Done(cx.list(&[h.real(seconds), value])))
    }
}
