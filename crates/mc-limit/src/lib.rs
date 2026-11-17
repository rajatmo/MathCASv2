//! Limits (roadmap item 2C, part 5): the native `limit(f, x -> a)` and its Rust entry point [`limit`].
//!
//! `limit(f, x -> a)` is the two-sided limit along the real line, `direction: from_above` (`x ↓ a`) and
//! `direction: from_below` (`x ↑ a`) the one-sided ones; `a` may be `inf` or `-inf` (approached from the finite
//! side; a contrary direction is an argument failure). One-sided limits come from the [`walk`] at `t → 0⁺`, and
//! where it does not decide from Gruntz's algorithm (`gruntz`, item 13C); every value passes a numeric check
//! (`check`) first. A two-sided limit is the common value of both sides (decision C.1):
//!
//! | sides | result |
//! |---|---|
//! | equal values (or both `inf`, both `-inf`) | that value |
//! | provably different | `failure(no_limit, limit, f)` |
//! | either side unknown, or equality not decided | the call stays unevaluated |
//!
//! An unknown limit leaves the call as it is (decision C.2), like `integrate` without an antiderivative. Inexact
//! numbers in `f` or `a` are rationalized first and the limit given numerically (decision C.3). A list `f` maps
//! elementwise.
//!
//! A crate of its own (split out of `mc-calculus`); [`bounds`] and [`switches`] hold the real
//! bounds and the functions defined by cases that limits share with definite integrals (`mc-integrals`).
#![warn(missing_docs)]
pub(crate) mod asym;
pub mod bounds;
mod check;
pub(crate) mod gruntz;
mod more;
pub mod switches;
pub mod walk;

use mc_calculus::common::wrap;
use self::walk::Lim;
use crate::bounds::{sign, Pt};
use mc_calculus::common::{apply, args_of, is_free};
use rustc_hash::FxHashSet;
use mc_calculus::term::Build;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_eval::{Allowance, Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The side a limit is taken from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// both sides: the limit exists when they agree
    TwoSided,
    /// `x ↓ a`
    FromAbove,
    /// `x ↑ a`
    FromBelow,
}

/// The outcome of a limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome<'h> {
    /// the limit: a value, `inf` or `-inf`
    Value(T<'h>),
    /// the one-sided limits differ: there is no two-sided limit
    NoLimit,
    /// not decided by this crate
    Unknown,
}

/// How a limit is computed, for Rust callers (item 13C; definite integration takes many limits and sets its
/// own budget).
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// the time Gruntz's algorithm may take for the whole call (both sides of a two-sided limit)
    pub budget: u64,
    /// check every limit numerically before it is returned (decision 13C.4)
    pub check: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options { budget: WORK, check: true }
    }
}

/// The limit of `f` as `x` tends to `a` (a term free of `x`, or `inf`, `-inf`) from `direction`. At an infinity
/// the direction must be the finite side or [`Direction::TwoSided`], else the outcome is [`Outcome::Unknown`].
pub fn limit<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    a: T<'h>,
    direction: Direction,
) -> Res<Outcome<'h>> {
    limit_with(cx, b, f, x, a, direction, &Options::default())
}

/// [`limit`] with explicit [`Options`]: the entry point for other crates (definite integration, 14D).
pub fn limit_with<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    a: T<'h>,
    direction: Direction,
    opts: &Options,
) -> Res<Outcome<'h>> {
    if is_free(b, f, x) {
        return Ok(Outcome::Value(f));
    }
    // a directed infinity off the real line (`im*inf`, `-im*inf`): x = c y with y -> inf along the ray
    if let Some(c) = complex_direction(b, a) {
        let g = crate::bounds::subst(cx, b, f, x, b.mul(&[c, x]))?;
        if g != f && !mc_calculus::common::is_failure(b, g) {
            let inf = b.heap().sym_term(b.heap().sym("inf"));
            return limit_with(cx, b, g, x, inf, Direction::TwoSided, opts);
        }
    }
    let run = Run { deadline: Allowance::new(cx, opts.budget), budget: opts.budget, check: opts.check };
    // complex infinity (|x| -> inf in every direction): a rational function by the degrees of its numerator and
    // denominator (0, the ratio of the leading coefficients, or complex_infinity); otherwise not decided here
    if a == b.heap().sym_term(b.heap().sym("complex_infinity")) {
        let h = b.heap();
        let r = apply(cx, b, "together", &[f])?;
        let (num, den) = (apply(cx, b, "numerator", &[r])?, apply(cx, b, "denominator", &[r])?);
        let xs = h.call(h.sym(names::LIST), &[x]);
        for p in [num, den] {
            let q = apply(cx, b, "is_polynomial_expression", &[p, xs])?;
            if q != h.sym_term(h.sym(names::TRUE)) {
                return Ok(Outcome::Unknown);
            }
        }
        let (dn, dd) = (apply(cx, b, "exponent", &[num, x])?, apply(cx, b, "exponent", &[den, x])?);
        let (View::Int(dn), View::Int(dd)) = (h.view(dn), h.view(dd)) else { return Ok(Outcome::Unknown) };
        return Ok(Outcome::Value(if dn < dd {
            b.zero()
        } else if dn > dd {
            h.sym_term(h.sym("complex_infinity"))
        } else {
            let (cn, cd) = (apply(cx, b, "coefficient", &[num, x, h.int(dn)])?, apply(cx, b, "coefficient", &[den, x, h.int(dd)])?);
            apply(cx, b, "simplify", &[b.div(cn, cd)])?
        }));
    }
    if inexact(b, f) || inexact(b, a) {
        // decision C.3: inexact numbers cancel to 0.0/0.0 under the walk; the limit is taken on the rationalized
        // input and the value given numerically
        let (fe, ae) = (apply(cx, b, "rationalize", &[f])?, apply(cx, b, "rationalize", &[a])?);
        if inexact(b, fe) || inexact(b, ae) {
            return Ok(Outcome::Unknown);
        }
        return Ok(match exact_limit(cx, b, fe, x, ae, direction, &run)? {
            Outcome::Value(v) => Outcome::Value(apply(cx, b, "n", &[v])?),
            o => o,
        });
    }
    let o = exact_limit(cx, b, f, x, a, direction, &run)?;
    if !matches!(o, Outcome::Unknown) {
        return Ok(o);
    }
    Ok(complex_parts(cx, b, f, x, a, direction, opts)?.unwrap_or(o))
}

/// The direction c of a directed infinity `c*inf` off the real line (`im*inf`, `-im*inf`, `(1 + im)*inf`: c an exact
/// complex number that is not real), or `None`. The ray x = c y, y -> inf, is the same for c and every positive
/// multiple of c, so c need not have modulus 1.
fn complex_direction<'h>(b: &Build<'h>, a: T<'h>) -> Option<T<'h>> {
    let h = b.heap();
    let View::App { head, args: [c, y] } = h.view(a) else { return None };
    if b.sym_of(head) != Some(b.hd.mul) || *y != h.sym_term(h.sym("inf")) {
        return None;
    }
    match h.view(*c) {
        View::App { head, args: [re, im] } if b.sym_of(head) == Some(h.sym("complex")) => {
            let exact = |t: &T<'h>| matches!(h.view(*t), View::Int(_) | View::BigInt { .. } | View::Rational { .. });
            (exact(re) && exact(im)).then_some(*c)
        }
        _ => None,
    }
}

