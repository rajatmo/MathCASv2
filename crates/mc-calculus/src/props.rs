//! Properties of functions (decision 2C.11): `function_domain`, `function_singularities`, `function_analytic`,
//! `function_sign`, `function_monotonicity`, `function_convexity`, `function_injective`.
//!
//! Each takes `f` or `[f, condition]`, a variable or a list of variables, and optionally a domain (`reals`, the
//! default, or `complexes`; `integers` for the sign and the monotonicity of a sequence); trailing options are
//! accepted and ignored.
//!
//! - **Domain**: the conditions under which every subterm is defined: bases of negative powers nonzero, of fractional
//!   powers with an even denominator nonnegative (positive under a negative power), arguments of `log` positive
//!   (nonzero over the complexes), of `asin`/`acos` in [-1, 1], `acosh` at least 1, `atanh` in (-1, 1), `cos(u) != 0`
//!   under `tan`/`sec`, `sin(u) != 0` under `cot`/`csc`. Over the reals with one variable the conjunction is given
//!   to `reduce`; otherwise it is the conjunction itself. A function this module does not know leaves the call
//!   unevaluated.
//! - **Singularities**: the negations of the domain conditions (`x <= 0` for `sqrt(x)`, `cos(x) == 0` for `tan(x)`);
//!   **analytic**: `true` when there are none (or the condition excludes them), `false` otherwise.
//! - **Sign, monotonicity, convexity** of the function, its derivative, its second derivative: `1` when the value
//!   is at least 0 everywhere on the domain, `-1` when at most 0, `0` when it vanishes, `indeterminate` otherwise.
//!   Decided by `refine` under the domain's facts when it can; otherwise, for one variable without parameters, by
//!   sampling: a change of sign among the samples is a proof of `indeterminate`, samples of one sign are taken as
//!   the answer (the sampling covers the domain from 10^-6 to 10^6 in both directions; decision 2C.12).
//!   A function with jumps or kinks (`floor`, `unit_step`, `abs`, `max`, ...: the derivative misses the jumps and the
//!   second derivative the kinks) is decided by the samples of the function itself: its differences for the
//!   monotonicity, its slopes between neighbouring samples for the convexity.
//! - **Injective**: strictly monotonic (by the derivative's sign) is `true`; two equal values found among the
//!   samples is `false`; equal neighbouring machine values of samples otherwise monotonic (a function that
//!   saturates, like `logistic_sigmoid` at 40) are compared again at 40 digits.
//! - Over the complexes the functions entire on the reals that have poles or branch points off the real line are
//!   not analytic everywhere: `tanh`, `sech`, `gudermannian` (`cosh(u) != 0`), `coth`, `csch` (`sinh(u) != 0`),
//!   and the inverse trigonometric and hyperbolic functions off their branch cuts.
//! - The special functions: what each one is (domain, singular set, range, bounds, sign, monotonicity, convexity,
//!   periodicity) is kept once in [`table`], with its literature, and read by every property.
//! - The samples of one call stop after a time budget and the call declines (a resource guard, see [`Budget`]).
mod domain_form;
mod onto;
mod period;
mod poles;
mod pw;
mod range;
mod sets;
mod shape;
mod smooth;
mod spec;
mod table;

use crate::common::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

thread_local! {
    /// When the samples of the current property call must stop (the outermost call sets it).
    static DEADLINE: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// The time the samples of one property call may take: numeric evaluation of some special functions is slow, and
/// a call that cannot finish its samples declines instead of running into the evaluation's time limit.
const SAMPLE_BUDGET: std::time::Duration = std::time::Duration::from_millis(2500);

/// The budget of the outermost property call on this thread, released on drop.
struct Budget(bool);

impl Budget {
    fn enter() -> Budget {
        DEADLINE.with(|d| {
            if d.get().is_some() {
                Budget(false)
            } else {
                d.set(Some(std::time::Instant::now() + SAMPLE_BUDGET));
                Budget(true)
            }
        })
    }
}

impl Drop for Budget {
    fn drop(&mut self) {
        if self.0 {
            DEADLINE.with(|d| d.set(None));
        }
    }
}

/// Has the sample budget of the current call run out?
pub(crate) fn out_of_time() -> bool {
    DEADLINE.with(|d| d.get().is_some_and(|t| std::time::Instant::now() > t))
}

/// The property a native computes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prop {
    /// `function_domain`
    Domain,
    /// `function_singularities`
    Singularities,
    /// `function_analytic`
    Analytic,
    /// `function_sign`
    Sign,
    /// `function_monotonicity`
    Monotonicity,
    /// `function_convexity`
    Convexity,
    /// `function_injective`
    Injective,
    /// `function_discontinuities`
    Discontinuities,
    /// `function_continuous`
    Continuous,
    /// `function_meromorphic`
    Meromorphic,
    /// `function_period`
    Period,
    /// `function_poles`
    Poles,
    /// `function_surjective`
    Surjective,
    /// `function_bijective`
    Bijective,
    /// `piecewise_expand`
    PiecewiseExpand,
    /// `function_range`
    Range,
}

/// The native of one property.
pub struct PropNative {
    /// the property
    pub p: Prop,
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// Functions defined (and analytic) for every real argument.
const ENTIRE: &[&str] = &[
    "sin", "cos", "exp", "sinh", "cosh", "tanh", "atan", "erf", "erfc", "erfi", "airy_ai", "airy_bi", "sech",
    "airy_ai_prime", "airy_bi_prime", "dawson_f", "fresnel_c", "fresnel_s", "sin_integral", "sinh_integral", "sinc",
    "gudermannian",
];
/// Functions of several arguments analytic in the last one for every real value of it, when the others are free of
/// the variables (polynomial families, entire hypergeometric functions; decision IOSYM.5).
const ENTIRE_LAST: &[&str] = &[
    "hermite_h", "laguerre_l", "chebyshev_t", "chebyshev_u", "lucas_l", "fibonacci", "hypergeometric_0f1",
    "hypergeometric_0f1_regularized", "hypergeometric_1f1_regularized",
];
/// Functions of an integer order and a variable analytic in the variable for an integer order.
const ENTIRE_INTEGER_ORDER: &[&str] = &["bessel_j", "bessel_i", "spherical_bessel_j"];
/// Functions defined for every real argument but not analytic everywhere.
const CONTINUOUS: &[&str] = &[
    "abs", "max", "min", "unit_step", "sign", "floor", "ceiling", "round", "re", "im_part", "conjugate", "integer_part",
    "fractional_part", "clip", "ramp", "real_abs", "real_sign", "unit_box", "unit_triangle", "heaviside_theta", "arg",
];

/// The call's pieces.
struct Call<'h> {
    f: T<'h>,
    cond: Option<T<'h>>,
    vars: Vec<T<'h>>,
    complex: bool,
    integers: bool,
    /// facts about the parameters
    facts: Vec<T<'h>>,
    /// `strict_inequalities: true`
    strict: bool,
}

