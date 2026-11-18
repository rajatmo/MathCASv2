//! `function_expand(e)`: special functions written with more elementary ones where an identity says so (decision
//! A.10), bottom up and evaluated:
//!
//! | form | becomes |
//! |---|---|
//! | `factorial(x)`, `binomial(n, k)`, `beta(a, b)`, `pochhammer(a, n)` | gamma functions |
//! | `harmonic_number(x)` | `euler_gamma + digamma(x + 1)` |
//! | `erfc(x)` | `1 - erf(x)` |
//! | `bessel_j(±1/2, x)`, `bessel_y(±1/2, x)`, `bessel_i(±1/2, x)`, `bessel_k(±1/2, x)` | sines, cosines, exponentials |
//! | `kelvin_ber`, `kelvin_bei`, `kelvin_ker`, `kelvin_kei` of half-integer order | `x^(-1/2)` times sines, cosines, hyperbolic functions and exponentials |
//! | `spherical_bessel_j(0, x)`, `spherical_bessel_y(0, x)` | `sin(x)/x`, `-cos(x)/x` |
//! | `sinc(x)` | `sin(x)/x` |
//! | `sec`, `csc`, `cot`, `sech`, `csch`, `coth` | reciprocals of `cos`, `sin`, `cosh`, `sinh` |
//! | `chebyshev_t(n, x)`, `chebyshev_u(n, x)` (symbolic n) | `cos(n acos(x))`, `sin((n + 1) acos(x))/sqrt(1 - x^2)` |
//! | `fibonacci(n, x)`, `lucas_l(n, x)` (symbolic n) | Binet's forms `(a^n - cos(pi n) a^-n)/sqrt(x^2 + 4)`, `a^n + cos(pi n) a^-n`, `a = (x + sqrt(x^2 + 4))/2` |
//! | `catalan_number(n)` (symbolic n), `factorial_power(x, n[, h])` | gamma functions |
//! | `dirac_delta(x, y, ...)` | `dirac_delta(x) dirac_delta(y) ...` |
//! | `elliptic_nome_q(m)` | `exp(-pi elliptic_k(1 - m)/elliptic_k(m))` |
//! | `logistic_sigmoid(x)`, `factorial2(n)` (symbolic n), `gamma_regularized(n + 1/2, z)` | exponentials, gamma functions, erf |
//! | `spherical_hankel_h1(n, z)`, `spherical_hankel_h2(n, z)` (0 <= n <= 20) | `exp(+-i z)` times a polynomial in `z` over `z^(n+1)` |
//! | `hankel_h1(n + 1/2, z)`, `hankel_h2(n + 1/2, z)` (`|n + 1/2| <= 41/2`); order `+-1/3` | exponentials over `z^(n+1/2)`; Airy functions at `-(3z/2)^(2/3)` |
//! | `digamma(p/q)` (`q <= 24`), `golden_ratio`, `elliptic_e(1/2)`, `elliptic_e(-1)` | Gauss's theorem; `(1 + sqrt(5))/2`; `Gamma(1/4)` |
//! | `dirac_delta(p(x))` (simple real roots, rational or quadratic) | `sum dirac_delta(x - r)/abs(p'(r))` |
//! | `legendre_p(nu, mu, x)` (mu not an integer), `hypergeometric_1f1(1, 3/2, z)`, `hypergeometric_1f1(1/2, 3/2, z)` | `2F1`; `erf` |
//! | `triangle_wave(x)` of a real x (facts given to `function_expand`) | `2 asin(sin(2 pi x))/pi` |
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
    let t = dirac_compose(cx, b, t)?;
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
        if let Some(r) = family(cx, b, EXPAND, n)? {
            return Ok(Some(r));
        }
        // Fibonacci and Lucas numbers of a symbolic order (after the index shifts under integrality) as the
        // polynomials at x = 1, which have Binet's forms
        // (an order the facts make an integer keeps the numbers: the shifts give their closed forms)
        Ok(match b.call_of(n) {
            Some((s, &[k])) if matches!(b.heap().sym_name(s).as_str(), "fibonacci" | "lucas_l") && b.ar.num(k).is_none() => {
                if crate::identities::is_integer(cx, f, k)? {
                    None
                } else {
                    Some(b.call(s, &[k, b.int(1)]))
                }
            }
            _ => None,
        })
    })?;
    // gamma ratios whose arguments differ by an integer as rising factorials (`factorial_power(i, -3)`)
    let r = crate::powers::gamma_ratio(cx, b, r)?;
    // sqrt(u^2) as u where the sign does not matter (`sin(sqrt(x^2))/sqrt(x^2)` is `sin(x)/x`)
    crate::moves::sqrt_parity(cx, b, r)
}

/// `golden_ratio` as `(1 + sqrt(5))/2`. Only the user-level `function_expand` writes it: as a move of `simplify` and
/// `full_simplify` the radical would be expanded in powers (`golden_ratio^20 + golden_ratio^-20`), where the
/// constant reaches the integer by the minimal polynomial.
pub fn golden_ratio_radical<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let phi = h.sym("golden_ratio");
    if h.mask(t) & h.mask(h.sym_term(phi)) == 0 {
        return Ok(t);
    }
    let to = b.mul(&[b.rat(1, 2), b.add(&[b.int(1), b.pow(b.int(5), b.rat(1, 2))])]);
    let rule = b.call(h.sym(mc_api::names::RULE), &[h.sym_term(phi), to]);
    cx.eval(b.call_named("rewrite", &[t, rule]))
}

