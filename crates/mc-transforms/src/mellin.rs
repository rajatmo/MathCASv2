//! The Mellin transform `M[f](s) = integrate(x^(s - 1) f(x), [x, 0, inf])` and its inverse.
//!
//! Forward, in order: linearity (undefined functions stay as transforms); powers of `x` as distributions
//! (`M[x^a] = 2 pi dirac_delta(i (s + a))`, the Mellin transform being a Fourier transform in `log x`);
//! `dirac_delta(x - a)`; the table ([`TABLE`], also behind a power factor `x^a`, by `M[x^a f](s) = F(s + a)`);
//! steps `unit_step(x - a)` (and `g(log x) unit_step(x - 1)` as a Laplace transform at `-s`); the scaling rules
//! `M[f(a x)] = a^-s F(s)` and `M[f(x^b)] = F(s/b)/|b|`; then the definite integral (item 14D). Sources:
//! Titchmarsh, *Introduction to the Theory of Fourier Integrals* (1948), section 1.5; Oberhettinger, *Tables of
//! Mellin Transforms* (1974), sections 1-8 for the table.
//!
//! Inverse, in order: linearity; the table ([`INVERSE_TABLE`]); the rules `a^s G(s) -> g(x/a)` and `G(s + b) ->
//! x^b g(x)`; then the Mellin-Barnes residue series: the contour of `1/(2 pi i) integrate(F(s) x^-s, s on a vertical
//! line)` closed to the left encloses the poles of one family of gamma factors `gamma(a s + b)` (`a > 0`) at `s =
//! -(b + k)/a` and the poles of rational factors; the residues are summed in closed form by `sum` (Paris and
//! Kaminski, *Asymptotics and Mellin-Barnes Integrals*, 2001, section 3.4). The strip is the one the assumptions
//! give (`assumptions: -1 < re(s) < 0`, `conditional_expression(F, strip)`), else the fundamental strip right of
//! every such pole. A rational `F` alone gives a function vanishing for `x > 1` (its contour closes to the right
//! without poles there), as `heaviside_theta(1 - x)`.
use super::pattern::Table;
use super::verify;
use super::{factors, held, split_constant, terms, unknown_function, with_condition, Cx, Kind, MAX_DEPTH};
use super::call_of;
use mc_calculus::common::is_failure;
use mc_limit::bounds::{sign, subst};
use mc_calculus::common::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The table of the Mellin transform (`t` the variable, `s` the transform's). Sources: Oberhettinger 1974,
/// sections 2 (exponentials and trigonometric functions), 6 (error functions and exponential integrals), 10
/// (Bessel functions); products of two Bessel functions of one argument from DLMF 10.22.57 (written with the
/// duplication formula, so gamma(1 - s) does not appear) and 10.43.19 (round 22).
pub(crate) const TABLE: &str = r#"
[bessel_j(n, t), 2^(s - 1)*gamma(n/2 + s/2)/gamma(n/2 - s/2 + 1), true, -re(n) < re(s) and re(s) < 3/2]
[bessel_j(n, t)^2, gamma(n + s/2)*gamma(1/2 - s/2)/(2*pi^(1/2)*gamma(1 - s/2)*gamma(1 + n - s/2)), true, -2*re(n) < re(s) and re(s) < 1]
[bessel_j(m, t)*bessel_j(n, t), gamma(m/2 + n/2 + s/2)*gamma(1/2 - s/2)*gamma(1 - s/2)/(2*pi^(1/2)*gamma(1 + n/2 - m/2 - s/2)*gamma(1 + m/2 + n/2 - s/2)*gamma(1 + m/2 - n/2 - s/2))]
[bessel_k(n, t)^2, 2^(s - 3)*gamma(s/2 + n)*gamma(s/2)^2*gamma(s/2 - n)/gamma(s), true, re(s) > 2*abs(re(n))]
[bessel_k(m, t)*bessel_k(n, t), 2^(s - 3)*gamma(s/2 + m/2 + n/2)*gamma(s/2 + m/2 - n/2)*gamma(s/2 - m/2 + n/2)*gamma(s/2 - m/2 - n/2)/gamma(s)]
[bessel_k(n, t), 2^(s - 2)*gamma(s/2 - n/2)*gamma(s/2 + n/2), true, re(s) > abs(re(n))]
[bessel_y(n, t), -2^(s - 1)*pi^(-1)*gamma(s/2 + n/2)*gamma(s/2 - n/2)*cos(s*pi/2 - n*pi/2), true, abs(re(n)) < re(s) and re(s) < 3/2]
[erf(t), -pi^(-1/2)*gamma(1/2 + s/2)/s, true, -1 < re(s) and re(s) < 0]
[erfc(t), pi^(-1/2)*gamma(1/2 + s/2)/s, true, re(s) > 0]
[exp_integral_e(n, t), gamma(s)/(s + n - 1), true, re(s) > 0]
[exp_integral_ei(-t), -gamma(s)/s, true, re(s) > 0]
[exp(-t - 1/t), 2*bessel_k(-s, 2)]
[exp(-t - t^(-1)), 2*bessel_k(-s, 2)]
[(exp(t) - 1)^(-1), gamma(s)*zeta(s), true, re(s) > 1]
[(exp(t) + 1)^(-1), (1 - 2^(1 - s))*gamma(s)*zeta(s), true, re(s) > 0]
[log(1 - exp(-t)), -gamma(s)*polylog(s + 1, 1), true, re(s) > 0]
[log(1 + t), pi/(s*sin(pi*s)), true, -1 < re(s) and re(s) < 0]
[sin(t), gamma(s)*sin(pi*s/2), true, -1 < re(s) and re(s) < 1]
[cos(t), gamma(s)*cos(pi*s/2), true, 0 < re(s) and re(s) < 1]
[atan(t), -pi/(2*s*cos(pi*s/2)), true, -1 < re(s) and re(s) < 0]
[(1 + t)^(-1), pi*csc(pi*s), true, 0 < re(s) and re(s) < 1]
[(1 + t)^(-a), gamma(s)*gamma(a - s)/gamma(a), true, 0 < re(s) and re(s) < re(a)]
[(1 + t^2)^(-1), (1/2)*pi*csc(pi*s/2), true, 0 < re(s) and re(s) < 2]
[dirac_delta(t - 1), 1]
"#;

/// The table of the inverse Mellin transform (`s` the variable, `t` the result's).
pub(crate) const INVERSE_TABLE: &str = r#"
[gamma(s)*zeta(s), (exp(t) - 1)^(-1)]
[pi/sin(pi*s), (1 + t)^(-1)]
[pi*csc(pi*s), (1 + t)^(-1)]
[pi*csc(a*pi*s), a^(-1)*(1 + t^(1/a))^(-1)]
[csc(a*pi*s), (a*pi)^(-1)*(1 + t^(1/a))^(-1)]
[pi*cot(pi*s), (1 - t)^(-1)]
[pi*cos(pi*s)/sin(pi*s), (1 - t)^(-1)]
[bessel_k(s, a), (1/2)*exp(-a*(t + 1/t)/2)]
[polygamma(n, s), (-1)^(n + 1)*(-log(t))^n/(1 - t)*heaviside_theta(1 - t), n > 0]
[2^(s - 1)*gamma(s/2 + n/2)/gamma(n/2 - s/2 + 1), bessel_j(n, t)]
[2^(s - 1)*gamma(s/2)/gamma(1 - s/2), bessel_j(0, t)]
[2^(s - 2)*gamma(s/2)^2, bessel_k(0, t)]
[2^(s - 2)*gamma(s/2 - n/2)*gamma(s/2 + n/2), bessel_k(n, t)]
"#;

/// The Mellin transform of `f` in `x` at `s`, or `None`.
pub fn mellin<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, f: T<'h>, x: T<'h>, s: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let table = Table::read(cx, b, TABLE, x, s)?;
    let mut st = State { table, cond: None, checked: true };
    let Some(v) = mel(cx, tc, &mut st, f, x, s, 0)? else { return Ok(None) };
    let v = cx.eval(v)?;
    if is_failure(b, v) || !super::free_outside(b, v, x) {
        return Ok(None);
    }
    let v = super::laplace::tidy(cx, b, v)?;
    let v = rational_bases(cx, b, v, s)?;
    if !st.checked && verify::mellin(cx, b, f, x, s, v, st.cond)? == verify::Verdict::Refuted {
        return Ok(None);
    }
    Ok(Some(with_condition(cx, tc, v, st.cond)?))
}