impl Native for PropNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let _scope = StopScope::enter(Some(&self.stop));
        let _budget = Budget::enter();
        let b = Build::new(h, &self.stop);
        if self.p == Prop::PiecewiseExpand {
            return Ok(pw::expand(cx, &b, args)?.map_or(Reply::Decline, Reply::Done));
        }
        if self.p == Prop::Range {
            return Ok(range::range(cx, &b, args)?.map_or(Reply::Decline, Reply::Done));
        }
        let Some(mut sp) = spec::parse(cx, &b, args)? else { return Ok(Reply::Decline) };
        let new = match self.p {
            Prop::Discontinuities => Some(smooth::set_of(cx, &b, &sp, sets::Mode::Disc)?),
            Prop::Singularities => Some(smooth::set_of(cx, &b, &sp, sets::Mode::Sing)?),
            Prop::Continuous => Some(smooth::free_of(cx, &b, &sp, sets::Mode::Disc, false)?),
            Prop::Analytic => Some(smooth::free_of(cx, &b, &sp, sets::Mode::Sing, false)?),
            Prop::Meromorphic => {
                if !sp.dom_given {
                    sp.dom = spec::Dom::Complexes;
                }
                Some(smooth::free_of(cx, &b, &sp, sets::Mode::Sing, true)?)
            }
            Prop::Period => Some(period::period(cx, &b, &sp)?),
            Prop::Poles => Some(poles::poles(cx, &b, &sp)?),
            Prop::Surjective => Some(onto::decide(cx, &b, &sp, onto::Want::Surjective)?),
            Prop::Bijective => {
                let r = onto::decide(cx, &b, &sp, onto::Want::Bijective)?;
                let simple = !sp.vector && sp.vars.len() == 1 && sp.dom == spec::Dom::Reals && sp.ycond.is_none() && spec::params(&b, &sp.comps, &sp.vars).is_empty();
                if r.is_none() && simple {
                    // injective by the derivative's sign or samples, onto by the limits
                    let c = Call { f: sp.f, cond: sp.cond, vars: sp.vars.clone(), complex: false, integers: false, facts: sp.facts.clone(), strict: false };
                    match injective(cx, &b, &c)? {
                        Some(i) if cx.is_false(i) => Some(Some(i)),
                        Some(i) if cx.is_true(i) => Some(onto::decide(cx, &b, &sp, onto::Want::Surjective)?),
                        _ => Some(None),
                    }
                } else {
                    Some(r)
                }
            }
            Prop::Injective => {
                let simple = !sp.vector && sp.vars.len() == 1 && sp.dom == spec::Dom::Reals && sp.ycond.is_none() && spec::params(&b, &sp.comps, &sp.vars).is_empty();
                if simple {
                    None
                } else {
                    onto::decide(cx, &b, &sp, onto::Want::Injective)?.map(Some)
                }
            }
            _ => None,
        };
        match new {
            Some(Some(r)) => return Ok(Reply::Done(r)),
            Some(None) if !matches!(self.p, Prop::Singularities | Prop::Analytic) => return Ok(Reply::Decline),
            _ => {}
        }
        if sp.vector || sp.ycond.is_some() || !sp.yvars.is_empty() {
            return Ok(Reply::Decline);
        }
        // parameters, several variables, strict inequalities: the decomposition first
        if matches!(self.p, Prop::Sign | Prop::Monotonicity | Prop::Convexity)
            && (sp.vars.len() > 1 || sp.strict && sp.vars.len() == 1 || !spec::params(&b, &sp.comps, &sp.vars).is_empty())
        {
            let order = match self.p {
                Prop::Sign => 0,
                Prop::Monotonicity => 1,
                _ => 2,
            };
            if let Some(r) = shape::shape(cx, &b, &sp, order)? {
                return Ok(Reply::Done(r));
            }
        }
        let mut c = Call {
            f: sp.f,
            cond: sp.cond,
            vars: sp.vars.clone(),
            complex: sp.dom == spec::Dom::Complexes,
            integers: sp.dom == spec::Dom::Integers,
            facts: sp.facts.clone(),
            strict: sp.strict,
        };
        // inside assuming (or with assumptions: facts): the facts about the variables restrict the domain
        if c.cond.is_none() {
            let facts: Vec<T<'h>> = cx.assumptions().iter().copied().filter(|&f| c.vars.iter().any(|&x| !is_free(&Build::new(h, &self.stop), f, x))).collect();
            match facts.as_slice() {
                [] => {}
                [f] => c.cond = Some(*f),
                _ => c.cond = Some(h.call(h.sym("and"), &facts)),
            }
        }
        let r = match self.p {
            Prop::Domain => match domain(cx, &b, &c)? {
                // a real domain of one variable keeps `x < c or x > c` (as reduce gives it)
                Some(r) if !c.complex && c.vars.len() == 1 => Some(sets::tidy_cond_split(cx, &b, r)?),
                Some(r) => Some(sets::tidy_cond(cx, &b, r)?),
                None => None,
            },
            Prop::Singularities => singularities(cx, &b, &c)?,
            Prop::Analytic => analytic(cx, &b, &c)?,
            // a point the domain leaves out where f jumps (heaviside_theta on x != 0): the pieces are not one convex
            // or concave function, and the second derivative on them does not say so
            // a kink there (abs on x != 0): convex when f' jumps up and the pieces are convex or linear
            Prop::Convexity => {
                let ind = h.sym_term(h.sym("indeterminate"));
                match excluded_points_shape(cx, &b, &c)? {
                    2 => Some(ind),
                    0 => sign_of(cx, &b, &c, 2)?,
                    k => sign_of(cx, &b, &c, 2)?.map(|r| match h.view(r) {
                        View::Int(s) if s == 0 || s == i64::from(k) => h.int(i64::from(k)),
                        _ => ind,
                    }),
                }
            }
            Prop::Sign => sign_of(cx, &b, &c, 0)?,
            Prop::Monotonicity => sign_of(cx, &b, &c, 1)?,
            Prop::Injective => injective(cx, &b, &c)?,
            Prop::Discontinuities | Prop::Continuous | Prop::Meromorphic | Prop::Period | Prop::Poles | Prop::Surjective | Prop::Bijective | Prop::PiecewiseExpand | Prop::Range => None,
        };
        Ok(r.map_or(Reply::Decline, Reply::Done))
    }
}

/// The shape of f at the points `x != c` (alone or in a conjunction) a condition on one variable leaves out: 2 when
/// the one-sided limits of f are finite and differ at one (a jump) or the kinks disagree, 1 (-1) when f is
/// continuous there and the one-sided limits of f' jump up (down), 0 otherwise.
fn excluded_points_shape<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>) -> Res<i8> {
    let h = b.heap();
    let (Some(cond), [x]) = (c.cond, c.vars.as_slice()) else { return Ok(0) };
    let mut points = vec![];
    let mut stack = vec![cond];
    while let Some(u) = stack.pop() {
        let View::App { head, args } = h.view(u) else { continue };
        let View::Sym(s) = h.view(head) else { continue };
        match (h.sym_name(s).as_str(), args) {
            ("and", _) => stack.extend_from_slice(args),
            ("unequal", &[l, r]) if l == *x && is_free(b, r, *x) => points.push(r),
            ("unequal", &[l, r]) if r == *x && is_free(b, l, *x) => points.push(l),
            _ => {}
        }
    }
    // the one-sided limits (above, below) of g at p as machine numbers, `None` when one is not a finite number
    let sides = |cx: &mut NativeCx<'_, '_, 'h>, g: T<'h>, p: T<'h>| -> Res<Option<(f64, f64)>> {
        let mut v = vec![];
        for dir in ["from_above", "from_below"] {
            let rule = h.call(h.sym(names::RULE), &[*x, p]);
            let d = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("direction")), h.sym_term(h.sym(dir))]);
            let l = cx.eval(b.call("limit", &[g, rule, h.call(h.sym("options"), &[d])]))?;
            let n = cx.eval(b.call("n", &[l]))?;
            match h.view(n) {
                View::Int(i) => v.push(i as f64),
                View::Real(r) if r.is_finite() => v.push(r),
                _ => return Ok(None),
            }
        }
        Ok(Some((v[0], v[1])))
    };
    let mut shape = 0i8;
    for p in points {
        let Some((above, below)) = sides(cx, c.f, p)? else { continue };
        if (above - below).abs() > 1e-12 * (1.0 + above.abs()) {
            return Ok(2);
        }
        let df = cx.eval(b.call("d", &[c.f, *x]))?;
        let Some((da, db)) = sides(cx, df, p)? else { continue };
        let k = if da > db + 1e-12 * (1.0 + da.abs()) {
            1
        } else if da < db - 1e-12 * (1.0 + da.abs()) {
            -1
        } else {
            0
        };
        if k != 0 {
            if shape != 0 && shape != k {
                return Ok(2);
            }
            shape = k;
        }
    }
    Ok(shape)
}

