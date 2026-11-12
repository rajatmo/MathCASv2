//! Closed forms of definite integrals (item 14D, step 4 of the driver): Euler's gamma integral and its
//! derivatives, Gaussian integrals, Beta integrals of binomials. Convergence (and the conditions it needs) was
//! settled before; these formulas are applied only to integrands of exactly their shape.
//!
//! - `∫_0^∞ x^r exp(-a x^d) dx = Γ(s)/(d a^s)` with `s = (r + 1)/d`, for `re(a) > 0`, `re(s) > 0`, `d > 0`
//!   (DLMF 5.9.1 after `t = a x^d`); a factor `log(x)^k` is the k-th derivative in `r` (differentiation under the
//!   integral sign), so `∫_0^∞ log(x) exp(-x^2) dx = Γ'(1/2)/4 = -√π (γ + 2 log 2)/4`.
//! - `∫_{-∞}^{∞} P(x) exp(-a x^2 + b x + c) dx`: the square completed, `x = y + b/(2a)` (a shift of the contour,
//!   allowed since the integrand is entire and decays in the strip), the polynomial expanded in `y`, and
//!   `∫ y^{2j} exp(-a y^2) dy = Γ(j + 1/2)/a^{j + 1/2}` (DLMF 5.9.1). Factors `sin` and `cos` of linear arguments
//!   are written as exponentials first.
//! - Beta integrals (DLMF 5.12.1-5.12.3): `∫_a^b (x - a)^p (b - x)^q dx = (b - a)^{p+q+1} B(p + 1, q + 1)`;
//!   `∫_0^L x^α (A + B x^d)^β dx = A^β L^{α+1} B((α + 1)/d, β + 1)/d` with `A + B L^d = 0`, `A > 0`;
//!   `∫_0^∞ x^α (A + B x^d)^{-s} dx = A^{-s} (A/B)^μ B(μ, s - μ)/d` with `μ = (α + 1)/d`, `A, B > 0`. Beta
//!   functions are written as gamma quotients. Integrands with only integer exponents are left to the residue and
//!   Newton-Leibniz methods, whose answers are elementary, except `x^α/(A + B x^d)` with d odd, which the residue
//!   method (over the whole line) does not take: `B(μ, 1 - μ) = π/sin(π μ)` (DLMF 5.5.3, 5.12.1).
//! - (round 19, Q19-34) `∫_0^∞ x^(s-1)/sinh(a x) dx = 2 (1 - 2^-s) Γ(s) ζ(s)/a^s` for `s > 1` (DLMF 25.5.1 with
//!   `1/sinh(t) = 2 Σ e^(-(2k+1) t)`); `∫_0^∞ e^(-a x) sin(b x)/x dx = atan(b/a)` for `a > 0` (the Laplace
//!   transform of `sin(b x)/x`, `∫_a^∞ b/(t^2 + b^2) dt`); `∫_0^L log(1 + c x)/x dx = -Li_2(-c L)`
//!   (DLMF 25.12.2) for `c L >= -1`.
use super::{call_of, real_range, sign, subst, Job, Pt};
use crate::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The closed forms of the module comment, tried in order.
pub(crate) fn closed<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<T<'h>>> {
    match (lo, hi) {
        (Pt::Fin(z), Pt::PosInf) if z == b.zero() => {
            if let Some(r) = gamma_form(cx, b, f, x)? {
                return Ok(Some(r));
            }
            if let Some(r) = bessel_j_integral(cx, b, f, x)? {
                return Ok(Some(r));
            }
            if let Some(r) = bose_fermi(cx, b, f, x)? {
                return Ok(Some(r));
            }

            if let Some(r) = damped_trig(job, cx, b, f, x)? {
                return Ok(Some(r));
            }
            beta_half_line(cx, b, f, x)
        }
        (Pt::NegInf, Pt::PosInf) => gaussian(job, cx, b, f, x),
        (Pt::NegInf, Pt::Fin(z)) if z == b.zero() => {
            // x -> -x: the forms over (0, inf)
            let g = subst(cx, b, f, x, b.neg(x))?;
            closed(job, cx, b, g, x, Pt::Fin(b.zero()), Pt::PosInf)
        }
        (Pt::Fin(l), Pt::Fin(r)) => {
            if let Some(v) = beta_interval(cx, b, f, x, l, r)? {
                return Ok(Some(v));
            }
            if l == b.zero() {
                if let Some(v) = beta_binomial(cx, b, f, x, r)? {
                    return Ok(Some(v));
                }
                if let Some(v) = dilog(cx, b, f, x, r)? {
                    return Ok(Some(v));
                }
                return gamma_finite(cx, b, f, x, r);
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

/// The closed forms whose own conditions imply convergence (`x^r/sinh(a x)` with `r > 0`, `e^(-a x) sin(b x)/x`
/// with `a > 0`, over `(0, inf)`): tried also when the convergence test did not decide (it does not know the decay
/// of `1/sinh`).
pub(crate) fn self_checked<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, lo: Pt<'h>, hi: Pt<'h>) -> Res<Option<T<'h>>> {
    match (lo, hi) {
        (Pt::Fin(z), Pt::PosInf) if z == b.zero() => {
            if let Some(r) = over_sinh(cx, b, f, x)? {
                return Ok(Some(r));
            }
            damped_sinc(cx, b, f, x)
        }
        _ => Ok(None),
    }
}

/// The factors of a product (the term itself otherwise).
fn factors<'h>(b: &Build<'h>, f: T<'h>) -> Vec<T<'h>> {
    match args_of(b, f, b.hd.mul) {
        Some(fs) => fs.to_vec(),
        None => vec![f],
    }
}

/// The exponent `u` of an exponential factor `exp(u)`, `e^u` or `c^u` (as `u log(c)`), with `u` depending on `x`.
fn exponent_of<'h>(b: &Build<'h>, g: T<'h>, x: T<'h>) -> Option<T<'h>> {
    match call_of(b, g)? {
        (n, [u]) if n == "exp" => Some(*u),
        (n, [c, u]) if n == "pow" && !is_free(b, *u, x) && is_free(b, *c, x) => {
            if b.sym_of(*c) == Some(b.hd.e) {
                Some(*u)
            } else {
                Some(b.mul(&[*u, b.call("log", &[*c])]))
            }
        }
        _ => None,
    }
}