/// `triangle_wave(u)` of a real `u` (by the facts `f`) as `2 asin(sin(2 pi u))/pi`: both are 1-periodic, odd, and
/// linear with slope 4 on [-1/4, 1/4], where asin(sin(t)) = t for |t| <= pi/2 and sin(2 pi (1/2 - u)) = sin(2 pi u)
/// reflects the rest of a period.
pub fn triangle_wave_closed<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let wave = h.sym("triangle_wave");
    if h.mask(t) & h.mask(h.sym_term(wave)) == 0 {
        return Ok(t);
    }
    let pi = h.sym_term(h.sym("pi"));
    rewrite(cx, b, t, &mut |cx, b, node| {
        let Some((s, &[u])) = b.call_of(node) else { return Ok(None) };
        if s != wave || b.ar.num(u).is_some() {
            return Ok(None);
        }
        use crate::facts::Kind;
        let real = matches!(f.info(cx, u)?.kind, Kind::Integer | Kind::Rational | Kind::Real);
        Ok(real.then(|| {
            let asin = b.call_named("asin", &[b.call_named("sin", &[b.mul(&[b.int(2), pi, u])])]);
            b.div(b.mul(&[b.int(2), asin]), pi)
        }))
    })
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
        // Gauss's digamma theorem at p/q, 2 <= q <= 24 (DLMF 5.4.19), with the recurrence for the integer part
        // (DLMF 5.5.2)
        ("digamma", [x]) => {
            let View::Rational { .. } = h.view(*x) else { return None };
            let (num, den) = small_fraction(h, *x)?;
            if !(2..=24).contains(&den) || num.abs() > 50 * den {
                return None;
            }
            let p = num.rem_euclid(den);
            let n = (num - p) / den;
            let frac = b.rat(p, den);
            let mut terms = vec![
                b.neg(h.sym_term(h.sym("euler_gamma"))),
                b.neg(b.call_named("log", &[b.int(2 * den)])),
                b.neg(b.mul(&[b.rat(1, 2), pi, octant(b, "cot", p, den)])),
            ];
            for k in 1..=(den - 1) / 2 {
                // the angle 2 pi k p/q reduced to [0, pi] (cos is even and 2 pi periodic)
                let j = (2 * k * p).rem_euclid(2 * den);
                let j = if j > den { 2 * den - j } else { j };
                terms.push(b.mul(&[b.int(2), octant(b, "cos", j, den), b.call_named("log", &[octant(b, "sin", k, den)])]));
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
        // j_n of a positive integer order, which the evaluator keeps for a symbolic argument (the elementary form
        // has no value at 0, where j_n is 0): j_0 = sin(x)/x, j_1 = sin(x)/x^2 - cos(x)/x, j_(k+1) = (2k + 1)/x j_k -
        // j_(k-1) (DLMF 10.49.3, 10.51.1)
        ("spherical_bessel_j", [n, x]) if matches!(b.heap().view(*n), View::Int(k) if (1..=25).contains(&k)) => {
            let View::Int(k) = b.heap().view(*n) else { return None };
            let (s, c) = (b.call_named("sin", &[*x]), b.call_named("cos", &[*x]));
            let mut prev = b.div(s, *x);
            let mut cur = b.sub(b.div(s, b.powi(*x, 2)), b.div(c, *x));
            for j in 1..k {
                let next = b.sub(b.mul(&[b.div(b.int(2 * j + 1), *x), cur]), prev);
                (prev, cur) = (cur, next);
            }
            // the coefficients of sin and cos each one fraction (`(3 - x^2) sin(x)/x^3 - 3 cos(x)/x^2`)
            let trig = b.call(b.hd.list, &[s, c]);
            b.call_named("collect", &[b.call_named("expand", &[cur]), trig, h.sym_term(h.sym("together"))])
        }
        ("spherical_bessel_y", [n, x]) if *n == b.int(0) => b.neg(b.div(b.call_named("cos", &[*x]), *x)),
        // Kelvin functions of half-integer order nu = m + 1/2 (DLMF 10.61.1-2 with 10.49.3, 10.49.12): with
        // w = e^(3 pi i/4), ber + i bei = J_nu(w x) = sqrt(2/pi) e^(-3 pi i/8) x^(-1/2) P_m(w x), P_m(z) = z j_m(z)
        // (P_0 = sin z, P_-1 = cos z, P_(m+1) = (2m + 1)/z P_m - P_(m-1)); with v = e^(pi i/4),
        // ker + i kei = e^(-nu pi i/2) K_nu(v x) = sqrt(pi/2) e^(-nu pi i/2 - pi i/8) x^(-1/2) e^(-v x) R_m(v x)
        // (R_0 = R_-1 = 1, R_(m+1) = R_(m-1) + (2m + 1)/z R_m). The real and imaginary parts are taken as the
        // halves of the function and its mirror (conjugate constants, the conjugate rotation), which are analytic
        // and real for x > 0, so they hold wherever x^(-1/2) has its principal value.
        ("kelvin_ber" | "kelvin_bei" | "kelvin_ker" | "kelvin_kei", [nu, x]) => {
            let doubled = b.ar.num(b.mul(&[b.int(2), *nu])).and_then(|n| n.as_rat().cloned());
            let k2 = doubled.filter(|q| q.is_int())?.num.to_i64()?;
            if k2 % 2 == 0 || k2.abs() > 25 {
                return None;
            }
            let m = (k2 - 1).div_euclid(2);
            let im = h.sym_term(h.sym("im"));
            let cis = |q: T<'h>| b.call_named("exp", &[b.mul(&[im, pi, q])]);
            let first = name == "kelvin_ber" || name == "kelvin_bei";
            // the sequence P_m (Bessel J) or R_m (Bessel K) at z
            let seq = |z: T<'h>| {
                let (mut lo, mut hi, mut lo_k) = if first {
                    (b.call_named("cos", &[z]), b.call_named("sin", &[z]), -1i64)
                } else {
                    (one, one, -1i64)
                };
                let sign = if first { -1 } else { 1 };
                while lo_k + 1 < m {
                    let j = lo_k + 1; // hi is the order j
                    let next = b.add(&[b.mul(&[b.div(b.int(2 * j + 1), z), hi]), b.mul(&[b.int(sign), lo])]);
                    (lo, hi) = (hi, next);
                    lo_k += 1;
                }
                while lo_k + 1 > m {
                    let j = lo_k; // lo is the order j: A(j - 1) = (2j + 1)/z A(j) - A(j + 1) (J), A(j + 1) - (2j + 1)/z A(j) (K)
                    let t = b.mul(&[b.div(b.int(2 * j + 1), z), lo]);
                    let prev = if first { b.sub(t, hi) } else { b.sub(hi, t) };
                    (lo, hi) = (prev, lo);
                    lo_k -= 1;
                }
                hi
            };
            // the half-sum (real part) or half-difference over i (imaginary part) of A(+) and its mirror A(-)
            let part = |a: &dyn Fn(i64) -> T<'h>, real: bool| {
                let (p, q) = (a(1), a(-1));
                if real {
                    b.mul(&[half, b.add(&[p, q])])
                } else {
                    b.mul(&[half, b.neg(im), b.sub(p, q)])
                }
            };
            let real = name == "kelvin_ber" || name == "kelvin_ker";
            let body = if first {
                let a = |s: i64| {
                    let w = cis(b.rat(3 * s, 4));
                    b.mul(&[cis(b.rat(-3 * s, 8)), seq(b.mul(&[w, *x]))])
                };
                b.mul(&[b.pow(b.div(b.int(2), pi), half), part(&a, real)])
            } else {
                let a = |s: i64| {
                    let v = b.mul(&[cis(b.rat(s, 4)), *x]);
                    let phase = b.add(&[b.mul(&[b.int(-s), *nu, half]), b.rat(-s, 8)]);
                    b.mul(&[cis(phase), b.call_named("exp", &[b.neg(v)]), seq(v)])
                };
                b.mul(&[b.pow(b.div(pi, b.int(2)), half), part(&a, real)])
            };
            // (the imaginary parts of the halves cancel: simplify drops them)
            b.mul(&[inv_sqrt(b, *x), b.call_named("simplify", &[b.call_named("complex_expand", &[body])])])
        }
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
        // L_n(x) of a symbolic x as a^n + cos(pi n) a^-n with a = x/2 + sqrt(1 + x^2/4) itself
        ("lucas_l", [n, x]) if !is_integer(b, *n) && b.ar.num(*x).is_none() => {
            let a = b.add(&[b.mul(&[half, *x]), b.pow(b.add(&[one, b.mul(&[b.rat(1, 4), b.powi(*x, 2)])]), half)]);
            let c = b.call_named("cos", &[b.mul(&[*n, pi])]);
            b.add(&[b.pow(a, *n), b.mul(&[c, b.pow(a, b.neg(*n))])])
        }
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
/// imaginary-modulus transformation 19.7.5); 1F1(n + 1/2; 2n + 2; w) and U(n + 3/2; 2n + 2; w) by modified Bessel
/// functions (`confluent_bessel_form`).
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
        // 1F1(1/2; 3/2; -s^2) = sqrt(pi) erf(s)/(2 s) (DLMF 13.6.7) and
        // by Kummer's transformation (DLMF 13.2.39) 1F1(1; 3/2; s^2) = sqrt(pi) e^(s^2) erf(s)/(2 s); erf(s)/s is even
        // and entire in s, so any square root of the argument serves (u for an argument u^2)
        ("hypergeometric_1f1", [a, c, z]) if *c == b.rat(3, 2) && (*a == half || *a == one) => {
            let w = if *a == half { b.neg(*z) } else { *z };
            let s = match b.call_of(w) {
                Some((p, &[u, two])) if p == b.hd.pow && two == b.int(2) => u,
                _ => b.pow(w, half),
            };
            let r = b.div(b.mul(&[b.pow(pi, half), b.call_named("erf", &[s])]), b.mul(&[b.int(2), s]));
            if *a == half {
                r
            } else {
                b.mul(&[b.call_named("exp", &[*z]), r])
            }
        }
        // at w = 0 the powers w^-n and K_n(0) are poles: the function's own value stands
        ("hypergeometric_1f1" | "hypergeometric_u", [a, c, w])
            if confluent_bessel(b, name, *a, *c).is_some() && !b.ar.num(*w).is_some_and(|n| n.is_zero()) =>
        {
            confluent_bessel_form(b, name, confluent_bessel(b, name, *a, *c)?, *w)
        }
        ("hankel_h1" | "hankel_h2", [nu, z]) => {
            let i = h.sym_term(h.sym("im"));
            let s = if name == "hankel_h1" { 1 } else { -1 };
            if let Some(r) = hankel_half(b, s, *nu, *z).or_else(|| hankel_third(b, s, *nu, *z)) {
                return Some(r);
            }
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
        // the Ferrers function of an order mu that is not an integer by Gauss's function,
        // ((1 + x)/(1 - x))^(mu/2) 2F1(nu + 1, -nu; 1 - mu; (1 - x)/2)/Gamma(1 - mu) (DLMF 14.3.1), the power split
        // as (1 + x)^(mu/2) (1 - x)^(-mu/2) (both bases positive on -1 < x < 1, where the function is defined)
        ("legendre_p", [nu, mu, x]) if rat(*mu).map_or(b.ar.num(*mu).is_none(), |q| !q.is_int()) => {
            let f = b.call_named("hypergeometric_2f1", &[b.add(&[*nu, one]), b.neg(*nu), b.sub(one, *mu), b.mul(&[half, b.sub(one, *x)])]);
            let hm = b.mul(&[half, *mu]);
            let p = b.mul(&[b.pow(b.add(&[one, *x]), hm), b.pow(b.sub(one, *x), b.neg(hm))]);
            b.div(b.mul(&[f, p]), g(b.sub(one, *mu)))
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
        // [n]_q! = (q; q)_n/(1 - q)^n = (q; q)_inf/((q^(n + 1); q)_inf (1 - q)^n) for a rational 0 < |q| < 1 (for
        // q < 0 with the principal power q^(n + 1), complex at a non-integer n: the definition the numeric value
        // takes)
        ("q_factorial", [n, q]) if matches!(small_fraction(b.heap(), *q), Some((k, d)) if k != 0 && k.abs() < d) => {
            let (k, d) = small_fraction(b.heap(), *q)?;
            let num = b.call_named("q_pochhammer", &[*q, *q]);
            let den = b.mul(&[b.call_named("q_pochhammer", &[unit_pow(b, k, d, b.add(&[*n, one])), *q]), unit_pow(b, d - k, d, *n)]);
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
        // E(1/2) = pi/(4 K) + K/2 by Legendre's relation 2 E K - K^2 = pi/2 at m = 1/2 (DLMF 19.7.1) with
        // K(1/2) = Gamma(1/4)^2/(4 sqrt(pi)); E(-1) = sqrt(2) E(1/2) (DLMF 19.7.5)
        ("elliptic_e", [m]) if *m == half || *m == b.int(-1) => {
            let g2 = b.powi(g(b.rat(1, 4)), 2);
            let e = b.add(&[b.div(b.pow(pi, b.rat(3, 2)), g2), b.div(g2, b.mul(&[b.int(8), b.pow(pi, half)]))]);
            if *m == half {
                e
            } else {
                b.mul(&[b.pow(b.int(2), half), e])
            }
        }
        _ => return more_special_rule(b, name, args),
    })
}

