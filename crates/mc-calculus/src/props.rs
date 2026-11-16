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
//!   An answer is given only when it is proved: by `refine` under the domain's facts, by the table, or,
//!   for one variable without parameters on a domain of finitely many bounded intervals, by enclosures of the
//!   function (its derivative, its second derivative) over the intervals ([`proof::sign_on`]); a bounded set of
//!   integers by the exact values. Points refute only: values of both signs (of f, f' or f''; or of the first and
//!   second differences of f) at points of the domain, each sign proved at an exact point ([`proof`]), give
//!   `indeterminate`; samples of one sign give no answer. Convexity from the sign of f'' needs f'' to exist on the
//!   whole domain (a cusp such as x^(2/3) at 0 is not seen by f'').
//!   A function with jumps or kinks (`floor`, `unit_step`, `abs`, `max`, ...: the derivative misses the jumps and the
//!   second derivative the kinks) is refuted by the differences of the function itself (its first differences for
//!   the monotonicity, its chords for the convexity); it is never proved monotonic or convex by them. Its sign (and,
//!   with kinks only, its monotonicity) is proved piece by piece on its piecewise form ([`piecewise_shape`]).
//! - **Injective**: strictly monotonic (by a proved sign of the derivative) is `true`; two points of the domain
//!   with exactly equal values, or a proved turning point inside a stretch where the function is continuous (an
//!   enclosure over it exists; the intermediate value theorem), is `false`; nothing else.
//! - Over the complexes the functions entire on the reals that have poles or branch points off the real line are
//!   not analytic everywhere: `tanh`, `sech`, `gudermannian` (`cosh(u) != 0`), `coth`, `csch` (`sinh(u) != 0`),
//!   and the inverse trigonometric and hyperbolic functions off their branch cuts.
//! - The special functions: what each one is (domain, singular set, range, bounds, sign, monotonicity, convexity,
//!   periodicity) is kept once in [`table`], with its literature, and read by every property.
//! - The searches for refuting points are bounded by their number of points and, for slow functions, by a time
//!   budget ([`Budget`]) that only cuts a search short (no answer then); it is to become a work count.
mod domain_form;
mod inject;
mod interval;
mod onto;
mod period;
mod poles;
mod proof;
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
use mc_eval::{Allowance, Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

thread_local! {
    /// The work the searches of the current property call may still do (the outermost call sets it).
    static SEARCH: std::cell::Cell<Option<Allowance>> = const { std::cell::Cell::new(None) };
}

/// The work (units of fuel) the searches for refuting points and the proofs by enclosures and limits of one property
/// call may do. Only a search or a proof is cut (it then gives no answer): every answer comes from a proof or from
/// a counterexample confirmed at exact points. Some special functions take seconds per value (at 30 digits more), so
/// they need a bound; being work, not time, it cuts at the same point on every machine.
const SEARCH_WORK: u64 = 5_000_000;

/// The budget of the outermost property call on this thread, released on drop.
struct Budget(bool);

impl Budget {
    fn enter(cx: &NativeCx<'_, '_, '_>) -> Budget {
        SEARCH.with(|d| {
            if d.get().is_some() {
                Budget(false)
            } else {
                d.set(Some(Allowance::new(cx, SEARCH_WORK)));
                Budget(true)
            }
        })
    }
}

impl Drop for Budget {
    fn drop(&mut self) {
        if self.0 {
            SEARCH.with(|d| d.set(None));
        }
    }
}

/// Has the work allowance of the current call's searches run out?
pub(crate) fn out_of_time(cx: &NativeCx<'_, '_, '_>) -> bool {
    SEARCH.with(|d| d.get().is_some_and(|a| a.expired(cx)))
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
        let _budget = Budget::enter(cx);
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
            Prop::Poles => {
                let r = poles::poles(cx, &b, &sp)?;
                Some(poles::with_parameters(cx, &b, args, r)?)
            }
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
                    3 => None,
                    0 => sign_of(cx, &b, &c, 2)?,
                    k => sign_of(cx, &b, &c, 2)?.map(|r| match h.view(r) {
                        View::Int(s) if s == 0 || s == i64::from(k) => h.int(i64::from(k)),
                        _ => ind,
                    }),
                }
            }
            Prop::Sign => sign_of(cx, &b, &c, 0)?,
            Prop::Monotonicity => sign_of(cx, &b, &c, 1)?,
            // undecided here: the pairs f(x) == f(x2) by quantifier elimination (rational functions: 1/x, x/(x + 1))
            Prop::Injective => match injective(cx, &b, &c)? {
                Some(r) => Some(r),
                None => onto::decide(cx, &b, &sp, onto::Want::Injective)?,
            },
            Prop::Discontinuities | Prop::Continuous | Prop::Meromorphic | Prop::Period | Prop::Poles | Prop::Surjective | Prop::Bijective | Prop::PiecewiseExpand | Prop::Range => None,
        };
        Ok(r.map_or(Reply::Decline, Reply::Done))
    }
}

/// The shape of f at the points `x != c` (alone or in a conjunction) a condition on one variable leaves out: 2 when
/// the one-sided limits of f are finite and differ at one (a jump) or the kinks disagree, 1 (-1) when f is
/// continuous there and the one-sided limits of f' jump up (down), 0 otherwise; 3 when a comparison of the limits is
/// not decided (exact limits compared by the proved sign of their difference).
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
    // the one-sided limits (above, below) of g at p, `None` when one is not a finite number
    let sides = |cx: &mut NativeCx<'_, '_, 'h>, g: T<'h>, p: T<'h>| -> Res<Option<(T<'h>, T<'h>)>> {
        let mut v = vec![];
        for dir in ["from_above", "from_below"] {
            let rule = h.call(h.sym(names::RULE), &[*x, p]);
            let d = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("direction")), h.sym_term(h.sym(dir))]);
            let l = cx.eval(b.call("limit", &[g, rule, h.call(h.sym("options"), &[d])]))?;
            let n = cx.eval(b.call("n", &[l]))?;
            match h.view(n) {
                View::Int(_) => v.push(l),
                View::Real(r) if r.is_finite() => v.push(l),
                _ => return Ok(None),
            }
        }
        Ok(Some((v[0], v[1])))
    };
    let mut shape = 0i8;
    for p in points {
        let Some((above, below)) = sides(cx, c.f, p)? else { continue };
        match proof::closed_sign(cx, b, b.sub(above, below))? {
            Some(0) => {}
            Some(_) => return Ok(2),
            None => return Ok(3),
        }
        let df = cx.eval(b.call("d", &[c.f, *x]))?;
        let Some((da, db)) = sides(cx, df, p)? else { continue };
        let k = match proof::closed_sign(cx, b, b.sub(da, db))? {
            Some(k) => k,
            None => return Ok(3),
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
            // floor(u, m), ceiling(u, m), round(u, m) = m floor(u/m), ...: undefined at m == 0
            if matches!(name.as_str(), "floor" | "ceiling" | "round") && args.len() == 2 {
                if depends(args[1]) {
                    out.push(rel("unequal", args[1], zero));
                }
                continue;
            }
            // clip of a non-real argument stays unevaluated
            if name == "clip" && complex {
                out.push(b.call("element", &[args[0], h.sym_term(h.sym("reals"))]));
                continue;
            }
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

            // heaviside_theta(0) is undefined (it stays unevaluated), and so are heaviside_theta and unit_step of a
            // non-real argument
            "heaviside_theta" => {
                if complex {
                    out.push(b.call("or", &[rel("less", *a, zero), rel("greater", *a, zero)]));
                } else {
                    out.push(rel("unequal", *a, zero));
                }
            }
            "unit_step" | "clip" if complex => out.push(b.call("element", &[*a, h.sym_term(h.sym("reals"))])),
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
            // log_gamma (the continuation of log(gamma(u)) from u > 0, with the cut u <= 0) is real at real u only for
            // u > 0: on the negative axis its imaginary part is a nonzero multiple of pi
            "log_gamma" if !complex => out.push(rel("greater", *a, zero)),
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
        // a function whose real set is stated in the plane (the modular functions): that set, for real variables
        if hit.real_in_plane() && !analytic {
            let Some(d) = hit.domain(cx, b, false)? else { return Ok(None) };
            let d = cx.eval_assuming(&real_vars, h.call(h.sym("refine"), &[d]))?;
            return Ok(Some(vec![d]));
        }
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
    let mut stack = mc_api::walk::Walk::new(c.f);
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
    // with parameters the conditions are sufficient only (`x > 0` for `x^a`, analytic everywhere for a = 2; a cut of
    // a family that is a polynomial for integer parameters): they prove `true`, never `false`
    let mut all = vec![c.f];
    all.extend(c.cond);
    let unknown = if spec::params(b, &all, &c.vars).is_empty() { Some(cx.bool(false)) } else { None };
    let Some(k) = c.cond else { return Ok(unknown) };
    // every condition implied by the given one
    for d in cs {
        let r = cx.eval(h.call(h.sym("refine"), &[d, k]))?;
        if !cx.is_true(r) {
            return Ok(unknown);
        }
    }
    Ok(Some(cx.bool(true)))
}

/// Does `f` have a power of a function of the variables with an exponent that is a symbolic expression free of them
/// (`x^a`, `log(x)^n`)? Its domain conditions are those of generic exponents.
fn symbolic_power<'h>(b: &Build<'h>, f: T<'h>, vars: &[T<'h>]) -> bool {
    let h = b.heap();
    let mut stack = mc_api::walk::Walk::new(f);
    while let Some(u) = stack.pop() {
        let View::App { head, args } = h.view(u) else { continue };
        if b.sym_of(head) == Some(b.hd.pow) {
            if let [base, ex] = args {
                let dep = |t: T<'h>| vars.iter().any(|&x| !is_free(b, t, x));
                if dep(*base) && !dep(*ex) && b.ar.num(*ex).is_none() {
                    return true;
                }
            }
        }
        stack.extend(args.iter().copied());
    }
    false
}

