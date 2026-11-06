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
//!
//! Anything else is left as it is.
use crate::term::{fixed, B};
use mc_api::{Store, T};
use mc_eval::{NativeCx, Res};

/// `function_expand(e)`.
pub fn function_expand<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    fixed(cx, b, t, 8, &mut |_cx, b, node| Ok(rule(b, node)))
}

fn rule<'h>(b: &B<'h>, node: T<'h>) -> Option<T<'h>> {
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
        ("sec", [x]) => b.powi(b.call_named("cos", &[*x]), -1),
        ("csc", [x]) => b.powi(b.call_named("sin", &[*x]), -1),
        ("cot", [x]) => b.div(b.call_named("cos", &[*x]), b.call_named("sin", &[*x])),
        ("sech", [x]) => b.powi(b.call_named("cosh", &[*x]), -1),
        ("csch", [x]) => b.powi(b.call_named("sinh", &[*x]), -1),
        ("coth", [x]) => b.div(b.call_named("cosh", &[*x]), b.call_named("sinh", &[*x])),
        ("spherical_bessel_j", [n, x]) if *n == b.int(0) => b.div(b.call_named("sin", &[*x]), *x),
        ("spherical_bessel_y", [n, x]) if *n == b.int(0) => b.neg(b.div(b.call_named("cos", &[*x]), *x)),
        ("bessel_j" | "bessel_y" | "bessel_i" | "bessel_k", [nu, x]) => {
            let plus = *nu == half;
            let minus = *nu == b.rat(-1, 2);
            if !plus && !minus {
                return None;
            }
            let (sin, cos) = (b.call_named("sin", &[*x]), b.call_named("cos", &[*x]));
            match (name.as_str(), plus) {
                ("bessel_j", true) => b.mul(&[root(*x), sin]),
                ("bessel_j", false) => b.mul(&[root(*x), cos]),
                ("bessel_y", true) => b.neg(b.mul(&[root(*x), cos])),
                ("bessel_y", false) => b.mul(&[root(*x), sin]),
                ("bessel_i", true) => b.mul(&[root(*x), b.call_named("sinh", &[*x])]),
                ("bessel_i", false) => b.mul(&[root(*x), b.call_named("cosh", &[*x])]),
                // K_(1/2) = K_(-1/2) = sqrt(pi/(2x)) exp(-x)
                _ => b.mul(&[b.pow(b.div(pi, b.mul(&[b.int(2), *x])), half), b.call_named("exp", &[b.neg(*x)])]),
            }
        }
        _ => return None,
    })
}