/// Further definitions in other functions:
/// - the Weber function of an order that is not an integer by its two power series (DLMF 11.10.2 with 11.10.1's
///   S1, S2 as 1F2 and Gamma(1 + nu/2) Gamma(1 - nu/2) = (pi nu/2)/sin(pi nu/2),
///   Gamma(3/2 + nu/2) Gamma(3/2 - nu/2) = pi (1 - nu^2)/(4 cos(pi nu/2)), DLMF 5.5.3):
///   E_nu(z) = 2 sin^2(pi nu/2)/(pi nu) 1F2(1; 1 - nu/2, 1 + nu/2; -z^2/4)
///   - 2 z cos^2(pi nu/2)/(pi (1 - nu^2)) 1F2(1; 3/2 - nu/2, 3/2 + nu/2; -z^2/4);
/// - the parabolic cylinder function of a negative integer order -n, 1 <= n <= 20, from
///   D_(-1)(z) = sqrt(pi/2) e^(z^2/4) erfc(z/sqrt(2)) (DLMF 12.7.5) and D_0 = e^(-z^2/4) by the recurrence
///   D_(nu-1) = (z D_nu - D_(nu+1))/nu (DLMF 12.8.1); of a rational order that is not an integer by the Hermite
///   function, 2^(-nu/2) e^(-z^2/4) H_nu(z/sqrt(2)) (DLMF 12.7.2 continued in nu);
/// - the q-factorial of an order that is not an integer by q-Pochhammer symbols, (q; q)_inf/(q^(z+1); q)_inf
///   (1 - q)^-z (Gasper and Rahman, Basic Hypergeometric Series, 1.10.1-1.10.3) for an exact 0 < q < 1 or a
///   symbolic q; for an exact q > 1 through p = 1/q, [z]_q! = q^(z (z - 1)/2) [z]_p!; for q = +-1 the q-gamma
///   function at z + 1;
/// - the incomplete beta function B_z(a, n) and B_z(n, b) with a positive integer n <= 30 by the binomial expansion
///   of (1 - t)^(n-1) (resp. t^(n-1) about t = 1): sum_j C(n-1, j) (-1)^j z^(a+j)/(a+j), and
///   B(n, b) - sum_j C(n-1, j) (-1)^j (1 - z)^(b+j)/(b+j) (DLMF 8.17.1, 8.17.4);
/// - Appell's F1 with c = b1 + b2 by Gauss's function, (1 - y)^-a 2F1(a, b1; c; (x - y)/(1 - y)) (DLMF 16.16.2).
fn more_special_rule<'h>(b: &B<'h>, name: &str, args: &[T<'h>]) -> Option<T<'h>> {
    let h = b.heap();
    let one = b.int(1);
    let half = b.rat(1, 2);
    let pi = h.sym_term(h.sym("pi"));
    let rat = |t: T<'h>| b.ar.num(t).and_then(|n| n.as_rat().cloned());
    let small_int = |t: T<'h>| match h.view(t) {
        mc_api::View::Int(n) => Some(n),
        _ => None,
    };
    // a number known not to be an integer, or a symbolic term
    let non_integer = |t: T<'h>| match b.ar.num(t) {
        Some(n) => n.as_rat().is_some_and(|q| !q.is_int()),
        None => !is_integer(b, t),
    };
    Some(match (name, args) {
        ("weber_e", [nu, z]) if non_integer(*nu) => {
            let hn = b.mul(&[half, *nu]);
            let w = b.mul(&[b.rat(-1, 4), b.powi(*z, 2)]);
            let pfq = |lo: Vec<T<'h>>| b.call_named("hypergeometric_pfq", &[b.call(b.hd.list, &[one]), b.call(b.hd.list, &lo), w]);
            let s = b.call_named("sin", &[b.mul(&[pi, hn])]);
            let c = b.call_named("cos", &[b.mul(&[pi, hn])]);
            let first = b.div(b.mul(&[b.int(2), b.powi(s, 2), pfq(vec![b.sub(one, hn), b.add(&[one, hn])])]), b.mul(&[pi, *nu]));
            let second = b.div(
                b.mul(&[b.int(2), *z, b.powi(c, 2), pfq(vec![b.sub(b.rat(3, 2), hn), b.add(&[b.rat(3, 2), hn])])]),
                b.mul(&[pi, b.sub(one, b.powi(*nu, 2))]),
            );
            b.sub(first, second)
        }
        ("parabolic_cylinder_d", [nu, z]) => {
            if let Some(n) = small_int(*nu).filter(|n| (-20..=-1).contains(n)) {
                let e = |s: i64| b.call_named("exp", &[b.mul(&[b.rat(s, 4), b.powi(*z, 2)])]);
                let erfc = b.call_named("erfc", &[b.mul(&[*z, b.pow(b.int(2), b.rat(-1, 2))])]);
                // (D_v, D_(v+1)) from v = -1 down
                let (mut cur, mut up) = (b.mul(&[b.pow(b.div(pi, b.int(2)), half), e(1), erfc]), e(-1));
                for v in (n + 1..=-1).rev() {
                    let next = b.div(b.sub(b.mul(&[*z, cur]), up), b.int(v));
                    (cur, up) = (next, cur);
                }
                cur
            } else {
                rat(*nu).filter(|q| !q.is_int())?;
                let hn = b.call_named("hermite_h", &[*nu, b.mul(&[*z, b.pow(b.int(2), b.rat(-1, 2))])]);
                b.mul(&[b.pow(b.int(2), b.mul(&[b.rat(-1, 2), *nu])), b.call_named("exp", &[b.mul(&[b.rat(-1, 4), b.powi(*z, 2)])]), hn])
            }
        }
        ("q_factorial", [x, q]) if non_integer(*x) => {
            let qp = |a: T<'h>, base: T<'h>| b.call_named("q_pochhammer", &[a, base]);
            let form = |p: T<'h>| {
                let num = qp(p, p);
                // a rational base p = k/d: the powers of unit fractions as integer powers (`(1/2)^-z` is `2^z`)
                let (up, down) = match small_fraction(h, p) {
                    Some((k, d)) => (unit_pow(b, k, d, b.add(&[*x, one])), unit_pow(b, d - k, d, b.neg(*x))),
                    None => (b.pow(p, b.add(&[*x, one])), b.pow(b.sub(one, p), b.neg(*x))),
                };
                b.mul(&[b.div(num, qp(up, p)), down])
            };
            match b.ar.num(*q) {
                None => form(*q),
                // a real exact base: positive (a negative one makes q^(z + 1) need a branch), or +-1
                Some(mc_arith::Num::Real(r)) if matches!(r, mc_arith::Real::Exact(_)) => {
                    let v = r.to_f64();
                    if v.abs() == 1.0 {
                        b.call_named("q_gamma", &[b.add(&[*x, one]), *q])
                    } else if v > 0.0 && v < 1.0 {
                        form(*q)
                    } else if v > 1.0 {
                        let p = match (small_int(*q), small_fraction(h, *q)) {
                            (Some(m), _) => b.rat(1, m),
                            (_, Some((k, d))) => b.rat(d, k),
                            _ => b.powi(*q, -1),
                        };
                        b.mul(&[b.pow(*q, b.mul(&[half, *x, b.sub(*x, one)])), form(p)])
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }
        ("beta", [z, a, c]) => {
            let ok = |t: T<'h>| small_int(t).filter(|n| (1..=30).contains(n));
            // the other parameter must not make a + j vanish
            let not_pole = |t: T<'h>| !(is_integer(b, t) && small_int(t).is_none_or(|v| v <= 0));
            let sum = |n: i64, s: T<'h>, base: T<'h>| -> Vec<T<'h>> {
                let mut c = 1i64;
                let mut out = vec![];
                for j in 0..n {
                    let sign = if j % 2 == 0 { c } else { -c };
                    let p = b.add(&[s, b.int(j)]);
                    out.push(b.div(b.mul(&[b.int(sign), b.pow(base, p)]), p));
                    c = c * (n - 1 - j) / (j + 1);
                }
                out
            };
            if let Some(n) = ok(*c).filter(|_| not_pole(*a)) {
                b.add(&sum(n, *a, *z))
            } else {
                let n = ok(*a).filter(|_| not_pole(*c))?;
                // B(n, b) = (n - 1)!/(b (b + 1) ... (b + n - 1))
                let fact: i64 = (1..n).product();
                let den = b.mul(&(0..n).map(|j| b.add(&[*c, b.int(j)])).collect::<Vec<_>>());
                let whole = b.div(b.int(fact), den);
                let tail = sum(n, *c, b.sub(one, *z));
                b.sub(whole, b.add(&tail))
            }
        }
        ("appell_f1", [a, b1, b2, c, x, y]) => {
            let sum = b.add(&[*b1, *b2]);
            let same = sum == *c || matches!((rat(*b1), rat(*b2), rat(*c)), (Some(p), Some(q), Some(r)) if p.add(&q) == r);
            if !same {
                return None;
            }
            let w = b.div(b.sub(*x, *y), b.sub(one, *y));
            b.mul(&[b.pow(b.sub(one, *y), b.neg(*a)), b.call_named("hypergeometric_2f1", &[*a, *b1, *c, w])])
        }
        _ => return last_special_rule(b, name, args),
    })
}