/// The definedness conditions of `t` (see the module comment), or `None` for a function not known here.
fn conditions<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, vars: &[T<'h>], complex: bool) -> Res<Option<Vec<T<'h>>>> {
    conditions_of(cx, b, t, vars, complex, false)
}

/// [`conditions`]; with `analytic`, the conditions of analyticity (a fractional power's base positive, not only
/// nonnegative).
fn conditions_of<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    t: T<'h>,
    vars: &[T<'h>],
    complex: bool,
    analytic: bool,
) -> Res<Option<Vec<T<'h>>>> {
    let h = b.heap();
    let mut out: Vec<T<'h>> = vec![];
    let depends = |u: T<'h>| vars.iter().any(|&x| !is_free(b, u, x));
    let rel = |name: &str, l: T<'h>, r: T<'h>| h.call(h.sym(name), &[l, r]);
    // the subterms in postorder (inner conditions first)
    let mut order: Vec<T<'h>> = vec![];
    let mut seen = rustc_hash::FxHashSet::default();
    let mut stack: Vec<(T<'h>, bool)> = vec![(t, false)];
    while let Some((u, done)) = stack.pop() {
        if done {
            order.push(u);
            continue;
        }
        if !seen.insert(u) || !depends(u) {
            continue;
        }
        stack.push((u, true));
        if let View::App { args, .. } = h.view(u) {
            for &a in args.iter().rev() {
                stack.push((a, false));
            }
        }
    }
    for u in order {
        cx.tick(1)?;
        let View::App { head, args } = h.view(u) else { continue };
        let Some(s) = b.sym_of(head) else { return Ok(None) };
        let name = h.sym_name(s);
        let zero = b.zero();
        if s == b.hd.pow {
            let [base, ex] = args else { return Ok(None) };
            if !depends(*base) {
                // c^u: defined for a positive base (and any other constant base by the principal branch)
                continue;
            }
            match h.view(*ex) {
                View::Int(k) if k < 0 => out.push(rel("unequal", *base, zero)),
                View::Int(_) => {}
                View::Rational { .. } => {
                    let neg = b.ar.compare(*ex, zero).is_some_and(|c| c == mc_arith::Cmp3::Less);
                    let even_den = {
                        let d = cx.eval(b.call("denominator", &[*ex]))?;
                        matches!(h.view(d), View::Int(k) if k % 2 == 0)
                    };
                    if complex {
                        if neg {
                            out.push(rel("unequal", *base, zero));
                        }
                        if analytic {
                            // the branch cut of the principal root along the nonpositive reals
                            out.push(branch_cut(b, *base));
                        }
                    } else if even_den {
                        out.push(rel(if neg || analytic { "greater" } else { "greater_equal" }, *base, zero));
                    } else if neg {
                        out.push(rel("unequal", *base, zero));
                    }
                }
                _ => {
                    if depends(*ex) || !complex {
                        out.push(rel("greater", *base, zero));
                    } else {
                        out.push(rel("unequal", *base, zero));
                    }
                }
            }
            continue;
        }
        if s == b.hd.add || s == b.hd.mul || s == b.hd.list {
            continue;
        }
        if name == "elliptic_theta" && args.len() == 3 && depends(args[2]) && !depends(args[0]) && !depends(args[1]) {
            // the nome inside the unit disc
            let q = args[2];
            if complex {
                out.push(rel("less", b.call("abs", &[q]), b.one()));
            } else {
                out.push(rel("greater", q, b.int(-1)));
                out.push(rel("less", q, b.one()));
            }
            continue;
        }
        if name == "atan" && args.len() == 2 {
            out.push(rel("unequal", b.add(&[b.powi(args[0], 2), b.powi(args[1], 2)]), b.zero()));
            continue;
        }
        let [a] = args else {
            if CONTINUOUS.contains(&name.as_str()) {
                continue;
            }
            if let Some((last, params)) = args.split_last() {
                let fixed = params.iter().all(|&p| !depends(p));
                // a non-negative order for j_n (a negative one is a multiple of y: DLMF 10.47.10); 0F1 not at a pole of b
                let integer_order = matches!(params, [n] if matches!(h.view(*n), View::Int(k) if k >= 0 || name != "spherical_bessel_j"));
                let pole = name == "hypergeometric_0f1" && matches!(params, [n] if matches!(h.view(*n), View::Int(k) if k <= 0));
                // the Chebyshev functions of a non-integer degree have a branch point at -1 (the table)
                let polynomial = !matches!(name.as_str(), "chebyshev_t" | "chebyshev_u") || matches!(params, [n] if matches!(h.view(*n), View::Int(_)));
                if fixed && !pole && depends(*last) && (ENTIRE_LAST.contains(&name.as_str()) && polynomial || ENTIRE_INTEGER_ORDER.contains(&name.as_str()) && integer_order) {
                    continue;
                }
            }
            match table_conditions(cx, b, u, vars, complex, analytic)? {
                Some(cs) => {
                    out.extend(cs);
                    continue;
                }
                None => return Ok(None),
            }
        };
        let one = b.one();
        let re_im = |u: T<'h>| (b.call("re", &[u]), b.call("im_part", &[u]));
        // `not (part == 0 and abs(other) >= 1)`: off a cut along the real (or imaginary) axis beyond 1
        let cut_beyond_one = |on_real: bool, u: T<'h>| {
            let (re, im) = re_im(u);
            let (zero_part, other) = if on_real { (im, re) } else { (re, im) };
            let on = b.call("and", &[rel("equal", zero_part, zero), rel("greater_equal", b.call("abs", &[other]), one)]);
            b.call("not", &[on])
        };
        match name.as_str() {
            "tanh" | "sech" | "gudermannian" if complex => out.push(rel("unequal", b.call("cosh", &[*a]), zero)),
            "coth" | "csch" if complex => out.push(rel("unequal", b.call("sinh", &[*a]), zero)),
            // inverse functions over the complexes: their branch points (and, for analyticity, cuts)
            "atan" if complex => {
                out.push(rel("unequal", *a, h.sym_term(h.sym("im"))));
                out.push(rel("unequal", *a, b.neg(h.sym_term(h.sym("im")))));
                if analytic {
                    out.push(cut_beyond_one(false, *a));
                }
            }
            "asinh" if complex && analytic => out.push(cut_beyond_one(false, *a)),
            "asin" | "acos" if complex && analytic => out.push(cut_beyond_one(true, *a)),
            "acosh" if complex && analytic => {
                // the cut (-inf, 1] of the real line
                let (re, im) = re_im(*a);
                out.push(b.call("not", &[b.call("and", &[rel("equal", im, zero), rel("less_equal", re, one)])]));
            }
            "atanh" if complex && analytic => out.push(cut_beyond_one(true, *a)),

            n if ENTIRE.contains(&n) || CONTINUOUS.contains(&n) => {}
            "log" if complex && analytic => out.push(branch_cut(b, *a)),
            "log" => out.push(rel(if complex { "unequal" } else { "greater" }, *a, zero)),
            "asin" | "acos" if !complex => {
                out.push(rel("greater_equal", *a, b.neg(one)));
                out.push(rel("less_equal", *a, one));
            }
            "asin" | "acos" | "asinh" => {}
            "acosh" if !complex => out.push(rel("greater_equal", *a, one)),
            "atanh" if !complex => {
                out.push(rel("greater", *a, b.neg(one)));
                out.push(rel("less", *a, one));
            }
            "atanh" => {
                out.push(rel("unequal", *a, one));
                out.push(rel("unequal", *a, b.neg(one)));
            }
            "tan" | "sec" => out.push(rel("unequal", b.call("cos", &[*a]), zero)),
            "cot" | "csc" => out.push(rel("unequal", b.call("sin", &[*a]), zero)),
            "coth" | "csch" => out.push(rel("unequal", *a, zero)),
            "logistic_sigmoid" if !complex => {}
            "gamma" | "log_gamma" | "factorial" => {
                // defined except at the nonpositive integers (of u + 1 for the factorial)
                let u = if name == "factorial" { b.add(&[*a, one]) } else { *a };
                let side = if complex { b.call("re", &[u]) } else { u };
                let ints = h.sym_term(h.sym("integers"));
                out.push(b.call("or", &[rel("greater", side, zero), b.call("not_element", &[u, ints])]));
            }
            "zeta" => out.push(rel("unequal", b.sub(*a, one), zero)),
            _ => match table_conditions(cx, b, u, vars, complex, analytic)? {
                Some(cs) => out.extend(cs),
                None => return Ok(None),
            },
        }
    }
    // evaluate, drop the ones that hold everywhere, and order them
    let mut done = vec![];
    for c in out {
        let v = cx.eval(c)?;
        if cx.is_true(v) {
            continue;
        }
        if cx.is_false(v) {
            return Ok(Some(vec![v]));
        }
        if !done.contains(&v) {
            done.push(v);
        }
    }
    Ok(Some(done))
}

/// The conditions of a call of a function the table knows ([`table`]): where it is defined (and real, over the
/// reals), or with `analytic` the complement of its singular atoms; `None` when the table does not know them.
fn table_conditions<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, vars: &[T<'h>], complex: bool, analytic: bool) -> Res<Option<Vec<T<'h>>>> {
    let facts = cx.assumptions().to_vec();
    let Some(hit) = table::lookup(cx, b, u, vars, &facts)? else { return Ok(None) };
    // over the reals the facts are about a real argument: an argument that is never real (`x + im`) is not read
    if !complex {
        let h = b.heap();
        let real_vars: Vec<T<'h>> = vars.iter().map(|&x| h.call(h.sym("element"), &[x, h.sym_term(h.sym("reals"))])).collect();
        let r = cx.eval_assuming(&real_vars, h.call(h.sym("refine"), &[h.call(h.sym("im_part"), &[hit.arg])]))?;
        if b.ar.num(r).is_some() && r != b.zero() {
            return Ok(None);
        }
    }
    if analytic {
        let Some((atoms, partial)) = hit.atoms(cx, b, complex, sets::Mode::Sing)? else { return Ok(None) };
        if partial {
            return Ok(None);
        }
        let mut out = vec![];
        for (a, _) in atoms {
            out.push(cx.eval(b.call("not", &[a]))?);
        }
        return Ok(Some(out));
    }
    Ok(hit.domain(cx, b, complex)?.map(|d| vec![d]))
}

/// `not (im_part(u) == 0 and re(u) <= 0)`: off the branch cut of the principal root and logarithm.
fn branch_cut<'h>(b: &Build<'h>, u: T<'h>) -> T<'h> {
    let on = b.call("and", &[b.call("equal", &[b.call("im_part", &[u]), b.zero()]), b.call("less_equal", &[b.call("re", &[u]), b.zero()])]);
    b.call("not", &[on])
}

/// `a op b` for an inequality with a constant term: the constant moved to the right (`x^2 + y^2 >= 9`).
fn tidy<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let View::App { head, args: &[l, r] } = h.view(c) else { return Ok(c) };
    let Some(s) = b.sym_of(head) else { return Ok(c) };
    if !matches!(h.sym_name(s).as_str(), "greater" | "greater_equal" | "less" | "less_equal") || r != b.zero() {
        return Ok(c);
    }
    let View::App { head: ah, args: terms } = h.view(l) else { return Ok(c) };
    if b.sym_of(ah) != Some(b.hd.add) {
        return Ok(c);
    }
    let (num, rest): (Vec<T<'h>>, Vec<T<'h>>) = terms.iter().partition(|&&t| b.ar.num(t).is_some());
    if num.is_empty() || rest.is_empty() {
        return Ok(c);
    }
    let k = b.neg(b.add(&num));
    cx.eval(h.call(s, &[b.add(&rest), k]))
}

