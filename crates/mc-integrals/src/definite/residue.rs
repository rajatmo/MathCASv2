//! Definite integrals by residues (item 14D, step 5 of the driver). The residue theorem with the standard
//! contours (Ahlfors, *Complex Analysis*, ch. 4 §5; Whittaker and Watson §6.2-6.24):
//!
//! - **the real line**: the integrand is written as `Σ_a R_a(x) exp(i a x)` with rational `R_a` and real
//!   frequencies `a` (sines and cosines as exponentials). The contour is the real line indented by small
//!   half-circles *above* every real pole, the same for all terms, so their sum, which is regular on the line,
//!   integrates to the true value. A term with `a > 0` (Jordan's lemma, `R_a → 0`) or `a = 0` (`deg Q ≥ deg P + 2`)
//!   is closed in the upper half plane, which encloses its upper poles only: `2πi Σ_{im r > 0} res`. A term with
//!   `a < 0` is closed below, which encloses the lower poles and the real ones (the indentation passes above
//!   them): `-2πi Σ_{im r ≤ 0} res`. This gives `∫ sin(x)/x = π` and `∫ (1 - cos x)/x^2 = π` with no principal
//!   value or special case. Over `(0, inf)` an even integrand is half the real-line integral.
//! - **a period**: a rational function of `sin(k x)`, `cos(k x)` (integer `k`) over an interval of length `2π`
//!   becomes, with `z = exp(i x)`, a rational function of `z` over the unit circle: `2πi Σ_{|r| < 1} res R(z)/(i z)`.
//!   An even integrand over `(0, π)` is half of it.
//!
//! Poles are the roots of the denominator's irreducible factors over Q as `solve` gives them (radicals); a residue
//! at a root `r` of multiplicity `m` is `h^(m-1)(r)/(m-1)!` with `h = (z - r)^m R(z) exp(i a z)`, the quotient
//! taken factor by factor. The sum is put in real form (`complex_expand`, real part) when the integrand is real.
//! Whether a pole is above, on or below the line (inside or outside the circle) is decided by the decision service;
//! an undecided one (a parameter) leaves the method.
use super::{call_of, real_constant, sign, subst, Job, Pt};
use crate::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The most terms (after writing sines and cosines as exponentials) handled.
const MAX_TERMS: usize = 64;
/// The highest degree of a denominator handled.
const MAX_DEGREE: usize = 24;