/// `x^p` as `p` (`x` itself is `1`).
fn power_of_x<'h>(b: &Build<'h>, g: T<'h>, x: T<'h>) -> Option<T<'h>> {
    if g == x {
        return Some(b.one());
    }
    match call_of(b, g)? {
        (n, [base, p]) if n == "pow" && *base == x && is_free(b, *p, x) => Some(*p),
        _ => None,
    }
}

/// `c x^d` as `(c, d)`; a sum `c1 x^d + c2 x^d` too.
fn monomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let u = apply(cx, b, "expand", &[u])?;
    if let Some(ts) = args_of(b, u, b.hd.add) {
        let Some(&t) = ts.iter().find(|&&t| !is_free(b, t, x)) else { return Ok(None) };
        let Some((_, d)) = monomial_term(b, t, x) else { return Ok(None) };
        let c = apply(cx, b, "expand", &[b.div(u, b.pow(x, d))])?;
        return Ok(is_free(b, c, x).then_some((c, d)));
    }
    Ok(monomial_term(b, u, x))
}

/// One term `c x^d` as `(c, d)`.
fn monomial_term<'h>(b: &Build<'h>, u: T<'h>, x: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let mut c = vec![];
    let mut d = None;
    for g in factors(b, u) {
        if is_free(b, g, x) {
            c.push(g);
        } else if let (Some(p), None) = (power_of_x(b, g, x), d) {
            d = Some(p);
        } else {
            return None;
        }
    }
    d.map(|d| (b.mul(&c), d))
}

