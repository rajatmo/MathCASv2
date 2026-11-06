//! Parallel evaluation (decision sp-core.4): `parallel_table`, `parallel_map`, `parallel_sum`, and the option
//! `parallel: true` of `table` and `map`.
//!
//! The values are split over the session's worker threads at once, in runs of about `n / (4 threads)` elements,
//! whatever their estimated cost: the user asks for it, typically for parameter scans whose elements each solve
//! an equation or integrate. The runs are the evaluator's speculative tasks, so the result is exactly the serial
//! one: values come back in order; an element that cannot run speculatively (it prints, assigns, or reads a
//! mutable local of the caller) and every element a definition made since the split may have changed are
//! evaluated again in order; interrupts and the fuel, memory and time limits stop every worker. With one worker
//! thread, or inside a scratch scope, the values are evaluated one after another.
use crate::ctx::{apply, Lib};
use mc_api::names;
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The arguments without a trailing `options(parallel -> v)` and whether `v` is `true`; `None` when the last
/// argument is no such option list. `v` is evaluated (the argument may be held).
pub(crate) fn option<'a, 'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &'a [T<'h>]) -> Res<Option<(&'a [T<'h>], bool)>> {
    let h = cx.heap();
    let Some((&last, rest)) = args.split_last() else { return Ok(None) };
    if h.packed(last).is_some() {
        return Ok(None);
    }
    let View::App { head, args: &[rule] } = h.view(last) else { return Ok(None) };
    if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "options") {
        return Ok(None);
    }
    let View::App { head: r, args: &[k, v] } = h.view(rule) else { return Ok(None) };
    let is_rule = matches!(h.view(r), View::Sym(s) if h.sym_name(s) == names::RULE || h.sym_name(s) == names::RULE_DELAYED);
    if !is_rule || !matches!(h.view(k), View::Sym(s) if h.sym_name(s) == "parallel") {
        return Ok(None);
    }
    let v = cx.eval(v)?;
    Ok(Some((rest, v == h.sym_term(h.sym(names::TRUE)))))
}

/// `parallel_table(e, k, r, ...)`: `table`'s values split over the worker threads.
pub(crate) fn parallel_table<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let args = match option(cx, args)? {
        Some((rest, _)) => rest,
        None => args,
    };
    crate::construct::table_with(lib, cx, t, args, true)
}

/// `parallel_sum(e, k, r, ...)`: the sum of `parallel_table(e, k, r, ...)`, the values added in the canonical
/// order once all are known.
pub(crate) fn parallel_sum<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    if args.len() < 2 {
        return Ok(Reply::Decline);
    }
    let vals = match parallel_table(lib, cx, t, args)? {
        Reply::Done(v) | Reply::Again(v) => cx.eval(v)?,
        Reply::Decline => return Ok(Reply::Decline),
    };
    let h = cx.heap();
    if h.packed(vals).is_none() && !matches!(h.view(vals), View::App { head, .. } if head == h.sym_term(lib.s.list)) {
        // a failure of the table (an iterator that is not one) is the value
        return Ok(Reply::Done(vals));
    }
    Ok(Reply::Again(h.call(h.sym("sum"), &[vals])))
}

/// `parallel_map(f, xs)`: `map(f, xs)` with the calls split over the worker threads; a non-list `xs` maps as `map`
/// does.
pub(crate) fn parallel_map<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let args = match option(cx, args)? {
        Some((rest, _)) => rest,
        None => args,
    };
    let &[f, x] = args else { return Ok(Reply::Decline) };
    map_parallel(cx, f, x)
}

/// `map(f, x)` over the worker threads: the list of `f(xi)`; anything but a list goes to `map`.
pub(crate) fn map_parallel<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let Some(xs) = cx.as_list(x) else { return Ok(Reply::Again(h.call(h.sym("map"), &[f, x]))) };
    let out = cx.group_parallel(xs.len(), &|cx: &mut NativeCx<'_, '_, 'h>, i| {
        cx.tick(1)?;
        apply(cx, f, &[xs[i]])
    })?;
    Ok(Reply::Done(cx.list(&out)))
}
