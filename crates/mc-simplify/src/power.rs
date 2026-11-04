//! `power_expand(e)`: the power and logarithm laws applied without their side conditions,
//!
//! `(a b)^c = a^c b^c`, `(a^b)^c = a^(b c)`, `log(a b) = log(a) + log(b)`, `log(a^b) = b log(a)`, `log(exp(u)) = u`,
//!
//! which hold for positive real `a`, `b` (and real `u`), so `power_expand(sqrt(x^2))` is `x`. It is a formal
//! transformation, deliberately not used by `simplify` (decision 2B.3).
use crate::term::{fixed, B};
use mc_api::T;
use mc_eval::{NativeCx, Res};

/// `power_expand(e)`.
pub fn power_expand<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    fixed(cx, b, t, 64, &mut |_, b, node| {
        let hd = &b.hd;
        if let Some((s, [base, c])) = b.call_of(node) {
            if s == hd.pow {
                if let Some((g, fs)) = b.call_of(*base) {
                    if g == hd.mul {
                        let ps: Vec<T<'h>> = fs.iter().map(|&f| b.pow(f, *c)).collect();
                        return Ok(Some(b.mul(&ps)));
                    }
                    if let (true, [a, e]) = (g == hd.pow, fs) {
                        return Ok(Some(b.pow(*a, b.mul(&[*e, *c]))));
                    }
                }
            }
        }
        let Some(u) = b.unary(node, hd.log) else { return Ok(None) };
        if let Some(v) = b.exp_arg(u) {
            return Ok(Some(v));
        }
        match b.call_of(u) {
            Some((g, fs)) if g == hd.mul => {
                let ls: Vec<T<'h>> = fs.iter().map(|&f| b.call(hd.log, &[f])).collect();
                Ok(Some(b.add(&ls)))
            }
            Some((g, [a, e])) if g == hd.pow => Ok(Some(b.mul(&[*e, b.call(hd.log, &[*a])]))),
            _ => Ok(None),
        }
    })
}