/// `∫_0^∞ C x^r log(x)^k exp(-a x^d) dx` (module comment).
fn gamma_form<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let lx = b.call("log", &[x]);
    let (mut c, mut r, mut k, mut us) = (vec![], vec![], 0i64, vec![]);
    for g in factors(b, f) {
        if is_free(b, g, x) {
            c.push(g);
        } else if let Some(p) = power_of_x(b, g, x) {
            r.push(p);
        } else if let Some(u) = exponent_of(b, g, x) {
            us.push(u);
        } else if g == lx {
            k += 1;
        } else if let Some((n, [base, e])) = call_of(b, g) {
            match (n.as_str(), h.view(*e)) {
                ("pow", View::Int(j @ 1..=8)) if *base == lx => k += j,
                _ => return Ok(None),
            }
        } else {
            return Ok(None);
        }
    }
    if us.is_empty() {
        return Ok(None);
    }
    let Some((cu, d)) = monomial(cx, b, b.add(&us), x)? else { return Ok(None) };
    if sign(cx, b, d)? != Some(1) {
        return Ok(None);
    }
    let a = cx.eval(b.neg(cu))?;
    let rho = h.sym_term(h.sym("r$gamma"));
    let s = b.div(b.add(&[rho, b.one()]), d);
    let mut g = b.div(b.call("gamma", &[s]), b.mul(&[d, b.pow(a, s)]));
    for _ in 0..k {
        g = apply(cx, b, "d", &[g, rho])?;
    }
    let rv = cx.eval(b.add(&r))?;
    let g = subst(cx, b, g, rho, rv)?;
    Ok(Some(b.mul(&[b.mul(&c), g])))
}

/// `∫_{-∞}^{∞} C P(x) exp(-a x^2 + b x + c) dx` (module comment); trigonometric factors as exponentials first.
fn gaussian<'h>(job: &mut Job<'_, 'h>, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let (mut c, mut poly, mut us, mut trig) = (vec![], vec![], vec![], false);
    for g in factors(b, f) {
        if is_free(b, g, x) {
            c.push(g);
        } else if let Some(u) = exponent_of(b, g, x) {
            us.push(u);
        } else if matches!(call_of(b, g), Some((n, [_])) if n == "sin" || n == "cos") {
            trig = true;
        } else if let Some((n, [base, _])) = call_of(b, g) {
            if n == "pow" && matches!(call_of(b, *base), Some((m, [_])) if m == "sin" || m == "cos") {
                trig = true;
            } else {
                poly.push(g);
            }
        } else {
            poly.push(g);
        }
    }
    if us.is_empty() {
        return Ok(None);
    }
    let q = apply(cx, b, "expand", &[b.add(&us)])?;
    let Some(qs) = super::points::coefficients(cx, b, q, x)? else { return Ok(None) };
    let [q0, q1, q2] = qs.as_slice() else { return Ok(None) };
    if trig {
        // sin and cos as exponentials: a sum of Gaussians, term by term
        let g = apply(cx, b, "trig_to_exp", &[f])?;
        let g = apply(cx, b, "expand", &[g])?;
        if g == f || args_of(b, g, b.hd.add).is_none() {
            return Ok(None);
        }
        let Some(v) = real_range(job, cx, b, g, x, Pt::NegInf, Pt::PosInf)? else { return Ok(None) };
        if super::is_failure(b, v) {
            return Ok(None);
        }
        return Ok(Some(super::residue::real_form(cx, b, f, v)?));
    }
    let p = cx.eval(b.mul(&poly))?;
    let Some(_) = super::points::coefficients(cx, b, p, x)? else { return Ok(None) };
    let a = cx.eval(b.neg(*q2))?;
    if a == b.zero() {
        return Ok(None);
    }
    let m = b.div(*q1, b.mul(&[b.int(2), a]));
    let shifted = subst(cx, b, p, x, b.add(&[x, m]))?;
    let shifted = apply(cx, b, "expand", &[shifted])?;
    let Some(cs) = super::points::coefficients(cx, b, shifted, x)? else { return Ok(None) };
    let mut terms = vec![];
    for (j, &cj) in cs.iter().enumerate() {
        if j % 2 == 1 || cj == b.zero() {
            continue;
        }
        let s = b.rat(j as i64 + 1, 2);
        terms.push(b.mul(&[cj, b.call("gamma", &[s]), b.pow(a, b.neg(s))]));
    }
    let ex = b.add(&[*q0, b.div(b.mul(&[*q1, *q1]), b.mul(&[b.int(4), a]))]);
    Ok(Some(b.mul(&[b.mul(&c), b.call("exp", &[ex]), b.add(&terms)])))
}

