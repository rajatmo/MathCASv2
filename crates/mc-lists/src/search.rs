#![allow(clippy::needless_borrow, clippy::redundant_guards)]
//! Searching terms: `free_of`, `member_of`, `position` (by a pattern) and `select_first` (by a predicate).
//!
//! Patterns are compiled by the evaluator ([`NativeCx::compile_pattern`]), so they match as in a definition:
//! modulo the operator properties of associative and commutative heads, conditions refusing effects (decision
//! 1A.5). A pattern without pattern constructs is compared by identity, without the matcher.
use crate::ctx::{take, truth, Lib, Meter, MAX_LEN};
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Pat, Pattern, Reply, Res};
use rustc_hash::FxHashSet;

/// What a search looks for: one term, or the matches of a compiled pattern.
pub(crate) enum Probe<'h> {
    Lit(T<'h>),
    Pat(Pattern),
}

impl<'h> Probe<'h> {
    /// The probe of the (evaluated) pattern term `p`; `None` when it is not a valid pattern.
    pub fn new(cx: &NativeCx<'_, '_, 'h>, p: T<'h>) -> Option<Probe<'h>> {
        let l = mc_eval::syntax::pattern(cx.heap(), p).ok()?;
        // the literal the pattern stands for: p itself, or the term inside verbatim(...)
        if let Pat::Lit(lit) = &l.pat {
            return Some(Probe::Lit(lit.get(cx.heap())));
        }
        cx.compile_pattern(l.pat).ok().map(Probe::Pat)
    }

    /// Does `t` match?
    pub fn test(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<bool> {
        match self {
            Probe::Lit(p) => Ok(*p == t),
            Probe::Pat(p) => cx.matches(p, t),
        }
    }
}

/// The probe of argument `k` of `t`, or the reply `failure(argument, f, k)`.
fn probe<'h>(lib: &Lib, cx: &NativeCx<'_, '_, 'h>, t: T<'h>, p: T<'h>, k: usize) -> Result<Probe<'h>, Reply<'h>> {
    Probe::new(cx, p).ok_or_else(|| Reply::Done(lib.bad_arg(cx.heap(), t, k)))
}

/// Is `t` a sum (product) holding every term of the sum (product) `p` and more? `a + c` is a part of `a + b + c`
/// for `free_of` (flat and orderless heads).
fn sub_sum<'h>(h: &'h mc_core::Heap, p: T<'h>, t: T<'h>) -> bool {
    if p == t || h.packed(p).is_some() || h.packed(t).is_some() {
        return false;
    }
    let (View::App { head: ph, args: pa }, View::App { head: th, args: ta }) = (h.view(p), h.view(t)) else {
        return false;
    };
    if ph != th || pa.len() >= ta.len() || !matches!(h.view(ph), View::Sym(s) if matches!(h.sym_name(s).as_str(), "add" | "mul")) {
        return false;
    }
    let mut rest: Vec<T<'h>> = ta.to_vec();
    for a in pa {
        match rest.iter().position(|b| b == a) {
            Some(i) => {
                rest.swap_remove(i);
            }
            None => return false,
        }
    }
    true
}

/// `free_of(t, p)`: does no part of `t` (`t` itself, heads and arguments at every depth) match the pattern `p`?
/// Each distinct subterm is tested once (terms are hash-consed, so a shared subterm is not walked twice).
pub(crate) fn free_of<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x, p] = *args else { return Ok(Reply::Decline) };
    let probe = take!(probe(lib, cx, t, p, 2));
    let mut seen = FxHashSet::default();
    let mut stack = vec![x];
    let mut m = Meter::default();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        m.step(cx)?;
        if probe.test(cx, u)? || sub_sum(h, p, u) {
            return Ok(Reply::Done(cx.bool(false)));
        }
        if let View::App { head, args } = h.view(u) {
            stack.extend(args.iter().rev());
            stack.push(head);
        }
    }
    Ok(Reply::Done(cx.bool(true)))
}

/// `member_of(xs, p)`: does some element of `xs` match the pattern `p` (an element itself, by identity)?
pub(crate) fn member_of<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // member_of(x, p, levels): whether a part of x at the levels matches p (P9B.8)
    if let [x, p, levels] = *args {
        let count = h.call(h.sym("count"), &[x, p, levels]);
        return Ok(Reply::Again(h.call(h.sym("greater"), &[count, h.int(0)])));
    }
    let [xs, p] = *args else { return Ok(Reply::Decline) };
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let probe = take!(probe(lib, cx, t, p, 2));
    let mut m = Meter::default();
    for &x in xs {
        m.step(cx)?;
        if probe.test(cx, x)? {
            return Ok(Reply::Done(cx.bool(true)));
        }
    }
    Ok(Reply::Done(cx.bool(false)))
}

