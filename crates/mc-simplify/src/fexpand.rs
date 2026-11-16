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
//! | `fibonacci(n, x)`, `lucas_l(n, x)` (symbolic n) | Binet's forms `(a^n - cos(pi n) a^-n)/sqrt(x^2 + 4)`, `a^n + cos(pi n) a^-n`, `a = (x + sqrt(x^2 + 4))/2` |
//! | `catalan_number(n)` (symbolic n), `factorial_power(x, n[, h])` | gamma functions |
//! | `dirac_delta(x, y, ...)` | `dirac_delta(x) dirac_delta(y) ...` |
//! | `elliptic_nome_q(m)` | `exp(-pi elliptic_k(1 - m)/elliptic_k(m))` |
//!
//! Gamma ratios whose arguments differ by an integer are then written as rising factorials (`powers::gamma_ratio`).
//!
//! The identities of [`crate::identities`] marked for `function_expand` (trigonometric constants in
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
    // the basic forms first (the reciprocal functions only here, so that an identity's csc stays)
    let t = fixed(cx, b, t, 8, &mut |_cx, b, n| Ok(rule(b, n, true)))?;
    let t = crate::powers::gamma_ratio(cx, b, t)?;
    let r = fixed(cx, b, t, 8, &mut |cx, b, n| {
        if let Some(r) = rule(b, n, false) {
            return Ok(Some(r));
        }
        if let Some(r) = arithmetic(b, f, n) {
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

/// Functions of number theory by their definitions: `quotient(m, n)` is `floor(m/n)`; `euler_phi(p)` is
/// `p - 1` for a prime `p` the facts state.
fn arithmetic<'h>(b: &B<'h>, f: &Facts<'_, 'h>, n: T<'h>) -> Option<T<'h>> {
    let (s, args) = b.call_of(n)?;
    match (b.heap().sym_name(s).as_str(), args) {
        ("quotient", &[m, k]) if !(b.is_number(m) && b.is_number(k)) => Some(b.call_named("floor", &[b.div(m, k)])),
        ("euler_phi", &[p]) if f.d.facts.stated_prime(p) => Some(b.sub(p, b.int(1))),
        _ => None,
    }
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
        // H(r, m) = binomial(m + r - 1, r - 1) (H(m + r - 1) - H(r - 1)) (Conway and Guy, The Book of Numbers, 1996,
        // p. 258), continued by gamma and digamma
        ("hyper_harmonic_number", [r, m]) => {
            let ratio = b.div(g(b.add(&[*m, *r])), b.mul(&[g(b.add(&[*m, one])), g(*r)]));
            // a small integer m: digamma(m + r) - digamma(r) is the finite sum of 1/(r + j), j < m
            let diff = match h.view(*m) {
                View::Int(k) if (1..=20).contains(&k) => b.add(&(0..k).map(|j| b.powi(b.add(&[*r, b.int(j)]), -1)).collect::<Vec<_>>()),
                _ => b.sub(b.call_named("digamma", &[b.add(&[*m, *r])]), b.call_named("digamma", &[*r])),
            };
            b.mul(&[ratio, diff])
        }
        // Gauss's digamma theorem at p/q, q = 2, 3, 4, 6 (DLMF 5.4.19), with the recurrence for the integer part
        // (DLMF 5.5.2)
        ("digamma", [x]) => {
            let View::Rational { .. } = h.view(*x) else { return None };
            let (num, den) = small_fraction(h, *x)?;
            if ![2, 3, 4, 6].contains(&den) || num.abs() > 50 * den {
                return None;
            }
            let p = num.rem_euclid(den);
            let n = (num - p) / den;
            let frac = b.rat(p, den);
            let arg = |k: i64| b.mul(&[pi, b.rat(k, den)]);
            let mut terms = vec![
                b.neg(h.sym_term(h.sym("euler_gamma"))),
                b.neg(b.call_named("log", &[b.int(2 * den)])),
                b.neg(b.mul(&[b.rat(1, 2), pi, b.call_named("cot", &[b.mul(&[pi, frac])])])),
            ];
            for k in 1..=(den - 1) / 2 {
                terms.push(b.mul(&[b.int(2), b.call_named("cos", &[arg(2 * k * p)]), b.call_named("log", &[b.call_named("sin", &[arg(k)])])]));
            }
            // psi(f + n) = psi(f) + sum_(j < n) 1/(f + j); psi(f - n) = psi(f) - sum_(j = 1..n) 1/(f - j)
            if n > 0 {
                for j in 0..n {
                    terms.push(b.powi(b.add(&[frac, b.int(j)]), -1));
                }
            } else {
                for j in 1..=-n {
                    terms.push(b.neg(b.powi(b.sub(frac, b.int(j)), -1)));
                }
            }
            b.add(&terms)
        }
        // m! - (m - 1)! + ... the continuation e gamma(m + 2) gamma(-m - 1, 1) - cos(pi m) (1 + e Ei(-1))
        ("alternating_factorial", [m]) => {
            let e = h.sym_term(h.sym("e"));
            let cos = b.call_named("cos", &[b.mul(&[pi, *m])]);
            let first = b.mul(&[e, g(b.add(&[*m, b.int(2)])), b.call_named("gamma", &[b.sub(b.int(-1), *m), one])]);
            let ei = b.mul(&[e, cos, b.call_named("exp_integral_ei", &[b.int(-1)])]);
            b.sub(b.sub(first, cos), ei)
        }
        // the sum of (-1)^(k + 1)/k^r up to m by the digamma or polygamma functions of (m + 1)/2 and (m + 2)/2
        // (DLMF 25.11.12 for zeta(r, a) at an integer r >= 2)
        ("alternating_harmonic_number", [m]) => b.call_named("alternating_harmonic_number", &[*m, one]),
        ("alternating_harmonic_number", [m, r]) => {
            let sign = b.pow(b.int(-1), *m);
            let a1 = b.div(b.add(&[*m, one]), b.int(2));
            let a2 = b.div(b.add(&[*m, b.int(2)]), b.int(2));
            if *r == one {
                let d = b.sub(b.call_named("digamma", &[a2]), b.call_named("digamma", &[a1]));
                b.sub(b.call_named("log", &[b.int(2)]), b.mul(&[b.rat(1, 2), sign, d]))
            } else {
                let View::Int(k) = h.view(*r) else { return None };
                if !(2..=40).contains(&k) {
                    return None;
                }
                let fact: i64 = (1..k).product();
                let pg = |a: T<'h>| b.call_named("polygamma", &[b.int(k - 1), a]);
                let c = b.rat(if k % 2 == 0 { 1 } else { -1 }, fact);
                let zeta = b.mul(&[b.sub(one, b.pow(b.int(2), b.int(1 - k))), b.call_named("zeta", &[*r])]);
                let tail = b.mul(&[sign, b.pow(b.int(2), b.int(-k)), c, b.sub(pg(a1), pg(a2))]);
                b.sub(zeta, tail)
            }
        }
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
        // the closed forms of DLMF 18.5.1 (Chebyshev), 26.5.1 with 5.5.5 (Catalan numbers),
        // 5.2.5 (falling factorials), 1.17.12 (the product form of a multivariate delta), 22.2.1 (the nome)
        ("chebyshev_t", [n, x]) if !is_integer(b, *n) => b.call_named("cos", &[b.mul(&[*n, b.call_named("acos", &[*x])])]),
        ("chebyshev_u", [n, x]) if !is_integer(b, *n) => {
            let s = b.call_named("sin", &[b.mul(&[b.add(&[*n, one]), b.call_named("acos", &[*x])])]);
            b.div(s, b.mul(&[b.pow(b.add(&[one, *x]), half), b.pow(b.sub(one, *x), half)]))
        }
        // Binet's formulas continued in the order (Koshy, Fibonacci and Lucas Numbers with Applications, 2001,
        // ch. 37): with r = sqrt(x^2 + 4) and a = (x + r)/2, F_n(x) = (a^n - cos(pi n) a^-n)/r and
        // L_n(x) = a^n + cos(pi n) a^-n; a^(+-n) is written 2^(-+n) (x + r)^(+-n) (2 > 0, so the power splits)
        ("fibonacci" | "lucas_l", [n, x]) if !is_integer(b, *n) => {
            let r = b.pow(b.add(&[b.int(4), b.powi(*x, 2)]), half);
            let s = b.add(&[*x, r]);
            let up = b.mul(&[b.pow(b.int(2), b.neg(*n)), b.pow(s, *n)]);
            let c = b.call_named("cos", &[b.mul(&[*n, pi])]);
            let down = b.mul(&[c, b.pow(b.int(2), *n), b.pow(s, b.neg(*n))]);
            if name == "fibonacci" {
                b.div(b.sub(up, down), r)
            } else {
                b.add(&[up, down])
            }
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
        _ => return special_rule(b, &name, args),
    })
}

/// More special functions by their definitions in other functions: the Whittaker functions by the
/// confluent ones, M = e^(-z/2) z^(mu + 1/2) 1F1(1/2 + mu - kappa; 1 + 2 mu; z) and W the same with U (DLMF 13.14.2-3);
/// the Hankel functions J +- i Y (DLMF 10.4.3-4); the spherical Bessel functions of an order that is not an integer
/// by sqrt(pi/(2 z)) times the Bessel function of order n + 1/2 (DLMF 10.47.3-4); the subfactorial
/// Gamma(1 + x, -1)/e (the continuation of !n = Gamma(n + 1, -1)/e); the inverse nome m(q) = theta_2(q)^4/theta_3(q)^4
/// (DLMF 22.2.2 with 20.9.1); Legendre functions at 0 (DLMF 14.5.1-2 with mu = 0, the reflection formula 5.5.3 for
/// Q); the regularized 0F1 by I_(b-1) (DLMF 10.39.9); Y_l^0 by the Legendre polynomial (DLMF 14.30.1); the Anger
/// function of an integer order is the Bessel function J_n (DLMF 11.10.4); the Weber function of an order n >= 0 is
/// -H_n(z) + (1/pi) sum_(k < n/2) Gamma(k + 1/2) (z/2)^(n - 2k - 1)/Gamma(n + 1/2 - k) (DLMF 11.10.15-16); the
/// Hurwitz zeta at a nonpositive integer -B_(n+1)(a)/(n + 1) (DLMF 25.11.14); the upper incomplete gamma function of
/// a half-integer n + 1/2 > 0 by erf and the recurrence Gamma(a + 1, z) = a Gamma(a, z) plus z^a e^-z (DLMF 8.8.2,
/// 8.4.6); psi'(1/4) = pi^2 + 8 G, psi'(3/4) = pi^2 - 8 G; K(-1) and K(1/2) by Gamma(1/4) (DLMF 19.20.2 with the
/// imaginary-modulus transformation 19.7.5).
fn special_rule<'h>(b: &B<'h>, name: &str, args: &[T<'h>]) -> Option<T<'h>> {
    let h = b.heap();
    let one = b.int(1);
    let half = b.rat(1, 2);
    let pi = h.sym_term(h.sym("pi"));
    let g = |x: T<'h>| b.call_named("gamma", &[x]);
    let rat = |t: T<'h>| b.ar.num(t).and_then(|n| n.as_rat().cloned());
    let int = |t: T<'h>| match h.view(t) {
        mc_api::View::Int(n) => Some(n),
        _ => None,
    };
    let exact_num = |t: T<'h>| b.ar.num(t).is_some_and(|n| n.is_exact());
    Some(match (name, args) {
        ("whittaker_m" | "whittaker_w", [k, m, z]) => {
            let f = if name == "whittaker_m" { "hypergeometric_1f1" } else { "hypergeometric_u" };
            // M with 1 + 2 mu a nonpositive integer is not the 1F1 form (its lower parameter is a pole)
            let twice = b.ar.num(b.mul(&[b.int(2), *m])).and_then(|n| n.as_rat().cloned());
            if name == "whittaker_m" && twice.is_some_and(|q| q.is_int() && q.num.neg && !q.num.is_zero()) {
                return None;
            }
            let first = b.add(&[half, *m, b.neg(*k)]);
            let second = b.add(&[one, b.mul(&[b.int(2), *m])]);
            let e = b.call_named("exp", &[b.mul(&[b.rat(-1, 2), *z])]);
            b.mul(&[b.call_named(f, &[first, second, *z]), e, b.pow(*z, b.add(&[half, *m]))])
        }
        ("hankel_h1" | "hankel_h2", [nu, z]) => {
            let i = h.sym_term(h.sym("im"));
            let s = if name == "hankel_h1" { 1 } else { -1 };
            let y = b.mul(&[b.int(s), i, b.call_named("bessel_y", &[*nu, *z])]);
            b.add(&[b.call_named("bessel_j", &[*nu, *z]), y])
        }
        ("spherical_bessel_j" | "spherical_bessel_y" | "spherical_hankel_h1" | "spherical_hankel_h2", [n, z]) if !is_integer(b, *n) => {
            let order = b.mul(&[half, b.add(&[one, b.mul(&[b.int(2), *n])])]);
            let f = match name {
                "spherical_bessel_j" => "bessel_j",
                "spherical_bessel_y" => "bessel_y",
                "spherical_hankel_h1" => "hankel_h1",
                _ => "hankel_h2",
            };
            let pre = b.mul(&[half, b.pow(b.int(2), half), b.pow(pi, half), b.pow(*z, b.rat(-1, 2))]);
            if f.starts_with("hankel") {
                // the two parts J +- i Y each with the factor
                let s = if f == "hankel_h1" { 1 } else { -1 };
                let i = h.sym_term(h.sym("im"));
                let j = b.mul(&[pre, b.call_named("bessel_j", &[order, *z])]);
                let y = b.mul(&[b.int(s), i, pre, b.call_named("bessel_y", &[order, *z])]);
                return Some(b.add(&[j, y]));
            }
            b.mul(&[pre, b.call_named(f, &[order, *z])])
        }
        // the alternating factorial a(x) = int_0^inf e^-t (t^(x+1) - cos(pi x) t)/(1 + t) dt, the sum of
        // (-1)^(n-k) k! for integers (the geometric sum under the integral of k! = int e^-t t^k dt), with
        // int_0^inf e^-t t^(s-1)/(1 + t) dt = Gamma(s) e Gamma(1 - s, 1) and int e^-t t/(1 + t) dt = 1 + e Ei(-1)
        ("alternating_factorial", [x]) if !is_integer(b, *x) => {
            let e = h.sym_term(h.sym("e"));
            let c = b.call_named("cos", &[b.mul(&[*x, pi])]);
            let main = b.mul(&[e, g(b.add(&[b.int(2), *x])), b.call_named("gamma", &[b.sub(b.int(-1), *x), one])]);
            let ei = b.mul(&[e, c, b.call_named("exp_integral_ei", &[b.int(-1)])]);
            b.add(&[b.neg(c), main, b.neg(ei)])
        }
        // the radial Zernike function of numeric orders that are not both integers: cos(k pi) r^m binomial(s, k)
        // 2F1(-k, s + 1; m + 1; r^2), k = (n - m)/2, s = (n + m)/2 (the Jacobi form of the polynomial, Born and Wolf
        // 9.2.1, with (-1)^k continued as cos(k pi), as its numerical evaluation)
        ("zernike_r", [n, m, r]) if b.ar.num(*n).is_some() && b.ar.num(*m).is_some() && !(is_integer(b, *n) && is_integer(b, *m)) => {
            let k = b.mul(&[half, b.sub(*n, *m)]);
            let s = b.mul(&[half, b.add(&[*n, *m])]);
            let f = b.call_named("hypergeometric_2f1", &[b.neg(k), b.add(&[s, one]), b.add(&[*m, one]), b.pow(*r, b.int(2))]);
            b.mul(&[b.call_named("cos", &[b.mul(&[k, pi])]), b.pow(*r, *m), b.call_named("binomial", &[s, k]), f])
        }
        ("subfactorial", [x]) if !is_integer(b, *x) => {
            let e = h.sym_term(h.sym("e"));
            b.div(b.call_named("gamma", &[b.add(&[one, *x]), b.int(-1)]), e)
        }
        ("inverse_elliptic_nome_q", [q]) => {
            let th = |a: i64| b.call_named("elliptic_theta", &[b.int(a), b.int(0), *q]);
            b.div(b.powi(th(2), 4), b.powi(th(3), 4))
        }
        ("legendre_p", [nu, x]) if *x == b.int(0) && !is_integer(b, *nu) => {
            b.div(b.pow(pi, half), b.mul(&[g(b.sub(half, b.mul(&[half, *nu]))), g(b.add(&[one, b.mul(&[half, *nu])]))]))
        }
        ("legendre_q", [nu, x]) if *x == b.int(0) && !is_integer(b, *nu) => {
            let tan = b.call_named("tan", &[b.mul(&[half, *nu, pi])]);
            let den = b.mul(&[*nu, g(b.sub(half, b.mul(&[half, *nu]))), g(b.mul(&[half, *nu]))]);
            b.neg(b.div(b.mul(&[b.pow(pi, b.rat(3, 2)), tan]), den))
        }
        ("hypergeometric_0f1_regularized", [c, z]) => {
            let i = b.call_named("bessel_i", &[b.sub(*c, one), b.mul(&[b.int(2), b.pow(*z, half)])]);
            b.mul(&[b.pow(*z, b.mul(&[half, b.sub(one, *c)])), i])
        }
        ("spherical_harmonic_y", [l, m, th, _]) if *m == b.int(0) => {
            let p = b.call_named("legendre_p", &[*l, b.call_named("cos", &[*th])]);
            b.div(b.mul(&[p, b.pow(b.add(&[one, b.mul(&[b.int(2), *l])]), half)]), b.mul(&[b.int(2), b.pow(pi, half)]))
        }
        ("anger_j", [n, z]) if is_integer(b, *n) => b.call_named("bessel_j", &[*n, *z]),
        // the power series of the Anger function as two 1F2 (DLMF 11.10.1 with 11.10.4): sin(pi nu)/(pi nu)
        // 1F2(1; 1 - nu/2, 1 + nu/2; -z^2/4) + z sin(pi nu)/(pi (1 - nu^2)) 1F2(1; 3/2 - nu/2, 3/2 + nu/2; -z^2/4)
        ("anger_j", [n, z]) if b.ar.num(*n).is_none() => {
            let w = b.mul(&[b.rat(-1, 4), b.powi(*z, 2)]);
            let hn = b.mul(&[half, *n]);
            let f12 = |c: T<'h>| b.call_named("hypergeometric_pfq", &[b.call_named("list", &[one]), b.call_named("list", &[b.sub(c, hn), b.add(&[c, hn])]), w]);
            let s = b.call_named("sin", &[b.mul(&[pi, *n])]);
            let t1 = b.div(b.mul(&[s, f12(one)]), b.mul(&[pi, *n]));
            let t2 = b.div(b.mul(&[*z, s, f12(b.rat(3, 2))]), b.mul(&[pi, b.sub(one, b.powi(*n, 2))]));
            b.add(&[t1, t2])
        }
        // [n]_q! = (q; q)_n/(1 - q)^n = (q; q)_inf/((q^(n + 1); q)_inf (1 - q)^n) for a rational 0 < q < 1 (for
        // -1 < q < 0 the q-gamma function of a non-integer n is not this quotient: the power q^(n + 1) is complex)
        ("q_factorial", [n, q]) if matches!(small_fraction(b.heap(), *q), Some((k, d)) if 0 < k && k < d) => {
            let num = b.call_named("q_pochhammer", &[*q, *q]);
            let den = b.mul(&[b.call_named("q_pochhammer", &[b.pow(*q, b.add(&[*n, one])), *q]), b.pow(b.sub(one, *q), *n)]);
            b.div(num, den)
        }
        ("weber_e", [n, z]) if int(*n).is_some_and(|n| (1..=40).contains(&n)) => {
            let n = int(*n)?;
            let mut terms = vec![b.neg(b.call_named("struve_h", &[b.int(n), *z]))];
            for k in 0..=(n - 1) / 2 {
                let c = b.div(g(b.rat(2 * k + 1, 2)), b.mul(&[pi, g(b.rat(2 * n + 1 - 2 * k, 2))]));
                terms.push(b.mul(&[c, b.pow(b.mul(&[half, *z]), b.int(n - 2 * k - 1))]));
            }
            b.add(&terms)
        }
        ("zeta", [s, a]) if int(*s).is_some_and(|s| (-60..=0).contains(&s)) && !exact_num(*a) => {
            let n1 = 1 - int(*s)?;
            let minus_b = b.call_named("expand", &[b.neg(b.call_named("bernoulli_b", &[b.int(n1), *a]))]);
            b.mul(&[b.rat(1, n1), minus_b])
        }
        ("gamma", [a, z]) => {
            // a = n + 1/2, 1 <= n <= 40
            let q = rat(*a)?;
            let n2 = b.ar.num(b.mul(&[b.int(2), *a])).and_then(|n| n.as_rat().cloned()).filter(|d| d.is_int())?.num.to_i64()?;
            if q.is_int() || n2 % 2 == 0 || !(3..=81).contains(&n2) {
                return None;
            }
            let n = (n2 - 1) / 2;
            let rz = b.pow(*z, half);
            let mut terms = vec![];
            for j in 1..=n {
                terms.push(b.mul(&[b.div(g(*a), g(b.rat(2 * j + 1, 2))), b.pow(*z, b.int(j))]));
            }
            let head = b.mul(&[g(*a), b.sub(one, b.call_named("erf", &[rz]))]);
            let tail = b.div(b.mul(&[b.add(&terms), b.call_named("exp", &[b.neg(*z)])]), rz);
            b.add(&[head, tail])
        }
        ("polygamma", [m, x]) if *m == one => {
            let catalan = h.sym_term(h.sym("catalan"));
            let s = if *x == b.rat(1, 4) {
                8
            } else if *x == b.rat(3, 4) {
                -8
            } else {
                return None;
            };
            b.add(&[b.powi(pi, 2), b.mul(&[b.int(s), catalan])])
        }
        ("elliptic_k", [m]) if *m == b.int(-1) => {
            b.div(b.mul(&[b.pow(b.int(2), half), b.powi(g(b.rat(1, 4)), 2)]), b.mul(&[b.int(8), b.pow(pi, half)]))
        }
        ("elliptic_k", [m]) if *m == half => b.div(b.mul(&[b.int(8), b.pow(pi, b.rat(3, 2))]), b.powi(g(b.rat(-1, 4)), 2)),
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

/// A rational number with small numerator and denominator as (num, den).
fn small_fraction<'h>(h: &'h mc_core::Heap, x: T<'h>) -> Option<(i64, i64)> {
    let View::Rational { num, den } = h.view(x) else { return None };
    match (h.view(num), h.view(den)) {
        (View::Int(n), View::Int(d)) if d > 0 => Some((n, d)),
        _ => None,
    }
}
