//! Ranges of single special functions proved one by one from their monotonicity and limits:
//!
//! - **The hyperfactorial** on `x >= a` (or `x > a`) with `-1 <= a <= 0`. With `L = (log H)' =
//!   log_gamma(x + 1) + x + 1/2 - log(2 pi)/2` (from `G'/G`, DLMF 5.17.4; the derivative the engine gives is `L H`), `L' = psi(x + 1) + 1`
//!   increases strictly (`psi' > 0`, DLMF 5.15.1) and vanishes once, at `x0` with `psi(x0 + 1) = -1`; `x0 < 0` as
//!   `psi(1) + 1 = 1 - gamma > 0`. `L(-1+) = inf` (`log_gamma(0+)`), `L(0) = 1/2 - log(2 pi)/2 < 0` and `L(1) = 3/2 -
//!   log(2 pi)/2 > 0`, so `L` has exactly two zeros on `x > -1`: `c1` in `(-1, x0)` and `c2` in `(0, 1)` (`L`
//!   increases on `[0, inf)`). `H = exp(int L) > 0` increases on `[-1, c1]`, decreases on `[c1, c2]` and increases on
//!   `[c2, inf)` to `inf` (`L -> inf`). With `H(-1) = H(0) = 1`, every value on `[a, c2]` is at least `H(c2) < H(0)`, so
//!   the range is `y >= H(c2)`, taken at the root object `c2` of `L` (any certified root in `(0, 1)` is `c2`).
//! - **The complete elliptic integral of the third kind in its characteristic** `n` for a fixed `0 < m < 1`. For `n <
//!   1`, `Pi(n, m) = int_0^(pi/2) dt/((1 - n sin^2 t) sqrt(1 - m sin^2 t))` (DLMF 19.2.7) is positive, increases
//!   strictly with `n` (the integrand does), tends to 0 as `n -> -inf` (dominated by `1/((1 + |n| sin^2 t) sqrt(1 -
//!   m))`, whose integral is `pi/(2 sqrt((1 + |n|)(1 - m)))`) and to `inf` as `n -> 1-` (monotone convergence to the
//!   divergent integral of `1/cos^2 t`): the values `(0, inf)`. For `n > 1` the Cauchy principal value is `Pi(n, m) =
//!   K(m) - Pi(m/n, m)` (DLMF 19.6.5), and `m/n` runs over `(0, m)`, so the values run strictly between `K(m) - Pi(m,
//!   m)` (`n -> 1+`, below 0 since `Pi(m, m) > Pi(0, m) = K(m)`) and `K(m) - Pi(0, m) = 0` (`n -> inf`), neither
//!   taken. `n = 1` is outside the domain. The range is `K(m) - Pi(m, m) < y < 0 or y > 0`.
use super::range::affine_in;
use crate::common::is_free;
use crate::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The range of `f` (on the region `cond`) for the functions of the module comment; `None` for any other.
pub fn special_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, cond: Option<T<'h>>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let View::App { head, args } = h.view(f) else { return Ok(None) };
    let Some(s) = b.sym_of(head) else { return Ok(None) };
    match (h.sym_name(s).as_str(), args) {
        ("hyperfactorial", &[u]) if u == x => hyperfactorial_range(cx, b, cond, x, y),
        ("elliptic_pi", &[u, m]) if cond.is_none() && is_free(b, m, x) && !is_free(b, u, x) && affine_in(cx, b, u, x)? => elliptic_pi_range(cx, b, m, y),
        _ => Ok(None),
    }
}

/// The lower end `a` and whether it is closed, for a condition `x >= a`, `x > a` (either side written first).
fn lower_end<'h>(b: &Build<'h>, c: T<'h>, x: T<'h>) -> Option<(T<'h>, bool)> {
    let h = b.heap();
    let View::App { head, args: &[l, r] } = h.view(c) else { return None };
    let name = h.sym_name(b.sym_of(head)?);
    match name.as_str() {
        "greater_equal" if l == x && is_free(b, r, x) => Some((r, true)),
        "greater" if l == x && is_free(b, r, x) => Some((r, false)),
        "less_equal" if r == x && is_free(b, l, x) => Some((l, true)),
        "less" if r == x && is_free(b, l, x) => Some((l, false)),
        _ => None,
    }
}

/// The hyperfactorial on `x >= a` or `x > a` with `-1 <= a <= 0` (module comment).
fn hyperfactorial_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, cond: Option<T<'h>>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let Some((a, _)) = cond.and_then(|c| lower_end(b, c, x)) else { return Ok(None) };
    // -1 <= a <= 0, proved (an exact or closed end only: a machine number proves nothing)
    if !matches!(super::proof::closed_sign(cx, b, b.add(&[a, b.one()]))?, Some(0 | 1)) || !matches!(super::proof::closed_sign(cx, b, a)?, Some(0 | -1)) {
        return Ok(None);
    }
    // the zero c2 of L in (0, 1), as a certified root object
    let src = "root([t_hf => log_gamma(t_hf + 1) + t_hf + 1/2 - log(2*pi)/2, 0.54])";
    let c2 = cx.eval(b.call("to_expression", &[h.string(src)]))?;
    let inside = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<bool> {
        Ok(super::proof::closed_sign(cx, b, t)? == Some(1) && super::proof::closed_sign(cx, b, b.sub(b.one(), t))? == Some(1))
    };
    // an object that is not certified has no value, and no proved sign
    if !inside(cx, c2)? {
        return Ok(None);
    }
    let v = cx.eval(b.call("hyperfactorial", &[c2]))?;
    Ok(Some(cx.eval(b.call("greater_equal", &[y, v]))?))
}

/// `elliptic_pi(n, m)` in `n` for a fixed `0 < m < 1` (module comment).
fn elliptic_pi_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, m: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    // 0 < m < 1, proved
    if super::proof::closed_sign(cx, b, m)? != Some(1) || super::proof::closed_sign(cx, b, b.sub(b.one(), m))? != Some(1) {
        return Ok(None);
    }
    let low = cx.eval(b.sub(b.call("elliptic_k", &[m]), b.call("elliptic_pi", &[m, m])))?;
    let r = b.call("or", &[b.call("and", &[b.call("less", &[low, y]), b.call("less", &[y, b.zero()])]), b.call("greater", &[y, b.zero()])]);
    Ok(Some(cx.eval(r)?))
}
