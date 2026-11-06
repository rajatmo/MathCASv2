//! Predicates: the kind of a term (`is_integer`, `is_rational`, `is_number`, `is_exact`, `is_inexact`,
//! `is_machine_number`, `is_string`, `is_symbol`, `is_atom`), the shape of lists (`is_list`, `is_vector`,
//! `is_matrix`) and tests over elements (`all_true`, `any_true`, `none_true`).
//!
//! The kind predicates always answer `true` or `false`: an argument is evaluated before the test, so a symbol is a
//! symbol, not an unknown number (`is_integer(n)` is `false` for an undefined `n`, decision 1A.1).
use crate::ctx::{apply, take, truth, Lib};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The kind of a number: exact (integer or rational), machine real, or ball.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Integer,
    Rational,
    Machine,
    Ball,
}

/// The kind of a real number term, `None` for anything else.
fn real_kind(h: &Heap, t: T<'_>) -> Option<Kind> {
    Some(match h.view(t) {
        View::Int(_) | View::BigInt { .. } => Kind::Integer,
        View::Rational { .. } => Kind::Rational,
        View::Real(_) => Kind::Machine,
        View::Ball { .. } => Kind::Ball,
        _ => return None,
    })
}

/// The kinds of the parts of a number: one for a real number, the real and imaginary parts' for a complex number
/// `complex(re, im)` (semantics 1: a compound term with numeric parts), `None` for anything else.
fn number_kinds(lib: &Lib, h: &Heap, t: T<'_>) -> Option<(Kind, Kind)> {
    if let Some(k) = real_kind(h, t) {
        return Some((k, k));
    }
    match h.view(t) {
        View::App { head, args: &[re, im] } if head == h.sym_term(lib.s.complex) => {
            Some((real_kind(h, re)?, real_kind(h, im)?))
        }
        _ => None,
    }
}

/// A predicate of one term.
fn one<'h>(cx: &NativeCx<'_, '_, 'h>, args: &[T<'h>], test: impl Fn(&'h Heap, T<'h>) -> bool) -> Reply<'h> {
    match *args {
        [x] => Reply::Done(cx.bool(test(cx.heap(), x))),
        _ => Reply::Decline,
    }
}

/// `is_integer(x)`: is `x` an integer?
pub(crate) fn is_integer<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(one(cx, args, |h, x| real_kind(h, x) == Some(Kind::Integer)))
}

/// `is_rational(x)`: is `x` an exact real number, an integer or a fraction (decision 1A.2)?
pub(crate) fn is_rational<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(one(cx, args, |h, x| matches!(real_kind(h, x), Some(Kind::Integer | Kind::Rational))))
}

/// `is_number(x)`: is `x` a number of any kind, complex numbers included?
pub(crate) fn is_number<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(one(cx, args, |h, x| number_kinds(lib, h, x).is_some()))
}

fn exact(k: Kind) -> bool {
    matches!(k, Kind::Integer | Kind::Rational)
}

/// `is_exact(x)`: is `x` an exact number (integer, rational, or complex with exact parts)?
pub(crate) fn is_exact<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(one(cx, args, |h, x| number_kinds(lib, h, x).is_some_and(|(a, b)| exact(a) && exact(b))))
}

/// `is_inexact(x)`: is `x` an inexact number (machine real, ball, or complex with an inexact part)?
pub(crate) fn is_inexact<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(one(cx, args, |h, x| number_kinds(lib, h, x).is_some_and(|(a, b)| !exact(a) || !exact(b))))
}

/// `is_machine_number(x)`: is `x` a machine real, or a complex number with machine-real parts?
pub(crate) fn is_machine_number<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    _t: T<'h>,
    args: &[T<'h>],
) -> Res<Reply<'h>> {
    Ok(one(cx, args, |h, x| number_kinds(lib, h, x) == Some((Kind::Machine, Kind::Machine))))
}

/// `is_string(x)`
pub(crate) fn is_string<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(one(cx, args, |h, x| matches!(h.view(x), View::Str(_))))
}

/// `is_symbol(x)`
pub(crate) fn is_symbol<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(one(cx, args, |h, x| matches!(h.view(x), View::Sym(_))))
}

/// `is_atom(x)`: is `x` not a compound term? A complex number is the compound `complex(re, im)`, so it is not an
/// atom, consistent with `length` and `part` (decision 1A.3).
pub(crate) fn is_atom<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(one(cx, args, |h, x| !matches!(h.view(x), View::App { .. }) || crate::measure::is_complex_number(h, x)))
}

/// `is_list(x)`
pub(crate) fn is_list<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let r = match *args {
        [x] => cx.as_list(x).is_some(),
        _ => return Ok(Reply::Decline),
    };
    Ok(Reply::Done(cx.bool(r)))
}

/// Do all `xs` pass the optional element test? A failure of the test is `Err`.
fn all_pass<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    test: Option<T<'h>>,
    xs: &[T<'h>],
) -> Res<Result<bool, T<'h>>> {
    let Some(f) = test else { return Ok(Ok(true)) };
    for &x in xs {
        match truth(lib, cx, f, x)? {
            Ok(true) => {}
            other => return Ok(other),
        }
    }
    Ok(Ok(true))
}

/// The arguments of `is_vector` and `is_matrix`: the term and the optional element test.
fn shape_args<'h>(args: &[T<'h>]) -> Option<(T<'h>, Option<T<'h>>)> {
    match *args {
        [x] => Some((x, None)),
        [x, f] => Some((x, Some(f))),
        _ => None,
    }
}

