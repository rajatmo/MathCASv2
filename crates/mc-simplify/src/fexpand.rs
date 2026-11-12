//! `function_expand(e)`: special functions written with more elementary ones where an identity says so (decision
//! A.10), bottom up and evaluated:
//!
//! | form | becomes |
//! |---|---|
//! | `factorial(x)`, `binomial(n, k)`, `beta(a, b)`, `pochhammer(a, n)` | gamma functions |
//! | `harmonic_number(x)` | `euler_gamma + digamma(x + 1)` |
//! | `erfc(x)` | `1 - erf(x)` |
//! | `bessel_j(±1/2, x)`, `bessel_y(±1/2, x)`, `bessel_i(±1/2, x)`, `bessel_k(±1/2, x)` | sines, cosines, exponentials |
//! | `spherical_bessel_j(0, x)`, `spherical_bessel_y(0, x)` | `sin(x)/x`, `-cos(x)/x` |
//! | `sinc(x)` | `sin(x)/x` |
//! | `sec`, `csc`, `cot`, `sech`, `csch`, `coth` | reciprocals of `cos`, `sin`, `cosh`, `sinh` |
//! | `chebyshev_t(n, x)`, `chebyshev_u(n, x)` (symbolic n) | `cos(n acos(x))`, `sin((n + 1) acos(x))/sqrt(1 - x^2)` |
//! | `catalan_number(n)` (symbolic n), `factorial_power(x, n[, h])` | gamma functions |
//! | `dirac_delta(x, y, ...)` | `dirac_delta(x) dirac_delta(y) ...` |
//! | `elliptic_nome_q(m)` | `exp(-pi elliptic_k(1 - m)/elliptic_k(m))` |
//!
//! Gamma ratios whose arguments differ by an integer are then written as rising factorials (`powers::gamma_ratio`).
//!
//! Since round 19 the identities of [`crate::identities`] marked for `function_expand` (trigonometric constants in
//! radicals, trig of inverse trig, Bessel functions at `-x`, Airy rotations, the Jacobi functions by `sn`, `cn`,
//! `dn`, the zeta reflection, hypergeometric reductions, index shifts of Fibonacci and Lucas numbers under
//! integrality, ...) and the families of sums (gamma and polygamma arguments differing by integers, the dilogarithm
//! reflection, arctangent sums) apply too, after `degree` is written `pi/180` and multiple angles of inverse
//! functions are expanded. `function_expand(e, facts)` and `assumptions:` give the facts the guards ask.
//!
//! Anything else is left as it is.
use crate::facts::Facts;
use crate::identities::{family, node, EXPAND};
use crate::term::{fixed, rewrite, B};
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// `function_expand(e)` under the facts `f` (the identities of [`crate::identities`] marked for it use them as
/// guards).
pub fn function_expand<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let t = degrees(cx, b, t)?;
    let t = heaviside_shapes(cx, b, t)?;
    let t = multiple_angles(cx, b, f, t)?;
    // the forms of round 7 first (the reciprocal functions only here, so that an identity's csc stays)
    let t = fixed(cx, b, t, 8, &mut |_cx, b, n| Ok(rule(b, n, true)))?;
    let t = crate::powers::gamma_ratio(cx, b, t)?;
    let r = fixed(cx, b, t, 8, &mut |cx, b, n| {
        if let Some(r) = rule(b, n, false) {
            return Ok(Some(r));
        }
        if let Some((r, _)) = node(cx, b, f, EXPAND, n)? {
            return Ok(Some(r));
        }
        family(cx, b, EXPAND, n)
    })?;
    // gamma ratios whose arguments differ by an integer as rising factorials (`factorial_power(i, -3)`)
    let r = crate::powers::gamma_ratio(cx, b, r)?;
    // sqrt(u^2) as u where the sign does not matter (`sin(sqrt(x^2))/sqrt(x^2)` is `sin(x)/x`)
    crate::moves::sqrt_parity(cx, b, r)
}

