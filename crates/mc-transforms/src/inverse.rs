//! The inverse Laplace transform (one- and two-sided).
//!
//! In order: constants (`c dirac_delta(t)`); rational functions of `s` by their residues ([`residues`]: the
//! denominator factored, the roots of each factor, the Heaviside expansion `sum Res(F(s) exp(s t))` with the
//! derivative formula at multiple poles, then exponentials of complex conjugate roots written with `sin` and `cos`;
//! Doetsch section 22); linearity; transforms of undefined functions back to the functions; delays `exp(-a s) G(s)`
//! as `g(t - a) heaviside_theta(t - a)`; the table ([`TABLE`]); shifts `G(s + a)` as `exp(-a t) g(t)`; division by
//! `s` as the integral `integrate(g(u), [u, 0, t])`; and the derivative rule `L^-1[F] = -L^-1[F'](t)/t` for
//! logarithms and inverse tangents, whose derivatives are rational (Doetsch, theorem 8.4). Every value is checked
//! numerically by the forward integral at a sample point ([`super::verify`]).
//!
//! The two-sided inverse of a rational function on a strip `a < re(s) < b`: the residues of the poles left of the
//! strip for `t >= 0`, minus those right of it for `t < 0` (Doetsch section 24), as a `piecewise`.
use super::pattern::Table;
use super::verify;
use super::{factors, held, linear, split_constant, terms, Cx, Kind, MAX_DEPTH};
use super::call_of;
use mc_calculus::common::is_failure;
use mc_limit::bounds::{sign, subst};
use mc_calculus::common::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The table of the inverse Laplace transform (`s` the variable, `t` the result's; one-letter parameters).
/// Sources: Abramowitz and Stegun 29.3 (algebraic forms 29.3.4-6 and 29.3.43-46, logarithms 29.3.105-109, Bessel
/// functions 29.3.55-60, error functions 29.3.112-115).
pub(crate) const TABLE: &str = r#"
[s^n, t^(-n - 1)/gamma(-n)]
[(s^2 + a)^(-1/2), bessel_j(0, a^(1/2)*t)]
[(s^2 + a)^n, pi^(1/2)/gamma(-n)*(t/(2*a^(1/2)))^(-n - 1/2)*bessel_j(-n - 1/2, a^(1/2)*t)]
[s^(-1)*(s + a)^(-1/2), a^(-1/2)*erf((a*t)^(1/2))]
[log(s)/s, -euler_gamma - log(t)]
[log(s)^2/s, (euler_gamma + log(t))^2 - pi^2/6]
[s^n*log(a*s), t^(-n - 1)/gamma(-n)*(polygamma(0, -n) + log(a) - log(t))]
[exp(-a*s^(1/2)), a/(2*pi^(1/2))*t^(-3/2)*exp(-a^2/(4*t))]
[s^(-1/2)*exp(-a*s^(1/2)), pi^(-1/2)*t^(-1/2)*exp(-a^2/(4*t))]
[s^(-1)*exp(-a*s^(1/2)), erfc(a/(2*t^(1/2)))]
[exp(a*s^2)*erfc(b*s), exp(-t^2/(4*b^2))/(b*pi^(1/2)), a == b^2]
[erfc(a*s^(1/2)), a/(pi*t*(t - a^2)^(1/2))*heaviside_theta(t - a^2)]
[s^(-1/2)*erf(a*s^(1/2)), pi^(-1/2)*t^(-1/2)*heaviside_theta(a^2 - t)]
[(s^(1/2) + a)^(-1), pi^(-1/2)*t^(-1/2) - a*exp(a^2*t)*erfc(a*t^(1/2))]
[polygamma(n, s), (-1)^(n + 1)*t^n/(1 - exp(-t)), n > 0]
[atan(a/s), sin(a*t)/t]
[gamma(a, s)*s^(-a), t^(a - 1)*heaviside_theta(t - 1)]
[bessel_k(0, a*s), (t^2 - a^2)^(-1/2)*heaviside_theta(t - a)]
[exp(-a*s)*bessel_i(0, a*s), pi^(-1)*(t*(2*a - t))^(-1/2)*heaviside_theta(2*a - t)]
"#;

/// The inverse Laplace transform of `big` in `s` at `t`, or `None`.
pub fn inverse_laplace<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, big: T<'h>, s: T<'h>, t: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let table = Table::read_named(cx, b, TABLE, ("s", s), ("t", t))?;
    let Some(v) = inv(cx, tc, &table, big, s, t, 0)? else { return Ok(None) };
    let v = cx.eval(v)?;
    if is_failure(b, v) || !super::free_outside(b, v, s) {
        return Ok(None);
    }
    let v = super::laplace::tidy(cx, b, v)?;
    if verify::inverse_laplace(cx, b, big, s, t, v)? == verify::Verdict::Refuted {
        return Ok(None);
    }
    Ok(Some(v))
}

