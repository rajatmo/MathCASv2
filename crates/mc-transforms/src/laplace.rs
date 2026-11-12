//! The Laplace transform `L[f](s) = integrate(f(t) exp(-s t), [t, 0, inf])` and the two-sided one (over the whole
//! line).
//!
//! In order: constants; linearity (sums term by term, constant factors; a term with an undefined function stays as
//! a transform); the rules for undefined functions (derivatives `s^n F - s^(n-1) f(0) - ...`, integrals `F/s`,
//! scaling `f(a t)`); the table ([`TABLE`]); the shift rule `L[exp(a t) g] = G(s - a)`; trigonometric and
//! hyperbolic functions of linear arguments as exponentials; multiplication by `t^n` (`(-1)^n G^(n)(s)`); delayed
//! steps `unit_step(t - a) g(t)` as `exp(-a s) L[g(t + a)]` and cut-offs `unit_step(a - t)` as a finite integral;
//! division by `t` (`integrate(G(u), [u, s, inf])`); then the definite integral of item 14D. The rules are the
//! operational properties of Doetsch, *Introduction to the Theory and Application of the Laplace Transformation*
//! (1974), sections 8-12; the table follows Abramowitz and Stegun, *Handbook of Mathematical Functions*, 29.3.
//! A value from a rule or the table (not from the integral, which checks its own) is checked numerically
//! ([`super::verify`]) before it is returned.
use super::pattern::Table;
use super::verify;
use super::{definite, factors, held, linear, split_constant, terms, unknown_function, with_condition, Cx, Kind, MAX_DEPTH};
use super::call_of;
use mc_calculus::common::is_failure;
use mc_limit::bounds::{sign, subst};
use mc_calculus::common::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The table of the Laplace transform (`t` the variable, `s` the transform's; one-letter parameters, see
/// [`super::pattern`]). Sources: Abramowitz and Stegun 29.3 (Bessel functions 29.3.56-60, error functions
/// 29.3.43-45, logarithms 29.3.105-109, integral sines and cosines 29.3.97-99); Laguerre polynomials from their
/// generating function.
pub(crate) const TABLE: &str = r#"
[bessel_j(0, a*t), (s^2 + a^2)^(-1/2), true, re(s) > abs(im(a))]
[bessel_j(n, a*t), a^n*(s^2 + a^2)^(-1/2)*(s + (s^2 + a^2)^(1/2))^(-n), true, re(s) > abs(im(a))]
[bessel_i(0, a*t), (s^2 - a^2)^(-1/2), true, re(s) > abs(re(a))]
[bessel_i(n, a*t), a^n*(s^2 - a^2)^(-1/2)*(s + (s^2 - a^2)^(1/2))^(-n), true, re(s) > abs(re(a))]
[t^n*bessel_j(n, a*t), (2*a)^n*gamma(n + 1/2)*pi^(-1/2)*(s^2 + a^2)^(-n - 1/2), true, re(s) > abs(im(a))]
[t^n*bessel_i(n, a*t), (2*a)^n*gamma(n + 1/2)*pi^(-1/2)*(s^2 - a^2)^(-n - 1/2), true, re(s) > abs(re(a))]
[t*bessel_j(0, a*t), s*(s^2 + a^2)^(-3/2), true, re(s) > abs(im(a))]
[erf(a*t^(1/2)), a/(s*(s + a^2)^(1/2)), true, re(s) > 0]
[erf((a*t)^(1/2)), a^(1/2)/(s*(s + a)^(1/2)), true, re(s) > 0]
[erfc(a*t^(1/2)), 1/s - a/(s*(s + a^2)^(1/2)), true, re(s) > 0]
[erfc((a*t)^(1/2)), 1/s - a^(1/2)/(s*(s + a)^(1/2)), true, re(s) > 0]
[erf(a*t), exp(s^2/(4*a^2))*erfc(s/(2*a))/s, true, re(s) > 0]
[erfc(a*t), (1 - exp(s^2/(4*a^2))*erfc(s/(2*a)))/s, true, re(s) > 0]
[exp(-a*t^2), (1/2)*(pi/a)^(1/2)*exp(s^2/(4*a))*erfc(s/(2*a^(1/2))), possibly(re(a) > 0)]
[(t + a)^(-1), -exp(a*s)*exp_integral_ei(-a*s), true, re(s) > 0]
[sin_integral(a*t), atan(a/s)/s, true, re(s) > 0]
[cos_integral(a*t), -log(1 + s^2/a^2)/(2*s), true, re(s) > 0]
[exp_integral_e(1, a*t), log(1 + s/a)/s, true, re(s) > -re(a)]
[laguerre_l(n, t), (s - 1)^n*s^(-n - 1), true, re(s) > 0]
[log(a*t), -(euler_gamma + log(s/a))/s, true, re(s) > 0]
[log(t)^2, (pi^2/6 + (euler_gamma + log(s))^2)/s, true, re(s) > 0]
[t^n*log(t), gamma(n + 1)*s^(-n - 1)*(polygamma(0, n + 1) - log(s)), true, re(s) > 0]
[t^(-3/2)*exp(a/t), (-pi/a)^(1/2)*exp(-2*(-a*s)^(1/2)), possibly(re(a) <= 0), re(s) > 0]
[t^(-1/2)*exp(a/t), (pi/s)^(1/2)*exp(-2*(-a*s)^(1/2)), possibly(re(a) <= 0), re(s) > 0]
[sin(a*t)/t, atan(a/s), true, re(s) > abs(im(a))]
[sinh(a*t)/t, (1/2)*log((s + a)/(s - a)), true, re(s) > abs(re(a))]
[(1 - cos(a*t))/t, (1/2)*log(1 + a^2/s^2), true, re(s) > abs(im(a))]
[sin(a*t^(1/2)), (1/2)*a*pi^(1/2)*s^(-3/2)*exp(-a^2/(4*s)), true, re(s) > 0]
[t^(-1/2)*cos(a*t^(1/2)), (pi/s)^(1/2)*exp(-a^2/(4*s)), true, re(s) > 0]
[dirac_delta(t), 1]
"#;

/// The Laplace transform of `f` in `t` at `s`, or `None`.
pub fn laplace<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, f: T<'h>, t: T<'h>, s: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let table = Table::read(cx, b, TABLE, t, s)?;
    let mut st = State { table, cond: None, checked: true };
    // with generate_conditions: the integral first, which knows the region of convergence
    if tc.opts.conditions && !unknown_function(cx, b, f, t) {
        let h = b.heap();
        let g = cx.eval(b.mul(&[f, b.call("exp", &[b.neg(b.mul(&[s, t]))])]))?;
        if let Some(r) = definite(cx, b, g, t, b.zero(), h.sym_term(h.sym("inf")), true)? {
            return Ok(Some(r));
        }
    }
    let Some(v) = lap(cx, tc, &mut st, f, t, s, 0)? else { return Ok(None) };
    let v = cx.eval(v)?;
    if is_failure(b, v) || !super::free_outside(b, v, t) {
        return Ok(None);
    }
    let v = tidy(cx, b, v)?;
    if !st.checked && verify::laplace(cx, b, f, t, s, v)? == verify::Verdict::Refuted {
        return Ok(None);
    }
    Ok(Some(with_condition(cx, tc, v, st.cond)?))
}