/// The machine value of `f` at `x = v` (one variable), if finite and real.
fn value_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, v: f64) -> Res<Option<f64>> {
    let h = b.heap();
    if out_of_time(cx) {
        return Ok(None);
    }
    let rule = h.call(h.sym(names::RULE), &[x, h.real(v)]);
    // no sample where a gamma-type function has a huge argument (`gamma(125000.)` takes minutes)
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

/// Search points of the real line (or of the integers) for refuting witnesses: 0, ±10^k at quarter decades from
/// 10^-6 to 10^6, the integers -50..50 and a grid of step 0.1 over [-10, 10]. They only guide the search: a witness
/// is confirmed at the exact point.
fn samples(integers: bool) -> Vec<f64> {
    let mut v: Vec<f64> = (-50..=50).map(f64::from).collect();
    if !integers {
        for k in -24..=24 {
            let p = 10f64.powf(k as f64 / 4.0);
            v.push(p);
            v.push(-p);
        }
        for k in 0..200 {
            v.push(-10.0 + 0.1 * k as f64 + 0.0123);
        }
    }
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

/// At most this many candidate witnesses are confirmed by one search.
const MAX_CONFIRM: usize = 24;

/// Do the conditions hold at the machine point `v` (a cheap filter before a confirmation)?
fn holds_machine<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, conds: &[T<'h>], x: T<'h>, v: f64) -> Res<bool> {
    // an integral sample as the integer (a domain of integers: `subfactorial`)
    let p = if v.fract() == 0.0 && v.abs() < 9.0e15 { b.int(v as i64) } else { b.heap().real(v) };
    proof::holds_at(cx, b, conds, x, p)
}

/// A refutation of a sign of `g`: among the machine points `xs` (inside `conds`), values of both signs (with
/// `zero`, also an exact 0), each confirmed at the exact point. `true` when refuted.
#[allow(clippy::too_many_arguments)]
fn sign_witness<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, conds: &[T<'h>], xs: &[f64], zero: bool, real_known: bool) -> Res<bool> {
    let (mut pos, mut neg) = (false, false);
    let mut tries = 0;
    for &v in xs {
        cx.tick(1)?;
        if !holds_machine(cx, b, conds, x, v)? {
            continue;
        }
        let Some(y) = value_at(cx, b, g, x, v)? else { continue };
        let tol = 1e-12 * (1.0 + y.abs());
        let wanted = (y > tol && !pos) || (y < -tol && !neg) || (zero && y.abs() <= tol);
        if !wanted {
            continue;
        }
        tries += 1;
        if tries > MAX_CONFIRM {
            break;
        }
        let q = proof::exact_point(cx, b, v)?;
        let s = if real_known { proof::point_sign_real(cx, b, g, x, q, conds)? } else { proof::point_sign(cx, b, g, x, q, conds)? };
        match s {
            Some(1) => pos = true,
            Some(-1) => neg = true,
            Some(0) if zero => return Ok(true),
            _ => {}
        }
        if pos && neg {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The proved sign of the first difference f(q) - f(p) (`order` 1, points [p, q]) or of the chord defect
/// (r - q) f(p) + (q - p) f(r) - (r - p) f(q) (`order` 2, points [p, q, r]: positive when f(q) lies below the chord)
/// at the exact points next to the machine points, which must lie inside `conds`; for the chord also f must have an
/// enclosure over [p, r] (it is defined on the whole segment). `None` when not proved.
fn confirmed_diff<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, conds: &[T<'h>], vs: &[f64], order: usize) -> Res<Option<i8>> {
    confirmed_diff_across(cx, b, f, x, conds, vs, order, false)
}

/// [`confirmed_diff`]; with `across`, a chord whose segment crosses a gap of a region of several intervals (its
/// three points in the region: convexity on such a region compares the points of the region only).
#[allow(clippy::too_many_arguments)]
fn confirmed_diff_across<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, conds: &[T<'h>], vs: &[f64], order: usize, across: bool) -> Res<Option<i8>> {
    let mut qs = vec![];
    for &v in vs {
        let q = proof::exact_point(cx, b, v)?;
        if !proof::holds_at(cx, b, conds, x, q)? {
            return Ok(None);
        }
        qs.push(q);
    }
    let mut fs = vec![];
    for &q in &qs {
        fs.push(proof::at(cx, b, f, x, q)?);
    }
    if order == 1 {
        return proof::closed_sign(cx, b, b.sub(fs[1], fs[0]));
    }
    // f defined on the whole segment
    let (p, q, r) = (qs[0], qs[1], qs[2]);
    if !across && !defined_on(cx, b, f, x, conds, (p, vs[0]), (r, vs[2]), false)? {
        return Ok(None);
    }
    let defect = b.add(&[b.mul(&[b.sub(r, q), fs[0]]), b.mul(&[b.sub(q, p), fs[2]]), b.neg(b.mul(&[b.sub(r, p), fs[1]]))]);
    proof::closed_sign(cx, b, defect)
}

/// Is `f` defined on the whole segment [p, r] (exact ends with their machine values), and with `continuous` also
/// continuous there? Its definedness conditions and `conds` hold for p <= x <= r (refine; continuity then needs no
/// step function and no function the table does not describe completely), or else an enclosure of f over the
/// segment exists (no pole or cut inside; continuity then needs no step function).
#[allow(clippy::too_many_arguments)]
fn defined_on<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, conds: &[T<'h>], (p, pv): (T<'h>, f64), (r, rv): (T<'h>, f64), continuous: bool) -> Res<bool> {
    let h = b.heap();
    let steps = range::has_jumps(b, f) || contains_call(b, f, "arg");
    if continuous && steps {
        return Ok(false);
    }
    let incomplete = {
        let mut stack = mc_api::walk::Walk::new(f);
        let mut found = false;
        while let Some(u) = stack.pop() {
            if let View::App { head, args } = h.view(u) {
                found |= b.sym_of(head).is_some_and(|s| table::not_smooth(&h.sym_name(s)));
                stack.extend(args.iter().copied());
            }
        }
        found
    };
    let own = conditions(cx, b, f, &[x], false)?;
    let seg = [h.call(h.sym("element"), &[x, h.sym_term(h.sym("reals"))]), b.call("less_equal", &[p, x]), b.call("less_equal", &[x, r])];
    if !(continuous && incomplete) {
        if let Some(cs) = &own {
            let mut all = true;
            for k in cs.iter().chain(conds) {
                let v = cx.eval_assuming(&seg, b.call("refine", &[*k]))?;
                if !cx.is_true(v) {
                    all = false;
                    break;
                }
            }
            if all {
                return Ok(true);
            }
        }
    }
    // an enclosure covers f's own conditions; the others (the region of the call: `x != 1`, a domain of integers)
    // must hold on the whole segment
    for &k in conds {
        if own.as_ref().is_some_and(|cs| cs.contains(&k)) {
            continue;
        }
        let v = cx.eval_assuming(&seg, b.call("refine", &[k]))?;
        if !cx.is_true(v) {
            return Ok(false);
        }
    }
    let fb = proof::with_ball_constants(cx, b, f)?;
    Ok(proof::enclose(cx, b, fb, x, pv, rv)?.is_some())
}

/// A refutation of monotonicity (`order` 1) or convexity (`order` 2) from the triples: differences (chord defects)
/// of both signs, each confirmed ([`confirmed_diff`]). `true` when refuted.
#[allow(clippy::too_many_arguments)]
fn diff_witness<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, conds: &[T<'h>], triples: &[[f64; 3]], order: usize, across: bool) -> Res<bool> {
    let (mut up, mut down) = (false, false);
    let mut tries = 0;
    for t in triples {
        let mut ys = [0.0; 3];
        let mut ok = true;
        for (i, &v) in t.iter().enumerate() {
            cx.tick(1)?;
            if !holds_machine(cx, b, conds, x, v)? {
                ok = false;
                break;
            }
            match value_at(cx, b, f, x, v)? {
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
        let tol = 1e-9 * (1.0 + ys[0].abs() + ys[1].abs() + ys[2].abs());
        let cands: Vec<(f64, Vec<f64>)> = if order == 1 {
            vec![(ys[1] - ys[0], vec![t[0], t[1]]), (ys[2] - ys[1], vec![t[1], t[2]])]
        } else {
            let d = (t[2] - t[1]) * ys[0] + (t[1] - t[0]) * ys[2] - (t[2] - t[0]) * ys[1];
            vec![(d, t.to_vec())]
        };
        for (d, pts) in cands {
            let want = (d > tol && !up) || (d < -tol && !down);
            if !want {
                continue;
            }
            tries += 1;
            if tries > MAX_CONFIRM {
                return Ok(false);
            }
            match confirmed_diff_across(cx, b, f, x, conds, &pts, order, across)? {
                Some(1) => up = true,
                Some(-1) => down = true,
                _ => {}
            }
            if up && down {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// The sign of `f`, `f'` or `f''` on the domain (module comment): `1`, `-1`, `0`, `indeterminate`, or `None`.
/// A convexity claim is checked against the chords of the function (cusps, where the second derivative has one sign
/// on each side but the function is not convex or concave across the point).
fn sign_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, order: usize) -> Res<Option<T<'h>>> {
    let r = sign_of_inner(cx, b, c, order)?;
    // undecided convexity: chords above and below the function refute both (a cusp, as x^(2/3) at 0)
    if r.is_none() && order == 2 && !c.integers && !c.complex && c.vars.len() == 1 {
        let x = c.vars[0];
        let mut params = rustc_hash::FxHashSet::default();
        symbols(b, c.f, &mut params);
        if params.iter().all(|&s| s == x) {
            let fr = real_roots(cx, b, c.f)?;
            let conds: Vec<T<'h>> = c.cond.into_iter().collect();
            let mut triples = vec![];
            for x0 in (-10..=10).map(|i| f64::from(i) * 0.5) {
                for t in [0.1, 0.5, 1.0, 2.5] {
                    triples.push([x0 - t, x0, x0 + t]);
                }
            }
            // a condition whose region (with the domain) is several intervals: chords across its gaps
            let across = match c.cond {
                Some(k) => {
                    let mut all = conditions(cx, b, c.f, &c.vars, false)?.unwrap_or_default();
                    all.push(k);
                    let all = conj(cx, &all)?;
                    let r = cx.eval(b.call("reduce", &[all, x, b.heap().sym_term(b.heap().sym("reals"))]))?;
                    range::pieces(b, r, x).is_some_and(|p| p.len() > 1)
                }
                None => false,
            };
            if diff_witness(cx, b, fr, x, &conds, &triples, 2, across)? {
                return Ok(Some(b.heap().sym_term(b.heap().sym("indeterminate"))));
            }
        }
    }
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

/// Do the chords of `f` agree with the claim `k` (1 convex, -1 concave) at the triples `x0 - t, x0, x0 + t`? A
/// disagreement found by machine values is confirmed at the exact points ([`confirmed_diff`]); an unconfirmed one
/// leaves the claim undecided (`None`).
fn chords_agree<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, k: i64) -> Res<Option<bool>> {
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, c.f, &mut params);
    if params.iter().any(|&s| s != x) {
        return Ok(Some(true));
    }
    let fr = real_roots(cx, b, c.f)?;
    let conds: Vec<T<'h>> = c.cond.into_iter().collect();
    for x0 in (-10..=10).map(|i| f64::from(i) * 0.5) {
        for t in [0.1, 0.5, 1.0, 2.5] {
            cx.tick(1)?;
            let pts = [x0 - t, x0, x0 + t];
            let mut ys = [0.0; 3];
            let mut ok = true;
            for (i, &v) in pts.iter().enumerate() {
                if !holds_machine(cx, b, &conds, x, v)? {
                    ok = false;
                    break;
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
                return Ok(match confirmed_diff(cx, b, fr, x, &conds, &pts, 2)? {
                    Some(s) if i64::from(s) == -k => Some(false),
                    _ => None,
                });
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
    singular_on_line(cx, b, c, x, &[sets::Kind::Pole])
}

/// Like [`pole_on_line`] for the singular points of the given kinds.
fn singular_on_line<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, kinds: &[sets::Kind]) -> Res<bool> {
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
        if !kinds.contains(&k) {
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
    // conjugate(x), re(x), im_part(x) of a real x: x, x, 0
    if let Some(fr) = realified(cx, b, c, x)? {
        let c2 = Call { f: fr, cond: c.cond, vars: c.vars.clone(), complex: c.complex, integers: c.integers, facts: c.facts.clone(), strict: c.strict };
        return sign_of(cx, b, &c2, order);
    }
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
        if shaped {
            return sampled_shape(cx, b, c, x, order);
        }
        // a smooth function whose domain is not known: samples can still show that no sign, monotonicity or
        // convexity holds (values of both signs, or differences of both signs); they never prove one
        return witness_only(cx, b, c, x, order);
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
    // parameters: a power with a symbolic exponent has the domain of generic exponents (`x > 0` for `x^a`, the whole
    // line for a = 2), and domain conditions on the parameters may hold on the whole line for some values; the
    // property then fails only when the domain leaves out points for every value of the parameters
    let mut with_cond = vec![c.f];
    with_cond.extend(c.cond);
    let has_params = !spec::params(b, &with_cond, &c.vars).is_empty();
    let generic = has_params && symbolic_power(b, c.f, &c.vars);
    // a function not defined on the whole real line (and no condition): the property fails somewhere
    if c.cond.is_none() && !c.integers && !cs.is_empty() && !whole_line {
        if generic {
            return Ok(None);
        }
        let ps = spec::params(b, &cs, &c.vars);
        if !ps.is_empty() {
            // for every value of the parameters a point of the line outside the domain
            let all = conj(cx, &cs)?;
            let gap = b.call("exists", &[x, b.call("not", &[all])]);
            let r = cx.eval(b.call("resolve", &[b.call("for_all", &[cx.list(&ps), gap]), h.sym_term(h.sym("reals"))]))?;
            return Ok(cx.is_true(r).then_some(indet));
        }
        // the gamma family (`u > 0 or u not an integer`) fails at the negative integers: no `reduce` of a
        // trigonometric condition (`gamma(x^3)` takes minutes)
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
        if !c.integers && !generic && !cs.is_empty() && !cs.iter().any(|&d| contains_call(b, d, "not_element")) {
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
        // the variable is real: a sign stated for arguments on a line of the plane (the modular functions on the
        // imaginary axis) is decided for an argument such as i (x^2 + 1)
        let mut pfx = pf.clone();
        pfx.push(h.call(h.sym("element"), &[x, h.sym_term(h.sym("reals"))]));
        if let Some(hit) = table::lookup(cx, b, c.f, &[x], &pfx)? {
            // a nonconstant periodic function is neither monotonic nor convex on the line
            if order > 0 && c.cond.is_none() && range::affine_in(cx, b, hit.arg, x)? && hit.periodic(cx, b)? {
                return Ok((!has_params).then_some(indet));
            }
            // a sign holds at every point of the domain, whatever the argument's path; monotonicity and convexity
            // need the argument to be x (an affine function of x)
            let ok = match order {
                // the facts of most entries are about real arguments: a real argument (or x itself), or an entry whose
                // real set is stated in the plane
                0 => {
                    hit.real_in_plane() || range::affine_in(cx, b, hit.arg, x)? || {
                        let r = cx.eval_assuming(&pfx, h.call(h.sym("refine"), &[h.call(h.sym("element"), &[hit.arg, h.sym_term(h.sym("reals"))])]))?;
                        cx.is_true(r)
                    }
                }
                1 => hit.arg == x,
                _ => range::affine_in(cx, b, hit.arg, x)?,
            };
            if ok {
                // a sign also from the range on the real domain (values in [0, 1]: nonnegative)
                let k = match hit.shape(cx, b, order)? {
                    None if order == 0 => hit.range_sign(cx, b)?,
                    k => k,
                };
                // 2 (-2): only nonnegative or nondecreasing, which a strict question does not settle
                if let Some(k) = k.filter(|k| k.abs() == 1 || !c.strict) {
                    return Ok(Some(b.int(i64::from(k.signum()))));
                }
            }
        }
    }
    // functions with jumps or kinks: the sign (and, for a continuous one, the monotonicity) of each piece of its
    // piecewise form, proved by refine on the piece's region
    if !c.integers && !c.complex && order <= 1 && nonsmooth(b, c.f, x) && !has_params {
        if let Some(r) = piecewise_shape(cx, b, c, x, order, &cs)? {
            return Ok(Some(r));
        }
    }
    // the convexity of a continuous function with kinks: convex pieces whose slopes do not drop at the joints
    if !c.integers && !c.complex && order == 2 && nonsmooth(b, c.f, x) && !has_params && !range::has_jumps(b, c.f) {
        if let Some(k) = piecewise_convexity(cx, b, c, x, &cs)? {
            return Ok(Some(b.int(k)));
        }
    }
    // and by the rules of compositions (a nondecreasing step function of a monotonic argument, abs of an affine one);
    // 2 (-2) marks a strict direction
    if !c.integers && !c.complex && order >= 1 && c.cond.is_none() && cs.is_empty() && nonsmooth(b, c.f, x) && !has_params {
        if let Some(k) = compose_shape(cx, b, c, x, c.f, order)? {
            if !c.strict || k.abs() == 2 {
                return Ok(Some(b.int(k.signum())));
            }
        }
    }
    // otherwise only refuted, by the differences of the function itself
    // (with parameters there are no samples, and the derivatives miss the jumps and kinks: `c unit_step(x)` came out
    // linear; undecided)
    if shaped {
        return sampled_shape(cx, b, c, x, order);
    }
    // a function defined on the whole line with a cut or a jump at a real point is not continuous there: its
    // derivative's sign proves nothing across it, so only a refutation by values can answer
    if order > 0 && cs.is_empty() && !c.integers && !c.complex && !has_params && singular_on_line(cx, b, c, x, &[sets::Kind::Branch, sets::Kind::Jump])? {
        return witness_only(cx, b, c, x, order);
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
    // a derivative with no closed form (its values would be numerical derivatives): the differences of the
    // function's samples can still show that it is not monotonic or convex
    if order > 0 && !c.integers && contains_call(b, g, "derivative") {
        if let Some(t) = probe_differences(cx, b, c, x, order)? {
            return Ok(Some(t));
        }
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
    // monotonicity and convexity from the sign of f' or f'' need a domain of one piece: tan on x > 0 and -1/x on
    // x != 0 have f' > 0 on their domains and are not monotonic; on a domain of several
    // pieces only the values themselves can answer (a refutation across the gaps)
    if order > 0 && !c.integers && !one_interval(cx, b, c, x, &cs)? {
        // convexity is inherited by a part of the line: the chord inequality holds between any two points of the
        // part when it is proved on the whole line (x^2 on x != 0). Monotonicity is inherited too, but that case
        // is one interval already when the function is defined everywhere.
        if order == 2 && cs.is_empty() && c.cond.is_some() {
            let whole = Call {
                f: c.f,
                cond: None,
                vars: c.vars.clone(),
                complex: c.complex,
                integers: c.integers,
                facts: c.facts.clone(),
                strict: c.strict,
            };
            if let Some(v) = sign_of_inner(cx, b, &whole, order)? {
                if matches!(h.view(v), View::Int(_)) {
                    return Ok(Some(v));
                }
            }
        }
        return sampled_shape(cx, b, c, x, order);
    }
    let mut facts = cs.clone();
    facts.push(h.call(h.sym("element"), &[x, h.sym_term(h.sym(if c.integers { "integers" } else { "reals" }))]));
    if let Some(k) = c.cond {
        facts.push(k);
    }
    facts.extend(c.facts.iter().copied());
    let fact = conj(cx, &facts)?;
    // convexity from the sign of f'' needs f'' on the whole domain (x^(2/3) has f'' < 0 where it exists, and a cusp
    // at 0); without it only a refutation can answer
    let provable = order < 2 || c.integers || {
        match conditions(cx, b, g, &c.vars, false)? {
            Some(gc) => {
                let mut all = true;
                for k in gc {
                    let r = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[k]))?;
                    if !cx.is_true(r) {
                        all = false;
                        break;
                    }
                }
                all
            }
            None => false,
        }
    };
    // a proof by refine under the domain's facts
    let zero = b.zero();
    let (rge, rle) = if c.strict && order == 0 { ("greater", "less") } else { ("greater_equal", "less_equal") };
    if provable {
        let ge = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[h.call(h.sym(rge), &[g, zero])]))?;
        let le = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[h.call(h.sym(rle), &[g, zero])]))?;
        match (cx.is_true(ge), cx.is_true(le)) {
            (true, true) => return Ok(Some(zero)),
            (true, false) => return Ok(Some(b.one())),
            (false, true) => return Ok(Some(b.int(-1))),
            _ => {}
        }
    }
    // points and enclosures: only without parameters
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, g, &mut params);
    if params.iter().any(|&s| s != x) {
        return Ok(None);
    }
    let mut conds = cs.clone();
    conds.extend(c.cond);
    let strict0 = c.strict && order == 0;
    // a bounded set of integers: the exact values at each of them
    if c.integers {
        if let Some(r) = integer_points(cx, b, g, x, &conds, strict0)? {
            return Ok(Some(r));
        }
    }
    // a refutation: values of g of both signs at points of the domain (the points near 0 first, after a few spread
    // ones: a slow function whose values near 0 are not real shows a change of sign early)
    let mut xs = samples(c.integers);
    xs.sort_by(|p, q| p.abs().total_cmp(&q.abs()));
    if !c.integers {
        xs = spread_first(xs);
    }
    if sign_witness(cx, b, g, x, &conds, &xs, strict0, true)? {
        return Ok(Some(indet));
    }
    // a proof over the whole domain: facts, factors, limits at the ends with the sign of the next derivative,
    // enclosures, splitting (`interval`)
    if !c.integers {
        if let Some(k) = interval::shape_on_domain(cx, b, c.f, g, x, &conds, order, c.strict)? {
            return Ok(Some(b.int(k)));
        }
    }
    // a proof by enclosures of g over a bounded domain (one interval for monotonicity and convexity)
    if !c.integers && provable {
        if let Some(pieces) = proof::bounded_pieces(cx, b, &conds, x)? {
            if order == 0 || pieces.len() == 1 {
                let mut ans: Option<i8> = Some(0);
                for (lo, hi, lt, ut) in pieces {
                    let s = proof::sign_on(cx, b, g, x, (lo, hi), Some((lt, ut)), c.strict)?;
                    ans = match (ans, s) {
                        (Some(a), Some(s)) if a == s || s == 0 => Some(a),
                        (Some(0), Some(s)) => Some(s),
                        _ => None,
                    };
                    if ans.is_none() {
                        break;
                    }
                }
                if let Some(a) = ans {
                    return Ok(Some(b.int(i64::from(a))));
                }
            }
        }
    }
    Ok(None)
}

/// The nondecreasing functions of one argument with jumps or kinks (the step functions and `ramp`).
const NONDECREASING_STEPS: &[&str] = &["floor", "ceiling", "round", "integer_part", "unit_step", "sign", "heaviside_theta", "real_sign", "ramp"];

/// The monotonicity (`order` 1) or convexity (`order` 2) on the whole line of a function with jumps or kinks, by
/// the rules of compositions: a term free of x is constant; sums combine (terms of one direction, constants
/// aside); a constant factor of proved sign scales; a nondecreasing step function (`floor`, `unit_step`, ...) of an
/// argument keeps its monotonicity; `max` of nondecreasing (convex) arguments is nondecreasing (convex), `min`
/// likewise; `abs`, `real_abs` and `ramp` of an affine argument are convex; a smooth subterm by the proofs of
/// [`sign_of`]. `None` when a rule does not apply. With `strict_inequalities` a direction proved strict is 2 (-2):
/// a smooth part strictly monotonic, a sum with one strict part and the others of its direction or constant, a
/// constant multiple of a strict part, max and min of strict parts (a step function is never strict).
fn compose_shape<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, e: T<'h>, order: usize) -> Res<Option<i64>> {
    let h = b.heap();
    cx.tick(1)?;
    if is_free(b, e, x) {
        return Ok(Some(0));
    }
    if !nonsmooth(b, e, x) {
        let ce = Call { f: e, cond: None, vars: c.vars.clone(), complex: false, integers: false, facts: c.facts.clone(), strict: c.strict };
        return Ok(match sign_of(cx, b, &ce, order)?.map(|r| h.view(r)) {
            Some(View::Int(k)) if (-1..=1).contains(&k) => Some(if c.strict { 2 * k } else { k }),
            _ => None,
        });
    }
    let View::App { head, args } = h.view(e) else { return Ok(None) };
    let Some(s) = b.sym_of(head) else { return Ok(None) };
    // directions of several parts: one direction (zeros aside), else none; strict when one part is
    let combine = |ks: &[i64]| -> Option<i64> {
        let mut out = 0;
        let mut strict = false;
        for &k in ks {
            if k != 0 {
                if out != 0 && out != k.signum() {
                    return None;
                }
                out = k.signum();
                strict |= k.abs() == 2;
            }
        }
        Some(if strict { 2 * out } else { out })
    };
    let parts = |cx: &mut NativeCx<'_, '_, 'h>, us: &[T<'h>]| -> Res<Option<Vec<i64>>> {
        let mut ks = vec![];
        for &u in us {
            match compose_shape(cx, b, c, x, u, order)? {
                Some(k) => ks.push(k),
                None => return Ok(None),
            }
        }
        Ok(Some(ks))
    };
    if s == b.hd.add {
        return Ok(parts(cx, args)?.and_then(|ks| combine(&ks)));
    }
    if s == b.hd.mul {
        let (consts, deps): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| is_free(b, a, x));
        let [d] = deps.as_slice() else { return Ok(None) };
        let k = b.mul(&consts);
        let Some(sk) = proof::closed_sign(cx, b, k)? else { return Ok(None) };
        return Ok(compose_shape(cx, b, c, x, *d, order)?.map(|v| v * i64::from(sk)));
    }
    let name = h.sym_name(s);
    match (name.as_str(), args, order) {
        (n, [u], 1) if NONDECREASING_STEPS.contains(&n) => Ok(compose_shape(cx, b, c, x, *u, 1)?.map(i64::signum)),
        // max of nondecreasing ones is nondecreasing; max of convex ones is convex (and not linear when one of them is
        // not: of linear ones only, it may be linear, as max(x, x + 1), so it is not read)
        // (max of a strict and a constant part is not strict: max(x, 0))
        ("max" | "min", _, 1) => Ok(parts(cx, args)?.and_then(|ks| {
            let k = combine(&ks)?;
            Some(if ks.iter().all(|&v| v.abs() == 2) { k } else { k.signum() })
        })),
        ("max", _, _) => Ok(parts(cx, args)?.and_then(|ks| (ks.iter().all(|&k| k >= 0) && ks.iter().any(|&k| k > 0)).then_some(1))),
        ("min", _, _) => Ok(parts(cx, args)?.and_then(|ks| (ks.iter().all(|&k| k <= 0) && ks.iter().any(|&k| k < 0)).then_some(-1))),
        ("abs" | "real_abs" | "ramp", [u], 2) if sets::poly_degree(b, *u, &[x]).is_some_and(|d| d <= 1) => Ok(Some(1)),
        _ => Ok(None),
    }
}

/// The sign (`order` 0) or monotonicity (`order` 1) of a function with jumps or kinks from its piecewise form
/// (`piecewise_expand`): each piece (the value, or its derivative) has a sign proved by `refine` on its region (the
/// piece's condition and not the earlier ones, with the domain and the call's condition); the signs agree. A
/// monotonicity needs f continuous (no step function: kinks only), so that monotone pieces join into a monotone
/// function. `None` when a piece is not decided.
fn piecewise_shape<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, order: usize, cs: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if order == 1 && (range::has_jumps(b, c.f) || !cs.is_empty()) {
        return Ok(None);
    }
    let pw = cx.eval(b.call("piecewise_expand", &[c.f]))?;
    let View::App { head, args } = h.view(pw) else { return Ok(None) };
    if b.sym_of(head) != Some(h.sym("piecewise")) || args.is_empty() {
        return Ok(None);
    }
    let Some(rows) = cx.as_list(args[0]).map(<[T<'h>]>::to_vec) else { return Ok(None) };
    let default = args.get(1).copied().unwrap_or_else(|| b.zero());
    let mut pieces = vec![];
    let mut earlier: Vec<T<'h>> = vec![];
    for r in rows {
        let Some(&[v, k]) = cx.as_list(r) else { return Ok(None) };
        let region = if earlier.is_empty() { k } else { b.call("and", &[k, b.call("not", &[b.call("or", &earlier)])]) };
        pieces.push((v, region));
        earlier.push(k);
    }
    pieces.push((default, if earlier.is_empty() { cx.bool(true) } else { b.call("not", &[b.call("or", &earlier)]) }));
    let mut ans: Option<i64> = None;
    for (v, region) in pieces {
        cx.tick(1)?;
        if nonsmooth(b, v, x) {
            return Ok(None);
        }
        let mut facts = vec![h.call(h.sym("element"), &[x, h.sym_term(h.sym("reals"))]), region];
        facts.extend(cs.iter().copied());
        facts.extend(c.cond);
        facts.extend(c.facts.iter().copied());
        // an empty region
        let all = cx.eval(b.call("and", &facts[1..]))?;
        if cx.is_false(all) {
            continue;
        }
        let red = cx.eval(b.call("reduce", &[all, x, h.sym_term(h.sym("reals"))]))?;
        if cx.is_false(red) {
            continue;
        }
        // the region as intervals (refine reads `-1 < x and x < 0` where `x + x^2 < 0` says nothing to it)
        if !sets::is_head(b, red, "reduce") && !sets::is_head(b, red, names::FAILURE) {
            facts.push(red);
        }
        let g = if order == 0 { v } else { cx.eval(b.call("d", &[v, x]))? };
        // strict: every piece strictly of one sign (a constant piece is not strictly monotonic)
        let (rge, rle) = if c.strict { ("greater", "less") } else { ("greater_equal", "less_equal") };
        let ge = cx.eval_assuming(&facts, b.call("refine", &[b.call(rge, &[g, b.zero()])]))?;
        let le = cx.eval_assuming(&facts, b.call("refine", &[b.call(rle, &[g, b.zero()])]))?;
        let s = match (cx.is_true(ge), cx.is_true(le)) {
            (true, true) => 0,
            (true, false) => 1,
            (false, true) => -1,
            _ => return Ok(None),
        };
        ans = Some(match ans {
            None => s,
            Some(a) if a == s || s == 0 => a,
            Some(0) => s,
            Some(_) => return Ok(None),
        });
    }
    Ok(ans.map(|a| b.int(a)))
}

/// The convexity of a continuous function with kinks from its piecewise form: every piece convex (concave) by
/// `refine` of its second derivative on the piece's region (with `strict_inequalities`, not identically 0 there: the
/// zeros of an analytic piece's second derivative are isolated), f continuous at every end of a region, and its
/// one-sided slopes there not dropping (not rising), so that f' is nondecreasing (nonincreasing) on the whole line
/// (`abs(x) x^2` = |x|^3: -x^3 and x^3 meet at 0 with slope 0). `None` when not proved.
fn piecewise_convexity<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, cs: &[T<'h>]) -> Res<Option<i64>> {
    let h = b.heap();
    if !cs.is_empty() || c.cond.is_some() {
        return Ok(None);
    }
    let pw = cx.eval(b.call("piecewise_expand", &[c.f]))?;
    let View::App { head, args } = h.view(pw) else { return Ok(None) };
    if b.sym_of(head) != Some(h.sym("piecewise")) || args.is_empty() {
        return Ok(None);
    }
    let Some(rows) = cx.as_list(args[0]).map(<[T<'h>]>::to_vec) else { return Ok(None) };
    let default = args.get(1).copied().unwrap_or_else(|| b.zero());
    let mut pieces = vec![];
    let mut earlier: Vec<T<'h>> = vec![];
    for r in rows {
        let Some(&[v, k]) = cx.as_list(r) else { return Ok(None) };
        let region = if earlier.is_empty() { k } else { b.call("and", &[k, b.call("not", &[b.call("or", &earlier)])]) };
        pieces.push((v, region));
        earlier.push(k);
    }
    pieces.push((default, if earlier.is_empty() { cx.bool(true) } else { b.call("not", &[b.call("or", &earlier)]) }));
    let reals = h.sym_term(h.sym("reals"));
    let mut ans: Option<i64> = None;
    let mut joints: Vec<T<'h>> = vec![];
    for (v, region) in pieces {
        cx.tick(1)?;
        if nonsmooth(b, v, x) {
            return Ok(None);
        }
        let red = cx.eval(b.call("reduce", &[region, x, reals]))?;
        if cx.is_false(red) {
            continue;
        }
        if !cx.is_true(red) {
            let Some(ivs) = range::pieces(b, red, x) else { return Ok(None) };
            for p in &ivs {
                for (e, _) in [p.lo, p.hi].into_iter().flatten() {
                    if b.ar.num(e).is_none_or(|n| !n.is_exact()) {
                        return Ok(None);
                    }
                    if !joints.contains(&e) {
                        joints.push(e);
                    }
                }
            }
        }
        let facts = vec![h.call(h.sym("element"), &[x, reals]), red];
        let d1 = cx.eval(b.call("d", &[v, x]))?;
        let g = cx.eval(b.call("d", &[d1, x]))?;
        let ge = cx.eval_assuming(&facts, b.call("refine", &[b.call("greater_equal", &[g, b.zero()])]))?;
        let le = cx.eval_assuming(&facts, b.call("refine", &[b.call("less_equal", &[g, b.zero()])]))?;
        let s = match (cx.is_true(ge), cx.is_true(le)) {
            (true, true) if c.strict => return Ok(None),
            (true, true) => 0,
            (true, false) => 1,
            (false, true) => -1,
            _ => return Ok(None),
        };
        ans = Some(match ans {
            None => s,
            Some(a) if a == s || s == 0 => a,
            Some(0) => s,
            Some(_) => return Ok(None),
        });
    }
    let Some(k) = ans.filter(|&k| k != 0) else { return Ok(None) };
    // at each joint: f continuous, slopes ordered
    let df = cx.eval(b.call("d", &[c.f, x]))?;
    for p in joints {
        let side = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, above: bool| -> Res<Option<T<'h>>> {
            let dir = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("direction")), h.sym_term(h.sym(if above { "from_above" } else { "from_below" }))]);
            let l = cx.eval(b.call("limit", &[t, h.call(h.sym(names::RULE), &[x, p]), h.call(h.sym("options"), &[dir])]))?;
            Ok((!is_call(b, l, "limit") && !is_call(b, l, names::FAILURE) && is_free(b, l, x)).then_some(l))
        };
        let (Some(fl), Some(fr)) = (side(cx, c.f, false)?, side(cx, c.f, true)?) else { return Ok(None) };
        let fp = proof::at(cx, b, c.f, x, p)?;
        let (dl, dr) = (b.sub(fl, fp), b.sub(fr, fp));
        if proof::closed_sign(cx, b, dl)? != Some(0) || proof::closed_sign(cx, b, dr)? != Some(0) {
            return Ok(None);
        }
        let (Some(sl), Some(sr)) = (side(cx, df, false)?, side(cx, df, true)?) else { return Ok(None) };
        match proof::closed_sign(cx, b, b.sub(sr, sl))? {
            Some(0) => {}
            Some(s) if i64::from(s) == k => {}
            _ => return Ok(None),
        }
    }
    Ok(Some(k))
}

/// The sign of `g` on a bounded set of integers (the conditions reduce to finitely many bounded intervals holding
/// at most 2000 integers) from its exact values: `None` when the set is not bounded or a value is undecided.
fn integer_points<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, conds: &[T<'h>], strict: bool) -> Res<Option<T<'h>>> {
    let Some(pieces) = proof::bounded_pieces(cx, b, conds, x)? else { return Ok(None) };
    let mut ints = vec![];
    for (lo, hi, _, _) in pieces {
        let (a, c) = (lo.ceil(), hi.floor());
        if !(a.is_finite() && c.is_finite()) || c - a > 2000.0 {
            return Ok(None);
        }
        let mut k = a as i64;
        while (k as f64) <= c {
            ints.push(k);
            k += 1;
        }
        if ints.len() > 2000 {
            return Ok(None);
        }
    }
    let (mut pos, mut neg, mut any) = (false, false, false);
    for k in ints {
        let q = b.int(k);
        if !proof::holds_at(cx, b, conds, x, q)? {
            continue;
        }
        any = true;
        match proof::point_sign(cx, b, g, x, q, &[])? {
            Some(1) => pos = true,
            Some(-1) => neg = true,
            Some(0) if strict => return Ok(Some(b.heap().sym_term(b.heap().sym("indeterminate")))),
            Some(0) => {}
            _ => return Ok(None),
        }
    }
    if !any {
        return Ok(None);
    }
    let h = b.heap();
    Ok(Some(match (pos, neg) {
        (true, true) => h.sym_term(h.sym("indeterminate")),
        (true, false) => b.one(),
        (false, true) => b.int(-1),
        (false, false) => b.zero(),
    }))
}

/// `indeterminate` when the first (`order` 1) or second (`order` 2) differences of `f` at a few triples x0 - h, x0,
/// x0 + h (h = 1/4) inside the domain and the condition have both signs: f is then not monotonic (or not convex nor
/// concave); `None` otherwise. Cheap (at most 30 values), so it is tried before the numerical derivatives.
fn probe_differences<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, order: usize) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, c.f, &mut params);
    if params.iter().any(|&s| s != x) {
        return Ok(None);
    }
    let triples: Vec<[f64; 3]> = [0.5, -0.5, 1.5, -1.5, 3.1, -3.1, 7.3, -7.3, 15.2, -15.2].iter().map(|&x0| [x0 - 0.25, x0, x0 + 0.25]).collect();
    let conds: Vec<T<'h>> = c.cond.into_iter().collect();
    Ok(diff_witness(cx, b, c.f, x, &conds, &triples, order, false)?.then(|| h.sym_term(h.sym("indeterminate"))))
}

/// The probe points 0.5, 1.5, 3.1, 7.3, 15.2, 30.4 (and their negatives, then 0.1) before the samples `xs`.
fn spread_first(xs: Vec<f64>) -> Vec<f64> {
    let probes = [0.5, -0.5, 1.5, -1.5, 3.1, -3.1, 7.3, -7.3, 15.2, -15.2, 30.4, -30.4, 0.1, -0.1];
    let mut out: Vec<f64> = probes.to_vec();
    out.extend(xs.into_iter().filter(|v| !probes.contains(v)));
    out
}

/// `indeterminate` when samples of a function of one variable without parameters show that it has no sign (real
/// values of both signs, order 0) or is neither monotonic nor convex/concave (differences of both signs inside runs
/// of samples in the domain, [`sampled_shape`]); `None` otherwise: samples never prove a sign or a shape here.
fn witness_only<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, order: usize) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let indet = h.sym_term(h.sym("indeterminate"));
    if c.integers || c.complex {
        return Ok(None);
    }
    // no condition: a point of the line where f is defined but not real is outside its real domain, which is then
    // not the whole line (the property fails, as for any function whose domain leaves out points)
    if c.cond.is_none() && c.facts.is_empty() && nonreal_point(cx, b, c.f, x)? {
        return Ok(Some(indet));
    }
    if order > 0 {
        if let Some(t) = probe_differences(cx, b, c, x, order)? {
            return Ok(Some(t));
        }
        return Ok(sampled_shape(cx, b, c, x, order)?.filter(|&t| t == indet));
    }
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, c.f, &mut params);
    if params.iter().any(|&s| s != x) {
        return Ok(None);
    }
    let mut xs = samples(false);
    xs.sort_by(|p, q| p.abs().total_cmp(&q.abs()));
    let xs = spread_first(xs);
    let conds: Vec<T<'h>> = c.cond.into_iter().collect();
    Ok(sign_witness(cx, b, c.f, x, &conds, &xs, false, false)?.then_some(indet))
}