/// Definitions that complete the table:
/// - the logistic sigmoid 1/(1 + e^-x);
/// - the regularized incomplete gamma function of a half-integer a = n + 1/2 > 1 as Gamma(a, z)/Gamma(a) (and the
///   generalized Q(a, z0) - Q(a, z1)), whose numerator has the closed form above (DLMF 8.2.4);
/// - the double factorial of a symbolic order by its continuation
///   2^((1 - cos(pi n))/4 + n/2) pi^((cos(pi n) - 1)/4) Gamma(1 + n/2), which is n!! at every integer n >= -1
///   (Meserve, Amer. Math. Monthly 55, 1948; the form our numerical `factorial2` evaluates);
/// - the spherical Hankel functions of an integer order 0 <= n <= 20, h1_n(z) = e^(iz) sum_(k <= n)
///   i^(k - n - 1) a_k z^-(k + 1), a_k = (n + k)!/(2^k k! (n - k)!), and h2_n the same with -i (DLMF 10.49.6).
fn last_special_rule<'h>(b: &B<'h>, name: &str, args: &[T<'h>]) -> Option<T<'h>> {
    let h = b.heap();
    let one = b.int(1);
    let pi = h.sym_term(h.sym("pi"));
    let g = |x: T<'h>| b.call_named("gamma", &[x]);
    // a = n + 1/2 with 1 <= n <= 40 (the range of the closed form of Gamma(a, z))
    let half_order = |a: T<'h>| {
        b.ar.num(b.mul(&[b.int(2), a]))
            .and_then(|n| n.as_rat().cloned())
            .filter(|d| d.is_int())
            .and_then(|d| d.num.to_i64())
            .is_some_and(|n2| n2 % 2 != 0 && (3..=81).contains(&n2))
    };
    Some(match (name, args) {
        ("logistic_sigmoid", [x]) => b.powi(b.add(&[one, b.call_named("exp", &[b.neg(*x)])]), -1),
        ("gamma_regularized", [a, z]) if half_order(*a) => b.div(b.call_named("gamma", &[*a, *z]), g(*a)),
        ("gamma_regularized", [a, z0, z1]) if half_order(*a) => {
            b.div(b.sub(b.call_named("gamma", &[*a, *z0]), b.call_named("gamma", &[*a, *z1])), g(*a))
        }
        ("factorial2", [n]) if b.ar.num(*n).is_none() => {
            let c = b.call_named("cos", &[b.mul(&[pi, *n])]);
            let two = b.pow(b.int(2), b.add(&[b.mul(&[b.rat(1, 4), b.sub(one, c)]), b.mul(&[b.rat(1, 2), *n])]));
            let p = b.pow(pi, b.mul(&[b.rat(1, 4), b.sub(c, one)]));
            b.mul(&[two, p, g(b.add(&[one, b.mul(&[b.rat(1, 2), *n])]))])
        }
        // (the polynomial in 1/z written as one fraction, as `together` writes it)
        ("spherical_hankel_h1" | "spherical_hankel_h2", [n, z]) => {
            let mc_api::View::Int(n) = h.view(*n) else { return None };
            if !(0..=20).contains(&n) {
                return None;
            }
            let i = h.sym_term(h.sym("im"));
            let s = if name == "spherical_hankel_h1" { i } else { b.neg(i) };
            b.mul(&[b.call_named("exp", &[b.mul(&[s, *z])]), b.powi(*z, -n - 1), hankel_sum(b, n, s, *z)])
        }
        _ => return None,
    })
}