/// The two-sided Laplace transform `integrate(f exp(-s t), [t, -inf, inf])`, with its strip of convergence as the
/// condition (by default): linearity, the Gaussian closed form, the definite integral.
pub fn bilateral<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, f: T<'h>, t: T<'h>, s: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    if tc.depth > MAX_DEPTH {
        return Ok(None);
    }
    // linearity, undefined functions kept as transforms
    let ts = terms(b, f);
    if ts.len() > 1 {
        let mut out = vec![];
        let mut conds = None;
        for g in ts {
            let (c, rest) = split_constant(b, g, t);
            if unknown_function(cx, b, rest, t) {
                out.push(b.mul(&[c, held(b, Kind::BilateralLaplace, rest, t, s)]));
                continue;
            }
            let inner = Cx { b, stop: tc.stop, opts: tc.opts, depth: tc.depth + 1 };
            let Some(r) = bilateral(cx, &inner, rest, t, s)? else { return Ok(None) };
            let (v, c2) = super::conditional(b, r);
            conds = super::and(b, conds, c2);
            out.push(b.mul(&[c, v]));
        }
        let v = cx.eval(b.add(&out))?;
        return Ok(Some(match conds {
            Some(c) => cx.eval(b.call("conditional_expression", &[v, c]))?,
            None => v,
        }));
    }
    let (c, rest) = split_constant(b, f, t);
    if c != b.one() {
        let Some(r) = bilateral(cx, tc, rest, t, s)? else { return Ok(None) };
        let (v, cond) = super::conditional(b, r);
        let v = cx.eval(b.mul(&[c, v]))?;
        return Ok(Some(match cond {
            Some(k) => cx.eval(b.call("conditional_expression", &[v, k]))?,
            None => v,
        }));
    }
    if unknown_function(cx, b, f, t) {
        return Ok(Some(held(b, Kind::BilateralLaplace, f, t, s)));
    }
    if let Some((n, [u])) = call_of(b, f) {
        // dirac_delta(t - a): exp(-a s)
        if n == "dirac_delta" {
            if let Some((k, d)) = linear(cx, b, *u, t)? {
                let a = cx.eval(b.neg(b.div(d, k)))?;
                let ak = cx.eval(b.call("abs", &[k]))?;
                return Ok(Some(cx.eval(b.div(b.call("exp", &[b.neg(b.mul(&[a, s]))]), ak))?));
            }
        }
    }
    let inf = h.sym_term(h.sym("inf"));
    let g = cx.eval(b.mul(&[f, b.call("exp", &[b.neg(b.mul(&[s, t]))])]))?;
    let r = definite(cx, b, g, t, b.neg(inf), inf, tc.opts.conditions || tc.opts.strips)?;
    let Some(r) = r else { return Ok(None) };
    let (v, cond) = super::conditional(b, r);
    let v = tidy(cx, b, v)?;
    Ok(Some(match cond {
        Some(k) => cx.eval(b.call("conditional_expression", &[v, k]))?,
        None => v,
    }))
}