/// The residue methods of the module comment.
pub(crate) fn by_residues<'h>(
    job: &mut Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    lo: Pt<'h>,
    hi: Pt<'h>,
) -> Res<Option<T<'h>>> {
    if job.late() {
        return Ok(None);
    }
    let h = b.heap();
    let pi = h.sym_term(h.sym("pi"));
    match (lo, hi) {
        (Pt::NegInf, Pt::PosInf) => real_line(job, cx, b, f, x),
        (Pt::Fin(z), Pt::PosInf) if z == b.zero() => {
            if !even(cx, b, f, x)? {
                return Ok(None);
            }
            Ok(real_line(job, cx, b, f, x)?.map(|v| b.mul(&[b.rat(1, 2), v])))
        }
        (Pt::Fin(l), Pt::Fin(r)) => {
            let len = cx.eval(b.sub(r, l))?;
            if len == cx.eval(b.mul(&[b.int(2), pi]))? {
                return period(cx, b, f, x, l);
            }
            if len == pi && (l == b.zero() || cx.eval(b.add(&[l, pi]))? == b.zero()) && even(cx, b, f, x)? {
                // (0, π) or (-π, 0): half of (-π, π)
                return Ok(period(cx, b, f, x, b.neg(pi))?.map(|v| b.mul(&[b.rat(1, 2), v])));
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

/// Is `f(-x) = f(x)`?
fn even<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<bool> {
    let g = subst(cx, b, f, x, b.neg(x))?;
    let d = apply(cx, b, "together", &[b.sub(f, g)])?;
    Ok(d == b.zero() || apply(cx, b, "simplify", &[d])? == b.zero())
}

/// Does `f` contain trigonometric or hyperbolic functions of `x`?
fn has_trig<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if is_free(b, u, x) {
            continue;
        }
        if let Some((n, args)) = call_of(b, u) {
            if matches!(n.as_str(), "sin" | "cos" | "tan" | "cot" | "sec" | "csc" | "sinh" | "cosh" | "tanh" | "sech" | "csch" | "coth") {
                return true;
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

/// The terms of a sum (the term itself otherwise).
fn terms<'h>(b: &Build<'h>, f: T<'h>) -> Vec<T<'h>> {
    match args_of(b, f, b.hd.add) {
        Some(ts) => ts.to_vec(),
        None => vec![f],
    }
}

/// `∫_{-∞}^{∞} f dx` by residues (module comment), or `None`.
fn real_line<'h>(job: &Job<'_, 'h>, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let im = h.sym_term(h.sym("im"));
    let mut g = f;
    if has_trig(b, f, x) {
        g = apply(cx, b, "trig_to_exp", &[f])?;
    }
    let g = apply(cx, b, "expand", &[g])?;
    let ts = terms(b, g);
    if ts.len() > MAX_TERMS {
        return Ok(None);
    }
    // the terms grouped by frequency
    let mut groups: Vec<(T<'h>, Vec<T<'h>>)> = vec![];
    for t in ts {
        cx.tick(1)?;
        let fs = match args_of(b, t, b.hd.mul) {
            Some(fs) => fs.to_vec(),
            None => vec![t],
        };
        let (mut us, mut rest) = (vec![], vec![]);
        for g in fs {
            match call_of(b, g) {
                Some((n, [u])) if n == "exp" && !is_free(b, *u, x) => us.push(*u),
                Some((n, [c, u])) if n == "pow" && b.sym_of(*c) == Some(b.hd.e) && !is_free(b, *u, x) => us.push(*u),
                _ => rest.push(g),
            }
        }
        let u = apply(cx, b, "expand", &[b.add(&us)])?;
        let kappa = apply(cx, b, "coefficient", &[u, x])?;
        let w = apply(cx, b, "expand", &[b.sub(u, b.mul(&[kappa, x]))])?;
        if !is_free(b, kappa, x) || !is_free(b, w, x) {
            return Ok(None);
        }
        // kappa = i a
        let a = cx.eval(b.neg(b.mul(&[im, kappa])))?;
        if !real_constant(cx, b, a)? {
            return Ok(None);
        }
        let part = b.mul(&[b.mul(&rest), b.call("exp", &[w])]);
        match groups.iter_mut().find(|(k, _)| *k == a) {
            Some((_, v)) => v.push(part),
            None => groups.push((a, vec![part])),
        }
    }
    let two_pi_i = b.mul(&[b.int(2), h.sym_term(h.sym("pi")), im]);
    let mut total = vec![];
    let mut regular: Vec<T<'h>> = vec![];
    for (a, parts) in groups {
        let r = cx.eval(b.add(&parts))?;
        if r == b.zero() {
            continue;
        }
        let Some((p, q)) = super::points::fraction(cx, b, r, x)? else { return Ok(None) };
        let (Some(pc), Some(qc)) = (super::points::coefficients(cx, b, p, x)?, super::points::coefficients(cx, b, q, x)?) else {
            return Ok(None);
        };
        let s = sign(cx, b, a)?;
        let need = if s == Some(0) { 2 } else { 1 };
        if qc.len() < pc.len() + need || qc.len() > MAX_DEGREE + 1 {
            return Ok(None);
        }
        let Some(s) = s else { return Ok(None) };
        let kernel = if s == 0 { b.one() } else { b.call("exp", &[b.mul(&[im, a, x])]) };
        let Some(poles) = poles(cx, b, q, x)? else { return Ok(None) };
        for pole in &poles {
            cx.tick(1)?;
            let ip = im_part(cx, b, pole.root)?;
            let Some(side) = sign(cx, b, ip)? else { return Ok(None) };
            if side == 0 && !regular_at(job, cx, b, f, x, pole.root, &mut regular)? {
                // a pole of the integrand itself on the line: divergent, or a principal value (not by this method)
                return Ok(None);
            }
            let inside = if s >= 0 { side == 1 } else { side <= 0 };
            if !inside {
                continue;
            }
            let Some(res) = residue(cx, b, p, q, &poles, pole, kernel, x)? else { return Ok(None) };
            total.push(if s >= 0 { b.mul(&[two_pi_i, res]) } else { b.neg(b.mul(&[two_pi_i, res])) });
        }
    }
    let v = cx.eval(b.add(&total))?;
    Ok(Some(real_form(cx, b, f, v)?))
}

/// A pole: a root of an irreducible factor of the denominator, its multiplicity and its factor.
struct Pole<'h> {
    root: T<'h>,
    mult: i64,
    factor: T<'h>,
}

/// The imaginary part of a closed constant.
fn im_part<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, r: T<'h>) -> Res<T<'h>> {
    let ce = apply(cx, b, "complex_expand", &[r])?;
    let i = apply(cx, b, "im_part", &[ce])?;
    if call_of(b, i).is_some_and(|(n, _)| n == "im_part") {
        return apply(cx, b, "im_part", &[r]);
    }
    Ok(i)
}

/// The roots of `q` (a polynomial in `x` with constant coefficients) with their multiplicities, factor by factor;
/// `None` when a factor's roots are not all found.
fn poles<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, q: T<'h>, x: T<'h>) -> Res<Option<Vec<Pole<'h>>>> {
    let h = b.heap();
    let fq = apply(cx, b, "factor", &[q])?;
    let fs = match args_of(b, fq, b.hd.mul) {
        Some(fs) => fs.to_vec(),
        None => vec![fq],
    };
    let mut out = vec![];
    for g in fs {
        if is_free(b, g, x) {
            continue;
        }
        let (base, m) = match call_of(b, g) {
            Some((n, [base, e])) if n == "pow" => match h.view(*e) {
                View::Int(m @ 1..) => (*base, m),
                _ => return Ok(None),
            },
            _ => (g, 1),
        };
        let Some(cs) = super::points::coefficients(cx, b, base, x)? else { return Ok(None) };
        let deg = cs.len() - 1;
        let eq = h.call(h.sym("equal"), &[base, b.zero()]);
        let sol = apply(cx, b, "solve", &[eq, x])?;
        let Some(rows) = args_of(b, sol, b.hd.list) else { return Ok(None) };
        if rows.len() != deg {
            return Ok(None);
        }
        for &row in rows {
            let Some([rule]) = args_of(b, row, b.hd.list) else { return Ok(None) };
            let Some((n, [_, r])) = call_of(b, *rule) else { return Ok(None) };
            if n != "rule" || !is_free(b, *r, x) {
                return Ok(None);
            }
            out.push(Pole { root: *r, mult: m, factor: base });
        }
    }
    Ok(Some(out))
}

/// The residue of `kernel p/q` at `pole` (module comment).
#[allow(clippy::too_many_arguments)]
fn residue<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    p: T<'h>,
    q: T<'h>,
    poles: &[Pole<'h>],
    pole: &Pole<'h>,
    kernel: T<'h>,
    x: T<'h>,
) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let r = pole.root;
    if pole.mult == 1 {
        let dq = apply(cx, b, "d", &[q, x])?;
        let v = subst(cx, b, b.div(b.mul(&[p, kernel]), dq), x, r)?;
        return Ok(Some(apply(cx, b, "together", &[v])?));
    }
    // q / (x - r)^m: the leading coefficient, the other factors, and the quotient of r's own factor
    let lc = {
        let cs = super::points::coefficients(cx, b, q, x)?;
        let Some(cs) = cs else { return Ok(None) };
        let mut den = vec![];
        let mut seen = vec![];
        for o in poles {
            if seen.contains(&o.factor) {
                continue;
            }
            seen.push(o.factor);
            let Some(fc) = super::points::coefficients(cx, b, o.factor, x)? else { return Ok(None) };
            den.push(b.pow(*fc.last().unwrap(), b.int(o.mult)));
        }
        cx.eval(b.div(*cs.last().unwrap(), b.mul(&den)))?
    };
    let mut rest = vec![lc];
    let mut seen = vec![];
    for o in poles {
        if seen.contains(&o.factor) {
            continue;
        }
        seen.push(o.factor);
        if o.factor == pole.factor {
            let quot = apply(cx, b, "polynomial_quotient", &[o.factor, b.sub(x, r), x])?;
            rest.push(b.pow(quot, b.int(o.mult)));
        } else {
            rest.push(b.pow(o.factor, b.int(o.mult)));
        }
    }
    let hz = b.div(b.mul(&[p, kernel]), b.mul(&rest));
    let m = pole.mult;
    let spec = h.call(b.hd.list, &[x, b.int(m - 1)]);
    let dh = apply(cx, b, "d", &[hz, spec])?;
    let mut fact = 1i64;
    for k in 2..m {
        fact *= k;
    }
    let v = subst(cx, b, dh, x, r)?;
    Ok(Some(apply(cx, b, "together", &[b.div(v, b.int(fact))])?))
}