/// Is there a point of the line (among the probe points) where `f` (one variable, no parameters) has a value whose
/// imaginary part is proved nonzero (its 30-digit ball excludes 0)?
fn nonreal_point<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<bool> {
    let h = b.heap();
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, f, &mut params);
    if params.iter().any(|&s| s != x) {
        return Ok(false);
    }
    let mut tries = 0;
    for v in spread_first(vec![]) {
        cx.tick(1)?;
        if out_of_time(cx) {
            return Ok(false);
        }
        let r = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, h.real(v)])]))?;
        let r = cx.eval(h.call(h.sym("n"), &[r]))?;
        let View::App { head, args: &[_, im] } = h.view(r) else { continue };
        if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::COMPLEX) || !matches!(h.view(im), View::Real(y) if y.abs() > 1e-6) {
            continue;
        }
        tries += 1;
        if tries > 4 {
            break;
        }
        let q = proof::exact_point(cx, b, v)?;
        let e = proof::at(cx, b, f, x, q)?;
        let w = cx.eval(b.call("n", &[e, b.int(30)]))?;
        let im = cx.eval(b.call("im_part", &[w]))?;
        if proof::bounds(b, im).is_some_and(|(lo, hi)| lo > 0.0 || hi < 0.0) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `f` with every `(u^2)^(1/2)` replaced by `abs(u)` (the reals).