struct State<'h> {
    table: Table<'h>,
    /// conditions of convergence collected from the integrals (asked for with `generate_conditions: true`)
    cond: Option<T<'h>>,
    /// every part of the value came from an integral that checked itself
    checked: bool,
}

#[allow(clippy::too_many_arguments)]
fn lap<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, st: &mut State<'h>, f: T<'h>, t: T<'h>, s: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    cx.tick(1)?;
    if is_free(b, f, t) {
        let c = cx.eval(h.call(h.sym("greater"), &[b.call("re", &[s]), b.zero()]))?;
        st.cond = super::and(b, st.cond, Some(c));
        return Ok(Some(b.div(f, s)));
    }
    let f = split_powers(cx, b, f, t)?;
    // linearity (an expanded product too; when a term of the expansion fails, the product as it is)
    let mut ts = terms(b, f);
    let mut expanded = false;
    if ts.len() == 1 && factors(b, f).iter().any(|&g| args_of(b, g, b.hd.add).is_some() && !is_free(b, g, t)) {
        let e = apply(cx, b, "expand", &[f])?;
        if args_of(b, e, b.hd.add).is_some() {
            ts = terms(b, e);
            expanded = true;
        }
    }
    if expanded {
        let mut out = Vec::with_capacity(ts.len());
        let mut all = true;
        for &g in &ts {
            match lap(cx, tc, st, g, t, s, depth + 1)? {
                Some(r) => out.push(r),
                None => {
                    all = false;
                    break;
                }
            }
        }
        if all {
            return Ok(Some(b.add(&out)));
        }
        ts = vec![f];
    }
    if ts.len() > 1 {
        let mut out = Vec::with_capacity(ts.len());
        for g in ts {
            match lap(cx, tc, st, g, t, s, depth + 1)? {
                Some(r) => out.push(r),
                None => {
                    let (c, rest) = split_constant(b, g, t);
                    if unknown_function(cx, b, rest, t) {
                        out.push(b.mul(&[c, held(b, Kind::Laplace, rest, t, s)]));
                    } else {
                        return Ok(None);
                    }
                }
            }
        }
        return Ok(Some(b.add(&out)));
    }
    let (c, rest) = split_constant(b, f, t);
    if c != b.one() {
        return Ok(lap(cx, tc, st, rest, t, s, depth + 1)?.map(|r| b.mul(&[c, r])));
    }
    // a sum over an index free of t: term by term
    if let Some((n, [g, spec])) = call_of(b, f) {
        if n == "sum" && is_free(b, *spec, t) {
            return Ok(lap(cx, tc, st, *g, t, s, depth + 1)?.map(|r| b.call("sum", &[r, *spec])));
        }
    }
    if let Some(r) = undefined(cx, tc, st, f, t, s, depth)? {
        return Ok(Some(r));
    }
    // the table
    if let Some((r, strip)) = st.table.lookup_strip(cx, b, f)? {
        st.checked = false;
        st.cond = super::and(b, st.cond, strip);
        return Ok(Some(r));
    }
    let fs = factors(b, f);
    // the shift rule: exp(a t + c) g(t) is exp(c) G(s - a)
    for (i, &g) in fs.iter().enumerate() {
        if let Some((n, [u])) = call_of(b, g) {
            if n == "exp" {
                if let Some((a, c0)) = linear(cx, b, *u, t)? {
                    let mut rest = fs.clone();
                    rest.remove(i);
                    let rest = cx.eval(b.mul(&rest))?;
                    // the inner transform in s itself (the table is bound to s), then s -> s - a (engine B21-15,
                    // B21-16: with a fresh variable the table's values stayed in s and the shift was lost)
                    let before = st.cond.take();
                    let Some(r) = lap(cx, tc, st, rest, t, s, depth + 1)? else { return Ok(None) };
                    let r = cx.eval(r)?;
                    let r = subst(cx, b, r, s, b.sub(s, a))?;
                    shift_strip(cx, b, st, before, s, b.sub(s, a))?;
                    return Ok(Some(b.mul(&[b.call("exp", &[c0]), r])));
                }
            }
        }
    }
    // trigonometric and hyperbolic functions of linear arguments (and their powers) as exponentials
    if fs.iter().any(|&g| trig_linear(cx, b, g, t).unwrap_or(false)) {
        let e = apply(cx, b, "trig_to_exp", &[f])?;
        let e = apply(cx, b, "expand", &[e])?;
        if e != f && args_of(b, e, b.hd.add).is_some() {
            let Some(r) = lap(cx, tc, st, e, t, s, depth + 1)? else { return Ok(None) };
            let r = cx.eval(r)?;
            return Ok(Some(apply(cx, b, "together", &[r])?));
        }
    }
    // t^n g: (-1)^n G^(n)(s)
    for (i, &g) in fs.iter().enumerate() {
        let n = if g == t {
            Some(1)
        } else {
            match args_of(b, g, b.hd.pow) {
                Some(&[base, e]) if base == t => match h.view(e) {
                    View::Int(k @ 1..=12) => Some(k),
                    _ => None,
                },
                _ => None,
            }
        };
        let Some(n) = n else { continue };
        if fs.len() == 1 {
            break;
        }
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        let Some(r) = lap(cx, tc, st, rest, t, s, depth + 1)? else { break };
        let r = cx.eval(r)?;
        if super::has_held(b, r, Kind::Laplace) {
            break;
        }
        let dn = apply(cx, b, "d", &[r, h.call(b.hd.list, &[s, b.int(n)])])?;
        let sgn = if n % 2 == 0 { b.one() } else { b.int(-1) };
        return Ok(Some(apply(cx, b, "together", &[b.mul(&[sgn, dn])])?));
    }
    // steps of linear arguments
    for (i, &g) in fs.iter().enumerate() {
        let Some((n, [u])) = call_of(b, g) else { continue };
        if n != "unit_step" {
            continue;
        }
        let Some((k, d)) = linear(cx, b, *u, t)? else { continue };
        let a = cx.eval(b.neg(b.div(d, k)))?;
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        let (ks, as_) = (sign(cx, b, k)?, sign(cx, b, a)?);
        match (ks, as_) {
            // unit_step(t - a) with a <= 0, unit_step(a - t) with a < 0: 1 or 0 on t > 0
            (Some(1), Some(-1 | 0)) => return lap(cx, tc, st, rest, t, s, depth + 1),
            (Some(-1), Some(-1 | 0)) => return Ok(Some(b.zero())),
            // a delayed function: exp(-a s) L[g(t + a)]
            (Some(1), Some(1)) => {
                let shifted = subst(cx, b, rest, t, b.add(&[t, a]))?;
                let Some(r) = lap(cx, tc, st, shifted, t, s, depth + 1)? else { return Ok(None) };
                return Ok(Some(b.mul(&[b.call("exp", &[b.neg(b.mul(&[a, s]))]), r])));
            }
            // a cut-off: the integral over [0, a]
            (Some(-1), Some(1)) => {
                let g2 = cx.eval(b.mul(&[rest, b.call("exp", &[b.neg(b.mul(&[s, t]))])]))?;
                return definite(cx, b, g2, t, b.zero(), a, false);
            }
            _ => {}
        }
    }
    // g(t)/t: integrate(G(u), [u, s, inf]) when g(0) = 0
    for (i, &g) in fs.iter().enumerate() {
        if args_of(b, g, b.hd.pow) != Some(&[t, b.int(-1)][..]) {
            continue;
        }
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        let at0 = apply(cx, b, "limit", &[rest, h.call(h.sym(mc_api::names::RULE), &[t, b.zero()])])?;
        if !b.is_zero(at0) {
            break;
        }
        let u = h.sym_term(h.sym("transform·u"));
        let Some(r) = lap(cx, tc, st, rest, t, u, depth + 1)? else { break };
        let r = cx.eval(r)?;
        let Some(big) = mc_integrals::antiderivative(cx, b, r, u, 0)? else { break };
        let big = cx.eval(big)?;
        let inf = h.sym_term(h.sym("inf"));
        let lim = apply(cx, b, "limit", &[big, h.call(h.sym(mc_api::names::RULE), &[u, inf])])?;
        if !is_free(b, lim, h.sym_term(h.sym("limit"))) || !is_free(b, lim, inf) || is_failure(b, lim) {
            break;
        }
        let at_s = subst(cx, b, big, u, s)?;
        st.checked = false;
        return Ok(Some(b.sub(lim, at_s)));
    }
    // the definite integral (which checks its value)
    let g = cx.eval(b.mul(&[f, b.call("exp", &[b.neg(b.mul(&[s, t]))])]))?;
    let inf = h.sym_term(h.sym("inf"));
    let Some(r) = definite(cx, b, g, t, b.zero(), inf, tc.opts.conditions || tc.opts.strips)? else { return Ok(None) };
    let (v, c) = super::conditional(b, r);
    st.cond = super::and(b, st.cond, c);
    Ok(Some(v))
}