/// `∫_0^∞ J_nu(a x) dx = 1/a` for `a > 0` and `nu > -1` (DLMF 10.22.1 at `mu = 0`, the Mellin transform of the
/// Bessel function at `s = 1`).
pub(crate) fn bessel_j_integral<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let Some(("bessel_j", [nu, arg])) = call_of(b, f).map(|(n, a)| (if n == "bessel_j" { "bessel_j" } else { "" }, a)) else { return Ok(None) };
    if !is_free(b, *nu, x) {
        return Ok(None);
    }
    let a = apply(cx, b, "together", &[b.div(*arg, x)])?;
    if !is_free(b, a, x) || sign(cx, b, a)? != Some(1) {
        return Ok(None);
    }
    if sign(cx, b, b.add(&[*nu, b.one()]))? != Some(1) {
        return Ok(None);
    }
    Ok(Some(b.powi(a, -1)))
}

/// [`gaussian`] for an integrand whose convergence the analysis could not decide (a complex parameter in a linear
/// factor `sin(w x)`, `cos(w x)`, `exp(w x)`): the Gaussian factor `exp(-a x^2 + ...)` with a positive number `a`
/// dominates every exponential growth, so the integral converges for every `w` (and the value is entire in `w`).
pub(crate) fn dominated_gaussian<'h>(job: &mut Job<'_, 'h>, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let mut us = vec![];
    for g in factors(b, f) {
        if let Some(u) = exponent_of(b, g, x) {
            us.push(u);
        } else if !is_free(b, g, x) {
            // every other factor depending on x must be a sin or cos of a linear argument
            match call_of(b, g) {
                Some((n, [arg])) if n == "sin" || n == "cos" => {
                    let ex = apply(cx, b, "expand", &[*arg])?;
                    let c = super::points::coefficients(cx, b, ex, x)?;
                    if !matches!(c.as_deref(), Some([_, _])) {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            }
        }
    }
    if us.is_empty() {
        return Ok(None);
    }
    let q = apply(cx, b, "expand", &[b.add(&us)])?;
    let Some(qs) = super::points::coefficients(cx, b, q, x)? else { return Ok(None) };
    let [_, _, q2] = qs.as_slice() else { return Ok(None) };
    if sign(cx, b, b.neg(*q2))? != Some(1) {
        return Ok(None);
    }
    gaussian(job, cx, b, f, x)
}

/// `Γ(p) Γ(q)/Γ(p + q)`; `π/sin(π p)` when `p + q = 1` (the reflection formula, DLMF 5.5.3).
fn beta<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: T<'h>, q: T<'h>) -> Res<T<'h>> {
    let pi = b.heap().sym_term(b.heap().sym("pi"));
    if cx.eval(b.add(&[p, q]))? == b.one() {
        return Ok(b.div(pi, b.call("sin", &[b.mul(&[pi, p])])));
    }
    Ok(b.div(b.mul(&[b.call("gamma", &[p]), b.call("gamma", &[q])]), b.call("gamma", &[b.add(&[p, q])])))
}

/// Is `p` an integer (exactly)?
fn integer<'h>(b: &Build<'h>, p: T<'h>) -> bool {
    matches!(b.heap().view(p), View::Int(_) | View::BigInt { .. })
}