fn abs_of_root_squares<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>) -> Res<T<'h>> {
    // shared subterms rewritten once
    abs_of_root_squares_in(cx, b, f, &mut mc_api::walk::memo())
}

fn abs_of_root_squares_in<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, T<'h>>) -> Res<T<'h>> {
    if let Some(&r) = memo.get(&f) {
        return Ok(r);
    }
    let r = abs_of_root_squares_node(cx, b, f, memo)?;
    memo.insert(f, r);
    Ok(r)
}

fn abs_of_root_squares_node<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, T<'h>>) -> Res<T<'h>> {
    let h = b.heap();
    let View::App { head, args } = h.view(f) else { return Ok(f) };
    if b.sym_of(head) == Some(b.hd.pow) {
        if let [base, ex] = args {
            if *ex == b.rat(1, 2) {
                if let View::App { head: bh, args: &[u, two] } = h.view(*base) {
                    if b.sym_of(bh) == Some(b.hd.pow) && two == b.int(2) {
                        let u = abs_of_root_squares_in(cx, b, u, memo)?;
                        return cx.eval(b.call("abs", &[u]));
                    }
                }
            }
        }
    }
    let mut changed = false;
    let mut v = Vec::with_capacity(args.len());
    for &a in args {
        let r = abs_of_root_squares_in(cx, b, a, memo)?;
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
    let mut stack = mc_api::walk::Walk::new(f);
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

/// Is the domain of the call one interval: the whole line (no condition: a domain that is not the whole line was
/// answered before), or the region of the condition when the function's domain conditions `cs` hold on all of it
/// and the condition reduces to one interval (a relation of x with a term free of x, or two of them, or `true`)?
/// `false` when that is not shown.
fn one_interval<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>, cs: &[T<'h>]) -> Res<bool> {
    let h = b.heap();
    let Some(region) = c.cond else { return Ok(true) };
    let mut pf = c.facts.clone();
    pf.extend(cx.assumptions().iter().copied());
    pf.push(region);
    pf.push(h.call(h.sym("element"), &[x, h.sym_term(h.sym("reals"))]));
    for &k in cs {
        let r = cx.eval_assuming(&pf, h.call(h.sym("refine"), &[k]))?;
        if !cx.is_true(r) {
            return Ok(false);
        }
    }
    let r = cx.eval(h.call(h.sym("reduce"), &[region, x, h.sym_term(h.sym("reals"))]))?;
    let relation = |t: T<'h>| -> bool {
        let View::App { head, args: &[l, r] } = h.view(t) else { return false };
        let Some(s) = b.sym_of(head) else { return false };
        matches!(h.sym_name(s).as_str(), "less" | "greater" | "less_equal" | "greater_equal" | "equal")
            && ((l == x && is_free(b, r, x)) || (r == x && is_free(b, l, x)))
    };
    if cx.is_true(r) || relation(r) {
        return Ok(true);
    }
    match h.view(r) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => Ok(args.len() == 2 && args.iter().all(|&a| relation(a))),
        _ => Ok(false),
    }
}

