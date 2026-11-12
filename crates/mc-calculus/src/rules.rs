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
use crate::common::args_of;
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
    /// `piecewise([[v1, c1], ...], default)`: the conditions are kept, the values and the default differentiated
    Piecewise,
    /// not differentiable by this crate: `d` stays unevaluated
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
pub(crate) fn opaque_partial<'h>(b: &Build<'h>, f: &str, a: &[T<'h>], i: usize) -> T<'h> {
    let mut orders = vec![0; a.len()];
    orders[i] = 1;
    let f = b.sym(b.heap().sym(f));
    b.app(b.derivative(&orders, f), a)
}

/// d/dz of a Struve function of order nu: (f_(nu-1) + s f_(nu+1) + (z/2)^nu/(sqrt(pi) Gamma(nu + 3/2)))/2 with s = -1
/// for H and +1 for L (DLMF 11.4.27, 11.4.28).
fn struve_z<'h>(b: &Build<'h>, a: &[T<'h>], f: &str, s: i64) -> T<'h> {
    let (nu, z) = (a[0], a[1]);
    let lo = b.call(f, &[b.sub(nu, b.one()), z]);
    let hi = b.call(f, &[b.add(&[nu, b.one()]), z]);
    let g = b.call("gamma", &[b.add(&[nu, b.rat(3, 2)])]);
    let extra = b.div(b.pow(b.mul(&[b.rat(1, 2), z]), nu), b.mul(&[b.pow(pi(b), b.rat(1, 2)), g]));
    b.mul(&[b.rat(1, 2), b.add(&[lo, b.mul(&[b.int(s), hi]), extra])])
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

/// d/dx of a Kelvin function (`which`: 0 ber, 1 bei, 2 ker, 3 kei) of order nu (`None`: order 0). From/// K_nu = ber_nu + i bei_nu = e^(nu pi i) J_nu(x e^(3 pi i/4)) and the recurrence of J (DLMF 10.65.19-22, checked/// against central differences): with A, B = (ber, bei)_(nu+1) - (ber, bei)_(nu-1) over 2,/// ber' = (A + B)/sqrt(2), bei' = (B - A)/sqrt(2); with P, Q = (ker, kei)_(nu-1) - (ker, kei)_(nu+1) over 2,/// ker' = -(P + Q)/sqrt(2), kei' = (P - Q)/sqrt(2). For order 0 the order -1 is minus the order 1 (ber, bei) or/// likewise for ker, kei.fn kelvin_x<'h>(b: &Build<'h>, which: u8, nu: Option<T<'h>>, x: T<'h>) -> T<'h> {    let names = ["kelvin_ber", "kelvin_bei", "kelvin_ker", "kelvin_kei"];    let (re, im) = if which < 2 { (names[0], names[1]) } else { (names[2], names[3]) };    let at = |f: &str, o: i64| match nu {        Some(n) => b.call(f, &[b.add(&[n, b.int(o)]), x]),        None if o == 0 => b.call(f, &[x]),        None => b.call(f, &[b.int(o), x]),    };    // the two half-differences of the orders around nu (order 0: the order -1 value is minus the order 1 value)    let diff = |f: &str, up: i64, dn: i64| match nu {        Some(_) => b.mul(&[b.rat(1, 2), b.sub(at(f, up), at(f, dn))]),        None => at(f, 1),    };    let s2 = b.pow(b.int(2), b.rat(-1, 2));    let (a, c) = if which < 2 { (diff(re, 1, -1), diff(im, 1, -1)) } else { (diff(re, -1, 1), diff(im, -1, 1)) };    let v = match which {        0 => b.add(&[a, c]),        1 => b.sub(c, a),        2 => b.neg(b.add(&[a, c])),        _ => b.sub(a, c),    };    b.mul(&[s2, v])}
/// d/dx of a Kelvin function (`which`: 0 ber, 1 bei, 2 ker, 3 kei) of order nu (`None`: order 0). From
/// K_nu = ber_nu + i bei_nu = e^(nu pi i) J_nu(x e^(3 pi i/4)) and the recurrence of J (DLMF 10.65.19-22, checked
/// against central differences): with A, B = (ber, bei)_(nu+1) - (ber, bei)_(nu-1) over 2,
/// ber' = (A + B)/sqrt(2), bei' = (B - A)/sqrt(2); with P, Q = (ker, kei)_(nu-1) - (ker, kei)_(nu+1) over 2,
/// ker' = -(P + Q)/sqrt(2), kei' = (P - Q)/sqrt(2). For order 0 the order -1 is minus the order 1, so A, B
/// (P, Q) are the order-1 functions themselves.
fn kelvin_x<'h>(b: &Build<'h>, which: u8, nu: Option<T<'h>>, x: T<'h>) -> T<'h> {
    let names = ["kelvin_ber", "kelvin_bei", "kelvin_ker", "kelvin_kei"];
    let (re, im) = if which < 2 { (names[0], names[1]) } else { (names[2], names[3]) };
    let at = |f: &str, o: i64| match nu {
        Some(n) => b.call(f, &[b.add(&[n, b.int(o)]), x]),
        None => b.call(f, &[b.int(o), x]),
    };
    // the half-difference of the orders o1 and o2 around nu (order 0: the order -1 value is minus the order 1 value)
    let diff = |f: &str, o1: i64, o2: i64| match nu {
        Some(_) => b.mul(&[b.rat(1, 2), b.sub(at(f, o1), at(f, o2))]),
        None => at(f, 1),
    };
    let s2 = b.pow(b.int(2), b.rat(-1, 2));
    let (a, c) = if which < 2 { (diff(re, 1, -1), diff(im, 1, -1)) } else { (diff(re, -1, 1), diff(im, -1, 1)) };
    // order 0, ker and kei: P = (ker_-1 - ker_1)/2 = -ker_1, so the half-difference above has the opposite sign
    let (a, c) = if nu.is_none() && which >= 2 { (b.neg(a), b.neg(c)) } else { (a, c) };
    let v = match which {
        0 => b.add(&[a, c]),
        1 => b.sub(c, a),
        2 => b.neg(b.add(&[a, c])),
        _ => b.sub(a, c),
    };
    b.mul(&[s2, v])
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

/// The z-derivative of pFq: (a1 ... ap)/(b1 ... bq) pFq(a + 1; b + 1; z) (DLMF 16.3.1); the opaque derivative when the
/// parameters are not written as two lists.
pub(crate) fn hyppfq_z<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let (Some(ps), Some(qs)) = (crate::common::args_of(b, a[0], b.hd.list), crate::common::args_of(b, a[1], b.hd.list)) else {
        return opaque_partial(b, "hypergeometric_pfq", a, 2);
    };
    let up = |ts: &[T<'h>]| b.call("list", &ts.iter().map(|&t| b.add(&[t, b.one()])).collect::<Vec<_>>());
    let f = b.call("hypergeometric_pfq", &[up(ps), up(qs), a[2]]);
    let num = b.mul(ps);
    let den = b.mul(qs);
    b.mul(&[num, pw(b, den, -1), f])
}

/// The z-derivative of the regularized pFq (pFq divided by gamma(b1) ... gamma(bq)): (a1 ... ap) times the regularized
/// pFq(a + 1; b + 1; z), since gamma(b + 1) = b gamma(b) absorbs the denominator of DLMF 16.3.1.
pub(crate) fn hyppfq_reg_z<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let name = "hypergeometric_pfq_regularized";
    let (Some(ps), Some(qs)) = (crate::common::args_of(b, a[0], b.hd.list), crate::common::args_of(b, a[1], b.hd.list)) else {
        return opaque_partial(b, name, a, 2);
    };
    let up = |ts: &[T<'h>]| b.call("list", &ts.iter().map(|&t| b.add(&[t, b.one()])).collect::<Vec<_>>());
    let f = b.call(name, &[up(ps), up(qs), a[2]]);
    b.mul(&[b.mul(ps), f])
}

/// The z-derivative of the bilateral series H(a; b; z) = sum over all integers k of prod (a)_k/prod (b)_k z^k: with
/// k (a1)_k = a1 ((a1 + 1)_k - (a1)_k), z H' = a1 (H(a1 + 1) - H); without upper parameters, k/(b1)_k =
/// (b1 - 1)(1/(b1 - 1)_k - 1/(b1)_k) gives z H' = (b1 - 1)(H(b1 - 1) - H) (the contiguous relations of DLMF 16.3.3-4 applied
/// termwise, valid for every k of the two-sided sum).
pub(crate) fn bilateral_z<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let name = "bilateral_hypergeometric_pfq";
    let (Some(ps), Some(qs)) = (crate::common::args_of(b, a[0], b.hd.list), crate::common::args_of(b, a[1], b.hd.list)) else {
        return opaque_partial(b, name, a, 2);
    };
    let h = b.call(name, a);
    let shifted = |ts: &[T<'h>], d: i64| {
        let mut v = ts.to_vec();
        v[0] = b.add(&[v[0], b.int(d)]);
        b.call("list", &v)
    };
    let (c, other) = if let Some(&p1) = ps.first() {
        (p1, b.call(name, &[shifted(ps, 1), a[1], a[2]]))
    } else if let Some(&q1) = qs.first() {
        (b.sub(q1, b.one()), b.call(name, &[a[0], shifted(qs, -1), a[2]]))
    } else {
        return opaque_partial(b, name, a, 2);
    };
    b.mul(&[c, b.sub(other, h), pw(b, a[2], -1)])
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

/// The Jacobi functions as calls: `s`, `c`, `d` (sn, cn, dn) and `n` (1).
fn jac<'h>(b: &Build<'h>, f: char, a: &[T<'h>]) -> T<'h> {
    match f {
        's' => b.call("jacobi_sn", a),
        'c' => b.call("jacobi_cn", a),
        'd' => b.call("jacobi_dn", a),
        _ => b.one(),
    }
}

/// The u-derivative of sn, cn, dn or 1.
fn jac_d<'h>(b: &Build<'h>, f: char, a: &[T<'h>]) -> T<'h> {
    match f {
        's' => b.mul(&[jac(b, 'c', a), jac(b, 'd', a)]),
        'c' => b.neg(b.mul(&[jac(b, 's', a), jac(b, 'd', a)])),
        'd' => b.neg(b.mul(&[a[1], jac(b, 's', a), jac(b, 'c', a)])),
        _ => b.zero(),
    }
}

/// The u-derivative of the Jacobi function `pq` = p/q (name `jacobi_pq`) by the quotient rule.
fn jac_ratio<'h>(b: &Build<'h>, name: &str, a: &[T<'h>]) -> T<'h> {
    let mut cs = name.trim_start_matches("jacobi_").chars();
    let (p, q) = (cs.next().unwrap_or('n'), cs.next().unwrap_or('n'));
    let (np, nq) = (jac(b, p, a), jac(b, q, a));
    let num = b.sub(b.mul(&[jac_d(b, p, a), nq]), b.mul(&[np, jac_d(b, q, a)]));
    b.div(num, pw(b, nq, 2))
}

/// d/dm of the Jacobi amplitude at fixed u: (m sn cn - dn (E - (1 - m) u))/(2 m (1 - m)) with E = `jacobi_epsilon(u, m)`
/// (Byrd and Friedman 710.00; checked against central differences, see the mc-special NOTES, round 36).
fn jam_m<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let (u, m) = (a[0], a[1]);
    let om = b.sub(b.one(), m);
    let e = b.call("jacobi_epsilon", &[u, m]);
    let num = b.sub(b.mul(&[m, jac(b, 's', a), jac(b, 'c', a)]), b.mul(&[jac(b, 'd', a), b.sub(e, b.mul(&[om, u]))]));
    b.div(num, b.mul(&[b.int(2), m, om]))
}

/// The m-derivative of sn, cn, dn or 1 at fixed u: cn am_m, -sn am_m, -(sn^2 + 2 m sn cn am_m)/(2 dn), 0.
fn jac_m<'h>(b: &Build<'h>, f: char, a: &[T<'h>]) -> T<'h> {
    let (sn, cn, dn) = (jac(b, 's', a), jac(b, 'c', a), jac(b, 'd', a));
    match f {
        's' => b.mul(&[cn, jam_m(b, a)]),
        'c' => b.neg(b.mul(&[sn, jam_m(b, a)])),
        'd' => {
            let t = b.add(&[pw(b, sn, 2), b.mul(&[b.int(2), a[1], sn, cn, jam_m(b, a)])]);
            b.neg(b.div(t, b.mul(&[b.int(2), dn])))
        }
        _ => b.zero(),
    }
}

/// The m-derivative of the Jacobi function `pq` = p/q (name `jacobi_pq`) by the quotient rule.
fn jac_ratio_m<'h>(b: &Build<'h>, name: &str, a: &[T<'h>]) -> T<'h> {
    let mut cs = name.trim_start_matches("jacobi_").chars();
    let (p, q) = (cs.next().unwrap_or('n'), cs.next().unwrap_or('n'));
    let (np, nq) = (jac(b, p, a), jac(b, q, a));
    let num = b.sub(b.mul(&[jac_m(b, p, a), nq]), b.mul(&[np, jac_m(b, q, a)]));
    b.div(num, pw(b, nq, 2))
}

/// d/dm of E(am(u|m) | m): (E - u)/(2m) + dn am_m.
fn jeps_m<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let (u, m) = (a[0], a[1]);
    let e = b.call("jacobi_epsilon", &[u, m]);
    b.add(&[b.div(b.sub(e, u), b.mul(&[b.int(2), m])), b.mul(&[jac(b, 'd', a), jam_m(b, a)])])
}

/// d/dm of the Jacobi zeta function Z = E(am u) - u E(m)/K(m).
fn jzn_m<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let (u, m) = (a[0], a[1]);
    let om = b.sub(b.one(), m);
    let (e, k) = (b.call("elliptic_e", &[m]), b.call("elliptic_k", &[m]));
    let em = b.div(b.sub(e, k), b.mul(&[b.int(2), m]));
    let km = b.div(b.sub(e, b.mul(&[om, k])), b.mul(&[b.int(2), m, om]));
    let ratio_m = b.div(b.sub(b.mul(&[em, k]), b.mul(&[e, km])), pw(b, k, 2));
    b.sub(jeps_m(b, a), b.mul(&[u, ratio_m]))
}

/// d/dv of the inverse Jacobi function with the forward function `name` (`jacobi_pq`): 1/pq'(u) at u = the inverse.
fn ij_v<'h>(b: &Build<'h>, inv: &str, name: &str, a: &[T<'h>]) -> T<'h> {
    let u = b.call(inv, a);
    pw(b, jac_ratio(b, name, &[u, a[1]]), -1)
}

/// d/dm of the inverse Jacobi function: -(pq)_m/(pq)_u at u = the inverse (implicit differentiation of pq(u, m) = v).
fn ij_m<'h>(b: &Build<'h>, inv: &str, name: &str, a: &[T<'h>]) -> T<'h> {
    let u = b.call(inv, a);
    let ua = [u, a[1]];
    b.neg(b.div(jac_ratio_m(b, name, &ua), jac_ratio(b, name, &ua)))
}

/// d/ds of `inverse_gamma_regularized`: x = Q^-1(a, s) has dx/ds = -Gamma(a) e^x x^(1 - a); `sgn` is the sign of the
/// three-argument form Q(a, z0) - Q(a, x) = s.
fn igamma_s<'h>(b: &Build<'h>, f: &str, a: &[T<'h>], sgn: i64) -> T<'h> {
    let x = b.call(f, a);
    b.mul(&[b.int(-sgn), b.call("gamma", &[a[0]]), b.call("exp", &[x]), b.pow(x, b.sub(b.one(), a[0]))])
}

/// d/ds of `inverse_beta_regularized(s, a, b)`: B(a, b) x^(1 - a) (1 - x)^(1 - b) at x = the inverse.
fn ibeta_s<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let x = b.call("inverse_beta_regularized", a);
    let l = b.pow(x, b.sub(b.one(), a[1]));
    let r = b.pow(b.sub(b.one(), x), b.sub(b.one(), a[2]));
    b.mul(&[b.call("beta", &[a[1], a[2]]), l, r])
}

/// d/dtau of the modular lambda function: 4 i lambda (1 - lambda) K(lambda)^2/pi (from dq/dm of the nome).
fn lambda_tau<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let l = b.call("modular_lambda", a);
    let k = b.call("elliptic_k", &[l]);
    let i = b.sym(b.heap().sym("im"));
    b.mul(&[b.int(4), i, l, b.sub(b.one(), l), pw(b, k, 2), pw(b, pi(b), -1)])
}

/// d/dtau of Klein's J = 4 (1 - l + l^2)^3/(27 l^2 (1 - l)^2), l = lambda(tau): J (3 (2l - 1)/(1 - l + l^2) - 2/l +
/// 2/(1 - l)) lambda'(tau).
fn klein_tau<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let l = b.call("modular_lambda", a);
    let q = b.add(&[b.one(), b.neg(l), pw(b, l, 2)]);
    let t1 = b.div(b.mul(&[b.int(3), b.sub(b.mul(&[b.int(2), l]), b.one())]), q);
    let t2 = b.mul(&[b.int(-2), pw(b, l, -1)]);
    let t3 = b.mul(&[b.int(2), pw(b, b.sub(b.one(), l), -1)]);
    b.mul(&[b.call("klein_invariant_j", a), b.add(&[t1, t2, t3]), lambda_tau(b, a)])
}

/// d/dk of the k-th zero of J_nu or of Y_nu as a function of the index: pi^2 x M_nu(x)^2/2 with M^2 = J^2 + Y^2, from
/// the modulus-phase form theta_nu'(x) = 2/(pi x M^2) (DLMF 10.18.1, 10.18.8, 10.21(iii)). At an integer index one of
/// the two terms vanishes, but not between (the row with Y^2 alone gave 1e-30 for d j_(2, k)/dk at k = 7/2, true
/// 3.1756).
fn bessel_zero_k<'h>(b: &Build<'h>, zero: &str, _other: &str, a: &[T<'h>]) -> T<'h> {
    let x = b.call(zero, a);
    let m2 = b.add(&[pw(b, b.call("bessel_j", &[a[0], x]), 2), pw(b, b.call("bessel_y", &[a[0], x]), 2)]);
    b.mul(&[b.rat(1, 2), pw(b, pi(b), 2), x, m2])
}

/// `f(n - 1, x) - (n + 1)/x f(n, x)` (the spherical Bessel recurrence).
fn spherical_d<'h>(b: &Build<'h>, f: &str, a: &[T<'h>]) -> T<'h> {
    let lo = b.call(f, &[b.sub(a[0], b.one()), a[1]]);
    let k = b.div(b.add(&[a[0], b.one()]), a[1]);
    b.sub(lo, b.mul(&[k, b.call(f, a)]))
}

/// `sin(pi x^2/2)` or `cos(pi x^2/2)` (the Fresnel integrands).
fn fresnel<'h>(b: &Build<'h>, f: &str, x: T<'h>) -> T<'h> {
    let pi = b.sym(b.heap().sym("pi"));
    b.call(f, &[b.mul(&[b.rat(1, 2), pi, pw(b, x, 2)])])
}

/// `g2` of the invariants `[g2, g3]` of a Weierstrass function (`None` when they are not written as a list).
fn weier_g2<'h>(b: &Build<'h>, g: T<'h>) -> Option<T<'h>> {
    crate::common::args_of(b, g, b.hd.list).filter(|gs| gs.len() == 2).map(|gs| gs[0])
}

/// d/du p'(u) = 6 p(u)^2 - g2/2 (from p'^2 = 4 p^3 - g2 p - g3, DLMF 23.3.10).
fn weier_pp_u<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let Some(g2) = weier_g2(b, a[1]) else { return opaque_partial(b, "weierstrass_p_prime", a, 0) };
    let p = b.call("weierstrass_p", a);
    b.sub(b.mul(&[b.int(6), pw(b, p, 2)]), b.mul(&[b.rat(1, 2), g2]))
}

/// d/drho of a Coulomb function u_l (F, G, H+ or H-): S_(l+1) u_l - R_(l+1) u_(l+1) with S_k = k/rho + eta/k and
/// R_k = sqrt(1 + eta^2/k^2) (DLMF 33.4.4, valid for every l >= 0 and for all four functions).
fn coulomb_rho<'h>(b: &Build<'h>, f: &str, a: &[T<'h>]) -> T<'h> {
    let (l, eta, rho) = (a[0], a[1], a[2]);
    let k = b.add(&[l, b.one()]);
    let s = b.add(&[b.div(k, rho), b.div(eta, k)]);
    let r = b.pow(b.add(&[b.one(), b.div(pw(b, eta, 2), pw(b, k, 2))]), b.rat(1, 2));
    let hi = b.call(f, &[k, eta, rho]);
    b.sub(b.mul(&[s, b.call(f, a)]), b.mul(&[r, hi]))
}

/// d/dz of a Lommel function f_(mu, nu)(z) (s1, s2, t1, t2): (mu + nu - 1) f_(mu-1, nu-1)(z) - nu/z f_(mu, nu)(z)
/// (DLMF 11.9.ii for s and S; the same term-by-term computation holds for the modified series t, and T differs from
/// t by multiples of I_nu, K_nu, whose recurrences match; checked against central differences, see NOTES.md).
fn lommel_z<'h>(b: &Build<'h>, f: &str, a: &[T<'h>]) -> T<'h> {
    let (mu, nu, z) = (a[0], a[1], a[2]);
    // on mu + nu = 1 the lower function has a pole: use the companion (mu - nu - 1) f_(mu-1, nu+1) + nu/z f (the
    // functions are even in nu)
    let (c, s, nu1) = match b.sub(b.add(&[mu, nu]), b.one()) {
        c if !b.is_zero(c) => (c, -1, b.sub(nu, b.one())),
        _ => (b.sub(b.sub(mu, nu), b.one()), 1, b.add(&[nu, b.one()])),
    };
    if b.is_zero(c) {
        // mu = 1, nu = 0: s_(1,0) = 1 - J_0 and t_(1,0) = I_0 - 1 (the series); S and T stay opaque
        return match f {
            "lommel_s1" => b.call("bessel_j", &[b.one(), z]),
            "lommel_t1" => b.call("bessel_i", &[b.one(), z]),
            _ => opaque_partial(b, f, a, 2),
        };
    }
    let lo = b.call(f, &[b.sub(mu, b.one()), nu1, z]);
    b.add(&[b.mul(&[c, lo]), b.mul(&[b.int(s), b.div(nu, z), b.call(f, a)])])
}

/// The x- or y-derivative of an Appell function (DLMF 16.13): the coefficient times the function with the named
/// parameters raised by one. `up` lists the parameter positions raised, `coef` the positions multiplied and `den`
/// the position divided by.
fn appell_d<'h>(b: &Build<'h>, f: &str, a: &[T<'h>], up: &[usize], coef: &[usize], den: usize) -> T<'h> {
    let mut args = a.to_vec();
    for &i in up {
        args[i] = b.add(&[a[i], b.one()]);
    }
    let mut fs: Vec<T<'h>> = coef.iter().map(|&i| a[i]).collect();
    fs.push(pw(b, a[den], -1));
    fs.push(b.call(f, &args));
    b.mul(&fs)
}

/// Partial derivatives that are the operator itself (name, arity, argument): differentiating
/// `derivative(n...)(f)` in such an argument keeps the operator instead of resolving it again, which would not end.
pub const OPAQUE: &[(&str, usize, usize)] = &[
    ("meijer_g", 3, 0),
    ("meijer_g", 3, 1),
    ("fox_h", 3, 0),
    ("fox_h", 3, 1),
    ("spheroidal_s1", 4, 0),
    ("spheroidal_s1", 4, 1),
    ("spheroidal_s1", 4, 2),
    ("spheroidal_s1_prime", 4, 0),
    ("spheroidal_s1_prime", 4, 1),
    ("spheroidal_s1_prime", 4, 2),
    ("chebyshev_t", 2, 0),
    ("chebyshev_u", 2, 0),
    ("legendre_p", 2, 0),
    ("hermite_h", 2, 0),
    ("laguerre_l", 2, 0),
    ("laguerre_l", 3, 0),
    ("laguerre_l", 3, 1),
    ("legendre_q", 2, 0),
    ("hypergeometric_0f1_regularized", 2, 0),
    ("hypergeometric_1f1_regularized", 3, 0),
    ("hypergeometric_1f1_regularized", 3, 1),
    ("hypergeometric_2f1_regularized", 4, 0),
    ("hypergeometric_2f1_regularized", 4, 1),
    ("hypergeometric_2f1_regularized", 4, 2),
    ("gegenbauer_c", 3, 0),
    ("gegenbauer_c", 3, 1),
    ("spherical_bessel_j", 2, 0),
    ("spherical_bessel_y", 2, 0),
    ("hankel_h1", 2, 0),
    ("inverse_gamma_regularized", 2, 0),
    ("inverse_gamma_regularized", 3, 0),
    ("inverse_gamma_regularized", 3, 1),
    ("inverse_beta_regularized", 3, 1),
    ("inverse_beta_regularized", 3, 2),
    ("bessel_j_zero", 2, 0),
    ("bessel_y_zero", 2, 0),
    ("spherical_hankel_h1", 2, 0),
    ("spherical_hankel_h2", 2, 0),
    ("hankel_h2", 2, 0),
    ("kelvin_ber", 2, 0),
    ("kelvin_bei", 2, 0),
    ("kelvin_ker", 2, 0),
    ("kelvin_kei", 2, 0),
    ("surd", 2, 1),
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
    ("bessel_k_scaled", 2, 0),
    ("bessel_i_scaled", 2, 0),
    ("struve_h", 2, 0),
    ("struve_l", 2, 0),
    ("anger_j", 2, 0),
    ("weber_e", 2, 0),
    ("hypergeometric_u", 3, 0),
    ("hypergeometric_u", 3, 1),
    ("weierstrass_p", 2, 1),
    ("weierstrass_p_prime", 2, 1),
    ("weierstrass_zeta", 2, 1),
    ("weierstrass_sigma", 2, 1),
    ("inverse_weierstrass_p", 2, 1),
    ("elliptic_exp", 2, 1),
    ("coulomb_f", 3, 0),
    ("coulomb_f", 3, 1),
    ("coulomb_g", 3, 0),
    ("coulomb_g", 3, 1),
    ("coulomb_h1", 3, 0),
    ("coulomb_h1", 3, 1),
    ("coulomb_h2", 3, 0),
    ("coulomb_h2", 3, 1),
    ("lommel_s1", 3, 0),
    ("lommel_s1", 3, 1),
    ("lommel_s2", 3, 0),
    ("lommel_s2", 3, 1),
    ("lommel_t1", 3, 0),
    ("lommel_t1", 3, 1),
    ("lommel_t2", 3, 0),
    ("lommel_t2", 3, 1),
    ("mathieu_s", 3, 0),
    ("mathieu_s", 3, 1),
    ("mathieu_s_prime", 3, 0),
    ("mathieu_s_prime", 3, 1),
    ("mathieu_c", 3, 0),
    ("mathieu_c", 3, 1),
    ("mathieu_c_prime", 3, 0),
    ("mathieu_c_prime", 3, 1),
    ("appell_f1", 6, 0),
    ("appell_f1", 6, 1),
    ("appell_f1", 6, 2),
    ("appell_f1", 6, 3),
    ("appell_f2", 7, 0),
    ("appell_f2", 7, 1),
    ("appell_f2", 7, 2),
    ("appell_f2", 7, 3),
    ("appell_f2", 7, 4),
    ("appell_f3", 7, 0),
    ("appell_f3", 7, 1),
    ("appell_f3", 7, 2),
    ("appell_f3", 7, 3),
    ("appell_f3", 7, 4),
    ("appell_f4", 6, 0),
    ("appell_f4", 6, 1),
    ("appell_f4", 6, 2),
    ("appell_f4", 6, 3),
];

/// Can `derivative(orders)(name)` be resolved into a term (a known function, no order in an [`OPAQUE`] argument)?
pub fn resolvable(name: &str, orders: &[i64]) -> bool {
    partials(name, orders.len()).is_some()
        && !orders.iter().enumerate().any(|(i, &n)| n > 0 && OPAQUE.contains(&(name, orders.len(), i)))
}

/// (f(nu - 1, z) - f(nu + 1, z))/2, the derivative in z of the Anger and Weber functions (DLMF 11.10.33-34).
fn order_step<'h>(b: &Build<'h>, f: &str, a: &[T<'h>]) -> T<'h> {
    let lo = b.call(f, &[b.sub(a[0], b.one()), a[1]]);
    let hi = b.call(f, &[b.add(&[a[0], b.one()]), a[1]]);
    b.mul(&[b.rat(1, 2), b.sub(lo, hi)])
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
    // atan(x, y) = arg(x + i y): d/dx = -y/(x^2 + y^2), d/dy = x/(x^2 + y^2)
    (
        "atan",
        2,
        Rule::Partials(&[
            |b, a| b.neg(b.div(a[1], b.add(&[b.powi(a[0], 2), b.powi(a[1], 2)]))),
            |b, a| b.div(a[0], b.add(&[b.powi(a[0], 2), b.powi(a[1], 2)])),
        ]),
    ),
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
    // special functions; derivatives in the orders nu, n have no closed form: the operator stays
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
    // item 3C: error functions, incomplete gamma and beta, exponential integrals, polylog, hypergeometric,
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
    // round 19: Struve functions (DLMF 11.4.27, 11.4.28), Tricomi's U (DLMF 13.3.22), the logistic
    // function
    ("spheroidal_s1", 4, Rule::Partials(&[|b, a| opaque_partial(b, "spheroidal_s1", a, 0), |b, a| opaque_partial(b, "spheroidal_s1", a, 1), |b, a| opaque_partial(b, "spheroidal_s1", a, 2), |b, a| b.call("spheroidal_s1_prime", a)])),
    ("spheroidal_s1_prime", 4, Rule::Partials(&[|b, a| opaque_partial(b, "spheroidal_s1_prime", a, 0), |b, a| opaque_partial(b, "spheroidal_s1_prime", a, 1), |b, a| opaque_partial(b, "spheroidal_s1_prime", a, 2), spheroidal_s1_second])),
    ("fox_h", 3, Rule::Partials(&[|b, a| opaque_partial(b, "fox_h", a, 0), |b, a| opaque_partial(b, "fox_h", a, 1), fox_dz])),
    ("meijer_g", 3, Rule::Partials(&[|b, a| opaque_partial(b, "meijer_g", a, 0), |b, a| opaque_partial(b, "meijer_g", a, 1), meijer_dz])),
    ("struve_h", 2, Rule::Partials(&[|b, a| opaque_partial(b, "struve_h", a, 0), |b, a| struve_z(b, a, "struve_h", -1)])),
    ("struve_l", 2, Rule::Partials(&[|b, a| opaque_partial(b, "struve_l", a, 0), |b, a| struve_z(b, a, "struve_l", 1)])),
    // Anger and Weber functions: 2 J'_nu(z) = J_(nu-1)(z) - J_(nu+1)(z), the same for E_nu (DLMF 11.10.33-34)
    ("anger_j", 2, Rule::Partials(&[|b, a| opaque_partial(b, "anger_j", a, 0), |b, a| order_step(b, "anger_j", a)])),
    ("weber_e", 2, Rule::Partials(&[|b, a| opaque_partial(b, "weber_e", a, 0), |b, a| order_step(b, "weber_e", a)])),
    (
        "hypergeometric_u",
        3,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "hypergeometric_u", a, 0),
            |b, a| opaque_partial(b, "hypergeometric_u", a, 1),
            |b, a| {
                let u = b.call("hypergeometric_u", &[b.add(&[a[0], b.one()]), b.add(&[a[1], b.one()]), a[2]]);
                b.mul(&[b.neg(a[0]), u])
            },
        ]),
    ),
    // the Jacobi amplitude, epsilon and zeta functions in u: am' = dn, E(am u)' = dn^2, Z' = dn^2 - E(m)/K(m)
    // (DLMF 22.16.1, 22.16.14, 22.16.32); the parameter derivatives are jam_m, jeps_m and jzn_m
    (
        "jacobi_amplitude",
        2,
        Rule::Partials(&[|b, a| b.call("jacobi_dn", &[a[0], a[1]]), jam_m]),
    ),
    (
        "jacobi_epsilon",
        2,
        Rule::Partials(&[|b, a| b.powi(b.call("jacobi_dn", &[a[0], a[1]]), 2), jeps_m]),
    ),
    (
        "jacobi_zn",
        2,
        Rule::Partials(&[
            |b, a| {
                let dn2 = b.powi(b.call("jacobi_dn", &[a[0], a[1]]), 2);
                b.sub(dn2, b.div(b.call("elliptic_e", &[a[1]]), b.call("elliptic_k", &[a[1]])))
            },
            jzn_m,
        ]),
    ),
    // the Fresnel auxiliary functions: f' = -pi z g, g' = pi z f - 1 (DLMF 7.10.1, 7.10.2)
    ("fresnel_f", 1, Rule::Partials(&[|b, a| b.neg(b.mul(&[pi(b), a[0], b.call("fresnel_g", &[a[0]])]))])),
    ("fresnel_g", 1, Rule::Partials(&[|b, a| b.sub(b.mul(&[pi(b), a[0], b.call("fresnel_f", &[a[0]])]), b.one())])),
    (
        "logistic_sigmoid",
        1,
        Rule::Partials(&[|b, a| {
            let s = b.call("logistic_sigmoid", &[a[0]]);
            b.mul(&[s, b.sub(b.one(), s)])
        }]),
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
    // the Kelvin functions: d/dx of the order-0 forms (DLMF 10.65.19-22: ber' = (ber_1 + bei_1)/sqrt(2), ...) and the
    // recurrences for order nu; the order itself is opaque
    ("kelvin_ber", 1, Rule::Partials(&[|b, a| kelvin_x(b, 0, None, a[0])])),
    ("kelvin_bei", 1, Rule::Partials(&[|b, a| kelvin_x(b, 1, None, a[0])])),
    ("kelvin_ker", 1, Rule::Partials(&[|b, a| kelvin_x(b, 2, None, a[0])])),
    ("kelvin_kei", 1, Rule::Partials(&[|b, a| kelvin_x(b, 3, None, a[0])])),
    ("kelvin_ber", 2, Rule::Partials(&[|b, a| opaque_partial(b, "kelvin_ber", a, 0), |b, a| kelvin_x(b, 0, Some(a[0]), a[1])])),
    ("kelvin_bei", 2, Rule::Partials(&[|b, a| opaque_partial(b, "kelvin_bei", a, 0), |b, a| kelvin_x(b, 1, Some(a[0]), a[1])])),
    ("kelvin_ker", 2, Rule::Partials(&[|b, a| opaque_partial(b, "kelvin_ker", a, 0), |b, a| kelvin_x(b, 2, Some(a[0]), a[1])])),
    ("kelvin_kei", 2, Rule::Partials(&[|b, a| opaque_partial(b, "kelvin_kei", a, 0), |b, a| kelvin_x(b, 3, Some(a[0]), a[1])])),
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
            |b, a| jac_m(b, 's', a),
        ]),
    ),
    (
        "jacobi_cn",
        2,
        Rule::Partials(&[
            |b, a| b.neg(b.mul(&[b.call("jacobi_sn", a), b.call("jacobi_dn", a)])),
            |b, a| jac_m(b, 'c', a),
        ]),
    ),
    (
        "jacobi_dn",
        2,
        Rule::Partials(&[
            |b, a| b.neg(b.mul(&[a[1], b.call("jacobi_sn", a), b.call("jacobi_cn", a)])),
            |b, a| jac_m(b, 'd', a),
        ]),
    ),
    ("inverse_jacobi_sn", 2, Rule::Partials(&[ijsn_v, |b, a| ij_m(b, "inverse_jacobi_sn", "jacobi_sn", a)])),
    ("inverse_jacobi_cn", 2, Rule::Partials(&[|b, a| ij_v(b, "inverse_jacobi_cn", "jacobi_cn", a), |b, a| ij_m(b, "inverse_jacobi_cn", "jacobi_cn", a)])),
    ("inverse_jacobi_dn", 2, Rule::Partials(&[|b, a| ij_v(b, "inverse_jacobi_dn", "jacobi_dn", a), |b, a| ij_m(b, "inverse_jacobi_dn", "jacobi_dn", a)])),
    ("inverse_jacobi_cd", 2, Rule::Partials(&[|b, a| ij_v(b, "inverse_jacobi_cd", "jacobi_cd", a), |b, a| ij_m(b, "inverse_jacobi_cd", "jacobi_cd", a)])),
    ("inverse_jacobi_cs", 2, Rule::Partials(&[|b, a| ij_v(b, "inverse_jacobi_cs", "jacobi_cs", a), |b, a| ij_m(b, "inverse_jacobi_cs", "jacobi_cs", a)])),
    ("inverse_jacobi_dc", 2, Rule::Partials(&[|b, a| ij_v(b, "inverse_jacobi_dc", "jacobi_dc", a), |b, a| ij_m(b, "inverse_jacobi_dc", "jacobi_dc", a)])),
    ("inverse_jacobi_ds", 2, Rule::Partials(&[|b, a| ij_v(b, "inverse_jacobi_ds", "jacobi_ds", a), |b, a| ij_m(b, "inverse_jacobi_ds", "jacobi_ds", a)])),
    ("inverse_jacobi_nc", 2, Rule::Partials(&[|b, a| ij_v(b, "inverse_jacobi_nc", "jacobi_nc", a), |b, a| ij_m(b, "inverse_jacobi_nc", "jacobi_nc", a)])),
    ("inverse_jacobi_nd", 2, Rule::Partials(&[|b, a| ij_v(b, "inverse_jacobi_nd", "jacobi_nd", a), |b, a| ij_m(b, "inverse_jacobi_nd", "jacobi_nd", a)])),
    ("inverse_jacobi_ns", 2, Rule::Partials(&[|b, a| ij_v(b, "inverse_jacobi_ns", "jacobi_ns", a), |b, a| ij_m(b, "inverse_jacobi_ns", "jacobi_ns", a)])),
    ("inverse_jacobi_sc", 2, Rule::Partials(&[|b, a| ij_v(b, "inverse_jacobi_sc", "jacobi_sc", a), |b, a| ij_m(b, "inverse_jacobi_sc", "jacobi_sc", a)])),
    ("inverse_jacobi_sd", 2, Rule::Partials(&[|b, a| ij_v(b, "inverse_jacobi_sd", "jacobi_sd", a), |b, a| ij_m(b, "inverse_jacobi_sd", "jacobi_sd", a)])),

    // functions defined by formulas: reciprocal and inverse trigonometric and hyperbolic functions
    ("sec", 1, Rule::Partials(&[|b, a| b.mul(&[b.call("sec", a), b.call("tan", a)])])),
    ("csc", 1, Rule::Partials(&[|b, a| b.neg(b.mul(&[b.call("csc", a), b.call("cot", a)]))])),
    ("cot", 1, Rule::Partials(&[|b, a| b.neg(pw(b, b.call("csc", a), 2))])),
    ("sech", 1, Rule::Partials(&[|b, a| b.neg(b.mul(&[b.call("sech", a), b.call("tanh", a)]))])),
    ("csch", 1, Rule::Partials(&[|b, a| b.neg(b.mul(&[b.call("csch", a), b.call("coth", a)]))])),
    ("coth", 1, Rule::Partials(&[|b, a| b.neg(pw(b, b.call("csch", a), 2))])),
    ("asinh", 1, Rule::Partials(&[|b, a| b.pow(one_plus_sq(b, a[0], 1), b.rat(-1, 2))])),
    (
        "acosh",
        1,
        Rule::Partials(&[|b, a| {
            let h = b.rat(-1, 2);
            b.mul(&[b.pow(b.add(&[a[0], b.int(-1)]), h), b.pow(b.add(&[a[0], b.one()]), h)])
        }]),
    ),
    ("atanh", 1, Rule::Partials(&[|b, a| pw(b, one_plus_sq(b, a[0], -1), -1)])),
    ("acoth", 1, Rule::Partials(&[|b, a| pw(b, one_plus_sq(b, a[0], -1), -1)])),
    ("acot", 1, Rule::Partials(&[|b, a| b.neg(pw(b, one_plus_sq(b, a[0], 1), -1))])),
    ("asec", 1, Rule::Partials(&[|b, a| b.div(b.pow(b.sub(b.one(), pw(b, a[0], -2)), b.rat(-1, 2)), pw(b, a[0], 2))])),
    ("acsc", 1, Rule::Partials(&[|b, a| b.neg(b.div(b.pow(b.sub(b.one(), pw(b, a[0], -2)), b.rat(-1, 2)), pw(b, a[0], 2)))])),
    (
        "asech",
        1,
        Rule::Partials(&[|b, a| {
            let r = b.div(b.sub(b.one(), a[0]), b.add(&[b.one(), a[0]]));
            b.neg(b.div(b.pow(r, b.rat(-1, 2)), b.mul(&[a[0], b.add(&[b.one(), a[0]])])))
        }]),
    ),
    ("acsch", 1, Rule::Partials(&[|b, a| b.neg(b.div(b.pow(b.add(&[b.one(), pw(b, a[0], -2)]), b.rat(-1, 2)), pw(b, a[0], 2)))])),
    ("gudermannian", 1, Rule::Partials(&[|b, a| b.call("sech", a)])),
    ("sinc", 1, Rule::Partials(&[|b, a| b.sub(b.div(b.call("cos", a), a[0]), b.div(b.call("sin", a), pw(b, a[0], 2)))])),
    (
        "product_log",
        1,
        Rule::Partials(&[|b, a| {
            let w = b.call("product_log", a);
            b.div(w, b.mul(&[a[0], b.add(&[b.one(), w])]))
        }]),
    ),
    ("dawson_f", 1, Rule::Partials(&[|b, a| b.sub(b.one(), b.mul(&[b.int(2), a[0], b.call("dawson_f", a)]))])),
    ("fresnel_s", 1, Rule::Partials(&[|b, a| fresnel(b, "sin", a[0])])),
    ("fresnel_c", 1, Rule::Partials(&[|b, a| fresnel(b, "cos", a[0])])),
    ("harmonic_number", 1, Rule::Partials(&[|b, a| b.call("polygamma", &[b.one(), b.add(&[a[0], b.one()])])])),
    (
        "surd",
        2,
        Rule::Partials(&[|b, a| b.div(b.pow(b.call("surd", a), b.sub(b.one(), a[1])), a[1]), |b, a| opaque_partial(b, "surd", a, 1)]),
    ),
    ("ramp", 1, Rule::Partials(&[|b, a| b.call("unit_step", a)])),
    ("real_abs", 1, Rule::Partials(&[|b, a| b.call("real_sign", a)])),
    ("log10", 1, Rule::Partials(&[|b, a| pw(b, b.mul(&[a[0], b.call("log", &[b.int(10)])]), -1)])),
    ("log2", 1, Rule::Partials(&[|b, a| pw(b, b.mul(&[a[0], b.call("log", &[b.int(2)])]), -1)])),
    ("inverse_gudermannian", 1, Rule::Partials(&[|b, a| b.call("sec", &[a[0]])])),
    ("heaviside_theta", 1, Rule::Partials(&[|b, a| b.call("dirac_delta", &[a[0]])])),
    ("real_sign", 1, Rule::Constant),
    ("unit_box", ANY, Rule::Constant),
    // round 28: the Heaviside shapes and the unit-period waves (lib/calculus.mc)
    (
        "heaviside_pi",
        1,
        Rule::Partials(&[|b, a| {
            let two_u = b.mul(&[b.int(2), a[0]]);
            let up = b.call("dirac_delta", &[b.add(&[b.one(), two_u])]);
            let down = b.call("dirac_delta", &[b.sub(two_u, b.one())]);
            b.sub(b.mul(&[b.int(2), up]), b.mul(&[b.int(2), down]))
        }]),
    ),
    (
        "heaviside_lambda",
        1,
        Rule::Partials(&[|b, a| {
            let l = b.call("heaviside_theta", &[b.add(&[a[0], b.one()])]);
            let m = b.mul(&[b.int(-2), b.call("heaviside_theta", &[a[0]])]);
            let r = b.call("heaviside_theta", &[b.sub(a[0], b.one())]);
            b.add(&[l, m, r])
        }]),
    ),
    ("sawtooth_wave", 1, Rule::Partials(&[|b, _| b.one()])),
    ("square_wave", 1, Rule::Constant),
    ("triangle_wave", 1, Rule::Partials(&[|b, a| b.mul(&[b.int(4), b.call("square_wave", &[b.add(&[a[0], b.rat(1, 4)])])])])),
    ("integer_part", 1, Rule::Constant),
    ("fractional_part", 1, Rule::Partials(&[|b, _| b.one()])),
    // the classical orthogonal polynomials in their argument (the degree derivatives stay operators)
    (
        "chebyshev_t",
        2,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "chebyshev_t", a, 0),
            |b, a| b.mul(&[a[0], b.call("chebyshev_u", &[b.sub(a[0], b.one()), a[1]])]),
        ]),
    ),
    (
        "chebyshev_u",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "chebyshev_u", a, 0), |b, a| {
            let n1 = b.add(&[a[0], b.one()]);
            let t = b.mul(&[n1, b.call("chebyshev_t", &[n1, a[1]])]);
            let u = b.mul(&[a[1], b.call("chebyshev_u", a)]);
            b.div(b.sub(t, u), b.sub(pw(b, a[1], 2), b.one()))
        }]),
    ),
    (
        "legendre_p",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "legendre_p", a, 0), |b, a| {
            let p = b.sub(b.mul(&[a[1], b.call("legendre_p", a)]), b.call("legendre_p", &[b.sub(a[0], b.one()), a[1]]));
            b.div(b.mul(&[a[0], p]), b.sub(pw(b, a[1], 2), b.one()))
        }]),
    ),
    (
        "hermite_h",
        2,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "hermite_h", a, 0),
            |b, a| b.mul(&[b.int(2), a[0], b.call("hermite_h", &[b.sub(a[0], b.one()), a[1]])]),
        ]),
    ),
    (
        "laguerre_l",
        2,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "laguerre_l", a, 0),
            |b, a| b.neg(b.call("laguerre_l", &[b.sub(a[0], b.one()), b.one(), a[1]])),
        ]),
    ),
    // more special functions (decision IOSYM.11): the generalized Laguerre polynomial, Legendre Q,
    // the regularized hypergeometric functions in z, the factorial and the elliptic nome
    (
        "laguerre_l",
        3,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "laguerre_l", a, 0),
            |b, a| opaque_partial(b, "laguerre_l", a, 1),
            |b, a| b.neg(b.call("laguerre_l", &[b.sub(a[0], b.one()), b.add(&[a[1], b.one()]), a[2]])),
        ]),
    ),
    (
        "legendre_q",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "legendre_q", a, 0), |b, a| {
            let q = b.sub(b.mul(&[a[1], b.call("legendre_q", a)]), b.call("legendre_q", &[b.sub(a[0], b.one()), a[1]]));
            b.div(b.mul(&[a[0], q]), b.sub(pw(b, a[1], 2), b.one()))
        }]),
    ),
    (
        "hypergeometric_0f1_regularized",
        2,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "hypergeometric_0f1_regularized", a, 0),
            |b, a| b.call("hypergeometric_0f1_regularized", &[b.add(&[a[0], b.one()]), a[1]]),
        ]),
    ),
    (
        "hypergeometric_1f1_regularized",
        3,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "hypergeometric_1f1_regularized", a, 0),
            |b, a| opaque_partial(b, "hypergeometric_1f1_regularized", a, 1),
            |b, a| {
                let up = |t| b.add(&[t, b.one()]);
                b.mul(&[a[0], b.call("hypergeometric_1f1_regularized", &[up(a[0]), up(a[1]), a[2]])])
            },
        ]),
    ),
    (
        "hypergeometric_2f1_regularized",
        4,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "hypergeometric_2f1_regularized", a, 0),
            |b, a| opaque_partial(b, "hypergeometric_2f1_regularized", a, 1),
            |b, a| opaque_partial(b, "hypergeometric_2f1_regularized", a, 2),
            |b, a| {
                let up = |t| b.add(&[t, b.one()]);
                b.mul(&[a[0], a[1], b.call("hypergeometric_2f1_regularized", &[up(a[0]), up(a[1]), up(a[2]), a[3]])])
            },
        ]),
    ),
    ("factorial", 1, Rule::Partials(&[|b, a| b.mul(&[b.call("factorial", a), b.call("polygamma", &[b.zero(), b.add(&[a[0], b.one()])])])])),
    (
        "elliptic_nome_q",
        1,
        Rule::Partials(&[|b, a| {
            let m = a[0];
            let k = b.call("elliptic_k", &[m]);
            let den = b.mul(&[b.int(4), m, b.sub(b.one(), m), pw(b, k, 2)]);
            b.div(b.mul(&[pw(b, pi(b), 2), b.call("elliptic_nome_q", &[m])]), den)
        }]),
    ),
    (
        "gegenbauer_c",
        3,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "gegenbauer_c", a, 0),
            |b, a| opaque_partial(b, "gegenbauer_c", a, 1),
            |b, a| b.mul(&[b.int(2), a[1], b.call("gegenbauer_c", &[b.sub(a[0], b.one()), b.add(&[a[1], b.one()]), a[2]])]),
        ]),
    ),
    // spherical Bessel and Hankel functions in their argument
    (
        "spherical_bessel_j",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "spherical_bessel_j", a, 0), |b, a| spherical_d(b, "spherical_bessel_j", a)]),
    ),
    (
        "spherical_bessel_y",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "spherical_bessel_y", a, 0), |b, a| spherical_d(b, "spherical_bessel_y", a)]),
    ),
    ("hankel_h1", 2, Rule::Partials(&[|b, a| opaque_partial(b, "hankel_h1", a, 0), |b, a| bessel_diff(b, "hankel_h1", a)])),
    ("hankel_h2", 2, Rule::Partials(&[|b, a| opaque_partial(b, "hankel_h2", a, 0), |b, a| bessel_diff(b, "hankel_h2", a)])),
    // round 36: inverse gamma and beta in the probability, the inverse nome, the modular functions, the Bessel zeros
    (
        "inverse_gamma_regularized",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "inverse_gamma_regularized", a, 0), |b, a| igamma_s(b, "inverse_gamma_regularized", a, 1)]),
    ),
    (
        "inverse_gamma_regularized",
        3,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "inverse_gamma_regularized", a, 0),
            |b, a| opaque_partial(b, "inverse_gamma_regularized", a, 1),
            |b, a| igamma_s(b, "inverse_gamma_regularized", a, -1),
        ]),
    ),
    (
        "inverse_beta_regularized",
        3,
        Rule::Partials(&[
            ibeta_s,
            |b, a| opaque_partial(b, "inverse_beta_regularized", a, 1),
            |b, a| opaque_partial(b, "inverse_beta_regularized", a, 2),
        ]),
    ),
    (
        "inverse_elliptic_nome_q",
        1,
        Rule::Partials(&[|b, a| {
            let m = b.call("inverse_elliptic_nome_q", a);
            let k = b.call("elliptic_k", &[m]);
            b.div(b.mul(&[b.int(4), m, b.sub(b.one(), m), pw(b, k, 2)]), b.mul(&[pw(b, pi(b), 2), a[0]]))
        }]),
    ),
    ("modular_lambda", 1, Rule::Partials(&[lambda_tau])),
    ("klein_invariant_j", 1, Rule::Partials(&[klein_tau])),
    (
        "bessel_j_zero",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "bessel_j_zero", a, 0), |b, a| bessel_zero_k(b, "bessel_j_zero", "bessel_y", a)]),
    ),
    (
        "bessel_y_zero",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "bessel_y_zero", a, 0), |b, a| bessel_zero_k(b, "bessel_y_zero", "bessel_j", a)]),
    ),
    (
        "spherical_hankel_h1",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "spherical_hankel_h1", a, 0), |b, a| spherical_d(b, "spherical_hankel_h1", a)]),
    ),
    (
        "spherical_hankel_h2",
        2,
        Rule::Partials(&[|b, a| opaque_partial(b, "spherical_hankel_h2", a, 0), |b, a| spherical_d(b, "spherical_hankel_h2", a)]),
    ),
    // the Barnes G function: G'/G = (z - 1) digamma(z) - z + (1 + log(2 pi))/2 (DLMF 5.17.4 differentiated); round 36
    (
        "barnes_g",
        1,
        Rule::Partials(&[|b, a| {
            let z = a[0];
            let psi = b.call("digamma", &[z]);
            let l = b.div(b.add(&[b.one(), b.call("log", &[b.mul(&[b.int(2), pi(b)])])]), b.int(2));
            b.mul(&[b.call("barnes_g", a), b.add(&[b.mul(&[b.sub(z, b.one()), psi]), b.neg(z), l])])
        }]),
    ),
    // the other Jacobi elliptic functions, by the quotient rule over sn, cn, dn
    ("jacobi_cd", 2, Rule::Partials(&[|b, a| jac_ratio(b, "jacobi_cd", a), |b, a| jac_ratio_m(b, "jacobi_cd", a)])),
    ("jacobi_cs", 2, Rule::Partials(&[|b, a| jac_ratio(b, "jacobi_cs", a), |b, a| jac_ratio_m(b, "jacobi_cs", a)])),
    ("jacobi_dc", 2, Rule::Partials(&[|b, a| jac_ratio(b, "jacobi_dc", a), |b, a| jac_ratio_m(b, "jacobi_dc", a)])),
    ("jacobi_ds", 2, Rule::Partials(&[|b, a| jac_ratio(b, "jacobi_ds", a), |b, a| jac_ratio_m(b, "jacobi_ds", a)])),
    ("jacobi_nc", 2, Rule::Partials(&[|b, a| jac_ratio(b, "jacobi_nc", a), |b, a| jac_ratio_m(b, "jacobi_nc", a)])),
    ("jacobi_nd", 2, Rule::Partials(&[|b, a| jac_ratio(b, "jacobi_nd", a), |b, a| jac_ratio_m(b, "jacobi_nd", a)])),
    ("jacobi_ns", 2, Rule::Partials(&[|b, a| jac_ratio(b, "jacobi_ns", a), |b, a| jac_ratio_m(b, "jacobi_ns", a)])),
    ("jacobi_sc", 2, Rule::Partials(&[|b, a| jac_ratio(b, "jacobi_sc", a), |b, a| jac_ratio_m(b, "jacobi_sc", a)])),
    ("jacobi_sd", 2, Rule::Partials(&[|b, a| jac_ratio(b, "jacobi_sd", a), |b, a| jac_ratio_m(b, "jacobi_sd", a)])),
    (
        "pochhammer",
        2,
        Rule::Partials(&[
            |b, a| {
                let s = b.add(&[a[0], a[1]]);
                b.mul(&[b.call("pochhammer", a), b.sub(b.call("digamma", &[s]), b.call("digamma", &[a[0]]))])
            },
            |b, a| b.mul(&[b.call("pochhammer", a), b.call("digamma", &[b.add(&[a[0], a[1]])])]),
        ]),
    ),
    // d/dx e^x K_nu(x) = e^x K_nu - (e^x K_(nu-1) + e^x K_(nu+1))/2, and for e^-x I_nu likewise
    ("bessel_k_scaled", 2, Rule::Partials(&[|b, a| opaque_partial(b, "bessel_k_scaled", a, 0), |b, a| {
        let s = bessel_sum(b, "bessel_k_scaled", a);
        b.sub(b.call("bessel_k_scaled", a), b.mul(&[b.rat(1, 2), s]))
    }])),
    ("bessel_i_scaled", 2, Rule::Partials(&[|b, a| opaque_partial(b, "bessel_i_scaled", a, 0), |b, a| {
        let s = bessel_sum(b, "bessel_i_scaled", a);
        b.sub(b.mul(&[b.rat(1, 2), s]), b.call("bessel_i_scaled", a))
    }])),
    // the round 28 special functions (mc-special). Scorer: Gi'' = z Gi - 1/pi, Hi'' = z Hi + 1/pi (DLMF 9.12.1-2)
    ("scorer_gi", 1, Rule::Partials(&[|b, a| b.call("scorer_gi_prime", a)])),
    ("scorer_hi", 1, Rule::Partials(&[|b, a| b.call("scorer_hi_prime", a)])),
    (
        "scorer_gi_prime",
        1,
        Rule::Partials(&[|b, a| b.sub(b.mul(&[a[0], b.call("scorer_gi", a)]), b.div(b.one(), pi(b)))]),
    ),
    (
        "scorer_hi_prime",
        1,
        Rule::Partials(&[|b, a| b.add(&[b.mul(&[a[0], b.call("scorer_hi", a)]), b.div(b.one(), pi(b))])]),
    ),
    // Weierstrass (DLMF 23.2.8, 23.2.9, 23.3.10): p' named, p'' = 6 p^2 - g2/2, zeta' = -p, sigma' = sigma zeta; the
    // invariants are opaque
    (
        "weierstrass_p",
        2,
        Rule::Partials(&[|b, a| b.call("weierstrass_p_prime", a), |b, a| opaque_partial(b, "weierstrass_p", a, 1)]),
    ),
    ("weierstrass_p_prime", 2, Rule::Partials(&[weier_pp_u, |b, a| opaque_partial(b, "weierstrass_p_prime", a, 1)])),
    (
        "weierstrass_zeta",
        2,
        Rule::Partials(&[|b, a| b.neg(b.call("weierstrass_p", a)), |b, a| opaque_partial(b, "weierstrass_zeta", a, 1)]),
    ),
    (
        "weierstrass_sigma",
        2,
        Rule::Partials(&[
            |b, a| b.mul(&[b.call("weierstrass_sigma", a), b.call("weierstrass_zeta", a)]),
            |b, a| opaque_partial(b, "weierstrass_sigma", a, 1),
        ]),
    ),
    (
        "inverse_weierstrass_p",
        2,
        Rule::Partials(&[
            |b, a| pw(b, b.call("weierstrass_p_prime", &[b.call("inverse_weierstrass_p", a), a[1]]), -1),
            |b, a| opaque_partial(b, "inverse_weierstrass_p", a, 1),
        ]),
    ),
    (
        "elliptic_exp",
        2,
        Rule::Partials(&[|b, a| b.call("elliptic_exp_prime", a), |b, a| opaque_partial(b, "elliptic_exp", a, 1)]),
    ),
    // Coulomb wave functions: the order and eta are opaque
    ("coulomb_f", 3, Rule::Partials(&[|b, a| opaque_partial(b, "coulomb_f", a, 0), |b, a| opaque_partial(b, "coulomb_f", a, 1), |b, a| coulomb_rho(b, "coulomb_f", a)])),
    ("coulomb_g", 3, Rule::Partials(&[|b, a| opaque_partial(b, "coulomb_g", a, 0), |b, a| opaque_partial(b, "coulomb_g", a, 1), |b, a| coulomb_rho(b, "coulomb_g", a)])),
    ("coulomb_h1", 3, Rule::Partials(&[|b, a| opaque_partial(b, "coulomb_h1", a, 0), |b, a| opaque_partial(b, "coulomb_h1", a, 1), |b, a| coulomb_rho(b, "coulomb_h1", a)])),
    ("coulomb_h2", 3, Rule::Partials(&[|b, a| opaque_partial(b, "coulomb_h2", a, 0), |b, a| opaque_partial(b, "coulomb_h2", a, 1), |b, a| coulomb_rho(b, "coulomb_h2", a)])),
    // Lommel functions
    ("lommel_s1", 3, Rule::Partials(&[|b, a| opaque_partial(b, "lommel_s1", a, 0), |b, a| opaque_partial(b, "lommel_s1", a, 1), |b, a| lommel_z(b, "lommel_s1", a)])),
    ("lommel_s2", 3, Rule::Partials(&[|b, a| opaque_partial(b, "lommel_s2", a, 0), |b, a| opaque_partial(b, "lommel_s2", a, 1), |b, a| lommel_z(b, "lommel_s2", a)])),
    ("lommel_t1", 3, Rule::Partials(&[|b, a| opaque_partial(b, "lommel_t1", a, 0), |b, a| opaque_partial(b, "lommel_t1", a, 1), |b, a| lommel_z(b, "lommel_t1", a)])),
    ("lommel_t2", 3, Rule::Partials(&[|b, a| opaque_partial(b, "lommel_t2", a, 0), |b, a| opaque_partial(b, "lommel_t2", a, 1), |b, a| lommel_z(b, "lommel_t2", a)])),
    // Mathieu: y'' = -(a - 2 q cos 2z) y (DLMF 28.2.1) for y = mathieu_s and mathieu_c
    (
        "mathieu_c",
        3,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "mathieu_c", a, 0),
            |b, a| opaque_partial(b, "mathieu_c", a, 1),
            |b, a| b.call("mathieu_c_prime", a),
        ]),
    ),
    (
        "mathieu_c_prime",
        3,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "mathieu_c_prime", a, 0),
            |b, a| opaque_partial(b, "mathieu_c_prime", a, 1),
            |b, a| {
                let c = b.call("cos", &[b.mul(&[b.int(2), a[2]])]);
                let k = b.sub(a[0], b.mul(&[b.int(2), a[1], c]));
                b.neg(b.mul(&[k, b.call("mathieu_c", a)]))
            },
        ]),
    ),
    (
        "mathieu_s",
        3,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "mathieu_s", a, 0),
            |b, a| opaque_partial(b, "mathieu_s", a, 1),
            |b, a| b.call("mathieu_s_prime", a),
        ]),
    ),
    (
        "mathieu_s_prime",
        3,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "mathieu_s_prime", a, 0),
            |b, a| opaque_partial(b, "mathieu_s_prime", a, 1),
            |b, a| {
                let c = b.call("cos", &[b.mul(&[b.int(2), a[2]])]);
                let k = b.sub(a[0], b.mul(&[b.int(2), a[1], c]));
                b.neg(b.mul(&[k, b.call("mathieu_s", a)]))
            },
        ]),
    ),
    // Appell functions (DLMF 16.13): the parameters are opaque, x and y raise parameters
    (
        "appell_f1",
        6,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "appell_f1", a, 0),
            |b, a| opaque_partial(b, "appell_f1", a, 1),
            |b, a| opaque_partial(b, "appell_f1", a, 2),
            |b, a| opaque_partial(b, "appell_f1", a, 3),
            |b, a| appell_d(b, "appell_f1", a, &[0, 1, 3], &[0, 1], 3),
            |b, a| appell_d(b, "appell_f1", a, &[0, 2, 3], &[0, 2], 3),
        ]),
    ),
    (
        "appell_f2",
        7,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "appell_f2", a, 0),
            |b, a| opaque_partial(b, "appell_f2", a, 1),
            |b, a| opaque_partial(b, "appell_f2", a, 2),
            |b, a| opaque_partial(b, "appell_f2", a, 3),
            |b, a| opaque_partial(b, "appell_f2", a, 4),
            |b, a| appell_d(b, "appell_f2", a, &[0, 1, 3], &[0, 1], 3),
            |b, a| appell_d(b, "appell_f2", a, &[0, 2, 4], &[0, 2], 4),
        ]),
    ),
    (
        "appell_f3",
        7,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "appell_f3", a, 0),
            |b, a| opaque_partial(b, "appell_f3", a, 1),
            |b, a| opaque_partial(b, "appell_f3", a, 2),
            |b, a| opaque_partial(b, "appell_f3", a, 3),
            |b, a| opaque_partial(b, "appell_f3", a, 4),
            |b, a| appell_d(b, "appell_f3", a, &[0, 2, 4], &[0, 2], 4),
            |b, a| appell_d(b, "appell_f3", a, &[1, 3, 4], &[1, 3], 4),
        ]),
    ),
    (
        "appell_f4",
        6,
        Rule::Partials(&[
            |b, a| opaque_partial(b, "appell_f4", a, 0),
            |b, a| opaque_partial(b, "appell_f4", a, 1),
            |b, a| opaque_partial(b, "appell_f4", a, 2),
            |b, a| opaque_partial(b, "appell_f4", a, 3),
            |b, a| appell_d(b, "appell_f4", a, &[0, 1, 2], &[0, 1], 2),
            |b, a| appell_d(b, "appell_f4", a, &[0, 1, 3], &[0, 1], 3),
        ]),
    ),
    // complex parts, linear over the reals
    ("re", 1, Rule::Linear),
    ("im_part", 1, Rule::Linear),
    ("conjugate", 1, Rule::Linear),
    // structure: lists and equations map, conditionals differentiate their branches
    ("list", ANY, Rule::Linear),
    ("equal", 2, Rule::Linear),
    ("if", 2, Rule::Branches),
    ("if", 3, Rule::Branches),
    ("piecewise", 1, Rule::Piecewise),
    ("piecewise", 2, Rule::Piecewise),
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