/// `z^n sum_(k <= n) s^(k - n - 1) a_k z^-k`, `a_k = (n + k)!/(2^k k! (n - k)!)`, for `0 <= n <= 20` and `s = +-i`:
/// the Hankel functions of half-integer order and the spherical ones are `e^(s z)` times it over a power of z (DLMF
/// 10.49.6, 10.47.10). The polynomial is written with a real leading coefficient: for an even n the unit
/// `s^(-n-1) = +-i` comes out (`i (-3 + 3 i z + z^2)`).
fn hankel_sum<'h>(b: &B<'h>, n: i64, s: T<'h>, z: T<'h>) -> T<'h> {
    let unit = n % 2 == 0;
    let mut terms = vec![];
    for k in 0..=n {
        // a_k = binomial(n + k, k) n!/(n - k)! 2^-k (each factor below 2^63 for n <= 20)
        let binom = (1..=k).fold(1i64, |c, j| c * (n + j) / j);
        let falling = (0..k).fold(1i64, |c, j| c * (n - j));
        let a = b.mul(&[b.int(binom), b.int(falling), b.pow(b.int(2), b.int(-k))]);
        let e = if unit { k } else { k - n - 1 };
        terms.push(b.mul(&[a, b.pow(s, b.int(e)), b.pow(z, b.int(n - k))]));
    }
    if unit {
        b.mul(&[b.pow(s, b.int(-n - 1)), b.add(&terms)])
    } else {
        b.add(&terms)
    }
}