/// `degree` as `pi/180`.
fn degrees<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let degree = h.sym("degree");
    if h.mask(t) & h.mask(h.sym_term(degree)) == 0 {
        return Ok(t);
    }
    let to = b.div(h.sym_term(h.sym("pi")), b.int(180));
    let rule = b.call(h.sym(mc_api::names::RULE), &[h.sym_term(degree), to]);
    cx.eval(b.call_named("rewrite", &[t, rule]))
}

/// Multiple angles of inverse functions expanded first: `sin(2 atan(x))` by the addition theorem (then
/// `sin(atan(x))` and `cos(atan(x))` by the table), `exp(k atanh(x))` as `cosh + sinh` of `k atanh(x)`, each
/// part written as one fraction (DLMF 4.38.8, 4.35.3).
fn multiple_angles<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let inverse = ["atan", "asin", "acos", "atanh", "asinh", "acosh"];
    if !inverse.iter().any(|n| h.mask(t) & h.mask(h.sym_term(h.sym(n))) != 0) {
        return Ok(t);
    }
    let multiple = |b: &B<'h>, u: T<'h>| -> Option<(i64, T<'h>)> {
        let (m, &[k, g]) = b.call_of(u)? else { return None };
        let View::Int(k) = h.view(k) else { return None };
        let (gs, _) = b.call_of(g)?;
        (m == b.hd.mul && (2..=6).contains(&k.abs()) && inverse.contains(&h.sym_name(gs).as_str())).then_some((k, g))
    };
    let r = rewrite(cx, b, t, &mut |cx, b, n| {
        let (s, u) = match (b.exp_arg(n), b.call_of(n)) {
            (Some(u), _) => (b.hd.exp, u),
            (None, Some((s, &[u]))) => (s, u),
            _ => return Ok(None),
        };
        let Some((k, g)) = multiple(b, u) else { return Ok(None) };
        let trig = [b.hd.sin, b.hd.cos, b.hd.tan, b.hd.sinh, b.hd.cosh, b.hd.tanh];
        if trig.contains(&s) {
            return Ok(Some(crate::trig::trig_expand(cx, b, n)?));
        }
        let is_atanh = matches!(b.call_of(g), Some((a, _)) if h.sym_name(a) == "atanh");
        if s == b.hd.exp && is_atanh {
            let ku = b.mul(&[b.int(k), g]);
            let mut parts = vec![];
            for g in [b.hd.cosh, b.hd.sinh] {
                let e = crate::trig::trig_expand(cx, b, b.call(g, &[ku]))?;
                let e = fixed(cx, b, e, 4, &mut |cx, b, n| Ok(node(cx, b, f, EXPAND, n)?.map(|x| x.0)))?;
                parts.push(cx.eval(b.call_named("together", &[e]))?);
            }
            return Ok(Some(b.add(&parts)));
        }
        Ok(None)
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}

fn rule<'h>(b: &B<'h>, node: T<'h>, reciprocals: bool) -> Option<T<'h>> {
    let h = b.heap();
    let (s, args) = b.call_of(node)?;
    let name = h.sym_name(s);
    let g = |x: T<'h>| b.call_named("gamma", &[x]);
    let one = b.int(1);
    let half = b.rat(1, 2);
    let pi = h.sym_term(h.sym("pi"));
    // sqrt(2/(pi x))
    let root = |x: T<'h>| b.pow(b.div(b.int(2), b.mul(&[pi, x])), half);
    Some(match (name.as_str(), args) {
        ("factorial", [x]) => g(b.add(&[*x, one])),
        ("binomial", [n, k]) => {
            let den = b.mul(&[g(b.add(&[*k, one])), g(b.add(&[b.sub(*n, *k), one]))]);
            b.div(g(b.add(&[*n, one])), den)
        }
        ("beta", [a, c]) => b.div(b.mul(&[g(*a), g(*c)]), g(b.add(&[*a, *c]))),
        ("pochhammer", [a, n]) => b.div(g(b.add(&[*a, *n])), g(*a)),
        ("harmonic_number", [x]) => b.add(&[h.sym_term(h.sym("euler_gamma")), b.call_named("digamma", &[b.add(&[*x, one])])]),
        ("erfc", [x]) => b.sub(one, b.call_named("erf", &[*x])),
        ("sinc", [x]) => b.div(b.call_named("sin", &[*x]), *x),
        ("sec" | "csc" | "cot" | "sech" | "csch" | "coth", _) if !reciprocals => return None,
        ("sec", [x]) => b.powi(b.call_named("cos", &[*x]), -1),
        ("csc", [x]) => b.powi(b.call_named("sin", &[*x]), -1),
        ("cot", [x]) => b.div(b.call_named("cos", &[*x]), b.call_named("sin", &[*x])),
        ("sech", [x]) => b.powi(b.call_named("cosh", &[*x]), -1),
        ("csch", [x]) => b.powi(b.call_named("sinh", &[*x]), -1),
        ("coth", [x]) => b.div(b.call_named("cosh", &[*x]), b.call_named("sinh", &[*x])),
        ("spherical_bessel_j", [n, x]) if *n == b.int(0) => b.div(b.call_named("sin", &[*x]), *x),
        ("spherical_bessel_y", [n, x]) if *n == b.int(0) => b.neg(b.div(b.call_named("cos", &[*x]), *x)),
        ("bessel_j" | "bessel_y" | "bessel_i" | "bessel_k", [nu, x]) => {
            // the half-integer orders: the orders +-1/2 in sines, cosines and exponentials, the others by the
            // three-term recurrence A(v + 1) = alpha (2 v/x) A(v) + beta A(v - 1) (DLMF 10.6.1, 10.29.1)
            let (sin, cos) = (b.call_named("sin", &[*x]), b.call_named("cos", &[*x]));
            let base = |plus: bool| match (name.as_str(), plus) {
                ("bessel_j", true) => b.mul(&[root(*x), sin]),
                ("bessel_j", false) => b.mul(&[root(*x), cos]),
                ("bessel_y", true) => b.neg(b.mul(&[root(*x), cos])),
                ("bessel_y", false) => b.mul(&[root(*x), sin]),
                ("bessel_i", true) => b.mul(&[root(*x), b.call_named("sinh", &[*x])]),
                ("bessel_i", false) => b.mul(&[root(*x), b.call_named("cosh", &[*x])]),
                // K_(1/2) = K_(-1/2) = sqrt(pi/(2x)) exp(-x)
                _ => b.mul(&[b.pow(b.div(pi, b.mul(&[b.int(2), *x])), half), b.call_named("exp", &[b.neg(*x)])]),
            };
            // nu = k + 1/2
            let doubled = b.ar.num(b.mul(&[b.int(2), *nu])).and_then(|n| n.as_rat().cloned());
            let k2 = doubled.filter(|q| q.is_int())?.num.to_i64()?;
            if k2 % 2 == 0 || k2.abs() > 25 {
                return None;
            }
            let k = (k2 - 1).div_euclid(2); // nu = k + 1/2
            let (alpha, beta) = match name.as_str() {
                "bessel_j" | "bessel_y" => (1, -1),
                "bessel_i" => (-1, 1),
                _ => (1, 1),
            };
            // (A(-1/2), A(1/2)); step up or down
            let (mut lo, mut hi) = (base(false), base(true));
            let (mut lo_k, mut hi_k) = (-1i64, 0i64); // the orders lo_k + 1/2, hi_k + 1/2
            while hi_k < k {
                let v = b.rat(2 * hi_k + 1, 2);
                let t = b.mul(&[b.int(alpha), b.div(b.mul(&[b.int(2), v]), *x), hi]);
                let next = b.add(&[t, b.mul(&[b.int(beta), lo])]);
                (lo, hi) = (hi, next);
                hi_k += 1;
                lo_k += 1;
            }
            while lo_k > k {
                let v = b.rat(2 * lo_k + 1, 2);
                // A(v - 1) = beta (A(v + 1) - alpha (2 v/x) A(v))
                let t = b.mul(&[b.int(alpha), b.div(b.mul(&[b.int(2), v]), *x), lo]);
                let prev = b.mul(&[b.int(beta), b.sub(hi, t)]);
                (lo, hi) = (prev, lo);
                lo_k -= 1;
            }
            if k == lo_k {
                lo
            } else {
                hi
            }
        }
        // round 17: the closed forms of DLMF 18.5.1 (Chebyshev), 26.5.1 with 5.5.5 (Catalan numbers),
        // 5.2.5 (falling factorials), 1.17.12 (the product form of a multivariate delta), 22.2.1 (the nome)
        ("chebyshev_t", [n, x]) if !is_integer(b, *n) => b.call_named("cos", &[b.mul(&[*n, b.call_named("acos", &[*x])])]),
        ("chebyshev_u", [n, x]) if !is_integer(b, *n) => {
            let s = b.call_named("sin", &[b.mul(&[b.add(&[*n, one]), b.call_named("acos", &[*x])])]);
            b.div(s, b.mul(&[b.pow(b.add(&[one, *x]), half), b.pow(b.sub(one, *x), half)]))
        }
        ("catalan_number", [n]) if !is_integer(b, *n) => {
            let num = b.mul(&[b.pow(b.int(2), b.mul(&[b.int(2), *n])), g(b.add(&[*n, half]))]);
            b.div(num, b.mul(&[b.pow(pi, half), g(b.add(&[*n, b.int(2)]))]))
        }
        ("factorial_power", [x, n]) => b.div(g(b.add(&[*x, one])), g(b.add(&[b.sub(*x, *n), one]))),
        ("factorial_power", [x, n, step]) if *step == b.int(0) => b.pow(*x, *n),
        ("factorial_power", [x, n, step]) => {
            let y = b.div(*x, *step);
            b.mul(&[b.pow(*step, *n), b.div(g(b.add(&[y, one])), g(b.add(&[b.sub(y, *n), one])))])
        }
        ("dirac_delta", xs) if xs.len() > 1 => b.mul(&xs.iter().map(|&x| b.call_named("dirac_delta", &[x])).collect::<Vec<_>>()),
        ("elliptic_nome_q", [m]) => {
            let k = |u: T<'h>| b.call_named("elliptic_k", &[u]);
            b.call_named("exp", &[b.neg(b.div(b.mul(&[pi, k(b.sub(one, *m))]), k(*m)))])
        }
        _ => return None,
    })
}

/// Is `t` an integer (then the polynomial or the number is the closed form)?
fn is_integer<'h>(b: &B<'h>, t: T<'h>) -> bool {
    matches!(b.heap().view(t), mc_api::View::Int(_) | mc_api::View::BigInt { .. })
}