struct State<'h> {
    table: Table<'h>,
    cond: Option<T<'h>>,
    checked: bool,
}

/// Powers `(p/q)^e` with `e` mentioning `s` written `p^e q^-e` (the tables' form).
fn rational_bases<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>, s: T<'h>) -> Res<T<'h>> {
    fn walk<'h>(b: &Build<'h>, t: T<'h>, s: T<'h>) -> T<'h> {
        let h = b.heap();
        let View::App { head, args } = h.view(t) else { return t };
        let new: Vec<T<'h>> = args.iter().map(|&a| walk(b, a, s)).collect();
        if b.sym_of(head) == Some(b.hd.pow) && new.len() == 2 && new[1] == b.int(-1) {
            if let Some(&[u]) = args_of(b, new[0], h.sym("sin")) {
                return b.call("csc", &[u]);
            }
        }
        if b.sym_of(head) == Some(b.hd.pow) && new.len() == 2 && !is_free(b, new[1], s) {
            if let View::Rational { num, den } = h.view(new[0]) {
                if matches!(h.view(num), View::Int(n) if n > 0) {
                    let e = new[1];
                    return h.call(b.hd.mul, &[h.call(b.hd.pow, &[num, e]), h.call(b.hd.pow, &[den, h.call(b.hd.mul, &[b.int(-1), e])])]);
                }
            }
        }
        if new.as_slice() == args {
            t
        } else {
            h.app(head, &new)
        }
    }
    let r = walk(b, v, s);
    if r == v {
        return Ok(v);
    }
    cx.eval(r)
}