/// The limit of a function with complex values along the real line (`im*x`, `exp(im*pi/4)*x`, `x + im*x^2`) from
/// the limits of its real part u and imaginary part v (`complex_expand`, x real): u + im v when both are finite,
/// and a directed infinity (semantics 3.1) otherwise: `inf` or `-inf` when only u is infinite, `im*inf` or
/// `-im*inf` when only v is, and with both infinite the direction s (1 + im r) from the sign s of u and
/// r = lim v/u (`im*inf` times the sign of v when r is infinite). Only for an exact function of x alone (no
/// parameter, whose real part `complex_expand` would assume) with a complex number in it, at a real point; `None`
/// leaves the limit to the other methods, and anything but two values is not decided here.
fn complex_parts<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    a: T<'h>,
    direction: Direction,
    opts: &Options,
) -> Res<Option<Outcome<'h>>> {
    let h = b.heap();
    let complex = h.sym_term(h.sym("complex"));
    let constants = ["pi", "e", "euler_gamma", "catalan", "golden_ratio", "degree", "glaisher", "khinchin"];
    let mut has_complex = false;
    let mut seen = FxHashSet::default();
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        match h.view(u) {
            View::Real(_) | View::Ball { .. } => return Ok(None),
            View::Sym(s) if u != x && !constants.contains(&h.sym_name(s).as_str()) => return Ok(None),
            View::App { head, args } => {
                has_complex |= head == complex;
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    if !has_complex {
        return Ok(None);
    }
    let inf = h.sym_term(h.sym("inf"));
    let truth = h.sym_term(h.sym(names::TRUE));
    if !(a == inf || a == b.neg(inf) || apply(cx, b, "is_real", &[a])? == truth) {
        return Ok(None);
    }
    let part = |cx: &mut NativeCx<'_, '_, 'h>, name: &str| -> Res<T<'h>> {
        let p = apply(cx, b, name, &[f])?;
        apply(cx, b, "complex_expand", &[p])
    };
    let (u, v) = (part(cx, "re")?, part(cx, "im_part")?);
    // the parts must be real expressions, free of complex numbers (or the recursion would not end)
    for p in [u, v] {
        if !is_free(b, p, complex) || !is_free(b, p, h.sym_term(h.sym("re"))) || !is_free(b, p, h.sym_term(h.sym("im_part"))) {
            return Ok(None);
        }
    }
    // 0 for a finite number, the sign for inf or -inf
    let size = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<i8>> {
        Ok(if t == inf {
            Some(1)
        } else if t == b.neg(inf) {
            Some(-1)
        } else if apply(cx, b, "is_numeric", &[t])? == truth && apply(cx, b, "is_real", &[t])? == truth {
            Some(0)
        } else {
            None
        })
    };
    let (Outcome::Value(lu), Outcome::Value(lv)) =
        (limit_with(cx, b, u, x, a, direction, opts)?, limit_with(cx, b, v, x, a, direction, opts)?)
    else {
        return Ok(None);
    };
    let (Some(su), Some(sv)) = (size(cx, lu)?, size(cx, lv)?) else { return Ok(None) };
    let i = b.mul(&[h.sym_term(h.sym("im")), b.one()]);
    let i = cx.eval(i)?;
    let value = match (su, sv) {
        (0, 0) => b.add(&[lu, b.mul(&[i, lv])]),
        (_, 0) => lu,
        (0, _) => b.mul(&[i, lv]),
        _ => {
            let Outcome::Value(r) = limit_with(cx, b, b.div(v, u), x, a, direction, opts)? else {
                return Ok(None);
            };
            match size(cx, r)? {
                Some(0) => b.mul(&[b.int(su as i64), b.add(&[b.one(), b.mul(&[i, r])]), inf]),
                Some(_) => b.mul(&[i, lv]),
                None => return Ok(None),
            }
        }
    };
    Ok(Some(Outcome::Value(cx.eval(value)?)))
}

/// The settings of one limit call.
struct Run {
    deadline: Allowance,
    budget: u64,
    check: bool,
}

/// The work (fuel) of one limit call by default.
pub const WORK: u64 = 8_000_000;

/// `deadline`, or a quarter of `budget` from now if that is more (Gruntz's algorithm after a long walk).
fn at_least_quarter(cx: &NativeCx<'_, '_, '_>, deadline: &Allowance, budget: u64) -> Allowance {
    Allowance::new(cx, deadline.left(cx).max(budget / 4))
}

/// The one-sided limit of `g` at `pt` from `side`: the walk (the fast paths), then Gruntz's algorithm where the
/// walk does not decide; a value the numeric check rejects is not returned (decision 13C.4).
#[allow(clippy::too_many_arguments)]
pub(crate) fn one_sided<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    x: T<'h>,
    pt: Pt<'h>,
    side: i8,
    deadline: &Allowance,
    budget: u64,
    check: bool,
) -> Res<Option<Lim<'h>>> {
    let mut from_walk = true;
    let mut r = walk::limit(cx, b, g, x, pt, side)?;
    if r.is_none() {
        from_walk = false;
        // the walk may have used much of the budget: Gruntz's algorithm gets at least a quarter of it
        let quarter = at_least_quarter(cx, deadline, budget);
        r = gruntz::limit(cx, b, g, x, pt, side, &quarter)?;
    }
    let Some(l) = r else { return Ok(None) };
    if !check || check::plausible(cx, b, g, x, pt, side, l)? {
        return Ok(Some(l));
    }
    if from_walk {
        // the walk's value failed the check: Gruntz's algorithm, checked in turn
        if let Some(l) = gruntz::limit(cx, b, g, x, pt, side, deadline)? {
            if check::plausible(cx, b, g, x, pt, side, l)? {
                return Ok(Some(l));
            }
        }
    }
    Ok(None)
}

/// Does `t` contain an inexact number (a machine real or a ball)?
fn inexact<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    let mut seen = FxHashSet::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        match h.view(u) {
            View::Real(_) | View::Ball { .. } => return true,
            View::App { head, args } => {
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    false
}

/// [`limit`] of exact `f` and `a`.
fn exact_limit<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    a: T<'h>,
    direction: Direction,
    run: &Run,
) -> Res<Outcome<'h>> {
    let inf = b.heap().sym_term(b.heap().sym("inf"));
    // a function defined by cases: its branch on the side approached (decision IOSYM.16)
    let one = |cx: &mut NativeCx<'_, '_, 'h>, pt, side| {
        let g = crate::switches::resolve_near(cx, b, f, x, pt, side)?.unwrap_or(f);
        let g = match pt {
            Pt::Fin(a) => {
                let g = inverses_on_cuts(cx, b, g, x, a)?;
                // a log or power reaching its cut from a side that is not decided: no limit is given
                let Some(g) = off_branch_cuts(cx, b, g, x, a, side)? else { return Ok(None) };
                let Some(g) = special_cuts(cx, b, g, x, a, side)? else { return Ok(None) };
                g
            }
            _ => g,
        };
        one_sided(cx, b, g, x, pt, side, &run.deadline, run.budget, run.check)
    };
    let l = match (a, direction) {
        (a, Direction::TwoSided | Direction::FromBelow) if a == inf => one(cx, Pt::PosInf, 1)?,
        (a, Direction::TwoSided | Direction::FromAbove) if a == b.neg(inf) => one(cx, Pt::NegInf, 1)?,
        (a, _) if !is_free(b, a, inf) => None,
        (a, Direction::FromAbove) => one(cx, Pt::Fin(a), 1)?,
        (a, Direction::FromBelow) => one(cx, Pt::Fin(a), -1)?,
        (a, Direction::TwoSided) => {
            let (Some(above), Some(below)) = (one(cx, Pt::Fin(a), 1)?, one(cx, Pt::Fin(a), -1)?) else {
                return Ok(Outcome::Unknown);
            };
            return Ok(match both_sides(cx, b, above, below)? {
                Outcome::Value(v) => Outcome::Value(tidy(cx, b, v)?),
                o => o,
            });
        }
    };
    Ok(match l {
        Some(l) => Outcome::Value(tidy(cx, b, value(b, l))?),
        None => Outcome::Unknown,
    })
}

