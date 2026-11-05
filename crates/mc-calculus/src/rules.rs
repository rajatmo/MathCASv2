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
use mc_api::{Store, T};

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

/// `derivative(1, 0)(f)(a0, a1)`: a partial derivative in an order argument, which has no closed form (see
/// [`OPAQUE`]).
fn order_partial<'h>(b: &Build<'h>, f: &str, a: &[T<'h>]) -> T<'h> {
    let f = b.sym(b.heap().sym(f));
    b.app(b.derivative(&[1, 0], f), a)
}

/// `f(a0 - 1, a1) + f(a0 + 1, a1)` (the Bessel recurrences).
fn bessel_sum<'h>(b: &Build<'h>, f: &str, a: &[T<'h>]) -> T<'h> {
    let lo = b.call(f, &[b.sub(a[0], b.one()), a[1]]);
    let hi = b.call(f, &[b.add(&[a[0], b.one()]), a[1]]);
    b.add(&[lo, hi])
}

/// d/dx beta(x, y) = beta(x, y) (digamma(x) - digamma(x + y)).
fn beta_partial<'h>(b: &Build<'h>, x: T<'h>, y: T<'h>) -> T<'h> {
    let d = b.sub(b.call("digamma", &[x]), b.call("digamma", &[b.add(&[x, y])]));
    b.mul(&[b.call("beta", &[x, y]), d])
}

/// `derivative(...)(f)(a)` in argument `i` (a partial derivative with no closed form, see [`OPAQUE`]).
fn opaque_partial<'h>(b: &Build<'h>, f: &str, a: &[T<'h>], i: usize) -> T<'h> {
    let mut orders = vec![0; a.len()];
    orders[i] = 1;
    let f = b.sym(b.heap().sym(f));
    b.app(b.derivative(&orders, f), a)
}

/// `pi`
fn pi<'h>(b: &Build<'h>) -> T<'h> {
    b.sym(b.heap().sym("pi"))
}

/// `2/sqrt(pi) e^(s u^2)` (the error-function derivatives).
fn gauss<'h>(b: &Build<'h>, u: T<'h>, s: i64) -> T<'h> {
    let e = b.call("exp", &[b.mul(&[b.int(s), pw(b, u, 2)])]);
    b.mul(&[b.int(2), b.pow(pi(b), b.rat(-1, 2)), e])
}

/// `s sqrt(pi)/2 e^(f(u)^2)` (the inverse error-function derivatives).
fn inverse_gauss<'h>(b: &Build<'h>, f: &str, u: T<'h>, s: i64) -> T<'h> {
    let y = b.call(f, &[u]);
    b.mul(&[b.rat(s, 2), b.pow(pi(b), b.rat(1, 2)), b.call("exp", &[pw(b, y, 2)])])
}

/// `f(u)/u` (the sine and cosine integral derivatives).
fn over<'h>(b: &Build<'h>, f: &str, u: T<'h>) -> T<'h> {
    b.div(b.call(f, &[u]), u)
}

/// `(1 - m sin(phi)^2)^(k/2)`.
fn delta<'h>(b: &Build<'h>, phi: T<'h>, m: T<'h>, k: i64) -> T<'h> {
    let s2 = pw(b, b.call("sin", &[phi]), 2);
    b.pow(b.sub(b.one(), b.mul(&[m, s2])), b.rat(k, 2))
}

/// `x^(a-1) (1-x)^(b-1)` (the incomplete beta integrand).
fn beta_density<'h>(b: &Build<'h>, x: T<'h>, p: T<'h>, q: T<'h>) -> T<'h> {
    let l = b.pow(x, b.sub(p, b.one()));
    let r = b.pow(b.sub(b.one(), x), b.sub(q, b.one()));
    b.mul(&[l, r])
}

/// `-x^(a-1) e^-x` (the x-derivative of gamma(a, x)).
fn gamma_density<'h>(b: &Build<'h>, a: T<'h>, x: T<'h>) -> T<'h> {
    b.neg(b.mul(&[b.pow(x, b.sub(a, b.one())), b.call("exp", &[b.neg(x)])]))
}