#[allow(clippy::too_many_arguments)]
fn mel<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, st: &mut State<'h>, f: T<'h>, x: T<'h>, s: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    cx.tick(1)?;
    let f = super::laplace::split_powers(cx, b, f, x)?;
    // x^a (and constants) as distributions
    if let Some(a) = power_of(cx, b, f, x)? {
        let i = b.sym(h.sym("im"));
        return Ok(Some(b.mul(&[b.int(2), b.sym(h.sym("pi")), b.call("dirac_delta", &[b.mul(&[i, b.add(&[s, a])])])])));
    }
    let ts = terms(b, f);
    if ts.len() > 1 {
        let mut out = vec![];
        for g in ts {
            match mel(cx, tc, st, g, x, s, depth + 1)? {
                Some(r) => out.push(r),
                None => {
                    let (c, rest) = split_constant(b, g, x);
                    if unknown_function(cx, b, rest, x) {
                        out.push(b.mul(&[c, held(b, Kind::Mellin, rest, x, s)]));
                    } else {
                        return Ok(None);
                    }
                }
            }
        }
        return Ok(Some(b.add(&out)));
    }
    let (c, rest) = split_constant(b, f, x);
    if c != b.one() {
        return Ok(mel(cx, tc, st, rest, x, s, depth + 1)?.map(|r| b.mul(&[c, r])));
    }
    if unknown_function(cx, b, f, x) {
        return Ok(Some(held(b, Kind::Mellin, f, x, s)));
    }
    // dirac_delta(x - a): a^(s - 1) for a > 0
    if let Some((n, [u])) = call_of(b, f) {
        if n == "dirac_delta" {
            if let Some((k, d)) = super::linear(cx, b, *u, x)? {
                let a = cx.eval(b.neg(b.div(d, k)))?;
                if sign(cx, b, a)? == Some(1) {
                    let ak = cx.eval(b.call("abs", &[k]))?;
                    return Ok(Some(b.div(b.pow(a, b.sub(s, b.one())), ak)));
                }
                return Ok(None);
            }
        }
    }
    if let Some((r, strip)) = st.table.lookup_strip(cx, b, f)? {
        st.checked = false;
        st.cond = super::and(b, st.cond, strip);
        return Ok(Some(r));
    }
    // x^a g(x): G(s + a) with g in the table
    let fs = factors(b, f);
    for (i, &g) in fs.iter().enumerate() {
        let Some(a) = power_of(cx, b, g, x)? else { continue };
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        if let Some((r, strip)) = st.table.lookup_strip(cx, b, rest)? {
            st.checked = false;
            if let Some(c) = strip {
                let c = subst(cx, b, c, s, b.add(&[s, a]))?;
                let c = cx.eval(c)?;
                st.cond = super::and(b, st.cond, Some(c));
            }
            return Ok(Some(subst(cx, b, r, s, b.add(&[s, a]))?));
        }
    }
    // unit_step(x - a) g and unit_step(a - x) g with a > 0: the integral over (a, inf) or (0, a)
    for (i, &g) in fs.iter().enumerate() {
        let Some((n, [u])) = call_of(b, g) else { continue };
        if n != "unit_step" {
            continue;
        }
        let Some((k, d)) = super::linear(cx, b, *u, x)? else { continue };
        let a = cx.eval(b.neg(b.div(d, k)))?;
        if sign(cx, b, a)? != Some(1) {
            continue;
        }
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        // g(log x): the Laplace transform of g at -s (x > 1) or of g(-u) at s (x < 1)
        if a == b.one() {
            let above = sign(cx, b, k)? == Some(1);
            if let Some(r) = log_substitution(cx, tc, rest, x, s, above)? {
                st.checked = false;
                return Ok(Some(r));
            }
        }
        let g2 = cx.eval(b.mul(&[rest, b.pow(x, b.sub(s, b.one()))]))?;
        let inf = h.sym_term(h.sym("inf"));
        let (l, u2) = if sign(cx, b, k)? == Some(1) { (a, inf) } else { (b.zero(), a) };
        if let Some(r) = super::definite(cx, b, g2, x, l, u2, false)? {
            return Ok(Some(r));
        }
        break;
    }
    // scaling: f(a x) is a^-s F(s); f(x^k) is F(s/k)/|k|
    if let Some((a, k)) = scale_of(cx, b, f, x)? {
        let u = h.sym_term(h.sym("transform·u"));
        let g = subst(cx, b, f, x, b.pow(b.div(u, a), b.powi(k, -1)))?;
        let g = subst(cx, b, g, u, x)?;
        // in s itself (the table is bound to s), then s -> s/k
        let before = st.cond.take();
        let inner = mel(cx, tc, st, g, x, s, depth + 1)?;
        // the region of the inner value, scaled the same way (s -> s/k)
        let inner_cond = st.cond.take();
        st.cond = before;
        if let Some(r) = inner {
            if let Some(c) = inner_cond {
                let c = subst(cx, b, c, s, b.div(s, k))?;
                let c = cx.eval(c)?;
                st.cond = super::and(b, st.cond, Some(c));
            }
            let r = cx.eval(r)?;
            let r = subst(cx, b, r, s, b.div(s, k))?;
            let ak = cx.eval(b.call("abs", &[k]))?;
            st.checked = false;
            return Ok(Some(b.mul(&[b.pow(a, b.neg(b.div(s, k))), r, b.powi(ak, -1)])));
        }
    }
    // the definite integral
    let g = cx.eval(b.mul(&[f, b.pow(x, b.sub(s, b.one()))]))?;
    let inf = h.sym_term(h.sym("inf"));
    let Some(r) = super::definite(cx, b, g, x, b.zero(), inf, tc.opts.conditions || tc.opts.strips)? else { return Ok(None) };
    let (v, c) = super::conditional(b, r);
    st.cond = super::and(b, st.cond, c);
    Ok(Some(v))
}