/// `∫_l^r C (x - l)^p (r - x)^q dx` with linear factors whose roots are the bounds (module comment).
fn beta_interval<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, l: T<'h>, r: T<'h>) -> Res<Option<T<'h>>> {
    let (mut c, mut p, mut q) = (vec![], vec![], vec![]);
    let mut all_integer = true;
    for g in factors(b, f) {
        if is_free(b, g, x) {
            c.push(g);
            continue;
        }
        let (base, e) = match call_of(b, g) {
            Some((n, [base, e])) if n == "pow" && is_free(b, *e, x) => (*base, *e),
            _ => (g, b.one()),
        };
        // base = alpha x + beta
        let alpha = apply(cx, b, "d", &[base, x])?;
        if !is_free(b, alpha, x) || alpha == b.zero() {
            return Ok(None);
        }
        let beta0 = subst(cx, b, base, x, b.zero())?;
        let root = cx.eval(b.neg(b.div(beta0, alpha)))?;
        all_integer &= integer(b, e);
        let at_l = cx.eval(b.sub(root, l))? == b.zero();
        let at_r = cx.eval(b.sub(root, r))? == b.zero();
        match (at_l, at_r, sign(cx, b, alpha)?) {
            (true, _, Some(1)) => {
                c.push(b.pow(alpha, e));
                p.push(e);
            }
            (_, true, Some(-1)) => {
                c.push(b.pow(b.neg(alpha), e));
                q.push(e);
            }
            _ => return Ok(None),
        }
    }
    // both ends must carry a factor (a single power is Newton-Leibniz's, with an elementary answer)
    if all_integer || p.is_empty() || q.is_empty() {
        return Ok(None);
    }
    let (p, q) = (cx.eval(b.add(&p))?, cx.eval(b.add(&q))?);
    let len = b.sub(r, l);
    let v = b.mul(&[b.mul(&c), b.pow(len, b.add(&[p, q, b.one()])), beta(cx, b, b.add(&[p, b.one()]), b.add(&[q, b.one()]))?]);
    Ok(Some(v))
}

/// A binomial `A + B x^d` (A, B free of x, d ≠ 0) as `(A, B, d)`.
fn binomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>, T<'h>)>> {
    let Some(ts) = args_of(b, u, b.hd.add) else { return Ok(None) };
    let (free, dep): (Vec<T<'h>>, Vec<T<'h>>) = ts.iter().partition(|&&t| is_free(b, t, x));
    if free.is_empty() || dep.len() != 1 {
        return Ok(None);
    }
    let Some((bc, d)) = monomial(cx, b, dep[0], x)? else { return Ok(None) };
    Ok(Some((b.add(&free), bc, d)))
}

/// `C x^α (A + B x^d)^β` as `(C, α, A, B, d, β)`.
#[allow(clippy::type_complexity)]
fn binomial_parts<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
) -> Res<Option<(T<'h>, T<'h>, T<'h>, T<'h>, T<'h>, T<'h>)>> {
    let (mut c, mut al, mut bin) = (vec![], vec![], None);
    for g in factors(b, f) {
        if is_free(b, g, x) {
            c.push(g);
        } else if let Some(p) = power_of_x(b, g, x) {
            al.push(p);
        } else if bin.is_none() {
            let (u, e) = match call_of(b, g) {
                Some((n, [u, e])) if n == "pow" && is_free(b, *e, x) => (*u, *e),
                _ => (g, b.one()),
            };
            let Some((a0, b0, d)) = binomial(cx, b, u, x)? else { return Ok(None) };
            bin = Some((a0, b0, d, e));
        } else {
            return Ok(None);
        }
    }
    let Some((a0, b0, d, e)) = bin else { return Ok(None) };
    Ok(Some((b.mul(&c), cx.eval(b.add(&al))?, a0, b0, d, e)))
}

/// `∫_0^L C x^α (A + B x^d)^β dx` with `A + B L^d = 0` and `A > 0`.
fn beta_binomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, l: T<'h>) -> Res<Option<T<'h>>> {
    let Some((c, al, a0, b0, d, be)) = binomial_parts(cx, b, f, x)? else { return Ok(None) };
    if integer(b, al) && integer(b, be) || d == b.one() {
        return Ok(None);
    }
    let zero = cx.eval(b.add(&[a0, b.mul(&[b0, b.pow(l, d)])]))?;
    if zero != b.zero() || sign(cx, b, a0)? != Some(1) || sign(cx, b, d)? != Some(1) {
        return Ok(None);
    }
    let mu = b.div(b.add(&[al, b.one()]), d);
    Ok(Some(b.mul(&[c, b.pow(a0, be), b.pow(l, b.add(&[al, b.one()])), b.powi(d, -1), beta(cx, b, mu, b.add(&[be, b.one()]))?])))
}