/// The rules for functions without a definition: `g(t)` stays; `g^(n)(t)` is `s^n G - s^(n-1) g(0) - ... -
/// g^(n-1)(0)`; `g(a t)` (a > 0) is `G(s/a)/a`; `integrate(g(u), [u, 0, t])` is `G/s`.
fn undefined<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, st: &mut State<'h>, f: T<'h>, t: T<'h>, s: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    let View::App { head, args } = h.view(f) else { return Ok(None) };
    if let Some((n, [g, spec])) = call_of(b, f) {
        if n == "integrate" {
            if let Some(&[u, lo, hi]) = args_of(b, *spec, b.hd.list) {
                if hi == t && b.is_zero(lo) && matches!(h.view(u), View::Sym(_)) {
                    let g = subst(cx, b, *g, u, t)?;
                    return Ok(lap(cx, tc, st, g, t, s, depth + 1)?.map(|r| b.div(r, s)));
                }
            }
        }
    }
    let [arg] = args else { return Ok(None) };
    let (orders, fun) = match b.as_derivative(head) {
        Some((ns, g)) if ns.len() == 1 => (ns[0], g),
        Some(_) => return Ok(None),
        None => match h.view(head) {
            View::Sym(sy) if !cx.is_defined(sy) => (0, head),
            _ => return Ok(None),
        },
    };
    if orders > 0 && !matches!(h.view(fun), View::Sym(sy) if !cx.is_defined(sy)) {
        return Ok(None);
    }
    if *arg != t {
        // g(a t) with a > 0: G(s/a)/a
        if orders == 0 {
            if let Some((a, c0)) = linear(cx, b, *arg, t)? {
                if b.is_zero(c0) && sign(cx, b, a)? == Some(1) {
                    let base = held(b, Kind::Laplace, b.app(head, &[t]), t, b.div(s, a));
                    return Ok(Some(b.div(base, a)));
                }
            }
        }
        return Ok(None);
    }
    if orders == 0 {
        return Ok(Some(held(b, Kind::Laplace, f, t, s)));
    }
    let base = held(b, Kind::Laplace, b.app(fun, &[t]), t, s);
    let mut out = vec![b.mul(&[b.powi(s, orders), base])];
    for k in 0..orders {
        let dk = if k == 0 { b.app(fun, &[b.zero()]) } else { b.app(b.derivative(&[k], fun), &[b.zero()]) };
        out.push(b.neg(b.mul(&[b.powi(s, orders - 1 - k), dk])));
    }
    Ok(Some(b.add(&out)))
}