/// `M[g(log x) unit_step(x - 1)] = L[g](-s)` (`above`) and `M[g(log x) unit_step(1 - x)] = L[g(-u)](s)`, when `x`
/// occurs in `g` only inside `log(x)`.
fn log_substitution<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, g: T<'h>, x: T<'h>, s: T<'h>, above: bool) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    let lx = cx.eval(b.call("log", &[x]))?;
    if is_free(b, g, lx) {
        return Ok(None);
    }
    let u = h.sym_term(h.sym("transform·u"));
    let gu = mc_limit::bounds::subst(cx, b, g, lx, u)?;
    if !is_free(b, gu, x) {
        return Ok(None);
    }
    let gu = if above { gu } else { subst(cx, b, gu, u, b.neg(u))? };
    let sv = h.sym_term(h.sym("transform·sv"));
    let inner = Cx { b, stop: tc.stop, opts: super::Opts { conditions: false, strips: false, fourier: None }, depth: tc.depth + 1 };
    if inner.depth > MAX_DEPTH {
        return Ok(None);
    }
    let Some(l) = super::laplace::laplace(cx, &inner, gu, u, sv)? else { return Ok(None) };
    if super::has_held(b, l, Kind::Laplace) {
        return Ok(None);
    }
    Ok(Some(subst(cx, b, l, sv, if above { b.neg(s) } else { s })?))
}