/// `g` with each `log(u)` and `u^p` (p not an integer) whose argument tends to a point of the negative real axis
/// from above or below (`log(-10 + t im)` as t tends to 0 from one side) written with `-u`, which stays off the
/// cut: `log(u) = log(-u) + s im pi` and `u^p = (-u)^p exp(s im pi p)` with s the sign of `im_part(u)` near the
/// point. Both sides are equal on that side of the cut (principal branches), so the limit of the result is the
/// one-sided limit, where substituting the point would give the value on the cut (the side of positive
/// imaginary part). The side of `u` is read from its imaginary part at a + side 10^-k, k = 6, 9, 12 (40 digits),
/// which must agree. A point of the cut approached with an imaginary part whose sign is not decided (or changes)
/// gives `None`: the limit is not given, never the value of the wrong side.
fn off_branch_cuts<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, a: T<'h>, side: i8) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let log = h.sym("log");
    // the candidates: logs and fractional powers whose argument depends on x
    let mut found = vec![];
    let mut stack = vec![g];
    let mut seen = FxHashSet::default();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { continue };
        let s = b.sym_of(head);
        let log_of_x = s == Some(log) && args.len() == 1 && !is_free(b, args[0], x);
        let root_of_x = s == Some(b.hd.pow) && args.len() == 2 && !is_free(b, args[0], x) && is_free(b, args[1], x) && !matches!(h.view(args[1]), View::Int(_));
        if log_of_x || root_of_x {
            found.push(u);
        }
        stack.extend_from_slice(args);
    }
    if found.is_empty() {
        return Ok(Some(g));
    }
    let im = h.sym_term(h.sym("im"));
    let pi = h.sym_term(h.sym("pi"));
    let mut rules = vec![];
    for u in found {
        cx.tick(1)?;
        let View::App { head: uhead, args } = h.view(u) else { continue };
        let s = b.sym_of(uhead);
        let arg = args[0];
        // the argument at the point: a negative real number
        let at = apply(cx, b, "rewrite", &[arg, h.call(h.sym(names::RULE), &[x, a])])?;
        let at = apply(cx, b, "n", &[at, b.int(30)])?;
        let real_negative = match h.view(at) {
            View::Real(v) => v < 0.0,
            View::Ball { .. } => {
                let neg = apply(cx, b, "less", &[at, b.zero()])?;
                cx.is_true(neg)
            }
            _ => false,
        };
        if !real_negative {
            // the argument tends to 0 (or another point of a cut) through negative real values beside the
            // point: log(u) = log(-u) + i pi there (u^p = (-u)^p exp(i pi p)), the principal value on the cut
            let mut negative_beside = true;
            for k in [6i64, 9, 12] {
                let delta = b.mul(&[b.int(side as i64), b.pow(b.int(10), b.int(-k))]);
                let pt = b.add(&[a, delta]);
                let v = apply(cx, b, "rewrite", &[arg, h.call(h.sym(names::RULE), &[x, pt])])?;
                let v = apply(cx, b, "n", &[v, b.int(40)])?;
                let (re, im) = (apply(cx, b, "re", &[v])?, apply(cx, b, "im_part", &[v])?);
                let neg = apply(cx, b, "less", &[re, b.zero()])?;
                let flat = apply(cx, b, "equal", &[im, b.zero()])?;
                if !(cx.is_true(neg) && cx.is_true(flat)) {
                    negative_beside = false;
                    break;
                }
            }
            if negative_beside {
                let neg_arg = b.neg(arg);
                let turn = b.mul(&[im, pi]);
                let new = if s == Some(log) {
                    b.add(&[b.call("log", &[neg_arg]), turn])
                } else {
                    let p = args[1];
                    b.mul(&[b.pow(neg_arg, p), b.call("exp", &[b.mul(&[turn, p])])])
                };
                rules.push(h.call(h.sym(names::RULE), &[u, new]));
            }
            continue;
        }
        // the sign of the imaginary part beside the point
        let mut sgn = 0i8;
        let mut agree = true;
        let mut flat = true;
        for k in [6i64, 9, 12] {
            let delta = b.mul(&[b.int(side as i64), b.pow(b.int(10), b.int(-k))]);
            let pt = b.add(&[a, delta]);
            let v = apply(cx, b, "rewrite", &[arg, h.call(h.sym(names::RULE), &[x, pt])])?;
            let v = apply(cx, b, "im_part", &[v])?;
            let v = apply(cx, b, "n", &[v, b.int(40)])?;
            let pos = apply(cx, b, "greater", &[v, b.zero()])?;
            let neg = apply(cx, b, "less", &[v, b.zero()])?;
            let zero = apply(cx, b, "equal", &[v, b.zero()])?;
            flat &= cx.is_true(zero);
            let s = if cx.is_true(pos) { 1 } else if cx.is_true(neg) { -1 } else { 0 };
            if s == 0 || (sgn != 0 && s != sgn) {
                agree = false;
                break;
            }
            sgn = s;
        }
        if !agree || sgn == 0 {
            // along the cut itself (an imaginary part exactly 0 beside the point): the principal values on the cut,
            // which substitution gives; any other undecided side: no limit
            if flat {
                continue;
            }
            return Ok(None);
        }
        let neg_arg = b.neg(arg);
        let turn = b.mul(&[b.int(sgn as i64), im, pi]);
        let new = if b.sym_of(match h.view(u) {
            View::App { head, .. } => head,
            _ => u,
        }) == Some(log)
        {
            b.add(&[b.call("log", &[neg_arg]), turn])
        } else {
            let p = args[1];
            b.mul(&[b.pow(neg_arg, p), b.call("exp", &[b.mul(&[turn, p])])])
        };
        rules.push(h.call(h.sym(names::RULE), &[u, new]));
    }
    if rules.is_empty() {
        return Ok(Some(g));
    }
    Ok(Some(apply(cx, b, "rewrite", &[g, cx.list(&rules)])?))
}

/// The special functions with branch cuts on the real axis, with the index of the argument that has them.
const REAL_CUTS: &[(&str, usize, usize)] = &[
    // (name, arity, index of the argument)
    ("elliptic_k", 1, 0),
    ("elliptic_e", 1, 0),
    ("elliptic_e", 2, 1),
    ("elliptic_f", 2, 1),
    ("elliptic_pi", 2, 1),
    ("elliptic_pi", 2, 0),
    ("bessel_y", 2, 1),
    ("bessel_k", 2, 1),
    ("bessel_j", 2, 1),
    ("bessel_i", 2, 1),
    ("hankel_h1", 2, 1),
    ("hankel_h2", 2, 1),
    ("polylog", 2, 1),
    ("hypergeometric_2f1", 4, 3),
    ("gamma", 2, 1),
    ("gamma_regularized", 2, 1),
    ("exp_integral_e", 2, 1),
    ("exp_integral_ei", 1, 0),
    ("log_integral", 1, 0),
    ("product_log", 1, 0),
    ("legendre_q", 2, 1),
    ("struve_h", 2, 1),
];