/// `∫_0^∞ C x^α (A + B x^d)^{-s} dx` with `A, B > 0`.
fn beta_half_line<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let Some((c, al, a0, b0, d, be)) = binomial_parts(cx, b, f, x)? else { return Ok(None) };
    let odd_simple = be == b.int(-1) && matches!(b.heap().view(d), View::Int(k) if k % 2 == 1 && k > 1);
    if integer(b, al) && integer(b, be) && integer(b, d) && !odd_simple {
        return Ok(None);
    }
    if sign(cx, b, a0)? != Some(1) || sign(cx, b, b0)? != Some(1) || sign(cx, b, d)? != Some(1) {
        return Ok(None);
    }
    let s = cx.eval(b.neg(be))?;
    let mu = b.div(b.add(&[al, b.one()]), d);
    if odd_simple {
        // B(mu, 1 - mu) = pi/sin(pi mu), for 0 < mu < 1 (convergence was settled before)
        let mu = cx.eval(mu)?;
        let pi = b.heap().sym_term(b.heap().sym("pi"));
        let refl = b.div(pi, b.call("sin", &[b.mul(&[pi, mu])]));
        return Ok(Some(b.mul(&[c, b.pow(a0, be), b.pow(b.div(a0, b0), mu), b.powi(d, -1), refl])));
    }
    Ok(Some(b.mul(&[c, b.pow(a0, be), b.pow(b.div(a0, b0), mu), b.powi(d, -1), beta(cx, b, mu, b.sub(s, mu))?])))
}

/// `∫_0^∞ C x^r/(exp(a x) ∓ 1) dx = C Γ(s) ζ(s)/a^s` (Bose-Einstein, DLMF 25.5.1) and `C (1 - 2^(1-s)) Γ(s) ζ(s)/a^s`
/// (Fermi-Dirac, DLMF 25.5.3), `s = r + 1`; the Fermi-Dirac integral at `s = 1` is `C log(2)/a`.
fn bose_fermi<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let (mut c, mut r, mut den) = (vec![], vec![], None);
    for g in factors(b, f) {
        if is_free(b, g, x) {
            c.push(g);
        } else if let Some(p) = power_of_x(b, g, x) {
            r.push(p);
        } else if let (Some((n, [u, e])), None) = (call_of(b, g), den) {
            if n != "pow" || *e != b.int(-1) {
                return Ok(None);
            }
            den = Some(*u);
        } else {
            return Ok(None);
        }
    }
    let Some(u) = den else { return Ok(None) };
    let Some(ts) = args_of(b, u, b.hd.add) else { return Ok(None) };
    let [t1, t2] = ts else { return Ok(None) };
    let (one, ex) = if is_free(b, *t1, x) { (*t1, *t2) } else { (*t2, *t1) };
    let Some(w) = exponent_of(b, ex, x) else { return Ok(None) };
    let Some((a, d)) = monomial(cx, b, w, x)? else { return Ok(None) };
    if d != b.one() {
        return Ok(None);
    }
    let s = cx.eval(b.add(&[b.add(&r), b.one()]))?;
    let v = if one == b.int(-1) {
        b.mul(&[b.call("gamma", &[s]), b.call("zeta", &[s]), b.pow(a, b.neg(s))])
    } else if one == b.one() {
        if s == b.one() {
            b.div(b.call("log", &[b.int(2)]), a)
        } else {
            let k = b.sub(b.one(), b.pow(b.int(2), b.sub(b.one(), s)));
            b.mul(&[k, b.call("gamma", &[s]), b.call("zeta", &[s]), b.pow(a, b.neg(s))])
        }
    } else {
        return Ok(None);
    };
    Ok(Some(b.mul(&[b.mul(&c), v])))
}