#[allow(clippy::too_many_arguments)]
fn inv<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, table: &Table<'h>, big: T<'h>, s: T<'h>, t: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    cx.tick(1)?;
    if is_free(b, big, s) {
        return Ok(Some(b.mul(&[big, b.call("dirac_delta", &[t])])));
    }
    // a rational function: its residues
    if rational_in(b, big, s) {
        if let Some(r) = rational(cx, b, big, s, t)? {
            return Ok(Some(r));
        }
    }
    // linearity
    let ts = terms(b, big);
    if ts.len() > 1 {
        let mut out = vec![];
        for g in ts {
            match inv(cx, tc, table, g, s, t, depth + 1)? {
                Some(r) => out.push(r),
                None => return Ok(None),
            }
        }
        return Ok(Some(b.add(&out)));
    }
    let (c, rest) = split_constant(b, big, s);
    if c != b.one() {
        return Ok(inv(cx, tc, table, rest, s, t, depth + 1)?.map(|r| b.mul(&[c, r])));
    }
    // a transform of an undefined function, and the call itself for undefined functions of s
    if let Some((n, [g, x, y])) = call_of(b, big) {
        if n == "laplace_transform" && *y == s {
            return Ok(Some(subst(cx, b, *g, *x, t)?));
        }
    }
    if super::unknown_function(cx, b, big, s) {
        return Ok(Some(held(b, Kind::InverseLaplace, big, s, t)));
    }
    if let Some(r) = table.lookup(cx, b, big)? {
        return Ok(Some(r));
    }
    let fs = factors(b, big);
    // a delay: exp(-a s) G(s) is g(t - a) heaviside_theta(t - a)
    for (i, &g) in fs.iter().enumerate() {
        let Some((n, [u])) = call_of(b, g) else { continue };
        if n != "exp" {
            continue;
        }
        let Some((k, c0)) = linear(cx, b, *u, s)? else { continue };
        let a = cx.eval(b.neg(k))?;
        if sign(cx, b, a)? != Some(1) {
            continue;
        }
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        let Some(r) = inv(cx, tc, table, rest, s, t, depth + 1)? else { return Ok(None) };
        let r = cx.eval(r)?;
        let shifted = subst(cx, b, r, t, b.sub(t, a))?;
        return Ok(Some(b.mul(&[b.call("exp", &[c0]), shifted, b.call("heaviside_theta", &[b.sub(t, a)])])));
    }
    // a shift: G(s + a) is exp(-a t) g(t)
    if let Some(a) = shift_of(cx, b, big, s)? {
        let u = h.sym_term(h.sym("transform·u"));
        let g = subst(cx, b, big, s, b.sub(u, a))?;
        let g = subst(cx, b, g, u, s)?;
        if let Some(r) = inv(cx, tc, table, g, s, t, depth + 2)? {
            return Ok(Some(b.mul(&[b.call("exp", &[b.neg(b.mul(&[a, t]))]), r])));
        }
        // a completed square under a root, (s^2 + b s + c)^p: the shifted base expanded (s^2 + c - b^2/4)
        let ge = apply(cx, b, "expand_all", &[g])?;
        if ge != g {
            if let Some(r) = inv(cx, tc, table, ge, s, t, depth + 2)? {
                return Ok(Some(b.mul(&[b.call("exp", &[b.neg(b.mul(&[a, t]))]), r])));
            }
        }
    }
    // G(s)/s: the integral of g from 0 to t
    for (i, &g) in fs.iter().enumerate() {
        if args_of(b, g, b.hd.pow) != Some(&[s, b.int(-1)][..]) {
            continue;
        }
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        let Some(r) = inv(cx, tc, table, rest, s, t, depth + 1)? else { break };
        let r = cx.eval(r)?;
        let u = h.sym_term(h.sym("transform·u"));
        let ru = subst(cx, b, r, t, u)?;
        let range = h.call(b.hd.list, &[u, b.zero(), t]);
        let o = opts_no_conditions(cx, b);
        let v = apply(cx, b, "integrate", &[ru, range, o])?;
        if is_free(b, v, h.sym_term(h.sym("integrate"))) && !is_failure(b, v) {
            return Ok(Some(v));
        }
        break;
    }
    // the derivative rule for logarithms and inverse tangents: -L^-1[F'](t)/t
    if has_log_like(b, big) {
        let d = apply(cx, b, "d", &[big, s])?;
        let d = apply(cx, b, "together", &[d])?;
        if !has_log_like(b, d) {
            if let Some(r) = inv(cx, tc, table, d, s, t, depth + 1)? {
                return Ok(Some(b.neg(b.div(r, t))));
            }
        }
    }
    Ok(None)
}