/// `g` with each special function of [`REAL_CUTS`] whose argument tends to a point of a cut on the real axis from
/// the side where the function is not continuous replaced by its limit there. The jump and the side the principal
/// value belongs to are read from values at 40 digits at the point and at the point plus or minus 10^-30 im (a
/// difference above 10^-8 is a jump; the value at the point must equal one side to 10^-8). With the other arguments
/// real the function is symmetric (f(conj z) = conj f(z), checked by the same values), so the limit from the other
/// side is `conjugate(f(point))`. The side the argument comes from is the sign of its imaginary part beside the point
/// (as in [`off_branch_cuts`]); an undecided side on a cut gives `None`: no limit, never the value of the wrong side.
fn special_cuts<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, a: T<'h>, side: i8) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut found = vec![];
    let mut stack = vec![g];
    let mut seen = FxHashSet::default();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { continue };
        if let View::Sym(s) = h.view(head) {
            let name = h.sym_name(s);
            for &(n, arity, k) in REAL_CUTS {
                if n == name.as_str() && args.len() == arity && !is_free(b, args[k], x) && args.iter().enumerate().all(|(j, &p)| j == k || is_free(b, p, x)) {
                    found.push((u, k));
                }
            }
        }
        stack.extend_from_slice(args);
    }
    if found.is_empty() {
        return Ok(Some(g));
    }
    let im = h.sym_term(h.sym("im"));
    let rule = |l: T<'h>, r: T<'h>| h.call(h.sym(names::RULE), &[l, r]);
    let small = |cx: &mut NativeCx<'_, '_, 'h>, d: T<'h>, e: i64| -> Res<bool> {
        let ad = apply(cx, b, "abs", &[d])?;
        let lt = apply(cx, b, "less", &[ad, b.pow(b.int(10), b.int(-e))])?;
        Ok(cx.is_true(lt))
    };
    let mut rules = vec![];
    for (u, k) in found {
        cx.tick(1)?;
        let View::App { head, args } = h.view(u) else { continue };
        let arg = args[k];
        // the call at the point: every argument a number, the cut argument real
        let at = apply(cx, b, "rewrite", &[arg, rule(x, a)])?;
        let atn = apply(cx, b, "n", &[at, b.int(30)])?;
        if num_of(b, atn).is_none() || args.iter().enumerate().any(|(j, &p)| j != k && num_of(b, p).is_none()) {
            continue;
        }
        // the logarithmic singularities of Ei at 0 and li at 1 (-inf from both sides, taken by the walk)
        let name = b.sym_of(head).map(|s| h.sym_name(s).as_str().to_string());
        let singular = match name.as_deref() {
            Some("exp_integral_ei") => apply(cx, b, "equal", &[atn, b.zero()])? == h.sym_term(h.sym(names::TRUE)),
            Some("log_integral") => apply(cx, b, "equal", &[atn, b.one()])? == h.sym_term(h.sym(names::TRUE)),
            _ => false,
        };
        if singular {
            continue;
        }
        let call_at = |p: T<'h>| {
            let mut v = args.to_vec();
            v[k] = p;
            h.app(head, &v)
        };
        let eps = b.mul(&[im, b.pow(b.int(10), b.int(-30))]);
        let f0 = apply(cx, b, "n", &[call_at(at), b.int(40)])?;
        let fp = apply(cx, b, "n", &[call_at(b.add(&[at, eps])), b.int(40)])?;
        let fm = apply(cx, b, "n", &[call_at(b.sub(at, eps)), b.int(40)])?;
        // differences of the values themselves (n of a difference near 0 cannot reach its digits)
        let jump = apply(cx, b, "subtract", &[fp, fm])?;
        let ajump = apply(cx, b, "abs", &[jump])?;
        if num_of(b, ajump).is_none() || small(cx, jump, 8)? {
            // no jump at the point (or no number to tell): continuous there
            continue;
        }
        // the side whose limit the value at the point is
        let d_up = apply(cx, b, "subtract", &[f0, fp])?;
        let d_down = apply(cx, b, "subtract", &[f0, fm])?;
        let principal = if small(cx, d_up, 8)? {
            1
        } else if small(cx, d_down, 8)? {
            -1
        } else {
            return Ok(None);
        };
        // the side the argument comes from: the sign of its imaginary part beside the point
        let mut sgn = 0i8;
        let mut flat = true;
        for e in [6i64, 9, 12] {
            let pt = b.add(&[a, b.mul(&[b.int(side as i64), b.pow(b.int(10), b.int(-e))])]);
            let v = apply(cx, b, "rewrite", &[arg, rule(x, pt)])?;
            let v = apply(cx, b, "im_part", &[v])?;
            let v = apply(cx, b, "n", &[v, b.int(40)])?;
            let pos = apply(cx, b, "greater", &[v, b.zero()])?;
            let neg = apply(cx, b, "less", &[v, b.zero()])?;
            let zero = apply(cx, b, "equal", &[v, b.zero()])?;
            flat &= cx.is_true(zero);
            let s = if cx.is_true(pos) { 1 } else if cx.is_true(neg) { -1 } else { 0 };
            if s == 0 || (sgn != 0 && s != sgn) {
                sgn = 0;
                break;
            }
            sgn = s;
        }
        if flat {
            // along the cut itself: the principal values, which substitution gives
            continue;
        }
        if sgn == 0 {
            return Ok(None);
        }
        if sgn == principal {
            continue;
        }
        // the other side: the conjugate of the value at the point, when the function is symmetric there
        let cfp = apply(cx, b, "conjugate", &[fp])?;
        let sym = apply(cx, b, "subtract", &[cfp, fm])?;
        if !small(cx, sym, 8)? {
            return Ok(None);
        }
        rules.push(rule(u, b.call("conjugate", &[call_at(at)])));
    }
    if rules.is_empty() {
        return Ok(Some(g));
    }
    Ok(Some(apply(cx, b, "rewrite", &[g, cx.list(&rules)])?))
}

/// `g` with each inverse circular or hyperbolic function whose argument tends to a point of its branch cut written
/// with logarithms and square roots, the forms of the principal branches (DLMF 4.23.(iv), 4.37.(iv)):
///
/// | function | cut (argument at the point) | form |
/// |---|---|---|
/// | atan(z) | re z = 0, abs(im z) > 1 | (i/2)(log(1 - i z) - log(1 + i z)) |
/// | atanh(z) | im z = 0, abs(z) > 1 | (log(1 + z) - log(1 - z))/2 |
/// | asin(z), acos(z) | im z = 0, abs(z) > 1 | -i log(i z + (1 - z)^(1/2) (1 + z)^(1/2)), pi/2 - asin(z) |
/// | asinh(z) | re z = 0, abs(im z) > 1 | log(z + (z^2 + 1)^(1/2)) |
/// | acosh(z) | im z = 0, z < 1 | log(z + (z + 1)^(1/2) (z - 1)^(1/2)) |
///
/// and acot, acoth, asec, acsc, asech, acsch as the functions above of 1/z. Each form equals the function off its
/// cuts and has its cuts on the same set, so the side the argument comes from is then read by
/// [`off_branch_cuts`] from the logarithms and roots, as for any of them; substituting the point would give the value
/// on the cut, whichever side the path comes from.
fn inverses_on_cuts<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, a: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    const NAMES: [&str; 12] = ["atan", "acot", "atanh", "acoth", "asin", "acos", "asec", "acsc", "asinh", "acsch", "acosh", "asech"];
    let mut found = vec![];
    let mut stack = vec![g];
    let mut seen = FxHashSet::default();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { continue };
        if let View::Sym(s) = h.view(head) {
            let name = h.sym_name(s);
            if args.len() == 1 && !is_free(b, args[0], x) {
                if let Some(&n) = NAMES.iter().find(|&&n| n == name.as_str()) {
                    found.push((u, n, args[0]));
                }
            }
        }
        stack.extend_from_slice(args);
    }
    if found.is_empty() {
        return Ok(g);
    }
    let i = h.sym_term(h.sym("im"));
    let half = b.rat(1, 2);
    let mut rules = vec![];
    for (u, name, arg) in found {
        cx.tick(1)?;
        // the reciprocal functions: their base function of 1/z
        let (base, z) = match name {
            "acot" => ("atan", b.pow(arg, b.int(-1))),
            "acoth" => ("atanh", b.pow(arg, b.int(-1))),
            "asec" => ("acos", b.pow(arg, b.int(-1))),
            "acsc" => ("asin", b.pow(arg, b.int(-1))),
            "acsch" => ("asinh", b.pow(arg, b.int(-1))),
            "asech" => ("acosh", b.pow(arg, b.int(-1))),
            n => (n, arg),
        };
        // the argument at the point, as a number
        let at = apply(cx, b, "rewrite", &[z, h.call(h.sym(names::RULE), &[x, a])])?;
        let at = apply(cx, b, "n", &[at, b.int(30)])?;
        let re = apply(cx, b, "re", &[at])?;
        let im = apply(cx, b, "im_part", &[at])?;
        let is = |cx: &mut NativeCx<'_, '_, 'h>, rel: &str, l: T<'h>, r: T<'h>| -> Res<bool> {
            let v = apply(cx, b, rel, &[l, r])?;
            Ok(cx.is_true(v))
        };
        let (Some(_), Some(_)) = (num_of(b, re), num_of(b, im)) else { continue };
        let one = b.one();
        let abs_re = apply(cx, b, "abs", &[re])?;
        let abs_im = apply(cx, b, "abs", &[im])?;
        let on_cut = match base {
            "atan" | "asinh" => is(cx, "equal", re, b.zero())? && is(cx, "greater", abs_im, one)?,
            "atanh" | "asin" | "acos" => is(cx, "equal", im, b.zero())? && is(cx, "greater", abs_re, one)?,
            _ => is(cx, "equal", im, b.zero())? && is(cx, "less", re, one)?,
        };
        if !on_cut {
            continue;
        }
        let sqrt = |t: T<'h>| b.pow(t, half);
        let iz = b.mul(&[i, z]);
        let asin = |z: T<'h>| {
            let iz = b.mul(&[i, z]);
            let root = b.mul(&[sqrt(b.sub(b.one(), z)), sqrt(b.add(&[b.one(), z]))]);
            b.mul(&[b.neg(i), b.call("log", &[b.add(&[iz, root])])])
        };
        let new = match base {
            "atan" => b.mul(&[half, i, b.sub(b.call("log", &[b.sub(b.one(), iz)]), b.call("log", &[b.add(&[b.one(), iz])]))]),
            "atanh" => b.mul(&[half, b.sub(b.call("log", &[b.add(&[b.one(), z])]), b.call("log", &[b.sub(b.one(), z)]))]),
            "asin" => asin(z),
            "acos" => b.sub(b.mul(&[half, h.sym_term(h.sym("pi"))]), asin(z)),
            "asinh" => b.call("log", &[b.add(&[z, sqrt(b.add(&[b.pow(z, b.int(2)), b.one()]))])]),
            _ => b.call("log", &[b.add(&[z, b.mul(&[sqrt(b.add(&[z, b.one()])), sqrt(b.sub(z, b.one()))])])]),
        };
        rules.push(h.call(h.sym(names::RULE), &[u, new]));
    }
    if rules.is_empty() {
        return Ok(g);
    }
    apply(cx, b, "rewrite", &[g, cx.list(&rules)])
}