/// The Hankel functions of a half-integer order `+-(n + 1/2)`, `0 <= n <= 20`, in exponentials:
/// `H1_(n+1/2)(z) = sqrt(2/pi) z^(-1/2) e^(iz) sum_(k <= n) i^(k - n - 1) a_k z^-k` (`sqrt(2 z/pi) h1_n(z)`, DLMF
/// 10.47.10 with 10.49.6), `H2` the same with `-i`, and the negative orders by `H1_(-v) = e^(i pi v) H1_v`,
/// `H2_(-v) = e^(-i pi v) H2_v` (DLMF 10.4.6). The sum is written as a polynomial over `z^(n + 1/2)`.
fn hankel_half<'h>(b: &B<'h>, s: i64, nu: T<'h>, z: T<'h>) -> Option<T<'h>> {
    let h = b.heap();
    let k2 = b.ar.num(b.mul(&[b.int(2), nu])).and_then(|n| n.as_rat().cloned()).filter(|q| q.is_int())?.num.to_i64()?;
    if k2 % 2 == 0 || !(-41..=41).contains(&k2) {
        return None;
    }
    let n = (k2.abs() - 1) / 2;
    let i = h.sym_term(h.sym("im"));
    let si = b.mul(&[b.int(s), i]);
    let pi = h.sym_term(h.sym("pi"));
    let half = b.rat(1, 2);
    // e^(+-i pi (n + 1/2)) = (-1)^n (+-i) for a negative order
    let turn = if k2 < 0 { b.mul(&[b.int(if n % 2 == 0 { 1 } else { -1 }), si]) } else { b.int(1) };
    let pre = b.mul(&[b.pow(b.int(2), half), b.pow(b.powi(pi, -1), half), b.pow(z, b.rat(-1 - 2 * n, 2))]);
    Some(b.mul(&[turn, pre, b.call_named("exp", &[b.mul(&[si, z])]), hankel_sum(b, n, si, z)]))
}

