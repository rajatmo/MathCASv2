//! Substitutions of definite integrals (item 14D, step 6 of the driver).
//!
//! - **`x = log(u)`**: an integrand that is a rational function of `exp(x)` (hyperbolic functions included, written
//!   with exponentials) becomes `R(u)/u` over `(exp(a), exp(b))`, a rational integral the other methods finish
//!   (`∫ sech(x) dx` over the real line is `∫_0^∞ 2/(1 + u^2) du = π`). Exponentials `exp(k x + w)` with integer `k`
//!   only.
//! - **`x -> 1/x` on `(0, inf)`**: when `f(1/x)/x^2 = -f(x)`, the integrals over `(0, 1)` and `(1, inf)` cancel
//!   (`∫_0^∞ log(x)/(1 + x^2) dx = 0`).
//! - **`x = exp(-u)` on `(0, 1)`**: an integrand with `log(x)` becomes `f(exp(-u)) exp(-u)` over `(0, inf)` with
//!   `log(x) = -u`, for the gamma forms (`∫_0^1 x^a log(x)^k dx`, `∫_0^1 (-log x)^(s-1) dx = Γ(s)`).
use super::{call_of, forms, real_range, subst, Job, Pt};
use crate::{apply, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The substitution `x = log(u)` (module comment), or `None`.
pub(crate) fn exp_rational<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<T<'h>>> {
    if !has_exp(b, f, x) {
        return Ok(None);
    }
    let h = b.heap();
    let u = h.sym_term(h.sym("u$exp"));
    let g = apply(cx, b, "trig_to_exp", &[f])?;
    let Some(g) = to_u(cx, b, g, x, u, 0)? else { return Ok(None) };
    let g = cx.eval(b.div(g, u))?;
    if !is_free(b, g, x) {
        return Ok(None);
    }
    let g = apply(cx, b, "together", &[g])?;
    if super::points::fraction(cx, b, g, u)?.is_none() {
        return Ok(None);
    }
    let end = |cx: &mut NativeCx<'_, '_, 'h>, p: Pt<'h>| -> Res<Pt<'h>> {
        Ok(match p {
            Pt::NegInf => Pt::Fin(b.zero()),
            Pt::PosInf => Pt::PosInf,
            Pt::Fin(c) => Pt::Fin(cx.eval(b.call("exp", &[c]))?),
        })
    };
    let (l, r) = (end(cx, lo)?, end(cx, hi)?);
    let v = real_range(job, cx, b, g, u, l, r)?;
    Ok(v.filter(|&v| is_free(b, v, u)))
}

/// Does `f` hold an exponential or hyperbolic function of `x`?
fn has_exp<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![f];
    while let Some(t) = stack.pop() {
        if is_free(b, t, x) {
            continue;
        }
        if let Some((n, args)) = call_of(b, t) {
            match (n.as_str(), args) {
                ("exp" | "sinh" | "cosh" | "tanh" | "sech" | "csch" | "coth", _) => return true,
                ("pow", [c, _]) if b.sym_of(*c) == Some(b.hd.e) => return true,
                _ => stack.extend_from_slice(args),
            }
        }
    }
    false
}

/// `g` with every `exp(k x + w)` (integer `k`) written `exp(w) u^k`; `None` when `x` occurs otherwise.
fn to_u<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, u: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if is_free(b, g, x) {
        return Ok(Some(g));
    }
    if depth > 60 || g == x {
        return Ok(None);
    }
    let arg = match call_of(b, g) {
        Some((n, [a])) if n == "exp" => Some(*a),
        Some((n, [c, a])) if n == "pow" && b.sym_of(*c) == Some(b.hd.e) => Some(*a),
        _ => None,
    };
    if let Some(a) = arg {
        let a = apply(cx, b, "expand", &[a])?;
        let k = apply(cx, b, "coefficient", &[a, x])?;
        let w = apply(cx, b, "expand", &[b.sub(a, b.mul(&[k, x]))])?;
        if !is_free(b, w, x) {
            return Ok(None);
        }
        let View::Int(k) = h.view(k) else { return Ok(None) };
        return Ok(Some(b.mul(&[b.call("exp", &[w]), b.powi(u, k)])));
    }
    let View::App { head, args } = h.view(g) else { return Ok(None) };
    if !is_free(b, head, x) {
        return Ok(None);
    }
    let mut out = Vec::with_capacity(args.len());
    for &a in args {
        let Some(t) = to_u(cx, b, a, x, u, depth + 1)? else { return Ok(None) };
        out.push(t);
    }
    Ok(Some(b.app(head, &out)))
}

/// `x -> 1/x` on `(0, inf)` (module comment): 0 when the integrand is odd under it; `None` otherwise.
pub(crate) fn inversion<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, lo: Pt<'h>, hi: Pt<'h>) -> Res<Option<T<'h>>> {
    let (Pt::Fin(z), Pt::PosInf) = (lo, hi) else { return Ok(None) };
    if z != b.zero() {
        return Ok(None);
    }
    let g = subst(cx, b, f, x, b.powi(x, -1))?;
    let g = cx.eval(b.div(g, b.powi(x, 2)))?;
    let s = apply(cx, b, "together", &[b.add(&[f, g])])?;
    if s == b.zero() {
        return Ok(Some(b.zero()));
    }
    let s = apply(cx, b, "power_expand", &[s])?;
    if apply(cx, b, "simplify", &[s])? == b.zero() {
        return Ok(Some(b.zero()));
    }
    Ok(None)
}

/// `x = exp(-u)` on `(0, 1)` for integrands with `log(x)` (module comment), the gamma forms over `(0, inf)`.
pub(crate) fn log_to_exp<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<T<'h>>> {
    let (Pt::Fin(z), Pt::Fin(o)) = (lo, hi) else { return Ok(None) };
    if z != b.zero() || o != b.one() {
        return Ok(None);
    }
    let h = b.heap();
    let lx = b.call("log", &[x]);
    if is_free(b, f, lx) {
        return Ok(None);
    }
    let u = h.sym_term(h.sym("u$log"));
    let g = subst(cx, b, f, lx, b.neg(u))?;
    let g = subst(cx, b, g, x, b.call("exp", &[b.neg(u)]))?;
    let g = cx.eval(b.mul(&[g, b.call("exp", &[b.neg(u)])]))?;
    let g = apply(cx, b, "power_expand", &[g])?;
    if !is_free(b, g, x) || !is_free(b, g, h.sym_term(h.sym("log"))) {
        return Ok(None);
    }
    let v = forms::closed(job, cx, b, g, u, Pt::Fin(b.zero()), Pt::PosInf)?;
    Ok(v.filter(|&v| is_free(b, v, u)))
}
