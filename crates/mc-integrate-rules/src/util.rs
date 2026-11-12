//! Utility functions of the rules: the predicates of conditions and the term helpers of right sides.
//!
//! The predicates follow the conventions of the rule set they were translated from (Rich's rule-based
//! integrator): a comparison holds only when it is decided (`lt(a, b)` is false for a symbolic `a`), `eq(u, v)`
//! holds when `u - v` is zero after canonical arithmetic, and `ne` is its negation (so `ne(m, -1)` holds for a
//! symbolic `m`; the answer is checked by differentiation in the end, see `mc-calculus`'s portfolio).
use mc_api::{Store, Sym, View, T};
use mc_arith::Arith;
use mc_core::Heap;
use rustc_hash::FxHashSet;

/// A small exact rational (numerator, positive denominator) of a number term, if it is one.
pub fn small_rat(h: &Heap, t: T<'_>) -> Option<(i128, i128)> {
    match h.view(t) {
        View::Int(v) => Some((v as i128, 1)),
        View::Rational { num, den } => match (h.view(num), h.view(den)) {
            (View::Int(n), View::Int(d)) => Some((n as i128, d as i128)),
            _ => None,
        },
        _ => None,
    }
}

/// Is `t` an exact integer (any size)?
pub fn is_integer(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::Int(_) | View::BigInt { .. })
}

/// Is `t` an exact rational (an integer included)?
pub fn is_rational(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::Int(_) | View::BigInt { .. } | View::Rational { .. })
}

/// The head symbol and arguments of a call.
pub fn call_of<'h>(h: &'h Heap, t: T<'h>) -> Option<(Sym, &'h [T<'h>])> {
    match h.view(t) {
        View::App { head, args } => match h.view(head) {
            View::Sym(s) => Some((s, args)),
            _ => None,
        },
        _ => None,
    }
}

/// Is `t` free of `x`?
pub fn free_of<'h>(h: &'h Heap, t: T<'h>, x: T<'h>) -> bool {
    let mut seen: FxHashSet<T<'h>> = FxHashSet::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return false;
        }
        if let View::App { head, args } = h.view(u) {
            if !seen.insert(u) {
                continue;
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    true
}

/// `t` with every occurrence of `from` replaced by `to`, rebuilt through canonical arithmetic (calls of other
/// functions are rebuilt plain and simplify when the result is evaluated).
pub fn replace<'h>(ar: &Arith<'h, Heap>, t: T<'h>, from: T<'h>, to: T<'h>) -> T<'h> {
    let h = ar.store();
    let mut memo: rustc_hash::FxHashMap<T<'h>, T<'h>> = Default::default();
    replace_in(ar, h, t, from, to, &mut memo)
}

fn replace_in<'h>(
    ar: &Arith<'h, Heap>,
    h: &'h Heap,
    t: T<'h>,
    from: T<'h>,
    to: T<'h>,
    memo: &mut rustc_hash::FxHashMap<T<'h>, T<'h>>,
) -> T<'h> {
    if t == from {
        return to;
    }
    let View::App { head, args } = h.view(t) else {
        return t;
    };
    if let Some(&r) = memo.get(&t) {
        return r;
    }
    let new: Vec<T<'h>> = args.iter().map(|&a| replace_in(ar, h, a, from, to, memo)).collect();
    let r = if new.iter().zip(args.iter()).all(|(a, b)| a == b) { t } else { rebuild(ar, head, &new) };
    memo.insert(t, r);
    r
}

/// `head(args)` with sums, products and powers canonical.
pub fn rebuild<'h>(ar: &Arith<'h, Heap>, head: T<'h>, args: &[T<'h>]) -> T<'h> {
    let h = ar.store();
    if let View::Sym(s) = h.view(head) {
        let name = h.sym_name(s);
        match (name.as_str(), args) {
            (mc_api::names::ADD, _) => return ar.add(args),
            (mc_api::names::MUL, _) => return ar.mul(args),
            (mc_api::names::POW, [b, e]) => return ar.pow(*b, *e),
            _ => {}
        }
    }
    h.app(head, args)
}