/// The Hankel functions of order `+-1/3` by the Airy functions at `w = -(3z/2)^(2/3)`: with
/// `c = sqrt(3)/sqrt(-w) = 2^(1/3) 3^(1/6) z^(-1/3)`, DLMF 9.6.6 (`Ai(-t) = (sqrt(t)/3)(J_(1/3) + J_(-1/3))`,
/// `Bi(-t) = sqrt(t/3)(J_(-1/3) - J_(1/3))` at `zeta = (2/3) t^(3/2)`) and 10.4.7 give
/// `J_(1/3) = -c (Bi - sqrt(3) Ai)/2`, `Y_(1/3) = -c (Ai + sqrt(3) Bi)/2`, `J_(-1/3) = c (sqrt(3) Ai + Bi)/2`,
/// `Y_(-1/3) = c (Ai - sqrt(3) Bi)/2`, and `H = J +- i Y`. Both sides are analytic in `z` off the negative axis,
/// where the principal powers are, and equal for `z > 0`.
fn hankel_third<'h>(b: &B<'h>, s: i64, nu: T<'h>, z: T<'h>) -> Option<T<'h>> {
    let h = b.heap();
    let plus = if nu == b.rat(1, 3) {
        true
    } else if nu == b.rat(-1, 3) {
        false
    } else {
        return None;
    };
    let i = h.sym_term(h.sym("im"));
    let r3 = b.pow(b.int(3), b.rat(1, 2));
    let w = b.mul(&[b.rat(-1, 2), b.pow(b.int(2), b.rat(1, 3)), b.pow(b.int(3), b.rat(2, 3)), b.pow(z, b.rat(2, 3))]);
    let (ai, bi) = (b.call_named("airy_ai", &[w]), b.call_named("airy_bi", &[w]));
    let c = b.mul(&[b.pow(b.int(2), b.rat(1, 3)), b.pow(b.int(3), b.rat(1, 6)), b.pow(z, b.rat(-1, 3))]);
    let (j, y) = if plus {
        (
            b.mul(&[b.rat(-1, 2), b.sub(bi, b.mul(&[r3, ai])), c]),
            b.mul(&[b.rat(-1, 2), b.add(&[ai, b.mul(&[r3, bi])]), c]),
        )
    } else {
        let half = b.rat(1, 2);
        (b.mul(&[half, b.add(&[b.mul(&[r3, ai]), bi]), c]), b.mul(&[half, b.sub(ai, b.mul(&[r3, bi])), c]))
    };
    Some(b.add(&[j, b.mul(&[b.int(s), i, y])]))
}

/// `cos`, `sin` or `cot` of `pi j/q`, `0 <= j <= q` (`0 < j < q` for cot), with the angle reduced to `[0, pi/4]`:
/// `cos(pi - t) = -cos(t)`, `cot(pi - t) = -cot(t)`, and above `pi/4` the cofunction of `pi/2 - t` (`cos(4 pi/13)` is
/// `sin(5 pi/26)`, `cot(7 pi/13)` is `-tan(pi/26)`).
fn octant<'h>(b: &B<'h>, f: &str, j: i64, q: i64) -> T<'h> {
    let pi = b.heap().sym_term(b.heap().sym("pi"));
    let at = |g: &str, n: i64, d: i64| b.call_named(g, &[b.mul(&[pi, b.rat(n, d)])]);
    let (mut j, mut sign) = (j, 1);
    if f != "sin" && 2 * j > q {
        j = q - j;
        sign = -1;
    }
    // the angle is now pi j/q in [0, pi/2]; above pi/4 it is pi/2 - pi (q - 2j)/(2q)
    let g = if 4 * j > q {
        let co = match f {
            "cos" => "sin",
            "sin" => "cos",
            _ => "tan",
        };
        at(co, q - 2 * j, 2 * q)
    } else {
        at(f, j, q)
    };
    if sign < 0 {
        b.neg(g)
    } else {
        g
    }
}

/// The order `n` (`0 <= n <= 20`) of the confluent functions next to the Bessel cases: `1F1(n + 1/2; 2n + 2; w)` and
/// `U(n + 3/2; 2n + 2; w)`.
fn confluent_bessel<'h>(b: &B<'h>, name: &str, a: T<'h>, c: T<'h>) -> Option<i64> {
    let int = |t: T<'h>| b.ar.num(t).and_then(|n| n.as_rat().cloned()).filter(|q| q.is_int()).and_then(|q| q.num.to_i64());
    let shift = if name == "hypergeometric_1f1" { b.rat(-1, 2) } else { b.rat(-3, 2) };
    let n = int(b.add(&[a, shift])).filter(|n| (0..=20).contains(n))?;
    (int(c)? == 2 * n + 2).then_some(n)
}

/// With `nu = a - 1/2` and the Bessel cases `1F1(a; 2a; 2z) = Gamma(1 + nu) e^z (z/2)^-nu I_nu(z)` and
/// `U(a; 2a; 2z) = pi^(-1/2) e^z (2z)^-nu K_nu(z)` (DLMF 13.6.9, 13.6.10), the derivatives
/// `d/dz (e^-z 1F1(a; b; z)) = ((a - b)/b) e^-z 1F1(a; b + 1; z)` and `d/dz U(a; b; z) = -a U(a + 1; b + 1; z)` (the
/// differentiation formulas of DLMF 13.3(ii)) with `(z^-nu I_nu)' = z^-nu I_(nu+1)`, `(z^-nu K_nu)' = -z^-nu K_(nu+1)` (DLMF 10.29.4) give, at `nu = n`, `w = 2z`:
/// `1F1(n + 1/2; 2n + 2; w) = n! e^(w/2) (w/4)^-n (I_n(w/2) - I_(n+1)(w/2))` and
/// `U(n + 3/2; 2n + 2; w) = pi^(-1/2) e^(w/2) w^-n (K_(n+1)(w/2) - K_n(w/2))/(2n + 1)`. Both sides are analytic off the
/// negative axis (1F1 is entire: the powers of w cancel against I_n of integer order).
fn confluent_bessel_form<'h>(b: &B<'h>, name: &str, n: i64, w: T<'h>) -> T<'h> {
    let hw = b.mul(&[b.rat(1, 2), w]);
    let e = b.call_named("exp", &[hw]);
    if name == "hypergeometric_1f1" {
        let i = |k: i64| b.call_named("bessel_i", &[b.int(k), hw]);
        let f = b.call_named("factorial", &[b.int(n)]);
        b.mul(&[f, e, b.pow(b.mul(&[b.rat(1, 4), w]), b.int(-n)), b.sub(i(n), i(n + 1))])
    } else {
        let k = |k: i64| b.call_named("bessel_k", &[b.int(k), hw]);
        let pi = b.heap().sym_term(b.heap().sym("pi"));
        b.mul(&[b.rat(1, 2 * n + 1), b.pow(pi, b.rat(-1, 2)), e, b.pow(w, b.int(-n)), b.sub(k(n + 1), k(n))])
    }
}

