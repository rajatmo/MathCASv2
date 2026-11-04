//! The table of derivative rules of known functions, keyed by name and number of arguments.
//!
//! A row is `(name, arity, rule)`; [`ANY`] as the arity matches every call of that name not matched by a row with
//! its exact arity. Adding a function is adding a row: for a function of k arguments, [`Rule::Partials`] gives one
//! builder per argument, the partial derivative with respect to that argument as a term of the arguments; the chain
//! rule multiplies each by the derivative of its argument and sums. A name without a row is an unknown function and
//! differentiates to the operator `derivative(...)(f)` (decision 2A.3).
//!
//! Conventions (decision 2A.4): derivatives are those of functions of a real variable. Piecewise-constant functions
//! (`floor`, `sign`, `unit_step`, ...) have derivative 0 and piecewise functions the derivative of their pieces, both
//! valid away from the breakpoints (v2 has no distributions); `abs(u)` differentiates to `sign(u) u'`; `re`, `im_part`
//! and `conjugate` commute with differentiation (they are linear over the reals).
use crate::term::Build;
use mc_api::T;

/// The arity of a row that matches any number of arguments.
pub const ANY: usize = usize::MAX;

/// A partial derivative: the derivative of f(a1, ..., ak) with respect to its i-th argument, built from the
/// arguments.
pub type Partial = for<'b, 'h> fn(&'b Build<'h>, &[T<'h>]) -> T<'h>;

/// How a known function differentiates.
#[derive(Clone, Copy)]
pub enum Rule {
    /// one partial derivative per argument (chain rule)
    Partials(&'static [Partial]),
    /// constant almost everywhere: derivative 0
    Constant,
    /// linear: the derivative is the function applied to the derivatives of its arguments (lists, equations, `re`)
    Linear,
    /// a conditional `if(c, a, b)`: the condition is kept, the branches are differentiated
    Branches,
    /// not differentiable by this package: `d` stays unevaluated
    Unsupported,
}

/// `u^k`
fn pw<'h>(b: &Build<'h>, u: T<'h>, k: i64) -> T<'h> {
    b.powi(u, k)
}

/// `1 + s u^2` for s = ±1
fn one_plus_sq<'h>(b: &Build<'h>, u: T<'h>, s: i64) -> T<'h> {
    let sq = b.mul(&[b.int(s), pw(b, u, 2)]);
    b.add(&[b.one(), sq])
}

/// `if cmp(a0, a1) then 1 else 0` (`first`) or `if cmp(a0, a1) then 0 else 1`.
fn pick<'h>(b: &Build<'h>, cmp: &str, a: &[T<'h>], first: bool) -> T<'h> {
    let c = b.call(cmp, &[a[0], a[1]]);
    let (t, f) = if first { (b.one(), b.zero()) } else { (b.zero(), b.one()) };
    b.call("if", &[c, t, f])
}

/// The rows. Keep related functions together; see the module documentation for the conventions.
pub const RULES: &[(&str, usize, Rule)] = &[
    // exponential and logarithm
    ("exp", 1, Rule::Partials(&[|b, a| b.call("exp", &[a[0]])])),
    ("log", 1, Rule::Partials(&[|b, a| pw(b, a[0], -1)])),
    ("sqrt", 1, Rule::Partials(&[|b, a| b.mul(&[b.rat(1, 2), b.pow(a[0], b.rat(-1, 2))])])),
    // trigonometric
    ("sin", 1, Rule::Partials(&[|b, a| b.call("cos", &[a[0]])])),
    ("cos", 1, Rule::Partials(&[|b, a| b.neg(b.call("sin", &[a[0]]))])),
    ("tan", 1, Rule::Partials(&[|b, a| pw(b, b.call("cos", &[a[0]]), -2)])),
    // inverse trigonometric
    ("asin", 1, Rule::Partials(&[|b, a| b.pow(one_plus_sq(b, a[0], -1), b.rat(-1, 2))])),
    ("acos", 1, Rule::Partials(&[|b, a| b.neg(b.pow(one_plus_sq(b, a[0], -1), b.rat(-1, 2)))])),
    ("atan", 1, Rule::Partials(&[|b, a| pw(b, one_plus_sq(b, a[0], 1), -1)])),
    // hyperbolic
    ("sinh", 1, Rule::Partials(&[|b, a| b.call("cosh", &[a[0]])])),
    ("cosh", 1, Rule::Partials(&[|b, a| b.call("sinh", &[a[0]])])),
    ("tanh", 1, Rule::Partials(&[|b, a| pw(b, b.call("cosh", &[a[0]]), -2)])),
    // absolute value and the piecewise-constant functions (real variable, away from the jumps)
    ("abs", 1, Rule::Partials(&[|b, a| b.call("sign", &[a[0]])])),
    ("sign", 1, Rule::Constant),
    ("unit_step", ANY, Rule::Constant),
    ("arg", 1, Rule::Constant),
    ("floor", 1, Rule::Constant),
    ("ceiling", 1, Rule::Constant),
    ("round", 1, Rule::Constant),
    ("quotient", 2, Rule::Constant),
    // floor(x, a) = a floor(x/a), and likewise ceiling and round
    ("floor", 2, Rule::Partials(&[|b, _| b.zero(), |b, a| b.call("floor", &[b.div(a[0], a[1])])])),
    ("ceiling", 2, Rule::Partials(&[|b, _| b.zero(), |b, a| b.call("ceiling", &[b.div(a[0], a[1])])])),
    ("round", 2, Rule::Partials(&[|b, _| b.zero(), |b, a| b.call("round", &[b.div(a[0], a[1])])])),
    // mod(a, m) = a - m floor(a/m); mod(a, m, d) = a - m floor((a - d)/m)
    ("mod", 2, Rule::Partials(&[|b, _| b.one(), |b, a| b.neg(b.call("floor", &[b.div(a[0], a[1])]))])),
    (
        "mod",
        3,
        Rule::Partials(&[
            |b, _| b.one(),
            |b, a| b.neg(b.call("floor", &[b.div(b.sub(a[0], a[2]), a[1])])),
            |b, _| b.zero(),
        ]),
    ),
    // max(a, b) and min(a, b): the derivative of the larger (smaller) argument, away from ties
    ("max", 2, Rule::Partials(&[|b, a| pick(b, "greater_equal", a, true), |b, a| pick(b, "greater_equal", a, false)])),
    ("min", 2, Rule::Partials(&[|b, a| pick(b, "less_equal", a, true), |b, a| pick(b, "less_equal", a, false)])),
    // complex parts, linear over the reals
    ("re", 1, Rule::Linear),
    ("im_part", 1, Rule::Linear),
    ("conjugate", 1, Rule::Linear),
    // structure: lists and equations map, conditionals differentiate their branches
    ("list", ANY, Rule::Linear),
    ("equal", 2, Rule::Linear),
    ("if", 2, Rule::Branches),
    ("if", 3, Rule::Branches),
    // terms that are not functions of their arguments
    ("series_data", ANY, Rule::Unsupported),
    ("failure", ANY, Rule::Unsupported),
    ("fn", ANY, Rule::Unsupported),
    ("local_cell", ANY, Rule::Unsupported),
];

/// The rule for a call of `name` with `arity` arguments: the row with that exact arity, else an [`ANY`] row.
pub fn lookup(name: &str, arity: usize) -> Option<Rule> {
    let mut any = None;
    for &(n, k, r) in RULES {
        if n == name {
            if k == arity {
                return Some(r);
            }
            if k == ANY {
                any = Some(r);
            }
        }
    }
    any
}

/// The partial derivatives of a known function of `arity` arguments (the functions `derivative(n)(f)` resolves
/// for).
pub fn partials(name: &str, arity: usize) -> Option<&'static [Partial]> {
    match lookup(name, arity)? {
        Rule::Partials(ps) if ps.len() == arity => Some(ps),
        _ => None,
    }
}