/// `Some(t)` when `t` is a number (an integer, rational, real or ball).
fn num_of<'h>(b: &Build<'h>, t: T<'h>) -> Option<T<'h>> {
    matches!(b.heap().view(t), View::Int(_) | View::Real(_) | View::Ball { .. } | View::Rational { .. } | View::BigInt { .. }).then_some(t)
}

/// `v` with logarithms of rationals split over primes (`log(25)` as `2 log(5)`, decision 13C.5), when that gives
/// a smaller term: limits of `a^x` quotients come out as `log(5)/log(2)`, not `(log(25) - log(5))/(log(4) - log(2))`.
pub(crate) fn tidy<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let log = h.sym("log");
    let mut changed = false;
    let mut memo = rustc_hash::FxHashMap::default();
    let t = split_logs(b, v, log, &mut memo, &mut changed);
    if !changed {
        return Ok(v);
    }
    let t = cx.eval(t)?;
    Ok(if gruntz::leaves(b, t) <= gruntz::leaves(b, v) { t } else { v })
}

fn split_logs<'h>(
    b: &Build<'h>,
    t: T<'h>,
    log: mc_api::Sym,
    memo: &mut rustc_hash::FxHashMap<T<'h>, T<'h>>,
    changed: &mut bool,
) -> T<'h> {
    let h = b.heap();
    if let Some(&r) = memo.get(&t) {
        return r;
    }
    let r = match h.view(t) {
        View::App { head, args } if b.sym_of(head) == Some(log) && args.len() == 1 => {
            let (n, d) = match h.view(args[0]) {
                View::Int(n) => (n, 1),
                View::Rational { .. } => match b.ar.num(args[0]).and_then(|q| q.as_rat().cloned()) {
                    Some(q) => match (q.num.to_i64(), mc_arith::num::Int::from_nat(q.den.clone()).to_i64()) {
                        (Some(n), Some(d)) => (n, d),
                        _ => (0, 0),
                    },
                    None => (0, 0),
                },
                _ => (0, 0),
            };
            match (primes(n), primes(d)) {
                (Some(pn), Some(pd)) if n > 0 && (pn.len() + pd.len() > 1 || pn.iter().chain(&pd).any(|&(_, e)| e > 1)) => {
                    *changed = true;
                    let mut terms = vec![];
                    for (p, e) in pn {
                        terms.push(b.mul(&[b.int(e), b.call("log", &[b.int(p)])]));
                    }
                    for (p, e) in pd {
                        terms.push(b.mul(&[b.int(-e), b.call("log", &[b.int(p)])]));
                    }
                    b.add(&terms)
                }
                _ => t,
            }
        }
        // n^p for a composite integer n and a symbolic p: the prime powers (4^-x 2^x is 2^-x)
        View::App { head, args: &[base, p] } if b.sym_of(head) == Some(b.hd.pow) && b.ar.num(p).is_none() => {
            let p = split_logs(b, p, log, memo, changed);
            match h.view(base) {
                View::Int(n) if n > 1 => match primes(n) {
                    Some(ps) if ps.len() > 1 || ps.iter().any(|&(_, e)| e > 1) => {
                        *changed = true;
                        b.mul(&ps.iter().map(|&(q, e)| b.pow(b.int(q), b.mul(&[b.int(e), p]))).collect::<Vec<_>>())
                    }
                    _ => b.pow(base, p),
                },
                _ => b.pow(split_logs(b, base, log, memo, changed), p),
            }
        }
        View::App { head, args } => {
            let na: Vec<T<'h>> = args.iter().map(|&a| split_logs(b, a, log, memo, changed)).collect();
            if na.as_slice() == args {
                t
            } else {
                match b.sym_of(head) {
                    Some(s) if s == b.hd.add => b.add(&na),
                    Some(s) if s == b.hd.mul => b.mul(&na),
                    Some(s) if s == b.hd.pow && na.len() == 2 => b.pow(na[0], na[1]),
                    _ => b.app(head, &na),
                }
            }
        }
        _ => t,
    };
    memo.insert(t, r);
    r
}

/// The prime factorisation of `n` (1 < n < 10^12, by trial division), `Some([])` for 1, `None` otherwise.
fn primes(n: i64) -> Option<Vec<(i64, i64)>> {
    if n == 1 {
        return Some(vec![]);
    }
    if !(2..1_000_000_000_000).contains(&n) {
        return None;
    }
    let (mut n, mut out, mut p) = (n, vec![], 2i64);
    while p * p <= n {
        let mut e = 0;
        while n % p == 0 {
            n /= p;
            e += 1;
        }
        if e > 0 {
            out.push((p, e));
        }
        p += if p == 2 { 1 } else { 2 };
    }
    if n > 1 {
        out.push((n, 1));
    }
    Some(out)
}

thread_local! {
    /// why the last two-sided limit does not exist (read for the failure's message)
    static REASON: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
}

/// Record the reason of a missing two-sided limit.
fn note(why: &'static str) {
    REASON.with(|r| r.set(Some(why)));
}

/// The two-sided limit from the one-sided ones (decision C.1).
fn both_sides<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, above: Lim<'h>, below: Lim<'h>) -> Res<Outcome<'h>> {
    Ok(match (above, below) {
        (Lim::Inf(p), Lim::Inf(q)) if p == q => Outcome::Value(value(b, above)),
        (Lim::Inf(p), Lim::Inf(q)) => {
            let _ = (p, q);
            note("the two-sided limit does not exist: the function is unbounded, tending to inf on one side and -inf on the other");
            Outcome::NoLimit
        }
        (Lim::Inf(_), _) | (_, Lim::Inf(_)) => {
            note("the two-sided limit does not exist: the function is unbounded on one side");
            Outcome::NoLimit
        }
        (Lim::Fin(p, _), Lim::Fin(q, _)) if p == q => Outcome::Value(p),
        (Lim::Fin(p, _), Lim::Fin(q, _)) => {
            let diff = apply(cx, b, "expand", &[b.sub(p, q)])?;
            let diff = tidy(cx, b, diff)?;
            match sign(cx, b, diff)? {
                Some(0) => Outcome::Value(p),
                Some(_) => {
                    note("the two-sided limit does not exist: the one-sided limits differ");
                    Outcome::NoLimit
                }
                None => {
                    // two forms of one constant (log(25) and 2 log(5)): equal when simplify says so (13C)
                    let d = apply(cx, b, "simplify", &[diff])?;
                    if d == b.zero() {
                        let sp = apply(cx, b, "simplify", &[p])?;
                        let small = gruntz::leaves(b, sp) < gruntz::leaves(b, p);
                        Outcome::Value(if small { sp } else { p })
                    } else {
                        Outcome::Unknown
                    }
                }
            }
        }
    })
}

/// The term of a limit: the value, `inf` or `-inf`.
fn value<'h>(b: &Build<'h>, l: Lim<'h>) -> T<'h> {
    let inf = b.heap().sym_term(b.heap().sym("inf"));
    match l {
        Lim::Fin(v, _) => v,
        Lim::Inf(1) => inf,
        Lim::Inf(_) => b.neg(inf),
    }
}

/// Which function a [`LimitFn`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `limit`
    Limit,
    /// `max_limit`: the limit superior
    MaxLimit,
    /// `min_limit`: the limit inferior
    MinLimit,
    /// `discrete_limit`: the limit of a sequence
    Discrete,
    /// `asymptotic`: the leading term of the asymptotic expansion
    Asymptotic,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Limit => "limit",
            Kind::MaxLimit => "max_limit",
            Kind::MinLimit => "min_limit",
            Kind::Discrete => "discrete_limit",
            Kind::Asymptotic => "asymptotic",
        }
    }
}