/// The size of a term (nodes, counted with repetition, capped).
pub fn size(h: &Heap, t: T<'_>, cap: usize) -> usize {
    let mut n = 0;
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        n += 1;
        if n >= cap {
            return cap;
        }
        if let View::App { args, .. } = h.view(u) {
            stack.extend_from_slice(args);
        }
    }
    n
}

/// Is `t` a number we can compare (exact rational or real)?
pub fn real_number(ar: &Arith<'_, Heap>, t: T<'_>) -> bool {
    ar.num(t).is_some_and(|n| n.as_real().is_some())
}

/// The sign of a real number term (`None` when `t` is not a real number).
pub fn num_sign(ar: &Arith<'_, Heap>, t: T<'_>) -> Option<i8> {
    let n = ar.num(t)?;
    let r = n.as_real()?;
    Some(if r.is_zero() {
        0
    } else if r.is_positive() {
        1
    } else {
        -1
    })
}

/// The sign of a constant term when it is evident: a real number, `pi`, `e`, and products, sums, powers of
/// those; `None` otherwise.
pub fn const_sign(ar: &Arith<'_, Heap>, t: T<'_>) -> Option<i8> {
    let h = ar.store();
    if let Some(s) = num_sign(ar, t) {
        return Some(s);
    }
    match h.view(t) {
        View::Sym(s) => {
            let n = h.sym_name(s);
            if n == "pi" || n == "e" {
                Some(1)
            } else {
                None
            }
        }
        View::App { args, .. } => {
            let (s, _) = call_of(h, t)?;
            let name = h.sym_name(s);
            match name.as_str() {
                mc_api::names::MUL => {
                    let mut sg = 1i8;
                    for &a in args {
                        sg *= const_sign(ar, a)?;
                    }
                    Some(sg)
                }
                mc_api::names::ADD => {
                    let first = const_sign(ar, args[0])?;
                    for &a in &args[1..] {
                        if const_sign(ar, a)? != first {
                            return None;
                        }
                    }
                    Some(first)
                }
                mc_api::names::POW => {
                    let sb = const_sign(ar, args[0])?;
                    if sb > 0 {
                        return Some(1);
                    }
                    let (n, d) = small_rat(h, args[1])?;
                    if d == 1 {
                        Some(if n % 2 == 0 { 1 } else { sb })
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Positive-looking (the sense of the rule set's `PosQ`): a positive number, or a term whose leading numeric
/// coefficient is positive (`2 a` yes, `-a` no); used only to choose between equivalent forms.
pub fn pos_looking(ar: &Arith<'_, Heap>, t: T<'_>) -> bool {
    let h = ar.store();
    if let Some(s) = const_sign(ar, t) {
        return s > 0;
    }
    match call_of(h, t) {
        Some((s, args)) => {
            let name = h.sym_name(s);
            match name.as_str() {
                mc_api::names::MUL => num_sign(ar, args[0]).is_none_or(|s| s > 0),
                mc_api::names::ADD => pos_looking(ar, args[0]),
                _ => true,
            }
        }
        None => true,
    }
}

/// A plain text form of a term (for traces), cut at about `cap` characters.
pub fn show(h: &Heap, t: T<'_>, cap: usize) -> String {
    let mut out = String::new();
    show_in(h, t, cap, &mut out);
    out
}

fn show_in(h: &Heap, t: T<'_>, cap: usize, out: &mut String) {
    if out.len() > cap {
        return;
    }
    match h.view(t) {
        View::Int(v) => out.push_str(&v.to_string()),
        View::Sym(s) => out.push_str(&h.sym_name(s)),
        View::Rational { num, den } => {
            show_in(h, num, cap, out);
            out.push('/');
            show_in(h, den, cap, out);
        }
        View::App { head, args } => {
            show_in(h, head, cap, out);
            out.push('(');
            for (i, &a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                show_in(h, a, cap, out);
            }
            out.push(')');
        }
        View::Real(v) => out.push_str(&v.to_string()),
        _ => out.push('?'),
    }
}
