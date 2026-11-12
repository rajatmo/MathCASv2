//! Real bounds and critical points ([`Pt`]: a finite real constant or an infinity), the sign and order of constants,
//! and substitution, shared by limits and definite integrals (round 34: moved here from the definite integrals,
//! now `mc-integrals`, when limits and integration became crates of their own).
use mc_api::{names, Store, View, T};
use mc_calculus::common::{apply, is_free};
use mc_calculus::term::Build;
use mc_eval::{NativeCx, Res};
use std::cmp::Ordering;

/// A real bound or critical point: a finite real constant or an infinity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pt<'h> {
    /// `-inf`
    NegInf,
    /// a finite real constant (exact or numeric)
    Fin(T<'h>),
    /// `inf`
    PosInf,
}

/// The symbol `inf`.
pub fn inf<'h>(b: &Build<'h>) -> T<'h> {
    b.heap().sym_term(b.heap().sym("inf"))
}

/// A real bound: `inf`, `-inf` or a term whose numerical value is a finite machine real.
pub fn point<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<Option<Pt<'h>>> {
    let i = inf(b);
    if t == i {
        return Ok(Some(Pt::PosInf));
    }
    if t == b.neg(i) {
        return Ok(Some(Pt::NegInf));
    }
    if !is_free(b, t, i) {
        return Ok(None);
    }
    Ok(real_constant(cx, b, t)?.then_some(Pt::Fin(t)))
}

/// Is `t` a real constant: its numerical value a finite machine real?
pub fn real_constant<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<bool> {
    if let View::Int(_) | View::Rational { .. } = b.heap().view(t) {
        return Ok(true);
    }
    let v = apply(cx, b, "n", &[t])?;
    if matches!(b.heap().view(v), View::Real(r) if r.is_finite()) {
        return Ok(true);
    }
    // inside assuming: a symbolic constant the facts make real (decision 2C4.5)
    if cx.assumptions().is_empty() || !is_free(b, t, inf(b)) {
        return Ok(false);
    }
    let h = b.heap();
    let e = h.call(h.sym("element"), &[t, h.sym_term(h.sym("reals"))]);
    let r = apply(cx, b, "refine", &[e])?;
    Ok(cx.is_true(r))
}

/// The sign of a constant as -1, 0 or 1, or `None` when it is not decided: by the decision service under the facts
/// of the context (item F3), then by `sign` and `refine` as before.
pub fn sign<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>) -> Res<Option<i8>> {
    if v == b.zero() {
        return Ok(Some(0));
    }
    if let Some(s) = mc_decide::sign_in_context(cx, v)?.exact() {
        return Ok(Some(s));
    }
    let mut s = apply(cx, b, "sign", &[v])?;
    if !matches!(b.heap().view(s), View::Int(_)) && !cx.assumptions().is_empty() {
        // inside assuming: the sign the facts decide (decision 2C4.5)
        s = apply(cx, b, "refine", &[s])?;
    }
    Ok(match b.heap().view(s) {
        View::Int(k @ -1..=1) => Some(k as i8),
        _ => None,
    })
}

/// The order of two points, `None` when it is not decided.
pub fn cmp<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: Pt<'h>, q: Pt<'h>) -> Res<Option<Ordering>> {
    Ok(match (p, q) {
        (Pt::Fin(a), Pt::Fin(c)) => sign(cx, b, b.sub(a, c))?.map(|s| s.cmp(&0)),
        (Pt::NegInf, Pt::NegInf) | (Pt::PosInf, Pt::PosInf) => Some(Ordering::Equal),
        (Pt::NegInf, _) | (_, Pt::PosInf) => Some(Ordering::Less),
        (Pt::PosInf, _) | (_, Pt::NegInf) => Some(Ordering::Greater),
    })
}

/// `rewrite(g, [x -> v])`, evaluated.
pub fn subst<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, v: T<'h>) -> Res<T<'h>> {
    if is_free(b, g, x) {
        return Ok(g);
    }
    let h = b.heap();
    let rule = h.call(h.sym(names::RULE), &[x, v]);
    apply(cx, b, "rewrite", &[g, h.call(b.hd.list, &[rule])])
}