/// The exponent `a` of `f = x^a` (1 for `x`, 0 for a constant).
fn power_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let _ = cx;
    if is_free(b, f, x) {
        return Ok(None);
    }
    if f == x {
        return Ok(Some(b.one()));
    }
    Ok(match args_of(b, f, b.hd.pow) {
        Some(&[base, e]) if base == x && is_free(b, e, x) => Some(e),
        _ => None,
    })
}

/// `(a, k)` when `x` occurs in `f` only as `a x^k` with the same `a > 0` and `k != 0` real, not both 1.
fn scale_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let h = b.heap();
    let mut found: Option<(T<'h>, T<'h>)> = None;
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if is_free(b, u, x) {
            continue;
        }
        // the monomial a x^k
        let (a, k) = if u == x {
            (b.one(), b.one())
        } else if let Some(&[base, e]) = args_of(b, u, b.hd.pow).filter(|a| a[0] == x) {
            let _ = base;
            (b.one(), e)
        } else if let Some(fs) = args_of(b, u, b.hd.mul) {
            let (dep, free): (Vec<T<'h>>, Vec<T<'h>>) = fs.iter().partition(|&&g| !is_free(b, g, x));
            match dep.as_slice() {
                [g] if *g == x => (b.mul(&free), b.one()),
                [g] if args_of(b, *g, b.hd.pow).is_some_and(|a| a[0] == x && is_free(b, a[1], x)) => (b.mul(&free), args_of(b, *g, b.hd.pow).unwrap()[1]),
                _ => {
                    stack.extend_from_slice(fs);
                    continue;
                }
            }
        } else {
            if let View::App { args, .. } = h.view(u) {
                stack.extend_from_slice(args);
            }
            continue;
        };
        let a = cx.eval(a)?;
        match found {
            None => found = Some((a, k)),
            Some((a0, k0)) if a0 == a && k0 == k => {}
            // x^k and x^-k with the same a = 1 (exp(-x - 1/x)): not a scaling
            _ => return Ok(None),
        }
    }
    let Some((a, k)) = found else { return Ok(None) };
    if a == b.one() && k == b.one() {
        return Ok(None);
    }
    if sign(cx, b, a)? != Some(1) || !matches!(h.view(k), View::Int(_) | View::Rational { .. }) {
        return Ok(None);
    }
    Ok(Some((a, k)))
}

