//! The spot check of a rewrite (phase 12, "every rewrite is an identity under the stated assumptions"): before a
//! form found by an identity of the tables replaces its input, both are evaluated in ball arithmetic at a few
//! points that satisfy the facts of the context, and a difference that the balls prove nonzero refutes the form.
//!
//! - **Points**: the free symbols (unprotected, without a value) get values from a fixed pseudo-random sequence
//!   (seeded by the input's handle, so a session is reproducible), with parts spread over `(-3, 3)` avoiding
//!   integers (the poles of gamma and its relatives): complex values when the context has no facts (the
//!   variables are complex; an identity between principal branches may fail on a branch cut, a set of measure
//!   zero that real points would hit), real ones under facts; a point is used only when every fact of the
//!   context evaluates to `true` there. Up to [`TRIES`] points are drawn to find [`POINTS`] good ones.
//! - **Test**: `n(abs(a - c) - 10^-12 (1 + abs(a)), 30)` for the two values `a`, `c`; the form is refuted only when
//!   that ball is certainly positive. A point where either side has no numeric value (a pole, a branch point, a
//!   function without a numeric method) says nothing.
//!
//! So a refutation is certain (outward-rounded balls), and a form that passes is an identity at every point tried.
use crate::term::B;
use mc_api::{Store, Sym, View, T};
use mc_eval::{NativeCx, Res};

/// Good points per check.
pub const POINTS: usize = 3;
/// Most points drawn per check.
pub const TRIES: usize = 12;

/// Symbols that are numbers, not variables.
const CONSTANTS: &[&str] = &["e", "pi", "euler_gamma", "catalan", "golden_ratio", "degree", "glaisher", "khinchin", "inf", "true", "false", "complex_infinity", "indeterminate"];

fn free_symbols<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, out: &mut Vec<Sym>) {
    let h = b.heap();
    match h.view(t) {
        View::Sym(s) => {
            let constant = CONSTANTS.contains(&h.sym_name(s).as_str());
            if !constant && !out.contains(&s) && !cx.protected(s) && cx.value(s).is_none() {
                out.push(s);
            }
        }
        View::App { head, args } => {
            // the head of a call is a function, not a variable
            if b.sym_of(head).is_none() {
                free_symbols(cx, b, head, out);
            }
            for &a in args {
                free_symbols(cx, b, a, out);
            }
        }
        _ => {}
    }
}

/// A hash of the structure of `t` (symbol names, numbers, shape; to depth 12), the same in every session.
fn structure_hash<'h>(h: &'h mc_core::Heap, t: T<'h>, depth: u32) -> u64 {
    let mix = |x: u64, y: u64| (x ^ y).wrapping_mul(0x100_0000_01b3).rotate_left(29);
    let bytes = |s: &str| s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |acc, c| (acc ^ c as u64).wrapping_mul(0x100_0000_01b3));
    if depth > 12 {
        return 7;
    }
    match h.view(t) {
        View::Int(i) => mix(1, i as u64),
        View::BigInt { neg, mag } => mag.iter().fold(mix(2, neg as u64), |acc, &l| mix(acc, l)),
        View::Rational { num, den } => mix(mix(3, structure_hash(h, num, depth + 1)), structure_hash(h, den, depth + 1)),
        View::Real(x) => mix(4, x.to_bits()),
        View::Ball { neg, words } => words.iter().fold(mix(5, neg as u64), |acc, &l| mix(acc, l)),
        View::Str(s) => mix(6, bytes(s)),
        View::Sym(s) => mix(8, bytes(&h.sym_name(s))),
        View::App { head, args } => {
            args.iter().fold(mix(9, structure_hash(h, head, depth + 1)), |acc, &x| mix(acc, structure_hash(h, x, depth + 1)))
        }
    }
}

/// A rational in `(-3, 3)` that is not an integer.
fn point(state: &mut u64) -> (i64, i64) {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let r = (*state >> 33) % 5800;
    let num = r as i64 - 2900;
    let num = if num % 100 == 0 { num + 37 } else { num };
    (num, 1000)
}