fn opts_no_conditions<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>) -> T<'h> {
    let h = b.heap();
    h.call(h.sym("options"), &[h.call(h.sym(names::RULE), &[h.sym_term(h.sym("generate_conditions")), cx.bool(false)])])
}

fn has_log_like<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    ["log", "atan", "acot"].iter().any(|n| !is_free(b, t, h.sym_term(h.sym(n))))
}

/// The `a` of a term that depends on `s` only through `s + a` (a single linear form `s + a`, `a != 0`).
fn shift_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, s: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut found: Option<T<'h>> = None;
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if is_free(b, u, s) {
            continue;
        }
        if u == s {
            if found.is_none() {
                return Ok(None);
            }
            continue;
        }
        if args_of(b, u, b.hd.add).is_some() && found.is_none() {
            let cl = apply(cx, b, "coefficient_list", &[u, s])?;
            if let Some(&[_, c1, c2]) = args_of(b, cl, b.hd.list) {
                if c2 == b.one() && is_free(b, c1, s) && !b.is_zero(c1) {
                    found = Some(cx.eval(b.div(c1, b.int(2)))?);
                    continue;
                }
            }
        }
        if args_of(b, u, b.hd.add).is_some() {
            if let Some((k, c)) = linear(cx, b, u, s)? {
                if k == b.one() && !b.is_zero(c) {
                    match found {
                        None => found = Some(c),
                        Some(a) if a == c => {}
                        _ => return Ok(None),
                    }
                    continue;
                }
            }
        }
        if let View::App { args, .. } = h.view(u) {
            stack.extend_from_slice(args);
        }
    }
    // every s must be inside the form: substituting s = u - a must remove a
    let Some(a) = found else { return Ok(None) };
    let u = h.sym_term(h.sym("transform·u"));
    let g = subst(cx, b, f, s, b.sub(u, a))?;
    Ok((!is_free(b, g, u) && g != f).then_some(a))
}

/// Is `f` a rational function of `s` (sums, products, integer powers of polynomials in `s`)?
pub(crate) fn rational_in<'h>(b: &Build<'h>, f: T<'h>, s: T<'h>) -> bool {
    let h = b.heap();
    if is_free(b, f, s) {
        return true;
    }
    if f == s {
        return true;
    }
    if let Some(args) = args_of(b, f, b.hd.add).or_else(|| args_of(b, f, b.hd.mul)) {
        return args.iter().all(|&a| rational_in(b, a, s));
    }
    match args_of(b, f, b.hd.pow) {
        Some(&[base, e]) => matches!(h.view(e), View::Int(_)) && rational_in(b, base, s),
        _ => false,
    }
}

