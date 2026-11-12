//! Rationalising substitutions for monomial, linear and linear-fractional radicals.
//!
//! All radicals `L^(p_i/q_i)` of one base `L = (a x + b)/(c x + d)` (`a d - b c` not zero) are rationalised
//! together by `t = L^(1/N)`, `N = lcm(q_i)`: `x = (d t^N - b)/(a - c t^N)`, `L^(p_i/q_i) = t^(p_i N/q_i)`. When
//! every radical has this base the integrand becomes a rational function of `t`; otherwise (nested radicals such as
//! `sqrt(x + sqrt(x))`, several linear bases) the radicals left have bases that are functions of `t`, and the
//! portfolio runs again on the result. At most two bases are tried, each one a candidate.
use super::{change, num_den, sub, Cands, Level, Radical};
use crate::elementary::deriv;
use crate::{apply, is_free};
use mc_calculus::term::Build;
use mc_api::T;
use mc_eval::{NativeCx, Res};

/// The bases tried.
const MAX_BASES: usize = 2;

/// The candidates of the rationalising substitutions (see the module).
pub fn run<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &mut Cands<'h>, rads: &[Radical<'h>], lv: Level) -> Res<()> {
    let (f, x) = (c.f, c.x);
    let mut tried: Vec<T<'h>> = vec![];
    // innermost radicals last in `rads`: try those first
    for r in rads.iter().rev() {
        if tried.contains(&r.base) || tried.len() >= MAX_BASES || super::late() {
            continue;
        }
        let Some((a, bb, cc, d)) = linear_fractional(cx, b, r.base, x)? else { continue };
        tried.push(r.base);
        let same: Vec<&Radical<'h>> = rads.iter().filter(|s| s.base == r.base).collect();
        let n = same.iter().fold(1i64, |acc, s| mc_integrate::algebraic::binomial::lcm(acc, s.den));
        if n > 12 {
            continue;
        }
        let t = super::super::fresh(b, "r", lv.depth);
        let tn = b.powi(t, n);
        let xt = b.div(b.sub(b.mul(&[d, tn]), bb), b.sub(a, b.mul(&[cc, tn])));
        let Some(dx) = deriv(cx, b, xt, t)? else { continue };
        let reps: Vec<(T<'h>, T<'h>)> = same.iter().map(|s| (s.term, b.powi(t, s.num * n / s.den))).collect();
        let mut g = b.mul(&[change(b, f, x, &reps, xt), dx]);
        if !is_free(b, g, x) {
            continue;
        }
        if super::radicals(b, g, t).is_empty() {
            g = apply(cx, b, "together", &[g])?;
        }
        if let Some(rt) = sub(cx, b, g, t, lv)? {
            let back = b.pow(r.base, b.rat(1, n));
            c.offer(cx, b, super::super::subst(b, rt, t, back))?;
        }
    }
    Ok(())
}

/// `(a, b, c, d)` with `t = (a x + b)/(c x + d)`, `t` depending on `x`; `None` for any other base.
fn linear_fractional<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>, T<'h>, T<'h>)>> {
    if !super::radicals(b, t, x).is_empty() {
        return Ok(None);
    }
    let (n, d) = num_den(cx, b, t)?;
    let (Some(nc), Some(dc)) = (mc_integrate::terms::coeffs_in(&b.ar, n, x), mc_integrate::terms::coeffs_in(&b.ar, d, x)) else {
        return Ok(None);
    };
    if nc.len() > 2 || dc.len() > 2 || (nc.len() < 2 && dc.len() < 2) {
        return Ok(None);
    }
    let at = |v: &[T<'h>], k: usize| v.get(k).copied().unwrap_or_else(|| b.zero());
    let (a, bb, c, dd) = (at(&nc, 1), at(&nc, 0), at(&dc, 1), at(&dc, 0));
    // a d - b c must not vanish (else t is constant)
    let det = apply(cx, b, "together", &[b.sub(b.mul(&[a, dd]), b.mul(&[bb, c]))])?;
    if b.is_zero(det) {
        return Ok(None);
    }
    Ok(Some((a, bb, c, dd)))
}