/// The inverse Mellin transform of `big` in `s` at `x`, or `None`.
pub fn inverse_mellin<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, big: T<'h>, s: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let (f, strip) = super::conditional(b, big);
    let table = Table::read_named(cx, b, INVERSE_TABLE, ("s", s), ("t", x))?;
    // the strip: the condition of the argument, else the facts on re(s)
    let mut facts: Vec<T<'h>> = cx.assumptions().to_vec();
    if let Some(c) = strip {
        facts.push(c);
    }
    let lo = strip_lower(cx, b, &facts, s)?;
    let Some(v) = imel(cx, tc, &table, f, s, x, lo, 0)? else { return Ok(None) };
    let v = cx.eval(v)?;
    if is_failure(b, v) || !super::free_outside(b, v, s) || !is_free(b, v, b.heap().sym_term(b.heap().sym("sum"))) {
        return Ok(None);
    }
    let v = super::laplace::tidy(cx, b, v)?;
    if lo.is_none() && verify::inverse_mellin(cx, b, f, s, x, v)? == verify::Verdict::Refuted {
        return Ok(None);
    }
    if tc.opts.conditions && lo.is_none() {
        if let Some(sigma) = default_strip(cx, b, f, s)? {
            let re = cx.eval(b.call("re", &[s]))?;
            let c = b.heap().call(b.heap().sym("greater"), &[re, sigma]);
            return Ok(Some(cx.eval(b.call("conditional_expression", &[v, c]))?));
        }
    }
    Ok(Some(v))
}

#[allow(clippy::too_many_arguments)]
fn imel<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    tc: &Cx<'_, 'h>,
    table: &Table<'h>,
    f: T<'h>,
    s: T<'h>,
    x: T<'h>,
    lo: Option<T<'h>>,
    depth: u32,
) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    cx.tick(1)?;
    let ts = terms(b, f);
    if ts.len() > 1 && !super::inverse::rational_in(b, f, s) {
        let mut out = vec![];
        for g in ts {
            let Some(r) = imel(cx, tc, table, g, s, x, lo, depth + 1)? else { return Ok(None) };
            out.push(r);
        }
        return Ok(Some(b.add(&out)));
    }
    if lo.is_none() {
        if let Some(r) = table.lookup(cx, b, f)? {
            return Ok(Some(r));
        }
    }
    let (c, rest) = split_constant(b, f, s);
    if c != b.one() {
        return Ok(imel(cx, tc, table, rest, s, x, lo, depth + 1)?.map(|r| b.mul(&[c, r])));
    }
    if is_free(b, f, s) {
        return Ok(None);
    }
    if let Some((n, [g, y, z])) = call_of(b, f) {
        if n == "mellin_transform" && *z == s {
            return Ok(Some(subst(cx, b, *g, *y, x)?));
        }
    }
    // c^(k s + d) G(s): c^d g(x/c^k); alone, c^d c^k dirac_delta(x - c^k)
    let fs = factors(b, f);
    let mut order: Vec<usize> = (0..fs.len()).collect();
    order.sort_by_key(|&i| matches!(args_of(b, fs[i], b.hd.pow).map(|a| h.view(a[0])), Some(View::Int(_) | View::Rational { .. })));
    for i in order {
        let g = fs[i];
        let Some(&[a, e]) = args_of(b, g, b.hd.pow) else { continue };
        if !is_free(b, a, s) || is_free(b, e, s) {
            continue;
        }
        let Some((k, d)) = super::linear(cx, b, e, s)? else { continue };
        let ak = cx.eval(b.pow(a, k))?;
        let ad = cx.eval(b.pow(a, d))?;
        if fs.len() == 1 {
            return Ok(Some(b.mul(&[ad, ak, b.call("dirac_delta", &[b.sub(x, ak)])])));
        }
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        let Some(r) = imel(cx, tc, table, rest, s, x, lo, depth + 1)? else { continue };
        let r = cx.eval(r)?;
        return Ok(Some(b.mul(&[ad, subst(cx, b, r, x, b.div(x, ak))?])));
    }
    // gamma ratios as a Meijer G-function (round 22): first when it reduces to named functions, else after the
    // residue series (which sums one family of poles)
    let g = super::meijer_inv::inverse(cx, b, f, s, x, lo)?;
    let named = |t: T<'h>| is_free(b, t, h.sym_term(h.sym("meijer_g")));
    if let Some(r) = g.filter(|&r| named(r)) {
        return Ok(Some(r));
    }
    if let Some(r) = residue_series(cx, b, f, s, x, lo)? {
        return Ok(Some(r));
    }
    Ok(g)
}