/// The poles of the rational function `n/d` (numerator and denominator polynomials in `s`): each root with its
/// multiplicity and the cofactor `Q` with `d = (s - p)^m Q` (as a term in `s`); `None` when a factor's roots are
/// not found.
#[allow(clippy::type_complexity)]
pub(crate) fn poles<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, d: T<'h>, s: T<'h>) -> Res<Option<Vec<(T<'h>, i64, T<'h>)>>> {
    let h = b.heap();
    let fd = apply(cx, b, "factor", &[d])?;
    let mut konst = vec![];
    let mut bases: Vec<(T<'h>, i64)> = vec![];
    for g in factors(b, fd) {
        if is_free(b, g, s) {
            konst.push(g);
            continue;
        }
        match args_of(b, g, b.hd.pow) {
            Some(&[base, e]) => match h.view(e) {
                View::Int(m) if m > 0 => bases.push((base, m)),
                _ => return Ok(None),
            },
            _ => bases.push((g, 1)),
        }
    }
    // the roots of each factor, with its leading coefficient
    let mut info = vec![];
    for &(base, m) in &bases {
        let cl = apply(cx, b, "coefficient_list", &[base, s])?;
        let Some(cs) = args_of(b, cl, b.hd.list) else { return Ok(None) };
        if cs.iter().any(|&c| !is_free(b, c, s)) || cs.len() < 2 {
            return Ok(None);
        }
        let deg = cs.len() - 1;
        let lc = cs[deg];
        let roots = match deg {
            1 => vec![cx.eval(b.neg(b.div(cs[0], cs[1])))?],
            2 => {
                let (c0, c1, c2) = (cs[0], cs[1], cs[2]);
                let disc = cx.eval(b.sub(b.mul(&[c1, c1]), b.mul(&[b.int(4), c2, c0])))?;
                let r = apply(cx, b, "power_expand", &[b.pow(disc, b.rat(1, 2))])?;
                let two_a = b.mul(&[b.int(2), c2]);
                let r1 = apply(cx, b, "expand", &[b.div(b.sub(b.neg(c1), r), two_a)])?;
                let r2 = apply(cx, b, "expand", &[b.div(b.add(&[b.neg(c1), r]), two_a)])?;
                vec![r1, r2]
            }
            _ => {
                let eq = h.call(h.sym("equal"), &[base, b.zero()]);
                let sol = apply(cx, b, "solve", &[eq, s])?;
                let Some(rs) = args_of(b, sol, b.hd.list) else { return Ok(None) };
                let mut out = vec![];
                for &r in rs {
                    let Some(&[rule]) = args_of(b, r, b.hd.list) else { return Ok(None) };
                    let Some(&[_, v]) = args_of(b, rule, h.sym(names::RULE)) else { return Ok(None) };
                    if !is_free(b, v, h.sym_term(h.sym("root"))) {
                        return Ok(None);
                    }
                    out.push(v);
                }
                if out.len() != deg {
                    return Ok(None);
                }
                out
            }
        };
        if roots.len() == 2 && roots[0] == roots[1] {
            return Ok(None);
        }
        info.push((base, m, lc, roots));
    }
    let mut out = vec![];
    for (i, (_, m, lc, roots)) in info.iter().enumerate() {
        for (j, &p) in roots.iter().enumerate() {
            // the cofactor: the constant, the other factors, the other roots of this factor
            let mut q = konst.clone();
            for (k, &(base, mk)) in bases.iter().enumerate() {
                if k != i {
                    q.push(b.powi(base, mk));
                }
            }
            q.push(b.powi(*lc, *m));
            for (k, &r) in roots.iter().enumerate() {
                if k != j {
                    q.push(b.powi(b.sub(s, r), *m));
                }
            }
            out.push((p, *m, b.mul(&q)));
        }
    }
    Ok(Some(out))
}

/// The residue of `n(s)/((s - p)^m q(s)) exp(s t)` at `p`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn residue_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, n: T<'h>, p: T<'h>, m: i64, q: T<'h>, s: T<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let g = b.mul(&[n, b.powi(q, -1), b.call("exp", &[b.mul(&[s, t])])]);
    let g = if m > 1 {
        let dg = apply(cx, b, "d", &[g, h.call(b.hd.list, &[s, b.int(m - 1)])])?;
        let fact: i64 = (1..m).product();
        b.div(dg, b.int(fact))
    } else {
        g
    };
    let g = cx.eval(g)?;
    let r = subst(cx, b, g, s, p)?;
    Ok(r)
}

