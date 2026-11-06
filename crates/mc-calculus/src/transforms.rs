//! `fourier_transform(f, t, w)` and `inverse_fourier_transform(g, w, t)` in the unitary convention
//! (decision 2C.10): `F(w) = 1/sqrt(2 pi) integrate(f(t) exp(im w t), [t, -inf, inf])` and
//! `f(t) = 1/sqrt(2 pi) integrate(F(w) exp(-im w t), [w, -inf, inf])`.
//!
//! The integral is taken by `integrate` after two reductions: factors `unit_step(t - a)`, `unit_step(b - t)` (and
//! `unit_step(t)`, `unit_step(-t)`) of the integrand become the bounds; a Gaussian `c exp(-a t^2 + b t + d)` (a, b,
//! c, d free of t) has the closed form `c sqrt(pi/a) exp(b^2/(4 a) + d)` (for `re(a) > 0`). When neither the reductions
//! nor `integrate` give a value the call stays unevaluated.
use crate::integrate::{apply, is_free};
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The native of both transforms.
pub struct Fourier {
    /// `true` for the inverse transform (the kernel `exp(-im w t)`)
    pub inverse: bool,
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

impl Native for Fourier {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [f, t, w] = args else { return Ok(Reply::Decline) };
        if !matches!(h.view(*t), View::Sym(_)) {
            return Ok(Reply::Decline);
        }
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        // a list maps
        if let View::App { head, args: items } = h.view(*f) {
            if b.sym_of(head) == Some(b.hd.list) {
                let mut out = Vec::with_capacity(items.len());
                for &g in items {
                    match self.one(cx, &b, g, *t, *w)? {
                        Some(r) => out.push(r),
                        None => return Ok(Reply::Decline),
                    }
                }
                return Ok(Reply::Again(h.call(b.hd.list, &out)));
            }
        }
        Ok(self.one(cx, &b, *f, *t, *w)?.map_or(Reply::Decline, Reply::Again))
    }
}

impl Fourier {
    fn one<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, t: T<'h>, w: T<'h>) -> Res<Option<T<'h>>> {
        let h = b.heap();
        let i = h.call(h.sym(names::COMPLEX), &[b.int(0), b.int(if self.inverse { -1 } else { 1 })]);
        let kernel = b.call("exp", &[b.mul(&[i, w, t])]);
        let norm = b.pow(b.mul(&[b.int(2), b.sym(h.sym("pi"))]), b.rat(-1, 2));
        // the integrand, with unit steps turned into bounds
        let inf = b.sym(h.sym("inf"));
        let (mut lo, mut hi) = (b.neg(inf), inf);
        let mut rest = vec![];
        let factors = match h.view(f) {
            View::App { head, args } if b.sym_of(head) == Some(b.hd.mul) => args.to_vec(),
            _ => vec![f],
        };
        for g in factors {
            match self.step_bound(cx, b, g, t)? {
                Some((true, a)) => lo = a,
                Some((false, a)) => hi = a,
                None => rest.push(g),
            }
        }
        let g = cx.eval(b.mul(&[b.mul(&rest), kernel]))?;
        if lo == b.neg(inf) && hi == inf {
            if let Some(r) = gaussian(cx, b, g, t)? {
                return Ok(Some(b.mul(&[norm, r])));
            }
        }
        let range = h.call(b.hd.list, &[t, lo, hi]);
        let r = apply(cx, b, "integrate", &[g, range])?;
        if matches!(h.view(r), View::App { head, .. } if b.sym_of(head) == Some(h.sym("integrate"))) {
            return Ok(None);
        }
        if !is_free(b, r, h.sym_term(h.sym(names::FAILURE))) {
            return Ok(None);
        }
        Ok(Some(b.mul(&[norm, r])))
    }

    /// `unit_step(t - a)` as the lower bound `a` (`(true, a)`), `unit_step(b - t)` as the upper bound `b`.
    fn step_bound<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, t: T<'h>) -> Res<Option<(bool, T<'h>)>> {
        let h = b.heap();
        let View::App { head, args: &[u] } = h.view(g) else { return Ok(None) };
        if b.sym_of(head) != Some(h.sym("unit_step")) || is_free(b, u, t) {
            return Ok(None);
        }
        // u = c t + d with c = ±1
        let c = cx.eval(b.call("coefficient", &[u, t]))?;
        let d = cx.eval(b.sub(u, b.mul(&[c, t])))?;
        let d = cx.eval(b.call("expand", &[d]))?;
        if !is_free(b, d, t) {
            return Ok(None);
        }
        Ok(match h.view(c) {
            View::Int(1) => Some((true, b.neg(d))),
            View::Int(-1) => Some((false, d)),
            _ => None,
        })
    }
}

/// `integrate(c exp(q(t)), [t, -inf, inf])` for a quadratic `q = -a t^2 + b t + d` with `re(a) > 0` decided:
/// `c sqrt(pi/a) exp(b^2/(4 a) + d)`.
fn gaussian<'h>(cx: &mut NativeCx<'_, '_, 'h>, bd: &Build<'h>, g: T<'h>, t: T<'h>) -> Res<Option<T<'h>>> {
    let h = bd.heap();
    let factors = match h.view(g) {
        View::App { head, args } if bd.sym_of(head) == Some(bd.hd.mul) => args.to_vec(),
        _ => vec![g],
    };
    let mut expo = vec![];
    let mut coef = vec![];
    for f in factors {
        if is_free(bd, f, t) {
            coef.push(f);
            continue;
        }
        let e = match h.view(f) {
            View::App { head, args: &[u] } if bd.sym_of(head) == Some(h.sym("exp")) => u,
            View::App { head, args: &[base, u] } if bd.sym_of(head) == Some(bd.hd.pow) && bd.sym_of(base) == Some(h.sym("e")) => u,
            _ => return Ok(None),
        };
        expo.push(e);
    }
    if expo.is_empty() {
        return Ok(None);
    }
    let q = cx.eval(bd.call("expand", &[bd.add(&expo)]))?;
    let cl = cx.eval(bd.call("coefficient_list", &[q, t]))?;
    let View::App { args: &[d, b1, a2], .. } = h.view(cl) else { return Ok(None) };
    let a = bd.neg(a2);
    // re(a) > 0
    let re = cx.eval(bd.call("re", &[a]))?;
    let pos = cx.eval(bd.call("is_positive", &[re]))?;
    if !cx.is_true(pos) {
        return Ok(None);
    }
    let root = bd.pow(bd.div(bd.sym(h.sym("pi")), a), bd.rat(1, 2));
    let ex = bd.add(&[bd.div(bd.mul(&[b1, b1]), bd.mul(&[bd.int(4), a])), d]);
    Ok(Some(cx.eval(bd.mul(&[bd.mul(&coef), root, bd.call("exp", &[ex])]))?))
}