/// `v` in real form when `f` is real on the range (no `im` in it): the real part after `complex_expand`; else `v`
/// simplified.
pub(super) fn real_form<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, v: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let im = h.sym_term(h.sym("im"));
    let ce = apply(cx, b, "complex_expand", &[v])?;
    if is_free(b, f, im) {
        let re = apply(cx, b, "re", &[ce])?;
        if is_free(b, re, h.sym_term(h.sym("re"))) && is_free(b, re, im) {
            return apply(cx, b, "simplify", &[re]);
        }
    }
    apply(cx, b, "simplify", &[ce])
}

/// `∫` over a period of length `2π` starting at `l` (module comment), or `None`.
fn period<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, l: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    // the integrand is 2π-periodic: the start of the period does not matter
    let _ = l;
    if !has_trig(b, f, x) {
        return Ok(None);
    }
    let im = h.sym_term(h.sym("im"));
    let z = h.sym_term(h.sym("z$res"));
    let g = apply(cx, b, "trig_to_exp", &[f])?;
    let Some(g) = to_z(cx, b, g, x, z, 0)? else { return Ok(None) };
    let g = cx.eval(b.div(g, b.mul(&[im, z])))?;
    if !is_free(b, g, x) {
        return Ok(None);
    }
    let Some((p, q)) = super::points::fraction(cx, b, g, z)? else { return Ok(None) };
    let Some(qc) = super::points::coefficients(cx, b, q, z)? else { return Ok(None) };
    if qc.len() > MAX_DEGREE + 1 {
        return Ok(None);
    }
    let Some(poles) = poles(cx, b, q, z)? else { return Ok(None) };
    let two_pi_i = b.mul(&[b.int(2), h.sym_term(h.sym("pi")), im]);
    let mut total = vec![];
    for pole in &poles {
        cx.tick(1)?;
        let d = cx.eval(b.sub(b.one(), b.call("abs", &[pole.root])))?;
        let Some(s) = sign(cx, b, d)? else { return Ok(None) };
        if s == 0 {
            // a pole on the circle: a singularity on the range (left to the other methods)
            return Ok(None);
        }
        if s < 0 {
            continue;
        }
        let Some(res) = residue(cx, b, p, q, &poles, pole, b.one(), z)? else { return Ok(None) };
        total.push(b.mul(&[two_pi_i, res]));
    }
    let v = cx.eval(b.add(&total))?;
    Ok(Some(real_form(cx, b, f, v)?))
}