/// The inverse Laplace transform of a rational function of `s`: delta functions for its polynomial part, residues
/// for the rest (module comment).
fn rational<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, big: T<'h>, s: T<'h>, t: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let r = apply(cx, b, "together", &[big])?;
    let n = apply(cx, b, "numerator", &[r])?;
    let d = apply(cx, b, "denominator", &[r])?;
    if is_free(b, d, s) {
        return Ok(None);
    }
    let dn = degree(cx, b, n, s)?;
    let dd = degree(cx, b, d, s)?;
    let (Some(dn), Some(dd)) = (dn, dd) else { return Ok(None) };
    let mut out = vec![];
    let mut n = n;
    if dn >= dd {
        let q = apply(cx, b, "polynomial_quotient", &[n, d, s])?;
        n = apply(cx, b, "polynomial_remainder", &[n, d, s])?;
        let cl = apply(cx, b, "coefficient_list", &[q, s])?;
        let Some(cs) = args_of(b, cl, b.hd.list) else { return Ok(None) };
        for (k, &c) in cs.iter().enumerate() {
            if b.is_zero(c) {
                continue;
            }
            let delta = if k == 0 {
                b.call("dirac_delta", &[t])
            } else {
                b.app(b.derivative(&[k as i64], h.sym_term(h.sym("dirac_delta"))), &[t])
            };
            out.push(b.mul(&[c, delta]));
        }
    }
    let Some(ps) = poles(cx, b, d, s)? else { return Ok(None) };
    let mut res = vec![];
    for (p, m, q) in ps {
        cx.tick(1)?;
        res.push(residue_at(cx, b, n, p, m, q, s, t)?);
    }
    let sum = cx.eval(b.add(&res))?;
    out.push(trig_form(cx, b, sum)?);
    Ok(Some(cx.eval(b.add(&out))?))
}

/// Exponentials of complex arguments written with `sin` and `cos` (`exp_to_trig`, then simplified), when the value
/// holds `im`; the smaller of the forms.
pub(crate) fn trig_form<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let size = super::laplace::size;
    // powers with imaginary exponents as exponentials (x^(i a) is exp(i a log(x)))
    let v = imaginary_powers(cx, b, v)?;
    let mut best = v;
    // distributions are kept as they are (simplify would put dirac_delta(w) over a denominator with w)
    if !is_free(b, v, h.sym_term(h.sym("dirac_delta"))) {
        let e = apply(cx, b, "exp_to_trig", &[v])?;
        return Ok(if is_failure(b, e) { v } else { e });
    }
    if !is_free(b, v, h.sym_term(h.sym("complex"))) || call_of(b, v).is_some() {
        let e = apply(cx, b, "exp_to_trig", &[v])?;
        for cand in [e, apply(cx, b, "simplify", &[e])?] {
            if !is_failure(b, cand)
                && (is_free(b, cand, h.sym_term(h.sym("complex"))) || size(b, cand) < size(b, best))
                && (!is_free(b, best, h.sym_term(h.sym("complex"))) || size(b, cand) <= size(b, best))
            {
                best = cand;
            }
        }
    }
    let simp = apply(cx, b, "simplify", &[best])?;
    if !is_failure(b, simp) && size(b, simp) < size(b, best) {
        best = simp;
    }
    hyperbolic(cx, b, best)
}