/// `x^(-1/2)` with the positive constant factors of a product taken out, `(c u)^(-1/2) = c^(-1/2) u^(-1/2)` for
/// `c > 0` (a positive rational or a rational power of one): `(x 2^(1/2))^(-1/2)` is `2^(-1/4) x^(-1/2)`.
fn inv_sqrt<'h>(b: &B<'h>, x: T<'h>) -> T<'h> {
    let minus_half = b.rat(-1, 2);
    let positive = |t: T<'h>| b.ar.num(t).and_then(|n| n.as_rat().cloned()).is_some_and(|q| !q.num.neg && !q.num.is_zero());
    let constant = |t: T<'h>| match b.call_of(t) {
        Some((p, &[base, e])) if p == b.hd.pow => positive(base) && b.ar.num(e).and_then(|n| n.as_rat().cloned()).is_some(),
        _ => positive(t),
    };
    let factors = match b.call_of(x) {
        Some((m, fs)) if m == b.hd.mul => fs.to_vec(),
        _ => return b.pow(x, minus_half),
    };
    let (cs, us): (Vec<T<'h>>, Vec<T<'h>>) = factors.into_iter().partition(|&t| constant(t));
    if cs.is_empty() || us.is_empty() {
        return b.pow(x, minus_half);
    }
    b.mul(&[b.pow(b.mul(&cs), minus_half), b.pow(b.mul(&us), minus_half)])
}

/// `(k/d)^e` for a fraction in lowest terms, a unit fraction `1/d` written `d^-e` (`(1/2)^-n` is `2^n`).
fn unit_pow<'h>(b: &B<'h>, k: i64, d: i64, e: T<'h>) -> T<'h> {
    if k == 1 && d > 1 {
        b.pow(b.int(d), b.neg(e))
    } else {
        b.pow(b.rat(k, d), e)
    }
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

/// `dirac_delta(p(x))` of a polynomial of degree 2 to 24 in one variable with rational coefficients, whose real
/// roots are simple and rational or quadratic irrationals: `sum_r dirac_delta(x - r)/abs(p'(r))` over the real roots
/// (the composition of the delta distribution with a function whose zeros are simple, Hormander, The Analysis of
/// Linear Partial Differential Operators I, Theorem 6.1.2; Gel'fand and Shilov, Generalized Functions I, B.3.1),
/// 0 when p has no real root. A real root of higher multiplicity, or one of an irreducible factor of degree 3 or
/// more, leaves the delta as it is.
fn dirac_compose<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let delta = h.sym("dirac_delta");
    if h.mask(t) & h.mask(h.sym_term(delta)) == 0 {
        return Ok(t);
    }
    let r = rewrite(cx, b, t, &mut |cx, b, node| match b.call_of(node) {
        Some((s, &[p])) if s == delta && b.ar.num(p).is_none() => delta_of_polynomial(cx, b, p),
        _ => Ok(None),
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}

fn delta_of_polynomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, p: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let int = |t: T<'h>| match h.view(t) {
        View::Int(n) => Some(n),
        _ => None,
    };
    let items = |t: T<'h>| match b.call_of(t) {
        Some((s, xs)) if s == b.hd.list => Some(xs.to_vec()),
        _ => None,
    };
    let vars = cx.eval(b.call_named("variables", &[p]))?;
    let Some(&[x]) = items(vars).as_deref() else { return Ok(None) };
    if !matches!(h.view(x), View::Sym(_)) || cx.eval(b.call_named("is_polynomial", &[p, x]))? != h.sym_term(h.sym("true")) {
        return Ok(None);
    }
    let Some(deg) = int(cx.eval(b.call_named("exponent", &[p, x]))?) else { return Ok(None) };
    if !(2..=24).contains(&deg) {
        return Ok(None);
    }
    let Some(cs) = items(cx.eval(b.call_named("coefficient_list", &[p, x]))?) else { return Ok(None) };
    if cs.iter().any(|&c| b.ar.num(c).and_then(|n| n.as_rat().cloned()).is_none()) {
        return Ok(None);
    }
    let Some(factors) = items(cx.eval(b.call_named("factor_list", &[p]))?) else { return Ok(None) };
    let mut roots = vec![];
    for fm in factors.iter().skip(1) {
        let Some(&[f, m]) = items(*fm).as_deref() else { return Ok(None) };
        let Some(d) = int(cx.eval(b.call_named("exponent", &[f, x]))?) else { return Ok(None) };
        let Some(real) = int(cx.eval(b.call_named("count_roots", &[f, x]))?) else { return Ok(None) };
        if real == 0 {
            continue;
        }
        if m != b.int(1) || d > 2 {
            return Ok(None);
        }
        let Some(fc) = items(cx.eval(b.call_named("coefficient_list", &[f, x]))?) else { return Ok(None) };
        if d == 1 {
            roots.push(b.neg(b.div(fc[0], fc[1])));
        } else {
            // two real roots (-b +- sqrt(b^2 - 4 a c))/(2 a)
            let disc = b.pow(b.sub(b.powi(fc[1], 2), b.mul(&[b.int(4), fc[2], fc[0]])), b.rat(1, 2));
            for sign in [1, -1] {
                roots.push(b.div(b.add(&[b.neg(fc[1]), b.mul(&[b.int(sign), disc])]), b.mul(&[b.int(2), fc[2]])));
            }
        }
    }
    let dp = cx.eval(b.call_named("d", &[p, x]))?;
    let mut terms = vec![];
    for r in roots {
        let r = cx.eval(r)?;
        let rule = b.call(h.sym(mc_api::names::RULE), &[x, r]);
        let slope = cx.eval(b.call_named("abs", &[b.call_named("rewrite", &[dp, rule])]))?;
        if slope == b.int(0) ||h.mask(slope) & h.mask(h.sym_term(h.sym("abs"))) != 0 {
            return Ok(None);
        }
        terms.push(b.div(b.call(h.sym("dirac_delta"), &[b.sub(x, r)]), slope));
    }
    Ok(Some(b.add(&terms)))
}

/// A rational number with small numerator and denominator as (num, den).
fn small_fraction<'h>(h: &'h mc_core::Heap, x: T<'h>) -> Option<(i64, i64)> {
    let View::Rational { num, den } = h.view(x) else { return None };
    match (h.view(num), h.view(den)) {
        (View::Int(n), View::Int(d)) if d > 0 => Some((n, d)),
        _ => None,
    }
}