/// `∫_0^∞ C x^r/sinh(a x) dx = C 2 (1 - 2^-s) Γ(s) ζ(s)/a^s`, `s = r + 1 > 1`, `a > 0` (module comment).
fn over_sinh<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let (mut c, mut r, mut arg) = (vec![], vec![], None);
    for g in factors(b, f) {
        if is_free(b, g, x) {
            c.push(g);
        } else if let Some(p) = power_of_x(b, g, x) {
            r.push(p);
        } else if arg.is_none() {
            match call_of(b, g) {
                Some((n, [u])) if n == "csch" => arg = Some(*u),
                Some((n, [base, e])) if n == "pow" && *e == b.int(-1) => match call_of(b, *base) {
                    Some((m, [u])) if m == "sinh" => arg = Some(*u),
                    _ => return Ok(None),
                },
                _ => return Ok(None),
            }
        } else {
            return Ok(None);
        }
    }
    let Some(u) = arg else { return Ok(None) };
    let Some((a, d)) = monomial(cx, b, u, x)? else { return Ok(None) };
    if d != b.one() || sign(cx, b, a)? != Some(1) {
        return Ok(None);
    }
    let s = cx.eval(b.add(&[b.add(&r), b.one()]))?;
    let gt_one = cx.eval(b.call("greater", &[s, b.one()]))?;
    if !cx.is_true(gt_one) {
        return Ok(None);
    }
    let k = b.mul(&[b.int(2), b.sub(b.one(), b.pow(b.int(2), b.neg(s)))]);
    Ok(Some(b.mul(&[b.mul(&c), k, b.call("gamma", &[s]), b.call("zeta", &[s]), b.pow(a, b.neg(s))])))
}

/// `∫_0^∞ C e^(-a x) sin(b x)/x dx = C atan(b/a)`, `a > 0`, b real (module comment).
fn damped_sinc<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let (mut c, mut inv_x, mut rate, mut freq) = (vec![], false, None, None);
    for g in factors(b, f) {
        if is_free(b, g, x) {
            c.push(g);
        } else if power_of_x(b, g, x) == Some(b.int(-1)) && !inv_x {
            inv_x = true;
        } else if let (Some(u), None) = (exponent_of(b, g, x), rate) {
            rate = Some(u);
        } else if let (Some((n, [u])), None) = (call_of(b, g), freq) {
            if n != "sin" {
                return Ok(None);
            }
            freq = Some(*u);
        } else {
            return Ok(None);
        }
    }
    let (true, Some(w), Some(u)) = (inv_x, rate, freq) else { return Ok(None) };
    let (Some((ma, da)), Some((bb, db))) = (monomial(cx, b, w, x)?, monomial(cx, b, u, x)?) else { return Ok(None) };
    if da != b.one() || db != b.one() {
        return Ok(None);
    }
    let a = cx.eval(b.neg(ma))?;
    if sign(cx, b, a)? != Some(1) {
        return Ok(None);
    }
    let real_b = cx.eval(b.call("element", &[bb, b.heap().sym_term(b.heap().sym("reals"))]))?;
    if !cx.is_true(real_b) {
        return Ok(None);
    }
    Ok(Some(b.mul(&[b.mul(&c), b.call("atan", &[b.div(bb, a)])])))
}

/// `∫_0^L C log(1 + k x)/x dx = -C Li_2(-k L)` for `k L >= -1` (module comment).
fn dilog<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, l: T<'h>) -> Res<Option<T<'h>>> {
    let (mut c, mut inv_x, mut arg) = (vec![], false, None);
    for g in factors(b, f) {
        if is_free(b, g, x) {
            c.push(g);
        } else if power_of_x(b, g, x) == Some(b.int(-1)) && !inv_x {
            inv_x = true;
        } else if let (Some((n, [u])), None) = (call_of(b, g), arg) {
            if n != "log" {
                return Ok(None);
            }
            arg = Some(*u);
        } else {
            return Ok(None);
        }
    }
    let (true, Some(u)) = (inv_x, arg) else { return Ok(None) };
    let Some((one, k, d)) = binomial(cx, b, u, x)? else { return Ok(None) };
    if one != b.one() || d != b.one() {
        return Ok(None);
    }
    let kl = cx.eval(b.mul(&[k, l]))?;
    let ok = cx.eval(b.call("greater_equal", &[kl, b.int(-1)]))?;
    if !cx.is_true(ok) {
        return Ok(None);
    }
    Ok(Some(b.neg(b.mul(&[b.mul(&c), b.call("polylog", &[b.int(2), b.neg(kl)])]))))
}