/// The conjunction of conditions (`true` for none).
fn conj<'h>(cx: &mut NativeCx<'_, '_, 'h>, cs: &[T<'h>]) -> Res<T<'h>> {
    let h = cx.heap();
    Ok(match cs {
        [] => cx.bool(true),
        [c] => *c,
        _ => cx.eval(h.call(h.sym("and"), cs))?,
    })
}

/// Is `t` a call of `name` (left unevaluated)?
fn is_call<'h>(b: &Build<'h>, t: T<'h>, name: &str) -> bool {
    let h = b.heap();
    matches!(h.view(t), View::App { head, .. } if b.sym_of(head) == Some(h.sym(name)))
}

fn domain<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let Some(mut cs) = conditions(cx, b, c.f, &c.vars, c.complex)? else { return Ok(None) };
    if let Some(k) = c.cond {
        cs.push(k);
    }
    if let Some(r) = domain_form::finish(cx, b, &mut cs, &c.vars, c.complex)? {
        return Ok(Some(r));
    }
    if !c.complex && c.vars.len() == 1 && !cs.is_empty() {
        let all = conj(cx, &cs)?;
        let r = cx.eval(h.call(h.sym("reduce"), &[all, c.vars[0], h.sym_term(h.sym("reals"))]))?;
        if !is_call(b, r, "reduce") && !is_call(b, r, names::FAILURE) {
            return Ok(Some(r));
        }
    }
    let mut tidied = Vec::with_capacity(cs.len());
    for k in cs {
        tidied.push(tidy(cx, b, k)?);
    }
    Ok(Some(conj(cx, &tidied)?))
}

/// The negation of a definedness condition.
fn negate<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, k: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    if let View::App { head, args: &[l, r] } = h.view(k) {
        if let Some(s) = b.sym_of(head) {
            let flip = match h.sym_name(s).as_str() {
                "unequal" => Some("equal"),
                "greater" => Some("less_equal"),
                "greater_equal" => Some("less"),
                "less" => Some("greater_equal"),
                "less_equal" => Some("greater"),
                _ => None,
            };
            if let Some(n) = flip {
                return cx.eval(h.call(h.sym(n), &[l, r]));
            }
        }
    }
    cx.eval(b.call("not", &[k]))
}

fn singularities<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let Some(cs) = conditions_of(cx, b, c.f, &c.vars, c.complex, true)? else { return Ok(None) };
    let mut out = vec![];
    for k in cs {
        out.push(negate(cx, b, k)?);
    }
    Ok(Some(match out.as_slice() {
        [] => cx.bool(false),
        [k] => *k,
        _ => cx.eval(h.call(h.sym("or"), &out))?,
    }))
}

fn analytic<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    // a non-analytic continuous function of a variable itself (`floor(x)`, `abs(x)`): not analytic
    if let View::App { head, args: &[a, ..] } = h.view(c.f) {
        if b.sym_of(head).is_some_and(|s| CONTINUOUS.contains(&h.sym_name(s).as_str())) && c.vars.contains(&a) && c.cond.is_none() {
            return Ok(Some(cx.bool(false)));
        }
    }
    // non-analytic continuous functions of the variables: not decided here
    let mut stack = vec![c.f];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if b.sym_of(head).is_some_and(|s| CONTINUOUS.contains(&h.sym_name(s).as_str())) && c.vars.iter().any(|&x| !is_free(b, u, x)) {
                return Ok(None);
            }
            stack.extend(args.iter().copied());
        }
    }
    let Some(cs) = conditions_of(cx, b, c.f, &c.vars, c.complex, true)? else { return Ok(None) };
    if cs.is_empty() {
        return Ok(Some(cx.bool(true)));
    }
    let Some(k) = c.cond else { return Ok(Some(cx.bool(false))) };
    // every condition implied by the given one
    for d in cs {
        let r = cx.eval(h.call(h.sym("refine"), &[d, k]))?;
        if !cx.is_true(r) {
            return Ok(Some(cx.bool(false)));
        }
    }
    Ok(Some(cx.bool(true)))
}

/// The machine value of `f` at `x = v` (one variable), if finite and real.
fn value_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, v: f64) -> Res<Option<f64>> {
    let h = b.heap();
    if out_of_time() {
        return Ok(None);
    }
    let rule = h.call(h.sym(names::RULE), &[x, h.real(v)]);
    // no sample where a gamma-type function has a huge argument (`gamma(125000.)` takes minutes; F27.8)
    let mut growth = vec![];
    growth_args(b, f, x, &mut growth);
    for u in growth {
        let r = cx.eval(h.call(h.sym("rewrite"), &[u, rule]))?;
        let r = cx.eval(h.call(h.sym("n"), &[r]))?;
        match h.view(r) {
            View::Real(y) if y.abs() > 2000.0 => return Ok(None),
            View::Int(k) if k.abs() > 2000 => return Ok(None),
            _ => {}
        }
    }
    let r = cx.eval(h.call(h.sym("rewrite"), &[f, rule]))?;
    let r = cx.eval(h.call(h.sym("n"), &[r]))?;
    Ok(machine_real(h, r))
}

/// A finite machine value: an integer, a real, or a complex number whose imaginary part is exactly zero (a real
/// value some numeric methods return in complex form).
fn machine_real<'h>(h: &'h mc_core::Heap, t: T<'h>) -> Option<f64> {
    match h.view(t) {
        View::Real(y) if y.is_finite() => Some(y),
        View::Int(k) => Some(k as f64),
        View::App { head, args: &[re, im] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::COMPLEX) => {
            let zero = matches!(h.view(im), View::Real(v) if v == 0.0) || matches!(h.view(im), View::Int(0));
            if zero {
                machine_real(h, re)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The value of `f` at (a rational next to) `v` to 40 digits.
fn precise_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, v: f64) -> Res<T<'h>> {
    let h = b.heap();
    let q = cx.eval(b.call("rationalize", &[h.real(v), b.zero()]))?;
    let r = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, q])]))?;
    cx.eval(h.call(h.sym("n"), &[r, b.int(40)]))
}

/// Sample points of the real line (or of the integers): 0, ±10^k at quarter decades from 10^-6 to 10^6, and the
/// integers -50..50.
fn samples(integers: bool) -> Vec<f64> {
    let mut v: Vec<f64> = (-50..=50).map(f64::from).collect();
    if !integers {
        for k in -24..=24 {
            let p = 10f64.powf(k as f64 / 4.0);
            v.push(p);
            v.push(-p);
        }
        for k in 0..400 {
            v.push(-10.0 + 0.05 * k as f64 + 0.0123);
        }
    }
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

/// The sign of `f`, `f'` or `f''` on the domain (module comment): `1`, `-1`, `0`, `indeterminate`, or `None`.
/// A convexity claim is checked against the chords of the function (cusps, where the second derivative has one sign
/// on each side but the function is not convex or concave across the point: B26-3).
fn sign_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, order: usize) -> Res<Option<T<'h>>> {
    let r = sign_of_inner(cx, b, c, order)?;
    if order == 2 && !c.integers && !c.complex && c.vars.len() == 1 {
        if let Some(t) = r {
            if let View::Int(k @ (1 | -1)) = b.heap().view(t) {
                match chords_agree(cx, b, c, c.vars[0], k)? {
                    Some(true) => {}
                    Some(false) => return Ok(Some(b.heap().sym_term(b.heap().sym("indeterminate")))),
                    None => return Ok(None),
                }
            }
        }
    }
    Ok(r)
}

/// `f` with the powers `u^(p/q)` of an odd denominator `q` read as the real roots `(sign(u) |u|^(1/q))^p`.
fn real_roots<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let View::App { head, args } = h.view(f) else { return Ok(f) };
    let mut new = Vec::with_capacity(args.len());
    for &a in args {
        new.push(real_roots(cx, b, a)?);
    }
    if b.sym_of(head) == Some(b.hd.pow) {
        if let [base, ex] = new.as_slice() {
            if let View::Rational { num, den } = h.view(*ex) {
                if let (View::Int(p), View::Int(q)) = (h.view(num), h.view(den)) {
                    if q % 2 != 0 && q > 1 {
                        let root = b.mul(&[b.call("sign", &[*base]), b.pow(b.call("abs", &[*base]), b.rat(1, q))]);
                        return cx.eval(b.pow(root, b.int(p)));
                    }
                }
            }
        }
    }
    cx.eval(h.app(head, &new))
}