/// `limit(f, x -> a)`, `limit(f, x -> a, direction: from_above | from_below)`, and the relatives of [`Kind`]:
/// `x -> a` may also be a list of rules (iterated limits, the last rule innermost) and, for `limit`,
/// `[x, y] -> [a, b]` (a multivariate limit, decided only through one combination of the variables).
pub struct LimitFn {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
    /// the function
    pub kind: Kind,
}

/// `failure(tag, details...)`.
fn failure<'h>(b: &Build<'h>, tag: &str, details: &[T<'h>]) -> T<'h> {
    let h = b.heap();
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

/// `failure(argument, name, message)`.
fn bad<'h>(b: &Build<'h>, name: &str, message: &str) -> T<'h> {
    let h = b.heap();
    failure(b, "argument", &[h.sym_term(h.sym(name)), h.string(message)])
}

/// A direction off the real line.
enum Toward<'h> {
    /// along the real line (the [`Direction`])
    Real,
    /// along the ray x = a - d s, s → 0 from above, for a complex number d (`im`: from below the real axis)
    Along(T<'h>),
    /// from every direction in the complex plane
    Plane,
}

/// The direction of a trailing `options(direction -> d)`, and the arguments before it; `Err` with the failure.
fn options<'a, 'h>(b: &Build<'h>, fname: &str, args: &'a [T<'h>]) -> Result<(&'a [T<'h>], Direction, Toward<'h>), T<'h>> {
    let h = b.heap();
    let Some((&last, rest)) = args.split_last() else { return Ok((args, Direction::TwoSided, Toward::Real)) };
    let View::App { head, args: rules } = h.view(last) else { return Ok((args, Direction::TwoSided, Toward::Real)) };
    if b.sym_of(head) != Some(h.sym("options")) {
        return Ok((args, Direction::TwoSided, Toward::Real));
    }
    let mut direction = Direction::TwoSided;
    let mut toward = Toward::Real;
    for &r in rules {
        let View::App { head, args: &[k, v] } = h.view(r) else { return Err(bad(b, fname, "options are name: value")) };
        let (Some(rule), Some(name)) = (b.sym_of(head), b.sym_of(k)) else {
            return Err(bad(b, fname, "options are name: value"));
        };
        if rule != h.sym(names::RULE) {
            return Err(bad(b, fname, "options are name: value"));
        }
        if h.sym_name(name) != "direction" {
            return Err(failure(b, "unknown_option", &[k]));
        }
        // also the numeric forms -1 (from above) and 1 (from below), and reals for both sides along the real line
        direction = match (b.sym_of(v).map(|s| h.sym_name(s)).as_deref(), h.view(v)) {
            (Some("from_above"), _) | (_, View::Int(-1)) => Direction::FromAbove,
            (Some("from_below"), _) | (_, View::Int(1)) => Direction::FromBelow,
            (Some("two_sided" | "reals"), _) => Direction::TwoSided,
            (Some("complexes"), _) => {
                toward = Toward::Plane;
                Direction::TwoSided
            }
            // a complex direction (checked to be off the real line where it is used)
            _ if mc_calculus::diff::has_complex(h, v) || b.sym_of(v) == Some(h.sym("im")) => {
                toward = Toward::Along(v);
                Direction::TwoSided
            }
            _ => return Err(bad(b, fname, "direction is from_above, from_below, two_sided, reals, complexes or a complex number")),
        };
    }
    Ok((rest, direction, toward))
}

/// `limit(f, x -> a, direction: d)` for a complex d off the real line: the one-sided limit of f(a - d s) as the real
/// s tends to 0 from above (the ray from which x approaches a). `None` (unevaluated) for an infinite or variable
/// point, a d on the real line, and a limit not found.
fn along<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, spec: T<'h>, dv: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let View::App { head, args: &[x, a] } = h.view(spec) else { return Ok(None) };
    if b.sym_of(head) != Some(h.sym(names::RULE)) || !matches!(h.view(x), View::Sym(_)) || !is_free(b, a, x) {
        return Ok(None);
    }
    if !is_free(b, a, h.sym_term(h.sym("inf"))) || !is_free(b, a, h.sym_term(h.sym("complex_infinity"))) {
        return Ok(None);
    }
    let im = cx.eval(b.call("im_part", &[dv]))?;
    if !matches!(sign(cx, b, im)?, Some(1 | -1)) {
        return Ok(None);
    }
    let s = h.sym_term(h.sym("s%toward"));
    let point = cx.eval(b.sub(a, b.mul(&[dv, s])))?;
    let g = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, point])]))?;
    Ok(match limit(cx, b, g, s, b.zero(), Direction::FromAbove)? {
        Outcome::Value(v) if is_free(b, v, s) => Some(v),
        _ => None,
    })
}

/// `t` with the parts of a real variable `s` taken out of `re`, `im_part` and `conjugate`: a term c s^k of a sum
/// inside them (c a number, k a natural number) leaves as re(c) s^k, im_part(c) s^k or conjugate(c) s^k, since
/// these functions are additive and real-linear (`re(x + s) = re(x) + s` for a real s). `arg(u)` with re(u) > 0 at
/// the point `at` is `atan(im_part(u)/re(u))` near it, and is split the same way.
fn split_real_variable<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, s: T<'h>, at: T<'h>, depth: usize) -> Res<T<'h>> {
    let h = b.heap();
    if is_free(b, t, s) || depth > 64 {
        return Ok(t);
    }
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    let mut new = Vec::with_capacity(args.len());
    for &a in args {
        new.push(split_real_variable(cx, b, a, s, at, depth + 1)?);
    }
    let t = if new.iter().zip(args).all(|(a, b)| a == b) { t } else { b.app(head, &new) };
    let name = b.sym_of(head).map(|x| h.sym_name(x));
    let (Some(name), [u]) = (name.as_deref(), &new[..]) else { return Ok(t) };
    if name == "arg" {
        let u0 = cx.eval(h.call(h.sym("rewrite"), &[*u, h.call(h.sym(names::RULE), &[s, at])]))?;
        let r0 = cx.eval(b.call("re", &[u0]))?;
        if is_free(b, r0, s) && sign(cx, b, r0)? == Some(1) {
            let re = split_real_variable(cx, b, b.call("re", &[*u]), s, at, depth + 1)?;
            let im = split_real_variable(cx, b, b.call("im_part", &[*u]), s, at, depth + 1)?;
            return cx.eval(b.call("atan", &[b.div(im, re)]));
        }
        return Ok(t);
    }
    if !matches!(name, "re" | "im_part" | "conjugate") {
        return Ok(t);
    }
    let terms: Vec<T<'h>> = match h.view(*u) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym(names::ADD)) => args.to_vec(),
        _ => vec![*u],
    };
    // c s^k: the power of s, and the numeric factor
    let power = |f: T<'h>| -> bool {
        f == s || matches!(h.view(f), View::App { head, args: &[bb, e] } if b.sym_of(head) == Some(h.sym(names::POW)) && bb == s && matches!(h.view(e), View::Int(k) if k > 0))
    };
    let mut rest = vec![];
    let mut out = vec![];
    for term in terms {
        let factors: Vec<T<'h>> = match h.view(term) {
            View::App { head, args } if b.sym_of(head) == Some(h.sym(names::MUL)) => args.to_vec(),
            _ => vec![term],
        };
        let (ps, cs): (Vec<T<'h>>, Vec<T<'h>>) = factors.iter().partition(|&&f| power(f));
        let numeric = cs.iter().all(|&c| is_free(b, c, s)) && {
            let c = b.mul(&cs);
            let q = cx.eval(b.call("is_numeric", &[c]))?;
            q == h.sym_term(h.sym(names::TRUE))
        };
        if ps.len() == 1 && numeric {
            let c = cx.eval(b.call(name, &[b.mul(&cs)]))?;
            out.push(b.mul(&[c, ps[0]]));
        } else {
            rest.push(term);
        }
    }
    if out.is_empty() {
        return Ok(t);
    }
    out.push(b.call(name, &[b.add(&rest)]));
    cx.eval(b.add(&out))
}