fn imaginary_powers<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>) -> Res<T<'h>> {
    fn walk<'h>(b: &Build<'h>, t: T<'h>, im: T<'h>) -> T<'h> {
        let h = b.heap();
        let View::App { head, args } = h.view(t) else { return t };
        let new: Vec<T<'h>> = args.iter().map(|&a| walk(b, a, im)).collect();
        if b.sym_of(head) == Some(b.hd.pow) && new.len() == 2 && b.sym_of(new[0]) != Some(b.hd.e) && !is_free(b, new[1], im) && !matches!(h.view(new[0]), View::Int(_) | View::Rational { .. }) {
            return b.call("exp", &[b.mul(&[new[1], b.call("log", &[new[0]])])]);
        }
        if new.as_slice() == args {
            t
        } else {
            h.app(head, &new)
        }
    }
    let h = b.heap();
    let im = h.sym_term(h.sym("complex"));
    if is_free(b, v, im) {
        return Ok(v);
    }
    let r = walk(b, v, im);
    if r == v {
        return Ok(v);
    }
    cx.eval(r)
}

/// `c1 exp(u) + c2 exp(-u)` as `(c1 + c2) cosh(u) + (c1 - c2) sinh(u)` when `u` holds a parameter (the tables'
/// form of `sinh(a t)`, `cosh(a t)`); numeric rates stay exponentials.
fn hyperbolic<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    if !is_free(b, v, h.sym_term(h.sym("complex"))) {
        return Ok(v);
    }
    let e = apply(cx, b, "expand", &[v])?;
    let ts = terms(b, e);
    if ts.len() < 2 {
        return Ok(v);
    }
    // each term as coefficient times exp(u)
    let mut parts: Vec<(T<'h>, Option<T<'h>>)> = vec![];
    for &u in &ts {
        let fs = factors(b, u);
        let ex: Vec<usize> = fs.iter().enumerate().filter(|(_, &g)| matches!(call_of(b, g), Some((n, _)) if n == "exp")).map(|(i, _)| i).collect();
        if ex.len() == 1 {
            let (_, a) = call_of(b, fs[ex[0]]).unwrap();
            let mut c = fs.clone();
            c.remove(ex[0]);
            parts.push((b.mul(&c), Some(a[0])));
        } else {
            parts.push((u, None));
        }
    }
    let mut used = vec![false; parts.len()];
    let mut out = vec![];
    let mut changed = false;
    for i in 0..parts.len() {
        if used[i] {
            continue;
        }
        let (c1, Some(u)) = parts[i] else { continue };
        let num_only = mc_integrals::definite::check::symbols_count(b, u) <= 1;
        if num_only {
            continue;
        }
        let mu = cx.eval(b.neg(u))?;
        if let Some(j) = (i + 1..parts.len()).find(|&j| !used[j] && parts[j].1 == Some(mu)) {
            let c2 = parts[j].0;
            used[i] = true;
            used[j] = true;
            // the argument with a positive sign: u or -u, whichever has no leading minus
            let (u, c1, c2) = if matches!(args_of(b, u, b.hd.mul).and_then(|a| a.first().copied()).map(|k| h.view(k)), Some(View::Int(k)) if k < 0) {
                (mu, c2, c1)
            } else {
                (u, c1, c2)
            };
            let ch = cx.eval(b.add(&[c1, c2]))?;
            let sh = cx.eval(b.sub(c1, c2))?;
            out.push(b.mul(&[ch, b.call("cosh", &[u])]));
            out.push(b.mul(&[sh, b.call("sinh", &[u])]));
            changed = true;
        }
    }
    if !changed {
        return Ok(v);
    }
    for (i, &(c, u)) in parts.iter().enumerate() {
        if !used[i] {
            out.push(match u {
                Some(u) => b.mul(&[c, b.call("exp", &[u])]),
                None => c,
            });
        }
    }
    let r = cx.eval(b.add(&out))?;
    let r2 = apply(cx, b, "together", &[r])?;
    Ok(if super::laplace::size(b, r2) < super::laplace::size(b, r) { r2 } else { r })
}

