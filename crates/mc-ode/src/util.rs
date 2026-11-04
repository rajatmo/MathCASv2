//! Small helpers on terms shared by the natives: failures, numeric values of closed expressions, symbol scans.
use mc_api::names;
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Res};
use std::collections::HashSet;

/// `failure(tag, details...)`.
pub(crate) fn failure<'h>(h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

/// `failure(argument, f, k)`: argument (or option) `k` of the function `f` is wrong.
pub(crate) fn bad_argument<'h>(h: &'h Heap, f: &str, k: T<'h>) -> T<'h> {
    failure(h, "argument", &[h.sym_term(h.sym(f)), k])
}

/// Is `t` a failure?
pub(crate) fn is_failure(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym(names::FAILURE)))
}

/// The numeric value of a number, or of a closed expression through `n` (`pi`, `10^11`, `sqrt(2)`); `digits`
/// gives `n(t, digits)` (balls) instead of machine precision for expressions that are not numbers already.
pub(crate) fn number<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    t: T<'h>,
    digits: Option<u64>,
) -> Res<Option<Num>> {
    if let Some(v) = ar.num(t) {
        return Ok(Some(v));
    }
    let h = cx.heap();
    let call = match digits {
        None => h.call(h.sym("n"), &[t]),
        Some(d) => h.call(h.sym("n"), &[t, h.int(d.min(i64::MAX as u64) as i64)]),
    };
    let v = cx.eval(call)?;
    Ok(ar.num(v))
}

/// A real number as a machine real, if it is real and finite.
pub(crate) fn real(n: &Num) -> Option<f64> {
    let x = n.as_real()?.to_f64();
    x.is_finite().then_some(x)
}

/// Does `t` contain one of the symbols `syms` (as a head or anywhere)? Each shared subterm is visited once; one
/// unit of fuel per 1024 nodes.
pub(crate) fn mentions<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, syms: &[Sym]) -> Res<bool> {
    let h = cx.heap();
    let mut seen = HashSet::new();
    let mut stack = vec![t];
    let mut visited = 0u64;
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        visited += 1;
        if visited.is_multiple_of(1024) {
            cx.tick(1)?;
        }
        match h.view(u) {
            View::Sym(s) if syms.contains(&s) => return Ok(true),
            View::App { head, args } => {
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    Ok(false)
}

/// `[x -> v, ...]`.
pub(crate) fn rules<'h>(h: &'h Heap, pairs: &[(T<'h>, T<'h>)]) -> T<'h> {
    let rule = h.sym(names::RULE);
    let rs: Vec<T<'h>> = pairs.iter().map(|&(k, v)| h.call(rule, &[k, v])).collect();
    h.call(h.sym(names::LIST), &rs)
}