/// `limit(f, x -> a, direction: complexes)`: the limits along the rays from 1, i, -1 and -i (see [`along`]); two
/// of them found and different (their difference a nonzero number) prove that there is no limit. `None` otherwise
/// (equal limits along four rays do not prove one).
fn plane_rays<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, spec: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let im = h.sym_term(h.sym("im"));
    let mut found: Vec<T<'h>> = vec![];
    for d in [b.one(), im, b.int(-1), b.neg(im)] {
        let d = cx.eval(d)?;
        let Some(v) = ray(cx, b, f, spec, d)? else { continue };
        for &w in &found {
            let diff = cx.eval(b.sub(v, w))?;
            let numeric = cx.eval(b.call("is_numeric", &[diff]))? == h.sym_term(h.sym(names::TRUE));
            if numeric && !b.is_zero(diff) {
                let z = cx.eval(b.call("is_possible_zero", &[diff]))?;
                if z == h.sym_term(h.sym(names::FALSE)) {
                    let why = "the limits along two directions differ";
                    return Ok(Some(failure(b, "no_limit", &[h.sym_term(h.sym("limit")), f, h.string(why)])));
                }
            }
        }
        found.push(v);
    }
    Ok(None)
}

/// The one-sided limit of f(a - d s) as the real s tends to 0 from above, for any nonzero number d (on the real
/// line too); the parts of s are taken out of `re`, `im_part` and `conjugate` first. `None` when not found.
fn ray<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, spec: T<'h>, dv: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let View::App { head, args: &[x, a] } = h.view(spec) else { return Ok(None) };
    if b.sym_of(head) != Some(h.sym(names::RULE)) || !matches!(h.view(x), View::Sym(_)) || !is_free(b, a, x) {
        return Ok(None);
    }
    if !is_free(b, a, h.sym_term(h.sym("inf"))) || !is_free(b, a, h.sym_term(h.sym("complex_infinity"))) {
        return Ok(None);
    }
    let s = h.sym_term(h.sym("s%toward"));
    let point = cx.eval(b.sub(a, b.mul(&[dv, s])))?;
    let g = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, point])]))?;
    let g = split_real_variable(cx, b, g, s, b.zero(), 0)?;
    Ok(match limit(cx, b, g, s, b.zero(), Direction::FromAbove)? {
        Outcome::Value(v) if is_free(b, v, s) => Some(v),
        _ => None,
    })
}

/// `limit(f, x -> a, direction: complexes)` (and `max_limit`, `min_limit`, `real` set: the value must be real) for a
/// finite a: `f(a)` when f is continuous on the plane at a, i.e. built from the variable, constants, sums, products,
/// powers with integer exponents (a negative one only of a base that is not 0 at a), and the functions continuous
/// on the whole plane (exp, sin, cos, sinh, cosh, abs, re, im_part, conjugate). `None` otherwise.
fn plane_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, spec: T<'h>, real: bool) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let View::App { head, args: &[x, a] } = h.view(spec) else { return Ok(None) };
    if b.sym_of(head) != Some(h.sym(names::RULE)) || !matches!(h.view(x), View::Sym(_)) || !is_free(b, a, x) {
        return Ok(None);
    }
    for bad in ["inf", "complex_infinity", "indeterminate"] {
        if !is_free(b, a, h.sym_term(h.sym(bad))) {
            return Ok(None);
        }
    }
    if !continuous_at(cx, b, f, x, a)? {
        return Ok(None);
    }
    let v = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, a])]))?;
    for bad in ["inf", "complex_infinity", "indeterminate", names::FAILURE] {
        if !is_free(b, v, h.sym_term(h.sym(bad))) {
            return Ok(None);
        }
    }
    if real {
        let im = cx.eval(b.call("im_part", &[v]))?;
        if sign(cx, b, im)? != Some(0) {
            return Ok(None);
        }
    }
    Ok(Some(v))
}