/// The degree of a polynomial in `s` (`None` when it is not one).
pub(crate) fn degree<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: T<'h>, s: T<'h>) -> Res<Option<usize>> {
    if is_free(b, p, s) {
        return Ok(Some(0));
    }
    let cl = apply(cx, b, "coefficient_list", &[p, s])?;
    match args_of(b, cl, b.hd.list) {
        Some(cs) if cs.iter().all(|&c| is_free(b, c, s)) => Ok(Some(cs.len().saturating_sub(1))),
        _ => Ok(None),
    }
}

/// The two-sided inverse Laplace transform of `big` (a `conditional_expression(F, strip)` gives the strip).
pub fn inverse_bilateral<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, big: T<'h>, s: T<'h>, t: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    let (f, strip) = super::conditional(b, big);
    let (lo, hi) = match strip {
        Some(c) => strip_bounds(cx, b, c, s)?,
        None => (None, None),
    };
    if let Some((n, [g, x, y])) = call_of(b, f) {
        if n == "bilateral_laplace_transform" && *y == s {
            return Ok(Some(subst(cx, b, *g, *x, t)?));
        }
    }
    // a constant: a delta function; exp(c s^2 + d s): a Gaussian
    if is_free(b, f, s) {
        return Ok(Some(cx.eval(b.mul(&[f, b.call("dirac_delta", &[t])]))?));
    }
    let (c, rest) = split_constant(b, f, s);
    if let Some((n, [u])) = call_of(b, rest) {
        if n == "exp" {
            let cl = apply(cx, b, "coefficient_list", &[*u, s])?;
            if let Some(&[c0, c1, c2]) = args_of(b, cl, b.hd.list) {
                if [c0, c1, c2].iter().all(|&k| is_free(b, k, s)) && sign(cx, b, c2)? == Some(1) {
                    // exp(c2 s^2 + c1 s + c0): exp(c0 - (t + c1)^2/(4 c2))/sqrt(4 pi c2)
                    let tt = b.add(&[t, c1]);
                    let v = b.mul(&[
                        c,
                        b.call("exp", &[b.sub(c0, b.div(b.mul(&[tt, tt]), b.mul(&[b.int(4), c2])))]),
                        b.pow(b.mul(&[b.int(4), b.sym(h.sym("pi")), c2]), b.rat(-1, 2)),
                    ]);
                    return Ok(Some(cx.eval(v)?));
                }
            }
        }
    }
    // delays exp(a s) G(s): g(t + a)
    let ts = terms(b, f);
    if ts.len() > 1 && !rational_in(b, f, s) {
        let mut out = vec![];
        for g in ts {
            let inner = cx.eval(match strip {
                Some(k) => b.call("conditional_expression", &[g, k]),
                None => g,
            })?;
            let Some(r) = inverse_bilateral(cx, tc, inner, s, t)? else { return Ok(None) };
            out.push(r);
        }
        return Ok(Some(cx.eval(b.add(&out))?));
    }
    let fs = factors(b, f);
    for (i, &g) in fs.iter().enumerate() {
        let Some((n, [u])) = call_of(b, g) else { continue };
        if n != "exp" {
            continue;
        }
        let Some((k, c0)) = linear(cx, b, *u, s)? else { continue };
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        let inner = cx.eval(match strip {
            Some(kk) => b.call("conditional_expression", &[rest, kk]),
            None => rest,
        })?;
        let Some(r) = inverse_bilateral(cx, tc, inner, s, t)? else { return Ok(None) };
        let shifted = subst(cx, b, r, t, b.add(&[t, k]))?;
        return Ok(Some(cx.eval(b.mul(&[b.call("exp", &[c0]), shifted]))?));
    }
    if !rational_in(b, f, s) {
        return Ok(None);
    }
    // a rational function: residues left of the strip for t >= 0, right of it for t < 0
    let r = apply(cx, b, "together", &[f])?;
    let n = apply(cx, b, "numerator", &[r])?;
    let d = apply(cx, b, "denominator", &[r])?;
    let (Some(dn), Some(dd)) = (degree(cx, b, n, s)?, degree(cx, b, d, s)?) else { return Ok(None) };
    if dn >= dd {
        return Ok(None);
    }
    let Some(ps) = poles(cx, b, d, s)? else { return Ok(None) };
    let (mut left, mut right, mut axis) = (vec![], vec![], vec![]);
    for (p, m, q) in ps {
        // without a strip, a simple pole on the imaginary axis: the principal value, sign(t)/2
        if lo.is_none() && hi.is_none() && m == 1 {
            let re = apply(cx, b, "re", &[p])?;
            if b.is_zero(re) {
                axis.push(residue_at(cx, b, n, p, m, q, s, t)?);
                continue;
            }
        }
        let re = apply(cx, b, "re", &[p])?;
        // the side of the strip: left when re(p) <= lo, right when re(p) >= hi; without a strip the poles with
        // negative real parts are on the left (the strip containing the imaginary axis)
        let side = match (lo, hi) {
            (Some(l), _) if sign(cx, b, b.sub(l, re))?.is_some_and(|x| x >= 0) => -1,
            (_, Some(u)) if sign(cx, b, b.sub(re, u))?.is_some_and(|x| x >= 0) => 1,
            (None, None) => match sign(cx, b, re)? {
                Some(-1) => -1,
                Some(1) => 1,
                _ => return Ok(None),
            },
            (Some(_), None) => 1,
            (None, Some(_)) => -1,
            _ => return Ok(None),
        };
        let rr = residue_at(cx, b, n, p, m, q, s, t)?;
        if side < 0 {
            left.push(rr);
        } else {
            right.push(rr);
        }
    }
    if left.is_empty() && right.is_empty() && !axis.is_empty() {
        let v = cx.eval(b.mul(&[b.rat(1, 2), b.call("sign", &[t]), b.add(&axis)]))?;
        return Ok(Some(trig_form(cx, b, v)?));
    }
    if !axis.is_empty() {
        return Ok(None);
    }
    let l = cx.eval(b.add(&left))?;
    let l = trig_form(cx, b, l)?;
    let rgt = cx.eval(b.neg(b.add(&right)))?;
    let rgt = trig_form(cx, b, rgt)?;
    let ge = h.call(h.sym("greater_equal"), &[t, b.zero()]);
    let v = b.call("piecewise", &[h.call(b.hd.list, &[h.call(b.hd.list, &[l, ge])]), rgt]);
    Ok(Some(cx.eval(v)?))
}

/// The bounds `lo < re(s) < hi` of a strip condition (either may be absent).
#[allow(clippy::type_complexity)]
fn strip_bounds<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>, s: T<'h>) -> Res<(Option<T<'h>>, Option<T<'h>>)> {
    let h = b.heap();
    let re = cx.eval(b.call("re", &[s]))?;
    let mut lo = None;
    let mut hi = None;
    let mut stack = vec![c];
    while let Some(u) = stack.pop() {
        let Some((n, args)) = call_of(b, u) else { continue };
        match n.as_str() {
            "and" => stack.extend_from_slice(args),
            "less" | "less_equal" | "greater" | "greater_equal" => {
                let inc = matches!(n.as_str(), "less" | "less_equal");
                for w in args.windows(2) {
                    let (a, z) = if inc { (w[0], w[1]) } else { (w[1], w[0]) };
                    if z == re && is_free(b, a, s) {
                        lo = Some(a);
                    }
                    if a == re && is_free(b, z, s) {
                        hi = Some(z);
                    }
                }
            }
            _ => {}
        }
    }
    let _ = h;
    Ok((lo, hi))
}