/// Do the chords of `f` agree with the claim `k` (1 convex, -1 concave) at the triples `x0 - t, x0, x0 + t`?
fn chords_agree<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, k: i64) -> Res<Option<bool>> {
    let h = b.heap();
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, c.f, &mut params);
    if params.iter().any(|&s| s != x) {
        return Ok(Some(true));
    }
    let fr = real_roots(cx, b, c.f)?;
    for x0 in (-10..=10).map(|i| f64::from(i) * 0.5) {
        for t in [0.1, 0.5, 1.0, 2.5] {
            cx.tick(1)?;
            if out_of_time() {
                return Ok(None);
            }
            let pts = [x0 - t, x0, x0 + t];
            let mut ys = [0.0; 3];
            let mut ok = true;
            for (i, &v) in pts.iter().enumerate() {
                if let Some(cd) = c.cond {
                    let rule = h.call(h.sym(names::RULE), &[x, h.real(v)]);
                    let r = cx.eval(h.call(h.sym("rewrite"), &[cd, rule]))?;
                    if !cx.is_true(r) {
                        ok = false;
                        break;
                    }
                }
                match value_at(cx, b, fr, x, v)? {
                    Some(y) => ys[i] = y,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            let avg = 0.5 * (ys[0] + ys[2]);
            let tol = 1e-9 * (1.0 + ys[0].abs() + ys[1].abs() + ys[2].abs());
            if (k == 1 && ys[1] > avg + tol) || (k == -1 && ys[1] < avg - tol) {
                return Ok(Some(false));
            }
        }
    }
    Ok(Some(true))
}

/// Does the special function `f` (a call of the table at an affine function of `x`, no condition on `x`) have a
/// pole on the real line? It is then not defined on the whole line, and its sign, monotonicity and convexity are
/// indeterminate, as for any function whose domain leaves out points (the Weierstrass functions: their real
/// domain is not listed, but the pole at 0 is one of its gaps; DLMF 23.2.4-23.2.5).
fn pole_on_line<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>) -> Res<bool> {
    let h = b.heap();
    if c.integers || c.complex || c.cond.is_some() {
        return Ok(false);
    }
    let mut pf = c.facts.clone();
    pf.extend(cx.assumptions().iter().copied());
    let Some(hit) = table::lookup(cx, b, c.f, &[x], &pf)? else { return Ok(false) };
    if !range::affine_in(cx, b, hit.arg, x)? {
        return Ok(false);
    }
    if let View::App { args, .. } = h.view(c.f) {
        if args.iter().filter(|&&a| a != hit.arg).any(|&a| !is_free(b, a, x)) {
            return Ok(false);
        }
    }
    let Some((atoms, _)) = hit.atoms(cx, b, false, sets::Mode::Sing)? else { return Ok(false) };
    for (t, k) in atoms {
        if k != sets::Kind::Pole {
            continue;
        }
        let mut others = rustc_hash::FxHashSet::default();
        symbols(b, t, &mut others);
        if others.iter().any(|&s| s != x) {
            continue;
        }
        let r = cx.eval(h.call(h.sym("reduce"), &[t, x, h.sym_term(h.sym("reals"))]))?;
        if !cx.is_false(r) && !is_call(b, r, "reduce") && !is_call(b, r, names::FAILURE) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn sign_of_inner<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, order: usize) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let [x] = c.vars.as_slice() else { return sign_multi(cx, b, c, order) };
    let x = *x;
    // sqrt(u^2) is abs(u) on the reals: its kink is then seen
    let fa = abs_of_root_squares(cx, b, c.f)?;
    if fa != c.f {
        let c2 = Call { f: fa, cond: c.cond, vars: c.vars.clone(), complex: c.complex, integers: c.integers, facts: c.facts.clone(), strict: c.strict };
        return sign_of(cx, b, &c2, order);
    }
    // alpha + beta g(x) with parameters only in alpha and beta (beta != 0 under the facts): the shape of g, times the
    // sign of beta when it is known
    if order > 0 && !c.integers && !c.complex && c.cond.is_none() {
        if let Some((s, g)) = affine_part(cx, b, c, x)? {
            let cg = Call { f: g, cond: None, vars: c.vars.clone(), complex: false, integers: false, facts: c.facts.clone(), strict: c.strict };
            let indet = h.sym_term(h.sym("indeterminate"));
            return Ok(match sign_of(cx, b, &cg, order)?.map(|r| h.view(r)) {
                Some(View::Int(0)) => Some(b.zero()),
                Some(View::Int(k)) if s != 0 => Some(b.int(k * s)),
                Some(View::Sym(y)) if h.sym_term(y) == indet => Some(indet),
                _ => None,
            });
        }
    }
    let shaped = order > 0 && !c.integers && nonsmooth(b, c.f, x);
    if pole_on_line(cx, b, c, x)? {
        return Ok(Some(h.sym_term(h.sym("indeterminate"))));
    }
    let Some(cs) = conditions(cx, b, c.f, &c.vars, false)? else {
        // a function with jumps or kinks whose domain is not known: its samples alone
        return if shaped { sampled_shape(cx, b, c, x, order) } else { Ok(None) };
    };
    let indet = h.sym_term(h.sym("indeterminate"));
    // the facts about the parameters (a domain that is the whole line under them: `m >= 1` for `jacobi_nc`)
    let mut pf = c.facts.clone();
    pf.extend(cx.assumptions().iter().copied());
    let whole_line = !cs.is_empty() && !pf.is_empty() && {
        let all = conj(cx, &cs)?;
        let mut f2 = pf.clone();
        f2.push(h.call(h.sym("element"), &[x, h.sym_term(h.sym("reals"))]));
        let r = cx.eval_assuming(&f2, h.call(h.sym("refine"), &[all]))?;
        cx.is_true(r)
    };
    // a function not defined on the whole real line (and no condition): the property fails somewhere
    if c.cond.is_none() && !c.integers && !cs.is_empty() && !whole_line {
        // the gamma family (`u > 0 or u not an integer`) fails at the negative integers: no `reduce` of a
        // trigonometric condition (F27.8: `gamma(x^3)` took minutes)
        if cs.iter().any(|&k| contains_call(b, k, "not_element")) {
            return Ok(Some(indet));
        }
        let all = conj(cx, &cs)?;
        let r = cx.eval(h.call(h.sym("reduce"), &[all, x, h.sym_term(h.sym("reals"))]))?;
        if !cx.is_true(r) {
            return Ok(Some(indet));
        }
    }
    // a condition whose region leaves the domain: the property fails there, as without a condition
    if let Some(k) = c.cond {
        if !c.integers && !cs.is_empty() && !cs.iter().any(|&d| contains_call(b, d, "not_element")) {
            let all = conj(cx, &cs)?;
            let outside = cx.eval_assuming(&pf, h.call(h.sym("refine"), &[h.call(h.sym("and"), &[k, b.call("not", &[all])])]))?;
            let mut others = rustc_hash::FxHashSet::default();
            symbols(b, outside, &mut others);
            if !cx.is_false(outside) && others.iter().all(|&s| s == x) {
                let r = cx.eval(h.call(h.sym("reduce"), &[outside, x, h.sym_term(h.sym("reals"))]))?;
                if !cx.is_false(r) && !is_call(b, r, "reduce") && !is_call(b, r, names::FAILURE) {
                    return Ok(Some(indet));
                }
            }
        }
    }
    // a special function of one sign, monotonic or convex on its domain for the parameters' facts (the table)
    if !c.integers && !c.complex {
        if let Some(hit) = table::lookup(cx, b, c.f, &[x], &pf)? {
            // a nonconstant periodic function is neither monotonic nor convex on the line
            if order > 0 && c.cond.is_none() && range::affine_in(cx, b, hit.arg, x)? && hit.periodic(cx, b)? {
                return Ok(Some(indet));
            }
            let ok = if order == 1 { hit.arg == x } else { range::affine_in(cx, b, hit.arg, x)? };
            if ok {
                if let Some(k) = hit.shape(cx, b, order)? {
                    return Ok(Some(b.int(i64::from(k))));
                }
            }
        }
    }
    // functions with jumps or kinks: the samples of the function itself
    // (with parameters there are no samples, and the derivatives miss the jumps and kinks: `c unit_step(x)` came out
    // linear; undecided)
    if shaped {
        return sampled_shape(cx, b, c, x, order);
    }
    let mut g = c.f;
    if c.integers && order > 0 {
        // a sequence: differences instead of derivatives
        for _ in 0..order {
            let next = cx.eval(h.call(h.sym("rewrite"), &[g, h.call(h.sym(names::RULE), &[x, b.add(&[x, b.one()])])]))?;
            g = cx.eval(b.sub(next, g))?;
        }
    } else {
        for _ in 0..order {
            g = cx.eval(b.call("d", &[g, x]))?;
        }
    }
    if is_call(b, g, "d") {
        return Ok(None);
    }
    // strict: a zero of f is a counterexample
    if c.strict && order == 0 {
        let mut e = h.call(h.sym("equal"), &[g, b.zero()]);
        if let Some(k) = c.cond {
            e = h.call(h.sym("and"), &[e, k]);
        }
        let r = cx.eval(h.call(h.sym("solve"), &[e, x, h.sym_term(h.sym("reals"))]))?;
        // a real solution (the first member of a family)
        let c1 = h.call(h.sym("c"), &[b.one()]);
        for &row in cx.as_list(r).unwrap_or(&[]) {
            let Some(&[rule]) = cx.as_list(row) else { continue };
            let View::App { args: &[_, v], .. } = h.view(rule) else { continue };
            let v = match h.view(v) {
                View::App { head, args } if b.sym_of(head) == Some(h.sym(mc_decide::CONDITIONAL)) => args[0],
                _ => v,
            };
            let v = cx.eval(h.call(h.sym("rewrite"), &[v, h.call(h.sym(names::RULE), &[c1, b.zero()])]))?;
            let vn = cx.eval(h.call(h.sym("n"), &[v]))?;
            if matches!(h.view(vn), View::Real(_) | View::Int(_)) {
                return Ok(Some(indet));
            }
        }
    }
    let mut facts = cs.clone();
    facts.push(h.call(h.sym("element"), &[x, h.sym_term(h.sym(if c.integers { "integers" } else { "reals" }))]));
    if let Some(k) = c.cond {
        facts.push(k);
    }
    facts.extend(c.facts.iter().copied());
    // a proof by refine under the domain's facts
    let fact = conj(cx, &facts)?;
    let zero = b.zero();
    let (rge, rle) = if c.strict && order == 0 { ("greater", "less") } else { ("greater_equal", "less_equal") };
    let ge = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[h.call(h.sym(rge), &[g, zero])]))?;
    let le = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[h.call(h.sym(rle), &[g, zero])]))?;
    match (cx.is_true(ge), cx.is_true(le)) {
        (true, true) => return Ok(Some(zero)),
        (true, false) => return Ok(Some(b.one())),
        (false, true) => return Ok(Some(b.int(-1))),
        _ => {}
    }
    // sampling: only without parameters
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, g, &mut params);
    if params.iter().any(|&s| s != x) {
        return Ok(None);
    }
    let (mut pos, mut neg, mut any) = (false, false, false);
    // the samples near 0 first: a change of sign is found before the far (and slower) ones are evaluated
    let mut xs = samples(c.integers);
    xs.sort_by(|p, q| p.abs().total_cmp(&q.abs()));
    for v in xs {
        cx.tick(1)?;
        if out_of_time() {
            return Ok(None);
        }
        // inside the domain and the condition
        if let Some(k) = c.cond {
            let rule = h.call(h.sym(names::RULE), &[x, h.real(v)]);
            let ok = cx.eval(h.call(h.sym("rewrite"), &[k, rule]))?;
            if !cx.is_true(ok) {
                continue;
            }
        }
        if value_at(cx, b, c.f, x, v)?.is_none() {
            continue;
        }
        let Some(y) = value_at(cx, b, g, x, v)? else { continue };
        any = true;
        let tol = 1e-12 * (1.0 + y.abs());
        if y > tol {
            pos = true;
        } else if y < -tol {
            neg = true;
        } else if c.strict && order == 0 {
            return Ok(Some(indet));
        }
        if pos && neg {
            return Ok(Some(indet));
        }
    }
    if !any || out_of_time() {
        return Ok(None);
    }
    Ok(Some(match (pos, neg) {
        (true, false) => b.one(),
        (false, true) => b.int(-1),
        (false, false) => zero,
        _ => indet,
    }))
}