/// `is_vector(v)`: is `v` a list none of whose elements is a list? `is_vector(v, test)`: and `test(x)` is `true`
/// for every element (tested in order, stopping at the first that fails).
pub(crate) fn is_vector<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let Some((v, test)) = shape_args(args) else { return Ok(Reply::Decline) };
    let Some(xs) = cx.as_list(v) else { return Ok(Reply::Done(cx.bool(false))) };
    cx.tick(xs.len() as u64 / 16)?;
    if xs.iter().any(|&x| cx.as_list(x).is_some()) {
        return Ok(Reply::Done(cx.bool(false)));
    }
    Ok(Reply::Done(match all_pass(lib, cx, test, xs)? {
        Ok(b) => cx.bool(b),
        Err(e) => e,
    }))
}

/// `is_matrix(m)`: is `m` a nonempty list of lists of one length whose elements are not lists? `is_matrix(m,
/// test)`: and `test(x)` is `true` for every element, row by row.
pub(crate) fn is_matrix<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let Some((m, test)) = shape_args(args) else { return Ok(Reply::Decline) };
    let no = Reply::Done(cx.bool(false));
    let Some(rows) = cx.as_list(m) else { return Ok(no) };
    let Some(&first) = rows.first() else { return Ok(no) };
    let Some(width) = cx.as_list(first).map(<[T]>::len) else { return Ok(no) };
    let mut cells = Vec::with_capacity(rows.len());
    for &r in rows {
        match cx.as_list(r) {
            Some(xs) if xs.len() == width && xs.iter().all(|&x| cx.as_list(x).is_none()) => cells.push(xs),
            _ => return Ok(no),
        }
        cx.tick(1 + width as u64 / 16)?;
    }
    for xs in cells {
        match all_pass(lib, cx, test, xs)? {
            Ok(true) => {}
            Ok(false) => return Ok(no),
            Err(e) => return Ok(Reply::Done(e)),
        }
    }
    Ok(Reply::Done(cx.bool(true)))
}

/// What `all_true`, `any_true` and `none_true` look for.
#[derive(Clone, Copy)]
enum Quantifier {
    All,
    Any,
    None,
}

/// The test of every element in order, stopping as soon as the answer is known; a failure is the result (decision
/// I.11). Values neither `true` nor `false` are kept: when nothing decides, the result is the conjunction (`all_true`,
/// `none_true` of the negations) or disjunction (`any_true`) of them (decision p6core.4).
fn quantify<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], q: Quantifier) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, f, depth) = match *args {
        [xs, f] => (xs, f, None),
        // `all_true(xs, f, n)`: the parts down to level n, the atoms above it included (decision io-core.6)
        [xs, f, n] => match h.view(n) {
            View::Int(n @ 0..) => (xs, f, Some(n as usize)),
            View::Sym(s) if h.sym_name(s) == "inf" => (xs, f, Some(usize::MAX)),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    let parts: Vec<T<'h>>;
    let xs: &[T<'h>] = match depth {
        Some(n) => {
            parts = parts_to_level(cx, xs, n)?;
            &parts
        }
        // an atom other than a symbol has no elements: the test is vacuous (decision io-core.6)
        None if h.packed(xs).is_none() && !matches!(h.view(xs), View::App { .. } | View::Sym(_)) => &[],
        None => take!(lib.operand(h, t, xs, 1)).1,
    };
    let mut open = vec![];
    for &x in xs {
        cx.tick(1)?;
        let v = apply(cx, f, &[x])?;
        if lib.is_failure(h, v) {
            return Ok(Reply::Done(v));
        }
        let b = if cx.is_true(v) {
            true
        } else if cx.is_false(v) {
            false
        } else {
            open.push(match q {
                Quantifier::None => h.app(h.sym_term(h.sym("not")), &[v]),
                _ => v,
            });
            continue;
        };
        match (q, b) {
            (Quantifier::All, false) | (Quantifier::None, true) => return Ok(Reply::Done(cx.bool(false))),
            (Quantifier::Any, true) => return Ok(Reply::Done(cx.bool(true))),
            _ => {}
        }
    }
    Ok(match open.len() {
        0 => Reply::Done(cx.bool(!matches!(q, Quantifier::Any))),
        _ => {
            let junction = if matches!(q, Quantifier::Any) { "or" } else { "and" };
            Reply::Again(h.app(h.sym_term(h.sym(junction)), &open))
        }
    })
}

/// The parts of `x` at level `n`, and the atoms at the levels above it (level 0: `x` itself).
fn parts_to_level<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, n: usize) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let mut out = vec![];
    let mut stack = vec![(x, 0usize)];
    while let Some((u, d)) = stack.pop() {
        cx.tick(1)?;
        let args = match cx.as_list(u) {
            Some(a) => Some(a),
            None => match h.view(u) {
                View::App { args, .. } => Some(args),
                _ => None,
            },
        };
        match args {
            Some(args) if d < n => stack.extend(args.iter().rev().map(|&a| (a, d + 1))),
            _ if d == 0 && n > 0 => {}
            _ => out.push(u),
        }
    }
    Ok(out)
}

/// `all_true(xs, f)`: is `f(x)` `true` for every element (`true` for none)?
pub(crate) fn all_true<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    quantify(lib, cx, t, args, Quantifier::All)
}

/// `any_true(xs, f)`: is `f(x)` `true` for some element?
pub(crate) fn any_true<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    quantify(lib, cx, t, args, Quantifier::Any)
}

/// `none_true(xs, f)`: is `f(x)` `true` for no element?
pub(crate) fn none_true<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    quantify(lib, cx, t, args, Quantifier::None)
}