/// `position(t, p)`: the positions of the parts of `t` matching the pattern `p`, each the list of indices that
/// `part` takes (`[]` is `t` itself, `0` a head), at every depth. A part comes after its head and arguments, and
/// arguments in order (decision 1A.6).
pub(crate) fn position<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `position(t, p, levels)` (levels counted from the top) and `heads: false` (decision io-core.11)
    let (args, heads) = crate::ctx::option(h, args, 2, "heads");
    let heads = heads.is_none_or(|v| !cx.is_false(v));
    let (x, p, levels) = match *args {
        [x, p] => (x, p, None),
        [x, p, spec] => match crate::levels::level_spec(h, spec) {
            Some(l) if l.contains_level(0).is_some() => (x, p, Some(l)),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    let probe = take!(probe(lib, cx, t, p, 2));
    // the path to the top of `stack`; each entry: a term, its next child (0 the head, i argument i), its head and its
    // children with their path elements (an association's children are its values, at `key(k)`: keys are no parts)
    let mut stack: Vec<Frame<'h>> = vec![frame(h, x)];
    let mut path: Vec<T<'h>> = vec![];
    let mut out = vec![];
    let mut m = Meter::default();
    let key = h.sym("key");
    while let Some(top) = stack.last_mut() {
        if let Some((head, kids)) = &top.parts {
            if top.next == 0 && !heads {
                top.next += 1;
                continue;
            }
            if top.next <= kids.len() {
                let (elem, child) = if top.next == 0 { (h.int(0), *head) } else { kids[top.next - 1] };
                let elem = if top.next > 0 && top.keyed { h.call(key, &[elem]) } else { elem };
                top.next += 1;
                stack.push(frame(h, child));
                path.push(elem);
                continue;
            }
        }
        let u = top.term;
        m.step(cx)?;
        let inside = levels.is_none_or(|l| l.contains_level(path.len()) == Some(true));
        if inside && probe.test(cx, u)? {
            if out.len() == MAX_LEN {
                return Ok(Reply::Done(lib.too_large(h, t)));
            }
            out.push(cx.list(&path));
        }
        stack.pop();
        path.pop();
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// A term being walked by `position`.
struct Frame<'h> {
    term: T<'h>,
    next: usize,
    keyed: bool,
    parts: Option<(T<'h>, Vec<(T<'h>, T<'h>)>)>,
}

/// The frame of `u`: its head and children with their path elements (an index, or for an association's values the
/// key, wrapped in `key(...)` when the path is built).
fn frame<'h>(h: &'h mc_core::Heap, u: T<'h>) -> Frame<'h> {
    if let Some(es) = crate::levels::assoc_entries(h, u) {
        if let View::App { head, .. } = h.view(u) {
            return Frame { term: u, next: 0, keyed: true, parts: Some((head, es)) };
        }
    }
    match h.view(u) {
        View::App { head, args } => {
            let kids = args.iter().enumerate().map(|(i, &a)| (h.int(i as i64 + 1), a)).collect();
            Frame { term: u, next: 0, keyed: false, parts: Some((head, kids)) }
        }
        _ => Frame { term: u, next: 0, keyed: false, parts: None },
    }
}

/// `select_first(xs, f)`: the first element `x` with `f(x)` `true`, or `failure(not_found, select_first)`;
/// `select_first(xs, f, d)`: `d` when there is none. Elements are tested in order, stopping at the first.
pub(crate) fn select_first<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, f, default) = match *args {
        [xs, f] => (xs, f, None),
        [xs, f, d] => (xs, f, Some(d)),
        _ => return Ok(Reply::Decline),
    };
    // `select_first(xs, f -> "Index")`, `-> "Element"`, `-> ["Element", "Index"]` (a final rule may arrive as
    // options; decision cov-core.11)
    let f = match h.view(f) {
        View::App { head, args: &[r] } if head == h.sym_term(h.sym("options")) => r,
        _ => f,
    };
    let (f, prop) = match h.view(f) {
        View::App { head, args: &[p, q] } if head == h.sym_term(lib.s.rule) => (p, Some(q)),
        _ => (f, None),
    };
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let count = xs.len();
    let shown = |cx: &NativeCx<'_, '_, 'h>, x: T<'h>, i: usize, q: T<'h>| -> Option<T<'h>> {
        match h.view(q) {
            View::Str(s) if s == "Element" => Some(x),
            View::Str(s) if s == "Index" => Some(h.int(i as i64 + 1)),
            // a mask of the elements: only the one found is `true`
            View::Str(s) if s == "BitVectorMask" => {
                let bits: Vec<T<'h>> = (0..count).map(|j| cx.bool(j == i)).collect();
                Some(cx.list(&bits))
            }
            _ => None,
        }
    };
    for (i, &x) in xs.iter().enumerate() {
        match truth(lib, cx, f, x)? {
            Ok(true) if prop.is_some() => {
                let q = prop.unwrap_or(x);
                let v = match cx.as_list(q) {
                    Some(qs) => qs
                        .iter()
                        .map(|&q| shown(cx, x, i, q).map(|v| h.call(lib.s.rule, &[q, v])))
                        .collect::<Option<Vec<_>>>()
                        .map(|v| h.call(lib.s.assoc, &v)),
                    None => shown(cx, x, i, q),
                };
                return Ok(match v {
                    Some(v) => Reply::Done(v),
                    None => Reply::Decline,
                });
            }
            Ok(true) => return Ok(Reply::Done(x)),
            Ok(false) => {}
            Err(e) => return Ok(Reply::Done(e)),
        }
    }
    Ok(Reply::Done(match default {
        Some(d) => d,
        None => {
            let head = match h.view(t) {
                View::App { head, .. } => head,
                _ => t,
            };
            lib.fail(h, lib.s.not_found, &[head])
        }
    }))
}