/// `f` with every `(u^2)^(1/2)` replaced by `abs(u)` (the reals).
fn abs_of_root_squares<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let View::App { head, args } = h.view(f) else { return Ok(f) };
    if b.sym_of(head) == Some(b.hd.pow) {
        if let [base, ex] = args {
            if *ex == b.rat(1, 2) {
                if let View::App { head: bh, args: &[u, two] } = h.view(*base) {
                    if b.sym_of(bh) == Some(b.hd.pow) && two == b.int(2) {
                        let u = abs_of_root_squares(cx, b, u)?;
                        return cx.eval(b.call("abs", &[u]));
                    }
                }
            }
        }
    }
    let mut changed = false;
    let mut v = Vec::with_capacity(args.len());
    for &a in args {
        let r = abs_of_root_squares(cx, b, a)?;
        changed |= r != a;
        v.push(r);
    }
    if !changed {
        return Ok(f);
    }
    cx.eval(h.app(head, &v))
}

/// Does `f` contain a function with jumps or kinks of one of the variables?
fn nonsmooth_any<'h>(b: &Build<'h>, f: T<'h>, vars: &[T<'h>]) -> bool {
    vars.iter().any(|&x| nonsmooth(b, f, x))
}

/// Does `f` contain a function with jumps or kinks (one of [`CONTINUOUS`], or not smooth by the table) of `x`?
fn nonsmooth<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if b.sym_of(head).is_some_and(|s| {
                let n = h.sym_name(s);
                CONTINUOUS.contains(&n.as_str()) || table::not_smooth(&n)
            }) && !is_free(b, u, x)
            {
                return true;
            }
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// The monotonicity (`order` 1) or convexity (`order` 2) of a function of one variable without parameters, from
/// its samples (module comment): `1` when the differences (the slopes) never decrease, `-1` when they never
/// increase, `0` for a constant, `indeterminate` when both happen; `None` when there are no parameter-free samples.
/// Samples outside the domain or the condition split the line: no difference is taken across them.
fn sampled_shape<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, order: usize) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, c.f, &mut params);
    if params.iter().any(|&s| s != x) {
        return Ok(None);
    }
    // runs of consecutive samples inside the domain
    let mut runs: Vec<Vec<(f64, f64)>> = vec![vec![]];
    let mut gap = 0usize;
    for v in samples(false) {
        cx.tick(1)?;
        if out_of_time() {
            return Ok(None);
        }
        let inside = match c.cond {
            Some(k) => {
                let rule = h.call(h.sym(names::RULE), &[x, h.real(v)]);
                let ok = cx.eval(h.call(h.sym("rewrite"), &[k, rule]))?;
                cx.is_true(ok)
            }
            None => true,
        };
        let y = if inside { value_at(cx, b, c.f, x, v)? } else { None };
        match y {
            Some(y) => {
                // an isolated undefined sample is a pole (`max(4, 3/(2 x))` jumps up across it): the values on its
                // two sides are compared; a longer gap (outside the domain) splits the run (B26-5)
                if gap >= 2 && runs.last().is_some_and(|r| !r.is_empty()) {
                    runs.push(vec![]);
                }
                gap = 0;
                runs.last_mut().expect("a run").push((v, y));
            }
            None if !inside => {
                gap = 0;
                if runs.last().is_some_and(|r| !r.is_empty()) {
                    runs.push(vec![]);
                }
            }
            None => gap += 1,
        }
    }
    if out_of_time() {
        return Ok(None);
    }
    let (mut up, mut down, mut any) = (false, false, false);
    // monotonicity compares values across the gaps of the domain too (x < y implies f(x) <= f(y) everywhere)
    if order == 1 {
        let ends: Vec<&Vec<(f64, f64)>> = runs.iter().filter(|r| !r.is_empty()).collect();
        for w in ends.windows(2) {
            let (a, c2) = (w[0].last().expect("a run").1, w[1][0].1);
            let tol = 1e-12 * (1.0 + a.abs().max(c2.abs()));
            if c2 > a + tol {
                up = true;
            } else if c2 < a - tol {
                down = true;
            }
        }
    }
    for run in &runs {
        // the differences with their tolerance (order 1), or the slopes' differences (order 2)
        let d: Vec<(f64, f64)> = if order == 1 {
            run.windows(2).map(|w| (w[1].1 - w[0].1, 1e-12 * (1.0 + w[0].1.abs().max(w[1].1.abs())))).collect()
        } else {
            let slopes: Vec<f64> = run.windows(2).map(|w| (w[1].1 - w[0].1) / (w[1].0 - w[0].0)).collect();
            slopes.windows(2).map(|s| (s[1] - s[0], 1e-9 * (1.0 + s[0].abs() + s[1].abs()))).collect()
        };
        for (di, tol) in d {
            any = true;
            if di > tol {
                up = true;
            } else if di < -tol {
                down = true;
            }
            // strict: a flat piece, relative to the size of the values (|x|^3 near 0 is not flat)
            if c.strict && di.abs() <= 1e-12 * tol {
                return Ok(Some(h.sym_term(h.sym("indeterminate"))));
            }
        }
    }
    if !any {
        return Ok(None);
    }
    Ok(Some(match (up, down) {
        (true, false) => b.one(),
        (false, true) => b.int(-1),
        (false, false) => b.zero(),
        _ => h.sym_term(h.sym("indeterminate")),
    }))
}

/// Several variables: what refine proves, or `indeterminate` proved by a grid of samples ([`grid_witness`]).
fn sign_multi<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, order: usize) -> Res<Option<T<'h>>> {
    if order == 2 {
        return grid_witness(cx, b, c, 2);
    }
    if order > 0 {
        return Ok(None);
    }
    let h = b.heap();
    // a domain not known: only a grid of samples can show a change of sign
    let Some(cs) = conditions(cx, b, c.f, &c.vars, false)? else { return grid_witness(cx, b, c, 0) };
    let mut facts = cs;
    let reals = h.sym_term(h.sym("reals"));
    facts.push(h.call(h.sym("element"), &[cx.list(&c.vars), reals]));
    if let Some(k) = c.cond {
        facts.push(k);
    }
    let fact = conj(cx, &facts)?;
    let zero = b.zero();
    let ge = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[h.call(h.sym("greater_equal"), &[c.f, zero])]))?;
    let le = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[h.call(h.sym("less_equal"), &[c.f, zero])]))?;
    Ok(match (cx.is_true(ge), cx.is_true(le)) {
        (true, true) => Some(zero),
        (true, false) => Some(b.one()),
        (false, true) => Some(b.int(-1)),
        _ => grid_witness(cx, b, c, 0)?,
    })
}