/// Is the form `c` refuted as a replacement of `a` under `facts`? `false` when no point decides.
pub fn refuted<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, facts: &[T<'h>], a: T<'h>, c: T<'h>) -> Res<bool> {
    if a == c {
        return Ok(false);
    }
    let h = b.heap();
    let mut vars = Vec::new();
    free_symbols(cx, b, a, &mut vars);
    free_symbols(cx, b, c, &mut vars);
    let rule_head = h.sym(mc_api::names::RULE);
    // seeded by the structure of the input, not its handle: the handle depends on the order of allocation in the
    // session (threads, what ran before), so the points and with them the cost of the check varied from run to run
    // (full_simplify of the dilogarithm reflection: 0.15 s to 3 s, a point where polylog converges slowly)
    let mut state = structure_hash(h, a, 0) ^ 0x9e37_79b9_7f4a_7c15;
    // without facts the variables are complex: complex points stay off the branch cuts (an identity of principal
    // branches may fail on a cut, which real points would hit); with facts, real points. The points do not depend
    // on what is found at them, so they are drawn first and tried in parallel (roadmap item 16A), in this order.
    let complex = facts.is_empty();
    let points: Vec<T<'h>> = (0..TRIES)
        .map(|_| {
            let rules: Vec<T<'h>> = vars
                .iter()
                .map(|&v| {
                    let (n, d) = point(&mut state);
                    let x = if complex {
                        let (m, e) = point(&mut state);
                        b.add(&[b.rat(n, d), b.mul(&[b.i(), b.rat(m, e)])])
                    } else {
                        b.rat(n, d)
                    };
                    b.call(rule_head, &[h.sym_term(v), x])
                })
                .collect();
            b.call(b.hd.list, &rules)
        })
        .collect();
    let stop = b.stop().clone();
    // in rounds of as many points as are still needed, so no point is evaluated that the sequential loop would
    // not evaluate (a numerical evaluation does not stop at once when it is no longer needed): the facts are
    // checked in order on this task (cheap), and the comparisons at the points that satisfy them run in parallel
    let (mut good, mut next) = (0, 0);
    while next < TRIES && good < POINTS {
        let mut batch = vec![];
        while next < TRIES && batch.len() < POINTS - good {
            if cx.stopped() {
                return Ok(false);
            }
            if facts_hold(cx, b, facts, points[next])? {
                batch.push(points[next]);
            }
            next += 1;
        }
        let found = cx.par_scan(
            batch.len(),
            &|r: &Point| *r == Point::Refutes,
            &mut |r: &Point| {
                good += (*r != Point::Silent) as usize;
                *r == Point::Refutes || *r == Point::Stopped || good >= POINTS
            },
            &|cx, i| {
                if cx.stopped() {
                    return Ok(Point::Stopped);
                }
                let _scope = mc_arith::num::StopScope::enter(Some(&stop));
                let b = B::new(cx.heap(), &stop);
                compare_at(cx, &b, a, c, batch[i])
            },
        )?;
        match found.last() {
            Some(Point::Refutes) => return Ok(true),
            Some(Point::Stopped) => return Ok(false),
            _ => {}
        }
    }
    Ok(false)
}

/// What one point says about a rewrite.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Point {
    /// a fact is not true there, or a side has no numeric value
    Silent,
    /// the two sides agree there
    Agrees,
    /// the balls prove the sides differ
    Refutes,
    /// the evaluation was stopped
    Stopped,
}

/// Does every fact evaluate to `true` at the point `rules`?
fn facts_hold<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, facts: &[T<'h>], rules: T<'h>) -> Res<bool> {
    for &f in facts {
        let v = cx.eval(b.call_named("rewrite", &[f, rules]))?;
        if !cx.is_true(v) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `a` and `c` compared at the point `rules`.
fn compare_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, a: T<'h>, c: T<'h>, rules: T<'h>) -> Res<Point> {
    let h = b.heap();
    let at = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| cx.eval(b.call_named("rewrite", &[t, rules]));
    let (va, vc) = (at(cx, a)?, at(cx, c)?);
    // each side to 30 digits on its own, then the gap of the two numbers: `n(abs(a - c) - ..., 30)` of two sides
    // that agree exactly asks for 30 digits of a difference that is zero, and the working precision climbs to its
    // cap (minutes for special functions at complex points, round 22)
    // and each under a time budget: a side that is an exact zero of special functions climbs the precision too
    // (a d(F) - f of 2F1 terms took minutes, round 28); a side that does not settle makes the point silent
    let digits = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| {
        cx.eval(b.call_named("time_constrained", &[b.call_named("n", &[t, b.int(30)]), b.int(2)]))
    };
    let (va, vc) = (digits(cx, va)?, digits(cx, vc)?);
    if cx.stopped() {
        return Ok(Point::Stopped);
    }
    let abs = |u: T<'h>| b.call_named("abs", &[u]);
    let gap = b.sub(abs(b.sub(va, vc)), b.mul(&[b.pow(b.int(10), b.int(-12)), b.add(&[b.int(1), abs(va)])]));
    let v = cx.eval(gap)?;
    if !matches!(h.view(v), View::Real(x) if !x.is_nan()) && !matches!(h.view(v), View::Ball { .. } | View::Int(_) | View::Rational { .. }) {
        return Ok(Point::Silent);
    }
    Ok(if b.ar.compare(v, b.int(0)).is_some_and(|o| o == mc_arith::number::Cmp3::Greater) { Point::Refutes } else { Point::Agrees })
}