/// The left edge of the fundamental strip: the rightmost pole of the gamma factors `gamma(a s + c)` (`a > 0`),
/// `-c/a`.
fn default_strip<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, s: T<'h>) -> Res<Option<T<'h>>> {
    let mut best: Option<T<'h>> = None;
    for g in factors(b, f) {
        if let Some((n, [u])) = call_of(b, g) {
            if n == "gamma" {
                if let Some((a, c)) = super::linear(cx, b, *u, s)? {
                    if sign(cx, b, a)? == Some(1) {
                        let p = cx.eval(b.neg(b.div(c, a)))?;
                        best = match best {
                            None => Some(p),
                            Some(q) => Some(if sign(cx, b, b.sub(p, q))? == Some(1) { p } else { q }),
                        };
                    }
                }
            }
        }
    }
    Ok(best)
}

/// The lower bound of the strip from conditions `a < re(s)`, `re(s) > a`, `s > a` (`None` when there is none).
fn strip_lower<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, facts: &[T<'h>], s: T<'h>) -> Res<Option<T<'h>>> {
    let re = cx.eval(b.call("re", &[s]))?;
    let mut lo = None;
    let mut stack: Vec<T<'h>> = facts.to_vec();
    while let Some(u) = stack.pop() {
        let Some((n, args)) = call_of(b, u) else { continue };
        match n.as_str() {
            "and" => stack.extend_from_slice(args),
            "less" | "less_equal" | "greater" | "greater_equal" => {
                let inc = matches!(n.as_str(), "less" | "less_equal");
                for w in args.windows(2) {
                    let (a, z) = if inc { (w[0], w[1]) } else { (w[1], w[0]) };
                    if (z == re || z == s) && is_free(b, a, s) {
                        lo = Some(a);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(lo)
}

/// The Mellin-Barnes residue series (module comment): one family of gamma poles and the simple poles of the
/// rational factors, left of the strip, summed by `sum`.
fn residue_series<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, s: T<'h>, x: T<'h>, lo: Option<T<'h>>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let fs = factors(b, f);
    // the gamma factors gamma(a s + c) with a > 0 in the numerator
    let mut family: Option<(usize, T<'h>, T<'h>)> = None;
    let mut rational = vec![];
    for (i, &g) in fs.iter().enumerate() {
        if is_free(b, g, s) {
            continue;
        }
        if let Some((n, [u])) = call_of(b, g) {
            if n == "gamma" {
                let Some((a, c)) = super::linear(cx, b, *u, s)? else { return Ok(None) };
                if sign(cx, b, a)? == Some(1) {
                    if family.is_some() {
                        return Ok(None);
                    }
                    family = Some((i, a, c));
                }
                continue;
            }
        }
        if super::inverse::rational_in(b, g, s) {
            rational.push(g);
        }
    }
    let k = h.sym_term(h.sym("transform·k"));
    let mut out = vec![];
    let all_rational = family.is_none() && fs.iter().all(|&g| super::inverse::rational_in(b, g, s));
    // the poles of the rational factors (simple, left of the strip)
    let rat = cx.eval(b.mul(&rational))?;
    let tg = apply(cx, b, "together", &[rat])?;
    let den = apply(cx, b, "denominator", &[tg])?;
    if !is_free(b, den, s) {
        let Some(ps) = super::inverse::poles(cx, b, den, s)? else { return Ok(None) };
        for (p, m, q) in ps {
            if m > 1 {
                return Ok(None);
            }
            if let Some(l) = lo {
                let re = apply(cx, b, "re", &[p])?;
                if !sign(cx, b, b.sub(l, re))?.is_some_and(|x| x >= 0) {
                    continue;
                }
            }
            // a pole of the family too: a double pole
            if let Some((_, a, c)) = family {
                let kk = cx.eval(b.neg(b.add(&[b.mul(&[a, p]), c])))?;
                if matches!(h.view(kk), View::Int(0..)) {
                    return Ok(None);
                }
            }
            // the residue of f x^-s = num other x^-s / ((s - p) q) at p
            let num = apply(cx, b, "numerator", &[tg])?;
            let other: Vec<T<'h>> = fs.iter().copied().filter(|&g| !super::inverse::rational_in(b, g, s)).collect();
            let n_all = cx.eval(b.mul(&[num, b.mul(&other)]))?;
            let lx = cx.eval(b.neg(b.call("log", &[x])))?;
            let r = super::inverse::residue_at(cx, b, n_all, p, m, q, s, lx)?;
            if is_failure(b, r) {
                return Ok(None);
            }
            out.push(r);
        }
    }
    if let Some((i, a, c)) = family {
        // poles s_k = -(c + k)/a, residue (-1)^k/(k! a) of the gamma factor
        let sk = cx.eval(b.neg(b.div(b.add(&[c, k]), a)))?;
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        let rk = subst(cx, b, rest, s, sk)?;
        if is_failure(b, rk) {
            return Ok(None);
        }
        let term = b.mul(&[b.pow(b.int(-1), k), b.powi(b.call("factorial", &[k]), -1), b.powi(a, -1), rk, b.pow(x, b.neg(sk))]);
        let term = cx.eval(term)?;
        // the first pole left of the strip
        let k0 = match lo {
            None => b.zero(),
            Some(l) => {
                // s_k <= l  <=>  k >= -c - a l
                let bound = cx.eval(b.neg(b.add(&[c, b.mul(&[a, l])])))?;
                let cl = apply(cx, b, "ceiling", &[bound])?;
                if !matches!(h.view(cl), View::Int(_)) {
                    return Ok(None);
                }
                cl.max_zero(b)
            }
        };
        let inf = h.sym_term(h.sym("inf"));
        let sum = apply(cx, b, "sum", &[term, h.call(b.hd.list, &[k, k0, inf])])?;
        if !is_free(b, sum, h.sym_term(h.sym("sum"))) || is_failure(b, sum) {
            return Ok(None);
        }
        out.push(sum);
    } else if !all_rational {
        return Ok(None);
    }
    if out.is_empty() {
        return Ok(None);
    }
    let v = cx.eval(b.add(&out))?;
    // powers x^(i a) as exp(i a L) with x = exp(L), written with sin and cos, then L = log(x)
    let lsym = h.sym_term(h.sym("transform·log"));
    let vl = subst(cx, b, v, x, b.call("exp", &[lsym]))?;
    let vl = apply(cx, b, "power_expand", &[vl])?;
    let vl = super::inverse::trig_form(cx, b, vl)?;
    let v = subst(cx, b, vl, lsym, b.call("log", &[x]))?;
    if all_rational {
        return Ok(Some(b.mul(&[v, b.call("heaviside_theta", &[b.sub(b.one(), x)])])));
    }
    let _ = names::RULE;
    Ok(Some(v))
}

trait MaxZero<'h> {
    fn max_zero(self, b: &Build<'h>) -> T<'h>;
}

impl<'h> MaxZero<'h> for T<'h> {
    fn max_zero(self, b: &Build<'h>) -> T<'h> {
        match b.heap().view(self) {
            View::Int(k) if k < 0 => b.zero(),
            _ => self,
        }
    }
}