/// The monotonicity (`order` 1) or convexity (`order` 2) of a function of one variable without parameters refuted
/// by its differences (module comment): `indeterminate` when differences (chord defects) of both signs are
/// confirmed at exact points, `None` otherwise (samples of one sign prove nothing; with `strict`, an exactly flat
/// step refutes strictness). Samples outside the domain or the condition split the line: no chord is taken across
/// them (first differences are, as monotonicity compares any two points of the domain).
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
                // two sides are compared; a longer gap (outside the domain) splits the run
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
    let indet = h.sym_term(h.sym("indeterminate"));
    let conds: Vec<T<'h>> = c.cond.into_iter().collect();
    // the candidate differences: (machine value, tolerance, points)
    let mut cands: Vec<(f64, f64, Vec<f64>)> = vec![];
    if order == 1 {
        // monotonicity compares values across the gaps of the domain too (x < y implies f(x) <= f(y) everywhere)
        let ends: Vec<&Vec<(f64, f64)>> = runs.iter().filter(|r| !r.is_empty()).collect();
        for w in ends.windows(2) {
            let (p, q) = (*w[0].last().expect("a run"), w[1][0]);
            cands.push((q.1 - p.1, 1e-12 * (1.0 + p.1.abs().max(q.1.abs())), vec![p.0, q.0]));
        }
        for run in &runs {
            for w in run.windows(2) {
                cands.push((w[1].1 - w[0].1, 1e-12 * (1.0 + w[0].1.abs().max(w[1].1.abs())), vec![w[0].0, w[1].0]));
            }
        }
        // and across one sample (a point whose exact value is not confirmed, as a special value at 0)
        for run in &runs {
            for w in run.windows(3) {
                cands.push((w[2].1 - w[0].1, 1e-12 * (1.0 + w[0].1.abs().max(w[2].1.abs())), vec![w[0].0, w[2].0]));
            }
        }
    } else {
        for run in &runs {
            for w in run.windows(3) {
                let ((p, fp), (q, fq), (r, fr)) = (w[0], w[1], w[2]);
                let d = (r - q) * fp + (q - p) * fr - (r - p) * fq;
                cands.push((d, 1e-9 * (r - p) * (1.0 + fp.abs() + fq.abs() + fr.abs()), vec![p, q, r]));
            }
        }
    }
    let (mut up, mut down) = (false, false);
    let mut tries = 0;
    for (d, tol, pts) in cands {
        let flat = c.strict && d.abs() <= 1e-12 * tol;
        let want = (d > tol && !up) || (d < -tol && !down) || flat;
        if !want {
            continue;
        }
        tries += 1;
        if tries > MAX_CONFIRM {
            break;
        }
        match confirmed_diff(cx, b, c.f, x, &conds, &pts, order)? {
            Some(1) => up = true,
            Some(-1) => down = true,
            Some(0) if c.strict => return Ok(Some(indet)),
            _ => {}
        }
        if up && down {
            return Ok(Some(indet));
        }
    }
    Ok(None)
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
    let own = conditions(cx, b, c.f, &c.vars, false)?;
    let known = own.is_some();
    let mut cs = own.unwrap_or_default();
    cs.extend(c.cond);
    const GRID: [f64; 5] = [-2.3, -0.7, 0.4, 1.1, 2.6];
    let n = c.vars.len();
    // the value at a point inside the domain and the condition (finite and real), else `None`
    let value = |cx: &mut NativeCx<'_, '_, 'h>, p: &[f64]| -> Res<Option<f64>> {
        cx.tick(1)?;
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
        // a real value, also one given in complex form with an imaginary part exactly 0
        Ok(machine_real(h, v))
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
    // the value at an exact point inside the conditions
    let exact_value_at = |cx: &mut NativeCx<'_, '_, 'h>, qs: &[T<'h>]| -> Res<Option<T<'h>>> {
        let rules: Vec<T<'h>> = c.vars.iter().zip(qs).map(|(&x, &q)| h.call(h.sym(names::RULE), &[x, q])).collect();
        let rl = cx.list(&rules);
        for &k in &cs {
            let ok = cx.eval(h.call(h.sym("rewrite"), &[k, rl]))?;
            if !cx.is_true(ok) {
                return Ok(None);
            }
        }
        Ok(Some(cx.eval(h.call(h.sym("rewrite"), &[c.f, rl]))?))
    };
    let exact_of = |cx: &mut NativeCx<'_, '_, 'h>, p: &[f64]| -> Res<Vec<T<'h>>> {
        let mut qs = vec![];
        for &v in p {
            qs.push(proof::exact_point(cx, b, v)?);
        }
        Ok(qs)
    };
    let exact_value = |cx: &mut NativeCx<'_, '_, 'h>, p: &[f64]| -> Res<Option<T<'h>>> {
        let qs = exact_of(cx, p)?;
        exact_value_at(cx, &qs)
    };
    let (mut up, mut down) = (false, false);
    let mut tries = 0;
    // no condition: a point where f is defined but not real (its 30-digit imaginary part proved nonzero) is outside
    // the real domain, which is then not the whole space, so no sign, monotonicity or convexity holds on it (as for
    // one variable)
    if c.cond.is_none() && c.facts.is_empty() {
        for p in points.iter().take(25) {
            cx.tick(1)?;
            let rules: Vec<T<'h>> = c.vars.iter().zip(p).map(|(&x, &v)| h.call(h.sym(names::RULE), &[x, h.real(v)])).collect();
            let v = cx.eval(h.call(h.sym("rewrite"), &[c.f, cx.list(&rules)]))?;
            let v = cx.eval(h.call(h.sym("n"), &[v]))?;
            let View::App { head, args: &[_, im] } = h.view(v) else { continue };
            if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::COMPLEX) || !matches!(h.view(im), View::Real(y) if y.abs() > 1e-6) {
                continue;
            }
            tries += 1;
            if tries > MAX_CONFIRM {
                break;
            }
            let Some(e) = exact_value(cx, p)? else { continue };
            let w = cx.eval(b.call("n", &[e, b.int(30)]))?;
            let im = cx.eval(b.call("im_part", &[w]))?;
            if proof::bounds(b, im).is_some_and(|(lo, hi)| lo > 0.0 || hi < 0.0) {
                return Ok(Some(indet));
            }
        }
        tries = 0;
    }
    if order == 0 {
        for p in &points {
            let Some(y) = value(cx, p)? else { continue };
            let tol = 1e-9 * (1.0 + y.abs());
            if !((y > tol && !up) || (y < -tol && !down)) {
                continue;
            }
            tries += 1;
            if tries > MAX_CONFIRM {
                break;
            }
            let Some(v) = exact_value(cx, p)? else { continue };
            match proof::closed_sign(cx, b, v)? {
                Some(1) => up = true,
                Some(-1) => down = true,
                _ => {}
            }
            if up && down {
                return Ok(Some(indet));
            }
        }
        return Ok(None);
    }
    if order != 2 {
        return Ok(None);
    }
    // a chord defect f(a) + f(b) - 2 f(m) proved, with f defined on the whole segment (an enclosure along it)
    let t = h.sym_term(h.sym("chord·t"));
    let confirm = |cx: &mut NativeCx<'_, '_, 'h>, p: &[f64], q: &[f64]| -> Res<Option<i8>> {
        let (pa, qa) = (exact_of(cx, p)?, exact_of(cx, q)?);
        let mut mid = vec![];
        for (&a, &e) in pa.iter().zip(&qa) {
            mid.push(cx.eval(b.div(b.add(&[a, e]), b.int(2)))?);
        }
        let (Some(fa), Some(fb), Some(fm)) = (exact_value_at(cx, &pa)?, exact_value_at(cx, &qa)?, exact_value_at(cx, &mid)?) else { return Ok(None) };
        let mut rules = vec![];
        for (j, &x) in c.vars.iter().enumerate() {
            let (a, e) = (pa[j], qa[j]);
            rules.push(h.call(h.sym(names::RULE), &[x, b.add(&[a, b.mul(&[b.sub(e, a), t])])]));
        }
        // f defined along the segment: its known conditions hold for 0 <= t <= 1 (no step function), or an enclosure
        // of f along it exists
        let mut defined = known && !nonsmooth_any(b, c.f, &c.vars);
        if defined {
            let seg = [h.call(h.sym("element"), &[t, h.sym_term(h.sym("reals"))]), b.call("less_equal", &[b.zero(), t]), b.call("less_equal", &[t, b.one()])];
            for &k in &cs {
                let kt = cx.eval(h.call(h.sym("rewrite"), &[k, cx.list(&rules)]))?;
                let r = cx.eval_assuming(&seg, b.call("refine", &[kt]))?;
                if !cx.is_true(r) {
                    defined = false;
                    break;
                }
            }
        }
        if !defined {
            let along = cx.eval(h.call(h.sym("rewrite"), &[c.f, cx.list(&rules)]))?;
            let along = proof::with_ball_constants(cx, b, along)?;
            if out_of_time(cx) || proof::enclose(cx, b, along, t, 0.0, 1.0)?.is_none() {
                return Ok(None);
            }
        }
        proof::closed_sign(cx, b, b.add(&[fa, fb, b.mul(&[b.int(-2), fm])]))
    };
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
            if !((ym < chord - tol && !up) || (ym > chord + tol && !down)) {
                continue;
            }
            tries += 1;
            if tries > MAX_CONFIRM {
                return Ok(None);
            }
            match confirm(cx, p, q)? {
                Some(1) => up = true,
                Some(-1) => down = true,
                _ => {}
            }
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
    let mut stack = mc_api::walk::Walk::new(t);
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
    if let Some(fr) = realified(cx, b, c, x)? {
        let c2 = Call { f: fr, cond: c.cond, vars: c.vars.clone(), complex: c.complex, integers: c.integers, facts: c.facts.clone(), strict: c.strict };
        return injective(cx, b, &c2);
    }
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
            // a right inverse g of a function p (p(g(x)) = x wherever g is defined) is injective: g(x) = g(x') gives
            // x = p(g(x)) = p(g(x')) = x' (the inverse Weierstrass function is a solution w of p(w) = x)
            n.starts_with("inverse_jacobi_") || matches!(&*n, "inverse_elliptic_nome_q" | "inverse_erf" | "inverse_erfc" | "inverse_weierstrass_p")
        });
        if inverse && args.first() == Some(&x) && args[1..].iter().all(|&a| is_free(b, a, x)) && c.cond.is_none() {
            return Ok(Some(cx.bool(true)));
        }
    }
    // strictly monotonic: the derivative of one sign (zero at isolated points only); not for functions with jumps or
    // flat pieces (`floor`, `fractional_part`, `ramp`, ...), whose derivative says nothing about that
    let jumps = {
        let mut stack = mc_api::walk::Walk::new(c.f);
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
    // the domain's conditions (the points of a witness must lie in it)
    let mut conds = conditions(cx, b, c.f, &c.vars, false)?.unwrap_or_default();
    conds.extend(c.cond);
    // machine values over the search points (a guide: every conclusion below is confirmed at exact points)
    let mut vals: Vec<(f64, f64)> = vec![];
    for v in samples(c.integers).into_iter().filter(|v| v.abs() <= 50.0) {
        cx.tick(1)?;
        if !holds_machine(cx, b, &conds, x, v)? {
            continue;
        }
        if let Some(y) = value_at(cx, b, c.f, x, v)? {
            vals.push((v, y));
        }
    }
    let mut tries = 0;
    // two points with exactly the same value: not injective
    let mut order: Vec<usize> = (0..vals.len()).collect();
    order.sort_by(|&i, &j| vals[i].1.total_cmp(&vals[j].1));
    // pairs of machine zeros last, and only a few (an underflow on both tails is not a tie; `ramp` at -1, -2 is)
    let mut pairs: Vec<(usize, usize)> = order.windows(2).map(|w| (w[0], w[1])).collect();
    pairs.sort_by_key(|&(i, j)| vals[i].1 == 0.0 && vals[j].1 == 0.0);
    let mut zero_pairs = 0;
    for (i, j) in pairs {
        let ((p, yp), (q, yq)) = (vals[i], vals[j]);
        // a relative tolerance: tails decaying to 0 on both sides (`csch`, x exp(-x^2)) are not ties
        if (yp - yq).abs() > 1e-12 * yp.abs().max(yq.abs()) {
            continue;
        }
        if yp == 0.0 && yq == 0.0 {
            zero_pairs += 1;
            if zero_pairs > 4 {
                break;
            }
        }
        tries += 1;
        if tries > MAX_CONFIRM {
            break;
        }
        let (ep, eq) = (proof::exact_point(cx, b, p)?, proof::exact_point(cx, b, q)?);
        if !proof::holds_at(cx, b, &conds, x, ep)? || !proof::holds_at(cx, b, &conds, x, eq)? {
            continue;
        }
        let (fp, fq) = (proof::at(cx, b, c.f, x, ep)?, proof::at(cx, b, c.f, x, eq)?);
        // a real number (not two equal unevaluated or failed values)
        let w = cx.eval(b.call("n", &[fp, b.int(30)]))?;
        let real = proof::bounds(b, w).is_some() || proof::bounds(b, fp).is_some();
        if real && proof::closed_sign(cx, b, b.sub(fp, fq))? == Some(0) {
            return Ok(Some(cx.bool(false)));
        }
    }
    // a turning point, or two stretches whose values meet, where f is continuous (no step function, and no pole
    // inside): some value is taken twice (the intermediate value theorem); first, as only what the samples show is
    // confirmed (nothing to confirm costs little)
    if !c.integers && !range::has_jumps(b, c.f) && inject::two_stretches(cx, b, c, x, &conds, &vals)? {
        return Ok(Some(cx.bool(false)));
    }
    if !c.integers && !jumps {
        // strictly monotonic on the domain (read as the condition: without one, the domain is not the whole line)
        let dom: Vec<T<'h>> = conditions(cx, b, c.f, &c.vars, false)?.unwrap_or_default();
        let cd = inject::on_domain(cx, c, &dom)?;
        let m = sign_of(cx, b, cd.as_ref().unwrap_or(c), 1)?;
        if let Some(m) = m {
            match h.view(m) {
                View::Int(1 | -1) => return Ok(Some(cx.bool(true))),
                View::Int(0) => return Ok(Some(cx.bool(false))),
                _ => {}
            }
        }
        // the images of the pieces of the domain
        if !nonsmooth(b, c.f, x) {
            if let Some(v) = inject::by_pieces(cx, b, c.f, x, &conds)? {
                return Ok(Some(cx.bool(v)));
            }
        }
    }
    Ok(None)
}