/// The z-derivative of `spheroidal_s1_prime(n, m, gamma, z)`, from the differential equation of the radial functions
/// (DLMF 30.11.1 with the radial variable z, Flammer 4.1.2): (z^2 - 1) S'' + 2 z S' - (lambda - gamma^2 (z^2 - 1) +
/// m^2/(z^2 - 1)) S = 0, lambda = `spheroidal_eigenvalue(n, m, gamma)`.
fn spheroidal_s1_second<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let z = a[3];
    let w = b.sub(pw(b, z, 2), b.one());
    let lam = b.call("spheroidal_eigenvalue", &a[..3]);
    let g2 = pw(b, a[2], 2);
    let s = b.call("spheroidal_s1", a);
    let sp = b.call("spheroidal_s1_prime", a);
    let coef = b.add(&[lam, b.neg(b.mul(&[g2, w])), b.mul(&[pw(b, a[1], 2), b.pow(w, b.int(-1))])]);
    b.mul(&[b.add(&[b.mul(&[coef, s]), b.neg(b.mul(&[b.int(2), z, sp]))]), b.pow(w, b.int(-1))])
}

/// The derivative of `meijer_g(as, bs, z)` (and `meijer_g(as, bs, z, r)`) in z (round 22): multiplying
/// the Mellin-Barnes integrand by s is one more gamma ratio, s = gamma(1 + s)/gamma(s), so
/// z G'(z) = G^{m,n+1}_{p+1,q+1}(z | 0, a; b, 1) (Luke 1969, 5.4; PBM 3, 8.2.2.32); with r, the chain rule through
/// z^(1/r) gives the same function at the same arguments divided by r z. The derivative row above calls it.
pub fn meijer_dz<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let h = b.heap();
    let list = |xs: &[T<'h>]| h.call(b.hd.list, xs);
    let parts = |t: T<'h>| -> Option<(Vec<T<'h>>, Vec<T<'h>>)> {
        let l = args_of(b, t, b.hd.list)?;
        if l.len() != 2 {
            return None;
        }
        Some((args_of(b, l[0], b.hd.list)?.to_vec(), args_of(b, l[1], b.hd.list)?.to_vec()))
    };
    let (Some((mut an, ap)), Some((bm, mut bq))) = (parts(a[0]), parts(a[1])) else {
        let mut orders = vec![0i64; a.len()];
        orders[2] = 1;
        return b.app(b.derivative(&orders, b.sym(h.sym("meijer_g"))), a);
    };
    an.insert(0, b.zero());
    bq.push(b.one());
    let mut args = vec![list(&[list(&an), list(&ap)]), list(&[list(&bm), list(&bq)])];
    args.extend_from_slice(&a[2..]);
    let g = b.call("meijer_g", &args);
    let den = match a.get(3) {
        Some(&r) => b.mul(&[r, a[2]]),
        None => a[2],
    };
    b.div(g, den)
}