/// `(f(nu - 1, x) - f(nu + 1, x))/2` (the J and Y recurrences).
fn bessel_diff<'h>(b: &Build<'h>, f: &str, a: &[T<'h>]) -> T<'h> {
    let lo = b.call(f, &[b.sub(a[0], b.one()), a[1]]);
    let hi = b.call(f, &[b.add(&[a[0], b.one()]), a[1]]);
    b.mul(&[b.rat(1, 2), b.sub(lo, hi)])
}

/// d/dm F(phi | m) = E(phi|m)/(2m(1-m)) - F(phi|m)/(2m) - sin(2 phi)/(4 (1-m) sqrt(1 - m sin^2 phi)).
fn ell_f_m<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let (phi, m) = (a[0], a[1]);
    let om = b.sub(b.one(), m);
    let e = b.div(b.call("elliptic_e", &[phi, m]), b.mul(&[b.int(2), m, om]));
    let f = b.div(b.call("elliptic_f", &[phi, m]), b.mul(&[b.int(2), m]));
    let s = b.call("sin", &[b.mul(&[b.int(2), phi])]);
    let t = b.div(b.mul(&[s, delta(b, phi, m, -1)]), b.mul(&[b.int(4), om]));
    b.sub(b.sub(e, f), t)
}

/// d/dm K(m) = (E(m) - (1-m) K(m))/(2 m (1-m)).
fn ell_k_m<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let m = a[0];
    let om = b.sub(b.one(), m);
    let num = b.sub(b.call("elliptic_e", &[m]), b.mul(&[om, b.call("elliptic_k", &[m])]));
    b.div(num, b.mul(&[b.int(2), m, om]))
}

/// The z-derivative of 2F1: a b / c 2F1(a+1, b+1; c+1; z).
fn hyp2f1_z<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let up = |t| b.add(&[t, b.one()]);
    let f = b.call("hypergeometric_2f1", &[up(a[0]), up(a[1]), up(a[2]), a[3]]);
    b.mul(&[a[0], a[1], pw(b, a[2], -1), f])
}

/// The z-derivative of 1F1: a / b 1F1(a+1; b+1; z).
fn hyp1f1_z<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let up = |t| b.add(&[t, b.one()]);
    let f = b.call("hypergeometric_1f1", &[up(a[0]), up(a[1]), a[2]]);
    b.mul(&[a[0], pw(b, a[1], -1), f])
}

/// The derivative of inverse_jacobi_sn in v: ((1 - v^2)(1 - m v^2))^(-1/2).
fn ijsn_v<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let v2 = pw(b, a[0], 2);
    let p = b.mul(&[b.sub(b.one(), v2), b.sub(b.one(), b.mul(&[a[1], v2]))]);
    b.pow(p, b.rat(-1, 2))
}

/// The derivative of elliptic_pi(n, phi, m) in phi.
fn ell_pi_phi<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let s2 = pw(b, b.call("sin", &[a[1]]), 2);
    let den = b.sub(b.one(), b.mul(&[a[0], s2]));
    b.div(delta(b, a[1], a[2], -1), den)
}

/// Partial derivatives that are the operator itself (name, arity, argument): differentiating
/// `derivative(n...)(f)` in such an argument keeps the operator instead of resolving it again, which would not end.
pub const OPAQUE: &[(&str, usize, usize)] = &[
    ("polygamma", 2, 0),
    ("bessel_k", 2, 0),
    ("bessel_i", 2, 0),
    ("bessel_j", 2, 0),
    ("bessel_y", 2, 0),
    ("gamma", 2, 0),
    ("gamma_regularized", 2, 0),
    ("beta", 3, 1),
    ("beta", 3, 2),
    ("beta_regularized", 3, 1),
    ("beta_regularized", 3, 2),
    ("exp_integral_e", 2, 0),
    ("polylog", 2, 0),
    ("hypergeometric_2f1", 4, 0),
    ("hypergeometric_2f1", 4, 1),
    ("hypergeometric_2f1", 4, 2),
    ("hypergeometric_1f1", 3, 0),
    ("hypergeometric_1f1", 3, 1),
    ("hypergeometric_0f1", 2, 0),
    ("elliptic_pi", 2, 0),
    ("elliptic_pi", 2, 1),
    ("elliptic_pi", 3, 0),
    ("elliptic_pi", 3, 2),
    ("jacobi_sn", 2, 1),
    ("jacobi_cn", 2, 1),
    ("jacobi_dn", 2, 1),
    ("inverse_jacobi_sn", 2, 1),
];