/// `g` with every `exp(i k x + w)` (integer `k`) written `exp(w) z^k`; `None` when `x` occurs otherwise.
fn to_z<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, z: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if is_free(b, g, x) {
        return Ok(Some(g));
    }
    if depth > 60 || g == x {
        return Ok(None);
    }
    let im = h.sym_term(h.sym("im"));
    let exp_arg = match call_of(b, g) {
        Some((n, [u])) if n == "exp" => Some(*u),
        Some((n, [c, u])) if n == "pow" && b.sym_of(*c) == Some(b.hd.e) => Some(*u),
        _ => None,
    };
    if let Some(u) = exp_arg {
        let u = apply(cx, b, "expand", &[u])?;
        let kappa = apply(cx, b, "coefficient", &[u, x])?;
        let w = apply(cx, b, "expand", &[b.sub(u, b.mul(&[kappa, x]))])?;
        let k = cx.eval(b.neg(b.mul(&[im, kappa])))?;
        if !is_free(b, w, x) {
            return Ok(None);
        }
        let View::Int(k) = h.view(k) else { return Ok(None) };
        return Ok(Some(b.mul(&[b.call("exp", &[w]), b.powi(z, k)])));
    }
    let View::App { head, args } = h.view(g) else { return Ok(None) };
    if !is_free(b, head, x) {
        return Ok(None);
    }
    let mut out = Vec::with_capacity(args.len());
    for &a in args {
        let Some(t) = to_z(cx, b, a, x, z, depth + 1)? else { return Ok(None) };
        out.push(t);
    }
    Ok(Some(b.app(head, &out)))
}

/// Is `f` regular at the real point `r` (a finite two-sided limit)? Points already shown regular are kept in `seen`.
fn regular_at<'h>(
    job: &Job<'_, 'h>,
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    r: T<'h>,
    seen: &mut Vec<T<'h>>,
) -> Res<bool> {
    if seen.contains(&r) {
        return Ok(true);
    }
    if job.opts.pv {
        return Ok(false);
    }
    let o = mc_limit::limit_with(
        cx,
        b,
        f,
        x,
        r,
        mc_limit::Direction::TwoSided,
        &mc_limit::Options { budget: std::time::Duration::from_secs(2), check: true },
    )?;
    let i = b.heap().sym_term(b.heap().sym("inf"));
    let ok = matches!(o, mc_limit::Outcome::Value(v) if is_free(b, v, i) && !super::is_failure(b, v));
    if ok {
        seen.push(r);
    }
    Ok(ok)
}