/// The derivative of `fox_h([[an], [ap]], [[bm], [bq]], z)` in z (round 31): multiplying the Mellin-Barnes
/// integrand of H(z) = (1/2 pi i) int Theta(s) z^(-s) ds by the k-th falling power of -s and moving z^(-s-k) to z^(-s)
/// (s -> s - k) shifts every parameter, a_j -> a_j - k A_j and b_j -> b_j - k B_j, and multiplies by
/// (-1)^k gamma(s)/gamma(s - k) = gamma(1 + k - s)/gamma(1 - s) (reflection), which is the new pair (-k, 1) at the
/// front of the first a-group and (0, 1) in the second b-group: H^(m,n+1)_(p+1,q+1)(z | (-k, 1), (a_j - k A_j, A_j);
/// (b_j - k B_j, B_j), (0, 1)) (Kilbas and Saigo, H-Transforms, 2004, 2.2, with z^(-k) taken into the shifted
/// parameters). First order here; higher orders are repeated applications. The opaque derivative when the parameters
/// are not written as lists of pairs.
pub fn fox_dz<'h>(b: &Build<'h>, a: &[T<'h>]) -> T<'h> {
    let h = b.heap();
    let list = |xs: &[T<'h>]| h.call(b.hd.list, xs);
    let pair_shift = |p: T<'h>| -> Option<(T<'h>, T<'h>)> {
        let l = args_of(b, p, b.hd.list)?;
        if l.len() != 2 {
            return None;
        }
        Some((b.sub(l[0], l[1]), l[1]))
    };
    let group = |t: T<'h>| -> Option<Vec<T<'h>>> {
        args_of(b, t, b.hd.list)?
            .iter()
            .map(|&p| pair_shift(p).map(|(x, s)| list(&[x, s])))
            .collect()
    };
    let parts = |t: T<'h>| -> Option<(Vec<T<'h>>, Vec<T<'h>>)> {
        let l = args_of(b, t, b.hd.list)?;
        if l.len() != 2 {
            return None;
        }
        Some((group(l[0])?, group(l[1])?))
    };
    let (Some((mut an, ap)), Some((bm, mut bq))) = (parts(a[0]), parts(a[1])) else {
        let mut orders = vec![0i64; a.len()];
        orders[2] = 1;
        return b.app(b.derivative(&orders, b.sym(h.sym("fox_h"))), a);
    };
    an.insert(0, list(&[b.int(-1), b.one()]));
    bq.insert(0, list(&[b.zero(), b.one()]));
    let args = [list(&[list(&an), list(&ap)]), list(&[list(&bm), list(&bq)]), a[2]];
    b.call("fox_h", &args)
}