/// Several variables without parameters: two points of the domain (and the condition) where `f` has opposite
/// signs (`order` 0), or two segments inside it along which `f` lies above and below its chord (`order` 2), prove
/// `indeterminate`; anything else is not decided (`None`). The points are a grid of five values per variable.
fn grid_witness<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, order: usize) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, c.f, &mut params);
    if params.iter().any(|s| !c.vars.contains(s)) || c.vars.len() > 3 || c.integers || c.complex {
        return Ok(None);
    }
    // a domain not known: the points where `f` has a real value are in it
    let mut cs = conditions(cx, b, c.f, &c.vars, false)?.unwrap_or_default();
    cs.extend(c.cond);
    const GRID: [f64; 5] = [-2.3, -0.7, 0.4, 1.1, 2.6];
    let n = c.vars.len();
    // the value at a point inside the domain and the condition (finite and real), else `None`
    let value = |cx: &mut NativeCx<'_, '_, 'h>, p: &[f64]| -> Res<Option<f64>> {
        cx.tick(1)?;
        if out_of_time() {
            return Ok(None);
        }
        let rules: Vec<T<'h>> = c.vars.iter().zip(p).map(|(&x, &v)| h.call(h.sym(names::RULE), &[x, h.real(v)])).collect();
        let rl = cx.list(&rules);
        for &k in &cs {
            let ok = cx.eval(h.call(h.sym("rewrite"), &[k, rl]))?;
            if !cx.is_true(ok) {
                return Ok(None);
            }
        }
        let v = cx.eval(h.call(h.sym("rewrite"), &[c.f, rl]))?;
        let v = cx.eval(h.call(h.sym("n"), &[v]))?;
        Ok(match h.view(v) {
            View::Real(y) if y.is_finite() => Some(y),
            View::Int(k) => Some(k as f64),
            _ => None,
        })
    };
    let points: Vec<Vec<f64>> = (0..GRID.len().pow(n as u32))
        .map(|mut i| {
            (0..n)
                .map(|_| {
                    let v = GRID[i % GRID.len()];
                    i /= GRID.len();
                    v
                })
                .collect()
        })
        .collect();
    let indet = h.sym_term(h.sym("indeterminate"));
    let (mut up, mut down) = (false, false);
    if order == 0 {
        for p in &points {
            let Some(y) = value(cx, p)? else { continue };
            let tol = 1e-9 * (1.0 + y.abs());
            up |= y > tol;
            down |= y < -tol;
            if up && down {
                return Ok(Some(indet));
            }
        }
        return Ok(None);
    }
    if order != 2 {
        return Ok(None);
    }
    // neighbouring grid points: the segment sampled at eight points inside the domain, its midpoint against the chord
    for (i, p) in points.iter().enumerate() {
        for d in 0..n {
            let step = GRID.len().pow(d as u32);
            if (i / step) % GRID.len() == GRID.len() - 1 {
                continue;
            }
            let q = &points[i + step];
            let at = |t: f64| -> Vec<f64> { p.iter().zip(q).map(|(a, c)| a + t * (c - a)).collect() };
            let mut ys = vec![];
            for k in 0..=8 {
                match value(cx, &at(f64::from(k) / 8.0))? {
                    Some(y) => ys.push(y),
                    None => break,
                }
            }
            if ys.len() < 9 {
                continue;
            }
            let (ya, ym, yb) = (ys[0], ys[4], ys[8]);
            let tol = 1e-7 * (1.0 + ya.abs() + ym.abs() + yb.abs());
            let chord = 0.5 * (ya + yb);
            up |= ym > chord + tol;
            down |= ym < chord - tol;
            if up && down {
                return Ok(Some(indet));
            }
        }
    }
    Ok(None)
}

/// The symbols of `t` (not heads).
fn symbols<'h>(b: &Build<'h>, t: T<'h>, out: &mut rustc_hash::FxHashSet<T<'h>>) {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) => {
                let n = h.sym_name(s);
                if !matches!(n.as_str(), "pi" | "e" | "euler_gamma" | "degree" | "im" | "inf") {
                    out.insert(u);
                }
            }
            View::App { args, .. } => stack.extend(args.iter().copied()),
            _ => {}
        }
    }
}

/// `f = alpha + beta g(x)` with the parameters of f in alpha and beta only (g has none) and beta != 0 under the facts:
/// `(s, g)`, s the sign of beta (1, -1; 0 when only beta != 0 is known). `None` for another f, or a trivial split.
fn affine_part<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>) -> Res<Option<(i64, T<'h>)>> {
    let h = b.heap();
    let parts = |t: T<'h>, s: mc_api::Sym| -> Vec<T<'h>> {
        match h.view(t) {
            View::App { head, args } if b.sym_of(head) == Some(s) => args.to_vec(),
            _ => vec![t],
        }
    };
    let terms = parts(c.f, b.hd.add);
    let dep: Vec<T<'h>> = terms.iter().copied().filter(|&t| !is_free(b, t, x)).collect();
    let [t] = dep.as_slice() else { return Ok(None) };
    let (consts, rest): (Vec<T<'h>>, Vec<T<'h>>) = parts(*t, b.hd.mul).into_iter().partition(|&u| is_free(b, u, x));
    if rest.is_empty() {
        return Ok(None);
    }
    let beta = cx.eval(b.mul(&consts))?;
    let g = cx.eval(b.mul(&rest))?;
    let mut ps = rustc_hash::FxHashSet::default();
    symbols(b, c.f, &mut ps);
    let mut gs = rustc_hash::FxHashSet::default();
    symbols(b, g, &mut gs);
    // only a split that moves every parameter out of g
    if g == c.f || !ps.iter().any(|&s| s != x) || gs.iter().any(|&s| s != x) {
        return Ok(None);
    }
    let mut pf = c.facts.clone();
    pf.extend(cx.assumptions().iter().copied());
    let holds = |cx: &mut NativeCx<'_, '_, 'h>, rel: &str| -> Res<bool> {
        let r = cx.eval_assuming(&pf, h.call(h.sym("refine"), &[h.call(h.sym(rel), &[beta, b.zero()])]))?;
        Ok(cx.is_true(r))
    };
    let s = if holds(cx, "greater")? {
        1
    } else if holds(cx, "less")? {
        -1
    } else if holds(cx, "unequal")? {
        0
    } else {
        return Ok(None);
    };
    Ok(Some((s, g)))
}

