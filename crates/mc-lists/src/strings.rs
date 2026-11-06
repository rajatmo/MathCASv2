//! Strings: `string_join`, `to_string`, `string_length`, `split`.
use crate::ctx::{charge_copy, take, Lib, Meter};
use mc_arith::num::{Nat, StopScope};
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};
use mc_syntax::{DefaultNumberFormat, NumberFormat, Printer};

/// Number text for `to_string`, from `mc-arith` (the engine's printer uses the same): big integers by its
/// divide-and-conquer conversion, balls with their correct digits only.
struct Numbers;

impl NumberFormat for Numbers {
    fn big_int(&self, mag: &[u64]) -> String {
        Nat::from_limbs(mag.to_vec()).to_decimal()
    }

    fn ball(&self, neg: bool, words: &[u64]) -> String {
        mc_arith::text::ball_words(neg, words).unwrap_or_else(|| DefaultNumberFormat.ball(neg, words))
    }
}

/// `string_join(s1, s2, ...)` or `string_join([s1, ...])`, lists flattened: the strings concatenated.
pub(crate) fn string_join<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let list = h.sym_term(lib.s.list);
    let mut out = String::new();
    let mut m = Meter::default();
    // (strings, next position), and the argument of the call each top-level item came from
    let mut stack: Vec<(&[T<'h>], usize)> = vec![(args, 0)];
    let mut arg = 0;
    while let Some(top) = stack.last_mut() {
        let (xs, i) = *top;
        if i == xs.len() {
            stack.pop();
            continue;
        }
        top.1 += 1;
        if stack.len() == 1 {
            arg = i + 1;
        }
        m.step(cx)?;
        match h.view(xs[i]) {
            View::Str(s) => {
                charge_copy(cx, s.len())?;
                out.push_str(s);
            }
            View::App { head, args } if head == list => stack.push((args, 0)),
            View::Sym(_) | View::App { .. } => return Ok(Reply::Decline),
            _ => return Ok(Reply::Done(lib.bad_arg(h, t, arg))),
        }
    }
    Ok(Reply::Done(h.string(&out)))
}

/// `to_string(t)`: a string itself, any other term as the printer writes it.
pub(crate) fn to_string<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // the text form is the input form: `to_string(x, input_form)` and a page width are accepted (decision io-core.9)
    let (args, _) = crate::ctx::option(h, args, "page_width");
    let x = match *args {
        [x] => x,
        [x, form] if matches!(h.view(form), View::Sym(s) if ["input_form", "output_form", "standard_form"].contains(&h.sym_name(s).as_str())) => {
            x
        }
        _ => return Ok(Reply::Decline),
    };
    match h.view(x) {
        View::Str(_) => return Ok(Reply::Done(x)),
        // the common case, without setting up a printer
        View::Int(v) => return Ok(Reply::Done(h.string(&v.to_string()))),
        _ => {}
    }
    // the printer's work is linear in the term; huge numbers are converted with the stop flag polled
    cx.tick(1)?;
    let text = {
        let _scope = StopScope::enter(Some(&lib.stop));
        Printer::with_numbers(h, Box::new(Numbers)).with_display().expr(x)
    };
    charge_copy(cx, text.len())?;
    Ok(Reply::Done(h.string(&text)))
}

/// `string_length(s)`: the number of characters (Unicode scalar values).
pub(crate) fn string_length<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [s] = *args else {
        return Ok(Reply::Decline);
    };
    let s = take!(string(lib, h, t, s, 1));
    charge_copy(cx, s.len())?;
    Ok(Reply::Done(h.int(s.chars().count() as i64)))
}

/// Argument `k` of the call `t` as a string: a symbol or compound term declines, another atom is a failure.
fn string<'h>(lib: &Lib, h: &'h mc_core::Heap, t: T<'h>, a: T<'h>, k: usize) -> Result<&'h str, Reply<'h>> {
    match h.view(a) {
        View::Str(s) => Ok(s),
        View::Sym(_) | View::App { .. } => Err(Reply::Decline),
        _ => Err(Reply::Done(lib.bad_arg(h, t, k))),
    }
}

/// `split(s)`: the words of `s` (runs of non-whitespace); `split(s, sep)`: the pieces between occurrences of the
/// nonempty string `sep`, empty pieces included.
pub(crate) fn split<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // a list: its runs of identical elements, or of neighbours for which the test holds (decision p6core.7)
    if let Some(&xs) = args.first() {
        if let View::App { head, args: elems } = h.view(xs) {
            if head == h.sym_term(lib.s.list) {
                return split_runs(lib, cx, elems, args.get(1).copied());
            }
        }
    }
    let (s, sep) = match *args {
        [s] => (take!(string(lib, h, t, s, 1)), None),
        [s, sep] => {
            let s = take!(string(lib, h, t, s, 1));
            match take!(string(lib, h, t, sep, 2)) {
                "" => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
                sep => (s, Some(sep)),
            }
        }
        _ => return Ok(Reply::Decline),
    };
    charge_copy(cx, s.len())?;
    let mut m = Meter::default();
    let mut out = vec![];
    let pieces: Box<dyn Iterator<Item = &str>> = match sep {
        None => Box::new(s.split_whitespace()),
        Some(sep) => Box::new(s.split(sep)),
    };
    for p in pieces {
        m.step(cx)?;
        out.push(h.string(p));
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// `split(xs)`, `split(xs, test)`: the runs of consecutive identical elements (or neighbours `a, b` with
/// `test(a, b)` true).
fn split_runs<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, xs: &[T<'h>], test: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut m = Meter::default();
    let mut out = vec![];
    let mut start = 0;
    for i in 1..=xs.len() {
        m.step(cx)?;
        let cut = i == xs.len()
            || match test {
                None => xs[i] != xs[i - 1],
                Some(f) => {
                    let v = crate::ctx::apply(cx, f, &[xs[i - 1], xs[i]])?;
                    if lib.is_failure(h, v) {
                        return Ok(Reply::Done(v));
                    }
                    !cx.is_true(v)
                }
            };
        if cut && i > start {
            out.push(cx.list(&xs[start..i]));
            start = i;
        }
    }
    Ok(Reply::Done(cx.list(&out)))
}
