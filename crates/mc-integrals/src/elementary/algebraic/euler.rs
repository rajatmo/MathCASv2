//! Euler's substitutions for one quadratic radical `y = sqrt(Q)`, `Q = a x^2 + b x + c`.
//!
//! 1. `a > 0`: `y = t - sqrt(a) x`, `x = (t^2 - c)/(b + 2 sqrt(a) t)`, `t = y + sqrt(a) x`;
//! 2. `c > 0`: `y = x t + sqrt(c)`, `x = (2 sqrt(c) t - b)/(a - t^2)`, `t = (y - sqrt(c))/x`;
//! 3. real roots `r_1`, `r_2`: `y = (x - r_1) t`, `x = (a r_2 - r_1 t^2)/(a - t^2)`, `t = y/(x - r_1)`.
//!
//! Each one that applies turns `R(x, y)` into a rational function of `t`; each answer is a candidate (the driver
//! keeps the smallest verified one). The identities `x = X(t(x))` and `y = Y(t(x))` hold for either branch of `y`,
//! so the answers are antiderivatives wherever they are defined.
use super::{change, sub, Cands, Level, Radical};
use crate::elementary::{deriv, sign};
use crate::{apply, is_free};
use mc_calculus::term::Build;
use mc_api::T;
use mc_eval::{NativeCx, Res};

/// The candidates of Euler's substitutions when every radical is a power of one quadratic's square root.
pub fn run<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &mut Cands<'h>, rads: &[Radical<'h>], lv: Level) -> Res<()> {
    let (f, x) = (c.f, c.x);
    let Some(r0) = rads.first() else { return Ok(()) };
    if rads.iter().any(|r| r.base != r0.base || r.den != 2) {
        return Ok(());
    }
    let Some(cs) = mc_integrate::terms::coeffs_in(&b.ar, r0.base, x) else { return Ok(()) };
    let [q0, q1, q2] = cs.as_slice() else { return Ok(()) };
    let (qa, qb, qc) = (*q2, *q1, *q0);
    let y = b.pow(r0.base, b.rat(1, 2));
    let t = super::super::fresh(b, "e", lv.depth);
    let half = b.rat(1, 2);
    let mut subs: Vec<(T<'h>, T<'h>, T<'h>, u64)> = vec![];
    if sign(cx, b, qa)? == Some(1) {
        let ra = b.pow(qa, half);
        let xt = b.div(b.sub(b.powi(t, 2), qc), b.add(&[qb, b.mul(&[b.int(2), ra, t])]));
        let yt = b.sub(t, b.mul(&[ra, xt]));
        subs.push((xt, yt, b.add(&[y, b.mul(&[ra, x])]), 100));
    }
    if sign(cx, b, qc)? == Some(1) {
        let rc = b.pow(qc, half);
        let xt = b.div(b.sub(b.mul(&[b.int(2), rc, t]), qb), b.sub(qa, b.powi(t, 2)));
        let yt = b.add(&[b.mul(&[xt, t]), rc]);
        subs.push((xt, yt, b.div(b.sub(y, rc), x), super::SINGULAR));
    }
    let disc = b.sub(b.powi(qb, 2), b.mul(&[b.int(4), qa, qc]));
    if sign(cx, b, disc)? == Some(1) {
        let sd = b.pow(disc, half);
        let two_a = b.mul(&[b.int(2), qa]);
        let r1 = b.div(b.sub(b.neg(qb), sd), two_a);
        let r2 = b.div(b.add(&[b.neg(qb), sd]), two_a);
        let t2 = b.powi(t, 2);
        let xt = b.div(b.sub(b.mul(&[qa, r2]), b.mul(&[r1, t2])), b.sub(qa, t2));
        let yt = b.mul(&[b.sub(xt, r1), t]);
        subs.push((xt, yt, b.div(y, b.sub(x, r1)), 100));
    }
    for (xt, yt, back, w) in subs {
        if super::late() {
            break;
        }
        let Some(dx) = deriv(cx, b, xt, t)? else { continue };
        let reps: Vec<(T<'h>, T<'h>)> = rads.iter().map(|r| (r.term, b.powi(yt, r.num))).collect();
        let g = b.mul(&[change(b, f, x, &reps, xt), dx]);
        if !is_free(b, g, x) || !super::radicals(b, g, t).is_empty() {
            continue;
        }
        let g = apply(cx, b, "together", &[g])?;
        if let Some(rt) = sub(cx, b, g, t, lv)? {
            c.offer_weighted(cx, b, super::super::subst(b, rt, t, back), w)?;
        }
    }
    Ok(())
}