/// Is `g` a trigonometric or hyperbolic function (or a power of one) of an argument linear in `t`?
fn trig_linear<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, t: T<'h>) -> Res<bool> {
    let h = b.heap();
    let g = match args_of(b, g, b.hd.pow) {
        Some(&[base, e]) if matches!(h.view(e), View::Int(1..=8)) => base,
        _ => g,
    };
    Ok(match call_of(b, g) {
        Some((n, [u])) if matches!(n.as_str(), "sin" | "cos" | "sinh" | "cosh") => linear(cx, b, *u, t)?.is_some(),
        _ => false,
    })
}

/// Powers of products holding `t` split for `t > 0`: `(t c)^e = t^e c^e` (and `(t^k c)^e`), so that the table and
/// the rules see the power of `t` (`(t/(2 a))^(v - 1/2)`).
pub(crate) fn split_powers<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, t: T<'h>) -> Res<T<'h>> {
    let r = split_powers_in(b, f, t);
    if r == f {
        return Ok(f);
    }
    cx.eval(r)
}

fn split_powers_in<'h>(b: &Build<'h>, f: T<'h>, t: T<'h>) -> T<'h> {
    let h = b.heap();
    let View::App { head, args } = h.view(f) else { return f };
    if b.sym_of(head) == Some(b.hd.pow) {
        if let [base, e] = args {
            if let Some(fs) = args_of(b, *base, b.hd.mul) {
                let (with_t, other): (Vec<T<'h>>, Vec<T<'h>>) = fs.iter().partition(|&&g| !is_free(b, g, t));
                let pos = with_t.len() == 1
                    && (with_t[0] == t || matches!(args_of(b, with_t[0], b.hd.pow), Some(&[bb, k]) if bb == t && matches!(h.view(k), View::Int(_) | View::Rational { .. })));
                if pos && !other.is_empty() {
                    return b.mul(&[b.pow(with_t[0], *e), b.pow(b.mul(&other), *e)]);
                }
            }
        }
    }
    let new: Vec<T<'h>> = args.iter().map(|&a| split_powers_in(b, a, t)).collect();
    if new.as_slice() == args {
        f
    } else {
        b.app(head, &new)
    }
}