/// Is `f` (a term in `x`) continuous on the complex plane at `x = a`? See [`plane`].
fn continuous_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, a: T<'h>) -> Res<bool> {
    const CONTINUOUS: &[&str] = &["exp", "sin", "cos", "sinh", "cosh", "abs", "re", "im_part", "conjugate"];
    let h = b.heap();
    cx.tick(1)?;
    if is_free(b, f, x) {
        // a constant: finite
        return Ok(["inf", "complex_infinity", "indeterminate"].iter().all(|s| is_free(b, f, h.sym_term(h.sym(s)))));
    }
    if f == x {
        return Ok(true);
    }
    let View::App { head, args } = h.view(f) else { return Ok(false) };
    let Some(name) = b.sym_of(head).map(|s| h.sym_name(s).to_string()) else { return Ok(false) };
    match (name.as_str(), args) {
        (names::ADD | names::MUL, _) => {
            for &u in args {
                if !continuous_at(cx, b, u, x, a)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        // c^u = exp(u log c) for a constant c != 0 (e^u among them)
        (names::POW, &[base, u]) if is_free(b, base, x) && !matches!(h.view(u), View::Int(_)) => {
            let m = cx.eval(b.call("abs", &[base]))?;
            Ok(sign(cx, b, m)? == Some(1) && continuous_at(cx, b, u, x, a)?)
        }
        (names::POW, &[base, k]) => {
            let View::Int(k) = h.view(k) else { return Ok(false) };
            if !continuous_at(cx, b, base, x, a)? {
                return Ok(false);
            }
            if k >= 0 {
                return Ok(true);
            }
            let v = cx.eval(h.call(h.sym("rewrite"), &[base, h.call(h.sym(names::RULE), &[x, a])]))?;
            let m = cx.eval(b.call("abs", &[v]))?;
            Ok(sign(cx, b, m)? == Some(1))
        }
        (g, &[u]) if CONTINUOUS.contains(&g) => continuous_at(cx, b, u, x, a),
        _ => Ok(false),
    }
}

impl Native for LimitFn {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let name = self.kind.name();
        let (args, direction, toward) = match options(&b, name, args) {
            Ok(v) => v,
            Err(f) => return Ok(Reply::Done(f)),
        };
        let [f, spec] = args else { return Ok(Reply::Decline) };
        match toward {
            Toward::Real => {}
            // from every direction of the plane: the value at the point when f is continuous there; else, for a
            // plain limit, no limit when two rays give different limits
            Toward::Plane if self.kind != Kind::Discrete && self.kind != Kind::Asymptotic => {
                if let Some(v) = plane_value(cx, &b, *f, *spec, self.kind != Kind::Limit)? {
                    return Ok(Reply::Again(v));
                }
                if self.kind == Kind::Limit {
                    return Ok(plane_rays(cx, &b, *f, *spec)?.map_or(Reply::Decline, Reply::Done));
                }
                return Ok(Reply::Decline);
            }
            Toward::Plane => return Ok(Reply::Decline),
            Toward::Along(dv) if self.kind == Kind::Limit => {
                return Ok(along(cx, &b, *f, *spec, dv)?.map_or(Reply::Decline, Reply::Again));
            }
            Toward::Along(_) => return Ok(Reply::Decline),
        }
        // the points: one rule, or a list of rules (iterated, the last innermost)
        let rules: Vec<T<'h>> = match h.view(*spec) {
            View::App { head, args: items } if b.sym_of(head) == Some(b.hd.list) && !items.is_empty() => items.to_vec(),
            _ => vec![*spec],
        };
        let mut points = Vec::with_capacity(rules.len());
        for &r in &rules {
            let (x, a) = match h.view(r) {
                View::App { head, args: &[x, a] } if b.sym_of(head) == Some(h.sym(names::RULE)) => (x, a),
                _ => return Ok(Reply::Done(bad(&b, name, "the point is x -> a"))),
            };
            if let (Some(xs), Some(az)) = (args_of(&b, x, b.hd.list), args_of(&b, a, b.hd.list)) {
                if rules.len() == 1 && self.kind == Kind::Limit && xs.len() == az.len() {
                    return Ok(multivariate(cx, &b, *f, xs, az)?.map_or(Reply::Decline, Reply::Again));
                }
                return Ok(Reply::Decline);
            }
            let x = match mc_calculus::common::variable(&b, x) {
                Ok(x) => x,
                Err(message) => return Ok(Reply::Done(bad(&b, name, message))),
            };
            if !is_free(&b, a, x) {
                return Ok(Reply::Done(bad(&b, name, "the point depends on the variable")));
            }
            let inf = h.sym_term(h.sym("inf"));
            let contrary = (a == inf && direction == Direction::FromAbove)
                || (a == b.neg(inf) && direction == Direction::FromBelow);
            if contrary {
                return Ok(Reply::Done(bad(&b, name, "an infinity is approached from the finite side")));
            }
            points.push((x, a));
        }
        // a sum over the roots of a polynomial free of the variable: the roots stay fixed, so the limit of the finite
        // sum is the sum of the limits of the summand (when that is a finite value)
        if let ([(x, a)], Kind::Limit, Direction::TwoSided) = (&points[..], self.kind, direction) {
            if let Some(&[p, g]) = args_of(&b, *f, h.sym("root_sum")) {
                if let (true, Some(&[t, body]), View::App { head: fh, .. }) = (is_free(&b, p, *x), args_of(&b, g, h.sym("fn")), h.view(g)) {
                    if t != *x {
                        let l = cx.eval(h.call(h.sym("limit"), &[body, h.call(h.sym(names::RULE), &[*x, *a])]))?;
                        // a finite value: no limit left, no infinity, no failure
                        let mut finite = true;
                        let mut stack = mc_api::walk::Walk::new(l);
                        while let Some(u) = stack.pop() {
                            match h.view(u) {
                                View::Sym(s) if matches!(h.sym_name(s).as_str(), "inf" | "complex_infinity" | "indeterminate") => finite = false,
                                View::App { head, args } => {
                                    if matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "limit" | "failure")) {
                                        finite = false;
                                    }
                                    stack.extend_from_slice(args);
                                }
                                _ => {}
                            }
                        }
                        if finite {
                            return Ok(Reply::Again(h.call(h.sym("root_sum"), &[p, h.app(fh, &[t, l])])));
                        }
                    }
                }
            }
        }
        let kind = self.kind;
        let stop = self.stop.clone();
        let one = |cx: &mut NativeCx<'_, '_, 'h>, g: T<'h>| -> Res<Option<T<'h>>> {
            let mut g = g;
            REASON.with(|r| r.set(None));
            for &(x, a) in points.iter().rev() {
                let opts = Options::default();
                let r = match kind {
                    Kind::Limit | Kind::Discrete => {
                        let o = if kind == Kind::Limit {
                            // the variable is real along the real line
                            g = split_real_variable(cx, &b, g, x, a, 0)?;
                            limit(cx, &b, g, x, a, direction)?
                        } else {
                            more::discrete(cx, &b, g, x, a, &opts)?
                        };
                        match o {
                            Outcome::Value(v) => Some(v),
                            Outcome::NoLimit => {
                                let mut d = vec![h.sym_term(h.sym(kind.name())), g];
                                if let Some(why) = REASON.with(|r| r.take()) {
                                    d.push(h.string(why));
                                }
                                return Ok(Some(failure(&b, "no_limit", &d)));
                            }
                            // undecided: a limit superior above the limit inferior proves that there is none
                            // (sin(1/x) at 0 oscillates between -1 and 1)
                            Outcome::Unknown if kind == Kind::Limit => {
                                let hi = more::extreme(cx, &b, g, x, a, direction, opts.budget, true)?;
                                let lo = more::extreme(cx, &b, g, x, a, direction, opts.budget, false)?;
                                match (hi, lo) {
                                    (Some(hi), Some(lo)) if sign(cx, &b, b.sub(hi, lo))? == Some(1) => {
                                        let why = "the limit superior is above the limit inferior";
                                        let d = vec![h.sym_term(h.sym(kind.name())), g, h.string(why)];
                                        return Ok(Some(failure(&b, "no_limit", &d)));
                                    }
                                    _ => None,
                                }
                            }
                            // a sequence: its upper limit above its lower limit (sin(n), whose values are dense in
                            // [-1, 1]) proves the same
                            Outcome::Unknown if kind == Kind::Discrete => {
                                let rule = h.call(h.sym(names::RULE), &[x, a]);
                                let hi = cx.eval(h.call(h.sym("discrete_max_limit"), &[g, rule]))?;
                                let lo = cx.eval(h.call(h.sym("discrete_min_limit"), &[g, rule]))?;
                                let held = |t: T<'h>| !is_free(&b, t, h.sym_term(h.sym("discrete_max_limit"))) || !is_free(&b, t, h.sym_term(h.sym("discrete_min_limit")));
                                if !held(hi) && !held(lo) && sign(cx, &b, b.sub(hi, lo))? == Some(1) {
                                    let why = "the limit superior is above the limit inferior";
                                    let d = vec![h.sym_term(h.sym(kind.name())), g, h.string(why)];
                                    return Ok(Some(failure(&b, "no_limit", &d)));
                                }
                                None
                            }
                            Outcome::Unknown => None,
                        }
                    }
                    Kind::MaxLimit | Kind::MinLimit => {
                        let upper = kind == Kind::MaxLimit;
                        match more::extreme(cx, &b, g, x, a, direction, opts.budget, upper)? {
                            Some(v) => Some(tidy(cx, &b, v)?),
                            // where the limit exists, the limits superior and inferior are the limit
                            None => match limit(cx, &b, g, x, a, direction)? {
                                Outcome::Value(v) => Some(v),
                                _ => None,
                            },
                        }
                    }
                    Kind::Asymptotic => match more::asymptotic(cx, &b, &stop, g, x, a, direction, opts.budget)? {
                        Some(v) => Some(tidy(cx, &b, v)?),
                        None => None,
                    },
                };
                match r {
                    Some(v) => g = v,
                    None => return Ok(None),
                }
            }
            Ok(Some(g))
        };
        let r = match h.view(*f) {
            View::App { head, args: items } if b.sym_of(head) == Some(b.hd.list) => {
                let mut out = Vec::with_capacity(items.len());
                for &g in items {
                    match one(cx, g)? {
                        Some(v) => out.push(v),
                        None => return Ok(Reply::Decline),
                    }
                }
                Some(h.call(b.hd.list, &out))
            }
            _ => one(cx, *f)?,
        };
        Ok(r.map_or(Reply::Decline, Reply::Again))
    }
}

/// `limit(f, [x, y, ...] -> [a, b, ...])` when `f` depends on the variables only through one subterm `u` (a call's
/// argument, `y q` with `y` the last variable and `q` free of it): the two-sided limit of `F(s)` at
/// `s → u(a, b, ...)`, which holds for every approach (item 13C; `tan(x y)/(x y)` at the origin). `None`
/// otherwise.
fn multivariate<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, xs: &[T<'h>], az: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let Some(&y) = xs.last() else { return Ok(None) };
    let s = h.sym_term(h.sym("s%multi"));
    // candidate combinations: the arguments of calls (not arithmetic) that depend on every variable
    let mut cands = vec![];
    let mut stack = vec![f];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(t) = stack.pop() {
        if !seen.insert(t) {
            continue;
        }
        if let View::App { head, args } = h.view(t) {
            let arith = [b.hd.add, b.hd.mul, b.hd.pow].iter().any(|&a| b.sym_of(head) == Some(a));
            for &a in args {
                if !arith && xs.iter().all(|&v| !is_free(b, a, v)) && !cands.contains(&a) {
                    cands.push(a);
                }
                stack.push(a);
            }
        }
    }
    for u in cands {
        // u = y q with q free of y: y = s / q
        let q = cx.eval(b.div(u, y))?;
        if !is_free(b, q, y) {
            continue;
        }
        let g = crate::bounds::subst(cx, b, f, y, b.div(s, q))?;
        if xs.iter().any(|&v| !is_free(b, g, v)) || mc_calculus::common::is_failure(b, g) {
            continue;
        }
        let mut u0 = u;
        for (&v, &a) in xs.iter().zip(az) {
            u0 = crate::bounds::subst(cx, b, u0, v, a)?;
        }
        if let Outcome::Value(v) = limit(cx, b, g, s, u0, Direction::TwoSided)? {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

/// Register the natives of this crate with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut mc_eval::Kernel, h: &mc_core::Heap, stop: &Arc<AtomicBool>) {
    for (name, kind) in [
        ("limit", Kind::Limit),
        ("max_limit", Kind::MaxLimit),
        ("min_limit", Kind::MinLimit),
        ("discrete_limit", Kind::Discrete),
        ("asymptotic", Kind::Asymptotic),
    ] {
        k.register(h.sym(name), mc_eval::Attributes::default(), wrap(name, LimitFn { stop: stop.clone(), kind }));
    }
}