/// `heaviside_pi(u)` as `theta(u + 1/2) - theta(u - 1/2)` and `heaviside_lambda(u)` as the second difference
/// `(u + 1) theta(u + 1) - 2 u theta(u) + (u - 1) theta(u - 1)`, expanded.
fn heaviside_shapes<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let (pi, lambda) = (h.sym("heaviside_pi"), h.sym("heaviside_lambda"));
    if h.mask(t) & (h.mask(h.sym_term(pi)) | h.mask(h.sym_term(lambda))) == 0 {
        return Ok(t);
    }
    let theta = |u: T<'h>| b.call_named("heaviside_theta", &[u]);
    let r = crate::term::rewrite(cx, b, t, &mut |_, b, node| {
        Ok(match b.call_of(node) {
            Some((s, [u])) if s == pi && b.ar.num(*u).is_none() => Some(b.sub(theta(b.add(&[*u, b.rat(1, 2)])), theta(b.sub(*u, b.rat(1, 2))))),
            Some((s, [u])) if s == lambda && b.ar.num(*u).is_none() => {
                let (up, um) = (b.add(&[*u, b.int(1)]), b.sub(*u, b.int(1)));
                Some(b.add(&[b.mul(&[up, theta(up)]), b.mul(&[b.int(-2), *u, theta(*u)]), b.mul(&[um, theta(um)])]))
            }
            _ => None,
        })
    })?;
    if r == t {
        return Ok(t);
    }
    let r = cx.eval(r)?;
    crate::term::apply(cx, b, "expand", r)
}
