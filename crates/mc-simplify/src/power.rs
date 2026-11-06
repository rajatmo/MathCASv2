//! `power_expand(e)`: the power and logarithm laws applied without their side conditions,
//!
//! `(a b)^c = a^c b^c`, `(a^b)^c = a^(b c)`, `log(a b) = log(a) + log(b)`, `log(a^b) = b log(a)`, `log(exp(u)) = u`,
//!
//! which hold for positive real `a`, `b` (and real `u`), so `power_expand(sqrt(x^2))` is `x`. It is a formal
//! transformation, deliberately not used by `simplify` (decision 2B.3).
//!
//! Inside `assuming` (decision A.7) the context is read: `e` is refined first (the laws that hold under the facts,
//! `sqrt(x^2)` is `-x` for `x < 0`), and a law is then not applied to a base the facts prove negative.
use crate::facts::Facts;
use crate::refine::refine;
use crate::term::{fixed, B};
use mc_api::{Store, T};
use mc_eval::{NativeCx, Res};

/// `power_expand(e)`, with the facts of the context if there are any.
pub fn power_expand<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, mut facts: Option<&mut Facts<'_, 'h>>, t: T<'h>) -> Res<T<'h>> {
    let t = match facts.as_deref_mut() {
        Some(f) => refine(cx, b, f, t)?,
        None => t,
    };
    // a base the facts prove negative keeps its law unapplied
    let mut negative = |cx: &mut NativeCx<'_, '_, 'h>, bases: &[T<'h>]| -> Res<bool> {
        let Some(f) = facts.as_deref_mut() else { return Ok(false) };
        for &a in bases {
            let i = f.info(cx, a)?;
            if f.negative(&i) {
                return Ok(true);
            }
        }
        Ok(false)
    };
    fixed(cx, b, t, 64, &mut |cx, b, node| {
        let hd = &b.hd;
        if let Some((s, [base, c])) = b.call_of(node) {
            if s == hd.pow {
                if let Some((g, _)) = b.call_of(*base) {
                    // a sum under a non-integer power: factored, so the law can split it (G51)
                    if g == hd.add && !matches!(b.heap().view(*c), mc_api::View::Int(_)) {
                        let fac = crate::term::apply(cx, b, "factor", *base)?;
                        if fac != *base && b.call_of(fac).is_some_and(|(h2, _)| h2 == hd.mul || h2 == hd.pow) {
                            return Ok(Some(b.pow(fac, *c)));
                        }
                    }
                }
                if let Some((g, fs)) = b.call_of(*base) {
                    if g == hd.mul && !negative(cx, fs)? {
                        let ps: Vec<T<'h>> = fs.iter().map(|&f| b.pow(f, *c)).collect();
                        return Ok(Some(b.mul(&ps)));
                    }
                    if let (true, [a, e]) = (g == hd.pow, fs) {
                        if !negative(cx, &[*a])? {
                            return Ok(Some(b.pow(*a, b.mul(&[*e, *c]))));
                        }
                    }
                }
            }
        }
        let Some(u) = b.unary(node, hd.log) else { return Ok(None) };
        if let Some(v) = b.exp_arg(u) {
            return Ok(Some(v));
        }
        match b.call_of(u) {
            Some((g, fs)) if g == hd.mul && !negative(cx, fs)? => {
                let ls: Vec<T<'h>> = fs.iter().map(|&f| b.call(hd.log, &[f])).collect();
                Ok(Some(b.add(&ls)))
            }
            Some((g, [a, e])) if g == hd.pow && !negative(cx, &[*a])? => Ok(Some(b.mul(&[*e, b.call(hd.log, &[*a])]))),
            _ => Ok(None),
        }
    })
}