fn injective<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let [x] = c.vars.as_slice() else { return Ok(None) };
    if c.complex || cx.as_list(c.f).is_some() {
        return Ok(None);
    }
    let x = *x;
    // alpha + beta g(x), beta != 0: injective when g is
    if c.cond.is_none() && !c.integers {
        if let Some((_, g)) = affine_part(cx, b, c, x)? {
            let cg = Call { f: g, cond: None, vars: c.vars.clone(), complex: false, integers: false, facts: c.facts.clone(), strict: c.strict };
            return injective(cx, b, &cg);
        }
    }
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, c.f, &mut params);
    if params.iter().any(|&s| s != x) {
        return Ok(None);
    }
    // an inverse function of x (with constant parameters) on its principal branch is injective on its domain; the
    // samples below cannot see that when the real domain is a bounded interval (complex values outside it)
    if let View::App { head, args } = h.view(c.f) {
        let inverse = b.sym_of(head).is_some_and(|s| {
            let n = h.sym_name(s);
            n.starts_with("inverse_jacobi_") || matches!(&*n, "inverse_elliptic_nome_q" | "inverse_erf" | "inverse_erfc")
        });
        if inverse && args.first() == Some(&x) && args[1..].iter().all(|&a| is_free(b, a, x)) && c.cond.is_none() {
            return Ok(Some(cx.bool(true)));
        }
    }
    // strictly monotonic: the derivative of one sign (zero at isolated points only); not for functions with jumps or
    // flat pieces (`floor`, `fractional_part`, `ramp`, ...), whose derivative says nothing about that
    let jumps = {
        let mut stack = vec![c.f];
        let mut found = false;
        while let Some(u) = stack.pop() {
            if let View::App { head, args } = h.view(u) {
                if b.sym_of(head).is_some_and(|s| {
                    let n = h.sym_name(s);
                    (CONTINUOUS.contains(&n.as_str()) || table::not_smooth(&n)) && n != "abs"
                }) && !is_free(b, u, x)
                {
                    found = true;
                    break;
                }
                stack.extend(args.iter().copied());
            }
        }
        found
    };
    // two equal values among the samples (of a function that is not constant)
    let mut vals: Vec<(f64, f64)> = vec![];
    for v in samples(c.integers).into_iter().filter(|v| v.abs() <= 50.0) {
        cx.tick(1)?;
        if out_of_time() {
            return Ok(None);
        }
        if let Some(k) = c.cond {
            let rule = h.call(h.sym(names::RULE), &[x, h.real(v)]);
            let ok = cx.eval(h.call(h.sym("rewrite"), &[k, rule]))?;
            if !cx.is_true(ok) {
                continue;
            }
        }
        if let Some(y) = value_at(cx, b, c.f, x, v)? {
            vals.push((v, y));
        }
    }
    if out_of_time() {
        return Ok(None);
    }
    // a value taken at two samples with a different value between them (not a saturation): not injective, before
    // the slower test of the derivative's sign
    for i in 0..vals.len() {
        for j in i + 2..vals.len() {
            // a relative tolerance: tails decaying to 0 on both sides (`csch`) are not ties
            let (yi, yj) = (vals[i].1, vals[j].1);
            let tol = 1e-12 * yi.abs().max(yj.abs());
            if (yi - yj).abs() <= tol && vals[i + 1..j].iter().any(|p| (p.1 - yi).abs() > 1e3 * tol.max(1e-300)) {
                return Ok(Some(cx.bool(false)));
            }
        }
    }
    if !c.integers && !jumps {
        let m = sign_of(cx, b, c, 1)?;
        if let Some(m) = m {
            match h.view(m) {
                View::Int(1 | -1) => return Ok(Some(cx.bool(true))),
                View::Int(0) => return Ok(Some(cx.bool(false))),
                _ => {}
            }
        }
    }
    // strictly monotonic samples: injective when the derivative has one sign (or the sequence is strictly monotonic)
    let increasing = vals.windows(2).all(|w| w[1].1 > w[0].1);
    let decreasing = vals.windows(2).all(|w| w[1].1 < w[0].1);
    if vals.len() >= 2 && (increasing || decreasing) {
        return Ok(Some(cx.bool(true)));
    }
    // monotonic with equal neighbouring machine values (a function that saturates): those neighbours at 40 digits
    let nondecreasing = vals.windows(2).all(|w| w[1].1 >= w[0].1);
    let nonincreasing = vals.windows(2).all(|w| w[1].1 <= w[0].1);
    if vals.len() >= 2 && (nondecreasing || nonincreasing) && vals.windows(2).any(|w| w[1].1 != w[0].1) {
        let rel = if nondecreasing { "less" } else { "greater" };
        let mut strict = true;
        for w in vals.windows(2).filter(|w| w[1].1 == w[0].1) {
            cx.tick(1)?;
            let (ya, yb) = (precise_value(cx, b, c.f, x, w[0].0)?, precise_value(cx, b, c.f, x, w[1].0)?);
            let r = cx.eval(h.call(h.sym(rel), &[ya, yb]))?;
            if !cx.is_true(r) {
                strict = false;
                break;
            }
        }
        if strict {
            return Ok(Some(cx.bool(true)));
        }
    }
    // a function with poles that is monotonic between them and takes no value twice (`1/x`, `x/(x + 1)`, `csch`)
    if !c.integers {
        if let Some(r) = injective_across_poles(cx, b, c, x)? {
            return Ok(Some(cx.bool(r)));
        }
    }
    let mut sorted: Vec<f64> = vals.iter().map(|p| p.1).collect();
    sorted.sort_by(f64::total_cmp);
    let tie = sorted.windows(2).any(|w| (w[1] - w[0]).abs() <= 1e-12 * (1.0 + w[0].abs()));
    if tie || !(increasing || decreasing) && vals.len() >= 3 {
        // values not monotonic along the line: some value is taken twice (intermediate values of a continuous
        // function; a sequence that is not monotonic may still be injective: only a tie decides it)
        if tie || !c.integers {
            return Ok(Some(cx.bool(false)));
        }
    }
    Ok(None)
}

/// Injectivity of a function with poles (F27.7): over the samples (wide, with the points near the poles) the
/// differences have one sign except at jumps across genuine poles (found by bisection: `|f|` grows past 10^8);
/// the pieces between the poles are then strictly monotonic and the function is injective exactly when their value
/// ranges do not overlap (touching ranges, as `1/x` at the infinities, are disjoint open intervals). `None` when
/// the samples show anything else (turning points, no pole).
fn injective_across_poles<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>) -> Res<Option<bool>> {
    let h = b.heap();
    let mut xs = samples(false);
    for k in 7..=30 {
        let p = 10f64.powi(k);
        xs.push(p);
        xs.push(-p);
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    let mut pts: Vec<(f64, f64)> = vec![];
    let defs = conditions(cx, b, c.f, &c.vars, false)?.unwrap_or_default();
    for v in xs {
        cx.tick(1)?;
        if out_of_time() {
            return Ok(None);
        }
        // outside the domain (`x == 0` for `tanh(1/x)`, which a machine evaluation would take as 1)
        let at = if v == 0.0 { b.zero() } else { h.real(v) };
        let mut outside = false;
        for &d in &defs {
            let ok = cx.eval(h.call(h.sym("rewrite"), &[d, h.call(h.sym(names::RULE), &[x, at])]))?;
            outside |= cx.is_false(ok);
        }
        if outside {
            continue;
        }
        if let Some(k) = c.cond {
            let rule = h.call(h.sym(names::RULE), &[x, h.real(v)]);
            let ok = cx.eval(h.call(h.sym("rewrite"), &[k, rule]))?;
            if !cx.is_true(ok) {
                continue;
            }
        }
        if let Some(y) = value_at(cx, b, c.f, x, v)? {
            pts.push((v, y));
        }
    }
    if pts.len() < 8 || out_of_time() {
        return Ok(None);
    }
    let tol = |y: f64| 1e-12 * (1.0 + y.abs());
    let ups = pts.windows(2).filter(|w| w[1].1 - w[0].1 > tol(w[0].1)).count();
    let downs = pts.windows(2).filter(|w| w[0].1 - w[1].1 > tol(w[0].1)).count();
    let dir = if ups >= downs { 1.0 } else { -1.0 };
    let minority = ups.min(downs);
    if minority == 0 || minority > 4 {
        return Ok(None);
    }
    // pieces: split at the violations, each of which must be a pole
    let mut pieces: Vec<(f64, f64)> = vec![];
    let (mut lo, mut hi) = (pts[0].1, pts[0].1);
    for w in pts.windows(2) {
        let d = (w[1].1 - w[0].1) * dir;
        if d < -tol(w[0].1) {
            let pole = pole_between(cx, b, c, x, w[0].0, w[1].0)?;
            if out_of_time() || !pole {
                return Ok(None);
            }
            pieces.push((lo, hi));
            lo = w[1].1;
            hi = w[1].1;
        } else {
            lo = lo.min(w[1].1);
            hi = hi.max(w[1].1);
        }
    }
    pieces.push((lo, hi));
    for i in 0..pieces.len() {
        for j in i + 1..pieces.len() {
            let overlap = pieces[i].1.min(pieces[j].1) - pieces[i].0.max(pieces[j].0);
            let scale = 1.0 + pieces[i].0.abs().min(pieces[i].1.abs()).min(1e12);
            if overlap > 1e-9 * scale {
                return Ok(Some(false));
            }
        }
    }
    Ok(Some(true))
}

/// Does `|f|` blow up between the samples `a < b` (bisection towards the larger values)?
fn pole_between<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, a: f64, bb: f64) -> Res<bool> {
    let h = b.heap();
    // a point between them outside the domain (`x != 0` for `1/x`, also under a saturating `tanh(1/x)`)
    let defs = conditions(cx, b, c.f, &c.vars, false)?.unwrap_or_default();
    let (mut lo, mut hi) = (a, bb);
    let mut ylo = value_at(cx, b, c.f, x, lo)?.unwrap_or(f64::INFINITY).abs();
    let mut yhi = value_at(cx, b, c.f, x, hi)?.unwrap_or(f64::INFINITY).abs();
    let mut best = ylo.max(yhi);
    for _ in 0..60 {
        cx.tick(1)?;
        let mid = 0.5 * (lo + hi);
        if mid <= lo || mid >= hi {
            break;
        }
        for &d in &defs {
            let at = if mid == 0.0 { b.zero() } else { h.real(mid) };
            let ok = cx.eval(h.call(h.sym("rewrite"), &[d, h.call(h.sym(names::RULE), &[x, at])]))?;
            if cx.is_false(ok) {
                return Ok(true);
            }
        }
        let Some(ym) = value_at(cx, b, c.f, x, mid)? else { return Ok(true) };
        let ym = ym.abs();
        best = best.max(ym);
        if ylo >= yhi {
            // the blow-up is on the side of the larger end value
            hi = mid;
            yhi = ym;
        } else {
            lo = mid;
            ylo = ym;
        }
    }
    Ok(best > 1e8)
}

/// Does `t` contain a call of `name`?
fn contains_call<'h>(b: &Build<'h>, t: T<'h>, name: &str) -> bool {
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } => b.sym_of(head) == Some(h.sym(name)) || args.iter().any(|&a| contains_call(b, a, name)),
        _ => false,
    }
}

/// The first arguments of the calls of the gamma-type functions in `t` that depend on `x`.
fn growth_args<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>, out: &mut Vec<T<'h>>) {
    let h = b.heap();
    if let View::App { head, args } = h.view(t) {
        if b.sym_of(head).is_some_and(|s| matches!(h.sym_name(s).as_str(), "gamma" | "factorial" | "log_gamma" | "digamma" | "polygamma" | "beta" | "pochhammer" | "factorial2" | "binomial")) && !is_free(b, t, x) {
            out.extend(args.iter().copied().filter(|&a| !is_free(b, a, x)));
        }
        for &a in args {
            growth_args(b, a, x, out);
        }
    }
}
