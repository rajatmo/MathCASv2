//! Special functions as candidates of the ansatz: monomials with known derivatives that are not in the integrand
//! but may be in its antiderivative (Bronstein 2007, section 4: "the special functions to be added to the field").
//!
//! For the integrand's monomials:
//!
//! - an exponential `exp(u)`, `u = a h^2 + b h + c` with `h = x`, or `h` an inner polynomial found by functional
//!   decomposition (`u = g(h)` with `g` quadratic, F2's `right_component`): `erf(sqrt(-a) (h + b/(2a)))` when `a`
//!   looks negative, else `erfi(sqrt(a) (h + b/(2a)))`, both with derivative
//!   `2/sqrt(pi) exp(b^2/(4a) - c) exp(u) Dv`;
//! - an exponential of a polynomial `u` (linear, or without a constant term): `exp_integral_ei(u)`
//!   (`D = exp(u)/u Du`), or the hyperbolic sine and cosine integrals of a linear `u` when the integrand was
//!   hyperbolic; with an algebraic monomial `x^a`, the incomplete gamma function
//!   `x^(a+1) (-k x)^(-a-1) gamma(a + 1, -k x)` of derivative `-x^a exp(k x)` (`u = k x`);
//! - a pair `sin(u)`, `cos(u)`: `sin_integral(u)`, `cos_integral(u)` for a polynomial `u`; for `u = a h^2 + b h + c`
//!   (h as above), `fresnel_s(v)` and `fresnel_c(v)` with `v = sqrt(2a/pi) (h + b/(2a))`;
//! - a logarithm `log(u)`: `log_integral(u)`, and for `u = x` also `exp_integral_ei(k log(x))`, `k = 2, 3, 4`
//!   (`D = x^(k-1)/log(x)`);
//! - an algebraic monomial `(1 - m sin(phi)^2)^(1/2)`: `elliptic_e(phi, m)` and `elliptic_f(phi, m)`, and with a factor
//!   `1/(1 - n sin(phi)^2)` of the integrand `elliptic_pi(n, phi, m)`;
//! - a quadratic and a linear exponential together (`exp(x) erf(x)`): `erf` of the completed square of their sum.
use crate::terms::{coeffs_in, fun, has_call, is_free, rat_of, Small};
use mc_arith::num::Rat;
use mc_arith::{Arith, Num};
use mc_core::Heap;
use mc_poly::decompose::right_component;
use mc_poly::ring::Rationals;
use mc_poly::work::Meter;
use crate::tower::{Builder, Kind};
use mc_api::{Store, View, T};

/// Does `a` look negative (a negative number, or a product with a negative numeric factor)?
fn negative<'h>(b: &Builder<'_, 'h>, a: T<'h>) -> bool {
    if let Some(n) = b.ar.num(a) {
        return n.as_real().is_some_and(|r| r.is_negative());
    }
    if let Some((name, args)) = fun(b.h, a) {
        if name == "mul" {
            return args.iter().any(|&f| b.ar.num(f).and_then(|n| n.as_real().cloned()).is_some_and(|r| r.is_negative()));
        }
    }
    false
}