/// `∫_0^∞` of sines and cosines (times an exponential or a power): written with exponentials, term by term (the
/// gamma integral with a complex rate, `∫_0^∞ x^(s-1) exp(-a x^d) dx = Γ(s/d)/(d a^(s/d))` also for `re(a) = 0`,
/// `0 < re(s) < d`, where it converges conditionally), then in real form.
fn damped_trig<'h>(job: &mut Job<'_, 'h>, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let fs = factors(b, f);
    let trig = fs.iter().any(|&g| {
        matches!(call_of(b, g), Some((n, [_])) if n == "sin" || n == "cos")
            || matches!(call_of(b, g), Some((n, [base, _])) if n == "pow" && matches!(call_of(b, *base), Some((m, [_])) if m == "sin" || m == "cos"))
    });
    if !trig {
        return Ok(None);
    }
    let g = apply(cx, b, "trig_to_exp", &[f])?;
    let g = apply(cx, b, "expand", &[g])?;
    if g == f || args_of(b, g, b.hd.add).is_none() {
        return Ok(None);
    }
    let Some(v) = real_range(job, cx, b, g, x, Pt::Fin(b.zero()), Pt::PosInf)? else { return Ok(None) };
    if super::is_failure(b, v) {
        return Ok(None);
    }
    Ok(Some(super::residue::real_form(cx, b, f, v)?))
}

/// `∫_0^L C x^r exp(-c x^d) dx = C (Γ(s) - Γ(s, c L^d))/(d c^s)` with `s = (r + 1)/d`, `L > 0`, `d > 0` (the lower
/// incomplete gamma function, DLMF 8.2.1, after `t = c x^d`; `γ(s, z) z^-s` is entire in `z`, so the principal
/// branches of `c^s` and `Γ(s, c L^d)` agree for every `c`). Only when `r` or `d` is not an integer (Newton-Leibniz
/// gives elementary answers otherwise).
fn gamma_finite<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, l: T<'h>) -> Res<Option<T<'h>>> {
    let (mut c, mut r, mut us) = (vec![], vec![], vec![]);
    for g in factors(b, f) {
        if is_free(b, g, x) {
            c.push(g);
        } else if let Some(p) = power_of_x(b, g, x) {
            r.push(p);
        } else if let Some(u) = exponent_of(b, g, x) {
            us.push(u);
        } else {
            return Ok(None);
        }
    }
    if us.is_empty() {
        return Ok(None);
    }
    let Some((cu, d)) = monomial(cx, b, b.add(&us), x)? else { return Ok(None) };
    let r = cx.eval(b.add(&r))?;
    if integer(b, r) && d == b.one() || sign(cx, b, l)? != Some(1) {
        return Ok(None);
    }
    if b.ar.num(d).is_some() && sign(cx, b, d)? != Some(1) {
        return Ok(None);
    }
    let a = cx.eval(b.neg(cu))?;
    let s = cx.eval(b.div(b.add(&[r, b.one()]), d))?;
    if s == b.rat(1, 2) {
        // the error function: √π erf(√a L^(d/2))/(d √a)
        let pi = b.heap().sym_term(b.heap().sym("pi"));
        let ra = b.pow(a, b.rat(1, 2));
        let e = b.call("erf", &[b.mul(&[ra, b.pow(l, b.div(d, b.int(2)))])]);
        return Ok(Some(b.mul(&[b.mul(&c), b.pow(pi, b.rat(1, 2)), e, b.powi(d, -1), b.powi(ra, -1)])));
    }
    let z = b.mul(&[a, b.pow(l, d)]);
    let g = b.sub(b.call("gamma", &[s]), b.call("gamma", &[s, z]));
    Ok(Some(b.mul(&[b.mul(&c), g, b.powi(d, -1), b.pow(a, b.neg(s))])))
}
