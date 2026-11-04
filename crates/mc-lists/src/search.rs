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
        if matches!(l.pat, Pat::Lit(_)) {
            return Some(Probe::Lit(p));
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
        if probe.test(cx, u)? {
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
    let [x, p] = *args else { return Ok(Reply::Decline) };
    let probe = take!(probe(lib, cx, t, p, 2));
    // the path to the top of `stack`; each entry: a term and its next child (0 the head, i argument i)
    let mut stack: Vec<(T<'h>, usize)> = vec![(x, 0)];
    let mut path: Vec<i64> = vec![];
    let mut out = vec![];
    let mut m = Meter::default();
    while let Some(&(u, next)) = stack.last() {
        if let View::App { head, args } = h.view(u) {
            if next <= args.len() {
                let child = if next == 0 { head } else { args[next - 1] };
                stack.last_mut().expect("nonempty").1 += 1;
                stack.push((child, 0));
                path.push(next as i64);
                continue;
            }
        }
        m.step(cx)?;
        if probe.test(cx, u)? {
            if out.len() == MAX_LEN {
                return Ok(Reply::Done(lib.too_large(h, t)));
            }
            let ix: Vec<T<'h>> = path.iter().map(|&i| h.int(i)).collect();
            out.push(cx.list(&ix));
        }
        stack.pop();
        path.pop();
    }
    Ok(Reply::Done(cx.list(&out)))
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
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    for &x in xs {
        match truth(lib, cx, f, x)? {
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