/// `f` with `conjugate`, `re` and `im_part` of terms that are real for real x refined away (x is real); `None` when
/// f has none of them or nothing changes.
fn realified<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if c.complex || !parts_of_polynomials(b, c.f, x) {
        return Ok(None);
    }
    let r = cx.eval_assuming(&[h.call(h.sym("element"), &[x, h.sym_term(h.sym("reals"))])], b.call("refine", &[c.f]))?;
    if r == c.f || ["conjugate", "re", "im_part"].iter().any(|n| contains_call(b, r, n)) {
        return Ok(None);
    }
    Ok(Some(r))
}

/// Does `t` contain `conjugate`, `re` or `im_part` of x, all of them of polynomials in x?
fn parts_of_polynomials<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut found = false;
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        let View::App { head, args } = h.view(u) else { continue };
        if b.sym_of(head).is_some_and(|s| matches!(h.sym_name(s).as_str(), "conjugate" | "re" | "im_part")) && !is_free(b, u, x) {
            if args.len() != 1 || sets::poly_degree(b, args[0], &[x]).is_none() {
                return false;
            }
            found = true;
            continue;
        }
        stack.extend(args.iter().copied());
    }
    found
}

/// Does `t` contain a call of `name`?
fn contains_call<'h>(b: &Build<'h>, t: T<'h>, name: &str) -> bool {
    // shared subterms are decided once (a term can be a tree exponentially larger than itself)
    contains_call_in(b, t, name, &mut mc_api::walk::memo())
}

fn contains_call_in<'h>(b: &Build<'h>, t: T<'h>, name: &str, memo: &mut mc_api::walk::Memo<T<'h>, bool>) -> bool {
    mc_api::walk::cached(memo, t, |memo| {
        let h = b.heap();
        match h.view(t) {
            View::App { head, args } => b.sym_of(head) == Some(h.sym(name)) || args.iter().any(|&a| contains_call_in(b, a, name, memo)),
            _ => false,
        }
    })
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