/// A value tidied: over a common denominator when that is smaller.
pub(crate) fn tidy<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>) -> Res<T<'h>> {
    let mut best = v;
    let h = b.heap();
    if size(b, v) < 400 && is_free(b, v, h.sym_term(h.sym("dirac_delta"))) {
        for f in ["together", "simplify"] {
            let r = apply(cx, b, f, &[best])?;
            if !is_failure(b, r) && size(b, r) < size(b, best) {
                best = r;
            }
        }
    }
    Ok(best)
}

/// The number of nodes of a term, capped.
pub(crate) fn size<'h>(b: &Build<'h>, t: T<'h>) -> usize {
    let h = b.heap();
    let mut n = 0;
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        n += 1;
        if n > 100_000 {
            break;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    n
}

/// After an inner transform in `s` whose region of convergence was added to `st.cond` (the part past `before`),
/// the value was shifted `s -> new_s`: the region is shifted the same way.
fn shift_strip<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, st: &mut State<'h>, before: Option<T<'h>>, s: T<'h>, new_s: T<'h>) -> Res<()> {
    if let Some(inner) = st.cond.take() {
        let shifted = subst(cx, b, inner, s, new_s)?;
        let shifted = cx.eval(shifted)?;
        st.cond = super::and(b, before, Some(shifted));
    } else {
        st.cond = before;
    }
    Ok(())
}