/// Register the candidates for the integrand `f` (original form) on the builder.
pub fn add<'h>(b: &mut Builder<'_, 'h>, f: T<'h>) {
    let ar = b.ar;
    let h = b.h;
    let x = b.x;
    let call = |n: &str, a: &[T<'h>]| h.call(h.sym(n), a);
    let half = ar.rational(ar.int(1), ar.int(2));
    let pi = h.sym_term(h.sym("pi"));
    let hyperbolic = ["sinh", "cosh", "tanh", "coth", "sech", "csch"].iter().any(|n| has_call(h, f, n));
    let mut cands: Vec<(T<'h>, T<'h>, T<'h>)> = vec![];
    // exponentials
    let powers: Vec<(T<'h>, T<'h>)> = b
        .vars
        .iter()
        .filter_map(|v| match (&v.kind, h.view(v.term)) {
            (Kind::Power { .. }, View::App { args: [base, e], .. }) if *base == x => Some((*base, *e)),
            _ => None,
        })
        .collect();
    let isqrtpi = ar.pow(pi, ar.neg(half));
    for (gen, var, used) in b.exp_groups() {
        if var.is_none() {
            continue;
        }
        for n in used {
            let u = ar.mul(&[ar.int(n), gen]);
            let Some(cs) = coeffs_in(ar, u, x) else { continue };
            if cs.len() < 2 {
                continue;
            }
            let zero_const = b.ar.num(cs[0]).is_some_and(|z| z.is_exact_zero());
            if hyperbolic && cs.len() == 2 {
                cands.push((call("sinh_integral", &[u]), ar.div(call("sinh", &[u]), u), u));
                cands.push((call("cosh_integral", &[u]), ar.div(call("cosh", &[u]), u), u));
            } else if zero_const || cs.len() == 2 {
                // exp(u) Du / u
                cands.push((call("exp_integral_ei", &[u]), ar.div(call("exp", &[u]), u), u));
            }
            // incomplete gamma with x^a: u = k x
            if cs.len() == 2 && zero_const {
                let k = cs[1];
                for &(_, a) in &powers {
                    let a1 = ar.add(&[a, ar.int(1)]);
                    let mkx = ar.neg(ar.mul(&[k, x]));
                    let g = call("gamma", &[a1, mkx]);
                    // x^(a+1) (-k x)^(-a-1) is 1 for k = -1
                    let t = if mkx == x { g } else { ar.mul(&[ar.pow(x, a1), ar.pow(mkx, ar.sub(ar.int(-1), a)), g]) };
                    let fac = ar.neg(ar.mul(&[ar.pow(x, a), call("exp", &[u])]));
                    cands.push((t, fac, x));
                }
            }
            // u = a h^2 + b h + c for an inner polynomial h (h = x, or a right component by decomposition)
            if let Some((a, bb, c, hh)) = quadratic_inner(ar, &cs, x) {
                let shift = ar.add(&[hh, ar.div(bb, ar.mul(&[ar.int(2), a]))]);
                let k = ar.sub(ar.div(ar.mul(&[bb, bb]), ar.mul(&[ar.int(4), a])), c);
                let fac = ar.mul(&[ar.int(2), isqrtpi, call("exp", &[k]), call("exp", &[u])]);
                let (name, s) = if negative(b, a) { ("erf", ar.pow(ar.neg(a), half)) } else { ("erfi", ar.pow(a, half)) };
                let v = ar.mul(&[s, shift]);
                cands.push((call(name, &[v]), fac, v));
            }
        }
    }
    // e^(l(x)) erf(x)-like products: a quadratic and a linear exponential together, exp(q) exp(l) = exp(q + l),
    // give erf of the completed square of q + l (the two monomials stay apart in the factor)
    let members: Vec<(T<'h>, usize)> = b
        .exp_groups()
        .into_iter()
        .filter(|(_, v, _)| v.is_some())
        .flat_map(|(gen, _, used)| used.into_iter().map(move |n| (gen, n)).collect::<Vec<_>>())
        .filter_map(|(gen, n)| {
            let u = ar.mul(&[ar.int(n), gen]);
            coeffs_in(ar, u, x).map(|cs| (u, cs.len()))
        })
        .collect();
    for &(q, lq) in &members {
        for &(l, ll) in &members {
            if lq != 3 || ll != 2 {
                continue;
            }
            let u = ar.add(&[q, l]);
            let Some(cs) = coeffs_in(ar, u, x) else { continue };
            if cs.len() != 3 {
                continue;
            }
            let (a, bb, c) = (cs[2], cs[1], cs[0]);
            let shift = ar.add(&[x, ar.div(bb, ar.mul(&[ar.int(2), a]))]);
            let k = ar.sub(ar.div(ar.mul(&[bb, bb]), ar.mul(&[ar.int(4), a])), c);
            let fac = ar.mul(&[ar.int(2), isqrtpi, call("exp", &[k]), call("exp", &[q]), call("exp", &[l])]);
            let (name, s) = if negative(b, a) { ("erf", ar.pow(ar.neg(a), half)) } else { ("erfi", ar.pow(a, half)) };
            let v = ar.mul(&[s, shift]);
            cands.push((call(name, &[v]), fac, v));
        }
    }
    // trigonometric pairs
    for (u, _, _) in b.trig_pairs() {
        let Some(cs) = coeffs_in(ar, u, x) else { continue };
        if cs.len() < 2 {
            continue;
        }
        if cs.len() == 2 || b.ar.num(cs[0]).is_some_and(|z| z.is_exact_zero()) {
            cands.push((call("sin_integral", &[u]), ar.div(call("sin", &[u]), u), u));
            cands.push((call("cos_integral", &[u]), ar.div(call("cos", &[u]), u), u));
        }
        if let Some((a, bb, c, hh)) = quadratic_inner(ar, &cs, x) {
            // pi v^2 / 2 = u - k, v = sqrt(2a/pi) (h + b/(2a)), k = c - b^2/(4a)
            let shift = ar.add(&[hh, ar.div(bb, ar.mul(&[ar.int(2), a]))]);
            let k = ar.sub(c, ar.div(ar.mul(&[bb, bb]), ar.mul(&[ar.int(4), a])));
            let v = ar.mul(&[ar.pow(ar.mul(&[ar.int(2), a]), half), ar.pow(pi, ar.neg(half)), shift]);
            let (su, cu) = (call("sin", &[u]), call("cos", &[u]));
            let (fs, fc) = if b.ar.num(k).is_some_and(|z| z.is_exact_zero()) {
                (su, cu)
            } else {
                let (sk, ck) = (call("sin", &[k]), call("cos", &[k]));
                (ar.sub(ar.mul(&[su, ck]), ar.mul(&[cu, sk])), ar.add(&[ar.mul(&[cu, ck]), ar.mul(&[su, sk])]))
            };
            cands.push((call("fresnel_s", &[v]), fs, v));
            cands.push((call("fresnel_c", &[v]), fc, v));
        }
    }
    // logarithms
    let logs: Vec<T<'h>> = b
        .vars
        .iter()
        .filter_map(|v| match &v.kind {
            Kind::Log { .. } => fun(h, v.term).map(|(_, a)| a[0]),
            _ => None,
        })
        .collect();
    for u in logs {
        if is_free(h, u, x) {
            continue;
        }
        let lg = call("log", &[u]);
        cands.push((call("log_integral", &[u]), ar.pow(lg, ar.int(-1)), u));
        if u == x {
            for k in 2..=4 {
                let t = call("exp_integral_ei", &[ar.mul(&[ar.int(k), lg])]);
                cands.push((t, ar.mul(&[ar.pow(x, ar.int(k - 1)), ar.pow(lg, ar.int(-1))]), x));
            }
        }
    }
    // elliptic integrals of the second and first kind: (1 - m sin(phi)^2)^(1/2)
    let ell: Vec<(T<'h>, T<'h>, T<'h>)> = b
        .vars
        .iter()
        .filter_map(|v| match (&v.kind, h.view(v.term)) {
            (Kind::Power { q: Some(2), .. }, View::App { args: [base, _], .. }) => Some(*base),
            _ => None,
        })
        .filter_map(|base| {
            for (phi, _, _) in b.trig_pairs() {
                let s = call("sin", &[phi]);
                if let Some(cs) = coeffs_in(ar, base, s) {
                    let one = cs.first().is_some_and(|&c| c == ar.int(1));
                    if cs.len() == 3 && one && b.ar.num(cs[1]).is_some_and(|z| z.is_exact_zero()) {
                        return Some((phi, ar.neg(cs[2]), base));
                    }
                }
            }
            None
        })
        .collect();
    for (phi, m, base) in ell {
        cands.push((call("elliptic_e", &[phi, m]), ar.pow(base, half), phi));
        cands.push((call("elliptic_f", &[phi, m]), ar.pow(base, ar.neg(half)), phi));
        // the third kind: a factor 1/(1 - n sin(phi)^2) of the integrand
        let s = call("sin", &[phi]);
        for d in reciprocal_bases(b, f) {
            if d == base {
                continue;
            }
            let Some(cs) = coeffs_in(ar, d, s) else { continue };
            let one = cs.first().is_some_and(|&c| c == ar.int(1));
            if cs.len() == 3 && one && b.ar.num(cs[1]).is_some_and(|z| z.is_exact_zero()) && is_free(h, cs[2], x) {
                let n = ar.neg(cs[2]);
                let fac = ar.mul(&[ar.pow(d, ar.int(-1)), ar.pow(base, ar.neg(half))]);
                cands.push((call("elliptic_pi", &[n, phi, m]), fac, phi));
            }
        }
    }
    for (t, fac, arg) in cands {
        b.candidate_with(t, fac, arg);
    }
}

/// `u = a h^2 + b h + c` (coefficients `cs` in x, constant first) with `h = x` for a quadratic `u`, or `h` the
/// right component of degree `deg u / 2` of a decomposition `u = g(h)` (F2's `right_component`, rational
/// coefficients) for a quadratic `g`: `(a, b, c, h)` as terms.
fn quadratic_inner<'h>(ar: &Arith<'h, Heap>, cs: &[T<'h>], x: T<'h>) -> Option<(T<'h>, T<'h>, T<'h>, T<'h>)> {
    let n = cs.len() - 1;
    if n == 2 {
        return Some((cs[2], cs[1], cs[0], x));
    }
    if n < 4 || n % 2 == 1 {
        return None;
    }
    let qs: Option<Vec<Rat>> = cs.iter().map(|&c| rat_of(ar, c)).collect();
    let qs = qs?;
    let mut w = Small(1 << 12);
    let mut m = Meter::new(&mut w);
    let (g, hp) = right_component(&Rationals, &qs, n / 2, None, &mut m).ok()???;
    if g.len() != 3 {
        return None;
    }
    let num = |q: &Rat| ar.number(&Num::rat(q.clone()));
    let mut ht = vec![];
    for (k, q) in hp.iter().enumerate() {
        if !q.is_zero() {
            ht.push(ar.mul(&[num(q), ar.pow(x, ar.int(k as i64))]));
        }
    }
    Some((num(&g[2]), num(&g[1]), num(&g[0]), ar.add(&ht)))
}

/// The bases `d` of the factors `d^k`, `k` a negative integer, of `f` (and of its subterms).
fn reciprocal_bases<'h>(b: &Builder<'_, 'h>, f: T<'h>) -> Vec<T<'h>> {
    let h = b.h;
    let mut out = vec![];
    let mut stack = vec![f];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if let Some((n, args)) = fun(h, u) {
            if n == "pow" && args.len() == 2 && crate::terms::int_of(h, args[1]).is_some_and(|k| k < 0) && !out.contains(&args[0]) {
                out.push(args[0]);
            }
            stack.extend_from_slice(args);
        }
    }
    out
}