/// Can `derivative(orders)(name)` be resolved into a term (a known function, no order in an [`OPAQUE`] argument)?
pub fn resolvable(name: &str, orders: &[i64]) -> bool {
    partials(name, orders.len()).is_some()
        && !orders.iter().enumerate().any(|(i, &n)| n > 0 && OPAQUE.contains(&(name, orders.len(), i)))
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
    // special functions (package SF); derivatives in the orders nu, n have no closed form: the operator stays
    ("gamma", 1, Rule::Partials(&[|b, a| b.mul(&[b.call("gamma", &[a[0]]), b.call("digamma", &[a[0]])])])),
    ("log_gamma", 1, Rule::Partials(&[|b, a| b.call("digamma", &[a[0]])])),
    ("digamma", 1, Rule::Partials(&[|b, a| b.call("polygamma", &[b.one(), a[0]])])),
    (
        "polygamma",
        2,
        Rule::Partials(&[|b, a| order_partial(b, "polygamma", a), |b, a| {
            b.call("polygamma", &[b.add(&[a[0], b.one()]), a[1]])
        }]),
    ),
    ("beta", 2, Rule::Partials(&[|b, a| beta_partial(b, a[0], a[1]), |b, a| beta_partial(b, a[1], a[0])])),
    (
        "bessel_k",
        2,
        Rule::Partials(&[|b, a| order_partial(b, "bessel_k", a), |b, a| {
            b.mul(&[b.rat(-1, 2), bessel_sum(b, "bessel_k", a)])
        }]),
    ),
    (
        "bessel_i",
        2,
        Rule::Partials(&[|b, a| order_partial(b, "bessel_i", a), |b, a| {
            b.mul(&[b.rat(1, 2), bessel_sum(b, "bessel_i", a)])
        }]),
    ),
    // package 3C: error functions, incomplete gamma and beta, exponential integrals, polylog, hypergeometric,
    // Bessel J/Y, Airy, elliptic integrals and Jacobi functions; parameter derivatives without a closed form stay
    // operators (OPAQUE)
    ("erf", 1, Rule::Partials(&[|b, a| gauss(b, a[0], -1)])),
    ("erfc", 1, Rule::Partials(&[|b, a| b.neg(gauss(b, a[0], -1))])),
    ("erfi", 1, Rule::Partials(&[|b, a| gauss(b, a[0], 1)])),
    ("inverse_erf", 1, Rule::Partials(&[|b, a| inverse_gauss(b, "inverse_erf", a[0], 1)])),
    ("inverse_erfc", 1, Rule::Partials(&[|b, a| inverse_gauss(b, "inverse_erfc", a[0], -1)])),
    ("gamma", 2, Rule::Partials(&[|b, a| opaque_partial(b, "gamma", a, 0), |b, a| gamma_density(b, a[0], a[1])])),
    (
        "gamma_regularized",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "gamma_regularized", a, 0), |b, a| {
            b.div(gamma_density(b, a[0], a[1]), b.call("gamma", &[a[0]]))
        }]),
    ),
    (
        "beta",
        3,
        Rule::Partials(&[
            |b, a| beta_density(b, a[0], a[1], a[2]),
            |b, a| opaque_partial(b, "beta", a, 1),
            |b, a| opaque_partial(b, "beta", a, 2),
        ]),
    ),
    (
        "beta_regularized",
        3,
        Rule::Partials(&[
            |b, a| b.div(beta_density(b, a[0], a[1], a[2]), b.call("beta", &[a[1], a[2]])),
            |b, a| opaque_partial(b, "beta_regularized", a, 1),
            |b, a| opaque_partial(b, "beta_regularized", a, 2),
        ]),
    ),
    ("exp_integral_ei", 1, Rule::Partials(&[|b, a| over(b, "exp", a[0])])),
    (
        "exp_integral_e",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "exp_integral_e", a, 0), |b, a| {
            b.neg(b.call("exp_integral_e", &[b.sub(a[0], b.one()), a[1]]))
        }]),
    ),
    ("log_integral", 1, Rule::Partials(&[|b, a| pw(b, b.call("log", &[a[0]]), -1)])),
    ("sin_integral", 1, Rule::Partials(&[|b, a| over(b, "sin", a[0])])),
    ("cos_integral", 1, Rule::Partials(&[|b, a| over(b, "cos", a[0])])),
    ("sinh_integral", 1, Rule::Partials(&[|b, a| over(b, "sinh", a[0])])),
    ("cosh_integral", 1, Rule::Partials(&[|b, a| over(b, "cosh", a[0])])),
    (
        "polylog",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "polylog", a, 0), |b, a| {
            b.div(b.call("polylog", &[b.sub(a[0], b.one()), a[1]]), a[1])
        }]),
    ),
    (
        "hypergeometric_2f1",
        4,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "hypergeometric_2f1", a, 0),
            |b, a| opaque_partial(b, "hypergeometric_2f1", a, 1),
            |b, a| opaque_partial(b, "hypergeometric_2f1", a, 2),
            hyp2f1_z,
        ]),
    ),
    (
        "hypergeometric_1f1",
        3,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "hypergeometric_1f1", a, 0),
            |b, a| opaque_partial(b, "hypergeometric_1f1", a, 1),
            hyp1f1_z,
        ]),
    ),
    (
        "hypergeometric_0f1",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "hypergeometric_0f1", a, 0), |b, a| {
            let f = b.call("hypergeometric_0f1", &[b.add(&[a[0], b.one()]), a[1]]);
            b.mul(&[pw(b, a[0], -1), f])
        }]),
    ),
    ("bessel_j", 2, Rule::Partials(&[|b, a| order_partial(b, "bessel_j", a), |b, a| bessel_diff(b, "bessel_j", a)])),
    ("bessel_y", 2, Rule::Partials(&[|b, a| order_partial(b, "bessel_y", a), |b, a| bessel_diff(b, "bessel_y", a)])),
    ("airy_ai", 1, Rule::Partials(&[|b, a| b.call("airy_ai_prime", &[a[0]])])),
    ("airy_bi", 1, Rule::Partials(&[|b, a| b.call("airy_bi_prime", &[a[0]])])),
    ("airy_ai_prime", 1, Rule::Partials(&[|b, a| b.mul(&[a[0], b.call("airy_ai", &[a[0]])])])),
    ("airy_bi_prime", 1, Rule::Partials(&[|b, a| b.mul(&[a[0], b.call("airy_bi", &[a[0]])])])),
    ("elliptic_k", 1, Rule::Partials(&[ell_k_m])),
    (
        "elliptic_e",
        1,
        Rule::Partials(&[|b, a| {
            let d = b.sub(b.call("elliptic_e", &[a[0]]), b.call("elliptic_k", &[a[0]]));
            b.div(d, b.mul(&[b.int(2), a[0]]))
        }]),
    ),
    (
        "elliptic_e",
        2,
        Rule::Partials(&[|b, a| delta(b, a[0], a[1], 1), |b, a| {
            let d = b.sub(b.call("elliptic_e", &[a[0], a[1]]), b.call("elliptic_f", &[a[0], a[1]]));
            b.div(d, b.mul(&[b.int(2), a[1]]))
        }]),
    ),
    ("elliptic_f", 2, Rule::Partials(&[|b, a| delta(b, a[0], a[1], -1), ell_f_m])),
    (
        "elliptic_pi",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "elliptic_pi", a, 0), |b, a| opaque_partial(b, "elliptic_pi", a, 1)]),
    ),
    (
        "elliptic_pi",
        3,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "elliptic_pi", a, 0),
            ell_pi_phi,
            |b, a| opaque_partial(b, "elliptic_pi", a, 2),
        ]),
    ),
    (
        "jacobi_sn",
        2,
        Rule::Partials(&[
            |b, a| b.mul(&[b.call("jacobi_cn", a), b.call("jacobi_dn", a)]),
            |b, a| opaque_partial(b, "jacobi_sn", a, 1),
        ]),
    ),
    (
        "jacobi_cn",
        2,
        Rule::Partials(&[
            |b, a| b.neg(b.mul(&[b.call("jacobi_sn", a), b.call("jacobi_dn", a)])),
            |b, a| opaque_partial(b, "jacobi_cn", a, 1),
        ]),
    ),
    (
        "jacobi_dn",
        2,
        Rule::Partials(&[
            |b, a| b.neg(b.mul(&[a[1], b.call("jacobi_sn", a), b.call("jacobi_cn", a)])),
            |b, a| opaque_partial(b, "jacobi_dn", a, 1),
        ]),
    ),
    ("inverse_jacobi_sn", 2, Rule::Partials(&[ijsn_v, |b, a| opaque_partial(b, "inverse_jacobi_sn", a, 1)])),
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
