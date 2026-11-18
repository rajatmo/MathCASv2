//! Ranges of the real solutions of the oscillating second-order equations: the Bessel functions `J_n`, `Y_n` of an
//! integer order `n >= 1` (and Anger's `J_n`, equal to Bessel's `J_n` for an integer order, DLMF 11.10.4), and the Airy
//! functions `Ai`, `Bi`. The extreme values are the values at critical points named exactly by root objects of the
//! derivative; the proof that no other critical point gives a more extreme value is the monotone function of Sonin
//! (Szego, Orthogonal Polynomials, 7.31; Watson, Treatise 15.3, 15.31; DLMF 9.9, 10.21):
//!
//! - `y'' + y'/x + q y = 0` with `q = 1 - n^2/x^2` (the Bessel equation): for `x > n`,
//!   `S = y^2 + y'^2/q` has `S' = -2 y'^2/(x q) - y'^2 q'/q^2 <= 0` (`q' = 2 n^2/x^3 > 0`), so at the critical
//!   points (`y' = 0`, `S = y^2`) the values `|y|` decrease with `x`. Every critical point of `J_n` lies beyond `n`
//!   (`J_n > 0` and `(x y')' = (n^2/x - x) y > 0` on `(0, n]`, as the first zero `j_(n,1)` exceeds `n`, DLMF 10.21.(i)),
//!   so the extreme values of `J_n` on `x > 0` are `J_n` at the first critical point (the maximum) and at the second
//!   (the minimum); `J_n(-x) = (-1)^n J_n(x)` gives the whole line. For `Y_n` (`Y_n -> -inf` at `0+`, DLMF 10.7.4)
//!   the values are negative up to the first zero, which is beyond `n` (DLMF 10.21.(i)), and in `x > n` each
//!   hump between two zeros holds exactly one critical point (at a critical point `y'' = -q y`, so a hump with
//!   `y > 0` has a maximum only), so the supremum is the value at the first positive-valued critical point and `Y_n`
//!   takes every value below it.
//! - `y'' + phi y = 0` with `phi = -x` (Airy, `y'' = x y`): for `x < 0`, `S = y^2 + y'^2/phi` has `S' = y'^2/phi^2 >=
//!   0`, so the values at the critical points decrease towards `-inf` in absolute value. `Ai' < 0` on `x > a'_1`
//!   (`Ai > 0` there, `Ai'' = x Ai` is negative on `x < 0` so `Ai'` decreases to `Ai'(0) < 0`, and positive and
//!   increasing on `x > 0` with `Ai' < 0`, tending to 0), so the range of `Ai` is `[Ai(a'_2), Ai(a'_1)]`; `Bi' > 0`
//!   on `x > b'_1` (convex on `(b'_1, b_1)` where `Bi < 0`, concave on `(b_1, 0)` where `Bi > 0` with `Bi'(0) > 0`,
//!   increasing for `x > 0`), `Bi -> inf` at `inf`, so the range of `Bi` is `[Bi(b'_1), inf)`.
//!
//! The critical points are located by scanning the derivative for sign changes and are named by the root objects
//! of the derivative, certified by the root isolation (an object that is not certified stays unevaluated and no
//! answer is given).
use super::range::{affine_in, outer_affine};
use super::spec;
use crate::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The families that are read.
#[derive(Clone, Copy)]
enum Fam {
    /// `J_n`, `n >= 1`
    J(i64),
    /// `Y_n`, `n >= 1`
    Y(i64),
    Ai,
    Bi,
    /// `sin(x)/sqrt(x)` on `x > 0`
    SinSqrt,
    /// `cos(x)/sqrt(x)` on `x > 0`
    CosSqrt,
}

/// A critical point found by scanning: its position and the kind (a maximum: `true`).
struct Crit {
    at: f64,
    max: bool,
}

/// The value of `t` as a machine real.
fn num<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = b.heap();
    let v = cx.eval(b.call("n", &[t]))?;
    Ok(match h.view(v) {
        View::Real(x) if x.is_finite() => Some(x),
        View::Int(k) => Some(k as f64),
        _ => None,
    })
}

/// An integer from 1 to 12 written as an integer or a real with no fractional part.
fn small_order<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<Option<i64>> {
    Ok(num(cx, b, t)?.filter(|v| v.fract() == 0.0 && (1.0..=12.0).contains(v)).map(|v| v as i64))
}

/// The critical points of `g` met going from `start` in the direction `dir` (+1 or -1) up to `len`, at most `want` of
/// them (sign changes of `g'` on a grid of step 0.02, refined by bisection). `None` when a value is not a number.
#[allow(clippy::too_many_arguments)]
fn scan<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, start: f64, dir: f64, len: f64, want: usize) -> Res<Option<Vec<Crit>>> {
    let dg = cx.eval(b.call("d", &[g, x]))?;
    let step = 0.02;
    let steps = (len / step) as usize;
    let mut out = vec![];
    let mut prev_t = start;
    let Some(mut prev) = super::value_at(cx, b, dg, x, prev_t)? else { return Ok(None) };
    for i in 1..=steps {
        cx.tick(1)?;
        let t = start + dir * step * i as f64;
        let Some(v) = super::value_at(cx, b, dg, x, t)? else { return Ok(None) };
        if prev != 0.0 && v != 0.0 && (prev > 0.0) != (v > 0.0) {
            // bisection on [lo, hi] with lo < hi in x
            let (mut lo, mut hi, mut flo) = if prev_t < t { (prev_t, t, prev) } else { (t, prev_t, v) };
            let max = flo > 0.0;
            for _ in 0..40 {
                let mid = 0.5 * (lo + hi);
                let Some(m) = super::value_at(cx, b, dg, x, mid)? else { return Ok(None) };
                if m == 0.0 {
                    lo = mid;
                    hi = mid;
                    break;
                }
                if (m > 0.0) == (flo > 0.0) {
                    lo = mid;
                    flo = m;
                } else {
                    hi = mid;
                }
            }
            out.push(Crit { at: 0.5 * (lo + hi), max });
            if out.len() == want {
                return Ok(Some(out));
            }
        }
        prev_t = t;
        prev = v;
    }
    Ok(Some(out))
}

/// The value of the call `head(args.., root)` at the certified root of the derivative of `g` near `at`
/// (`t_osc` stands for the variable of the root object); `None` when the root object is not certified.
fn value_at_crit<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, fam: Fam, at: f64) -> Res<Option<(T<'h>, f64)>> {
    let h = b.heap();
    let (call, outer) = match fam {
        Fam::J(n) => (format!("bessel_j({n}, t_osc)"), format!("bessel_j({n}, ROOT)")),
        Fam::Y(n) => (format!("bessel_y({n}, t_osc)"), format!("bessel_y({n}, ROOT)")),
        Fam::Ai => ("airy_ai(t_osc)".to_string(), "airy_ai(ROOT)".to_string()),
        Fam::Bi => ("airy_bi(t_osc)".to_string(), "airy_bi(ROOT)".to_string()),
        Fam::SinSqrt => ("sin(t_osc)/sqrt(t_osc)".to_string(), "sin(ROOT)/sqrt(ROOT)".to_string()),
        Fam::CosSqrt => ("cos(t_osc)/sqrt(t_osc)".to_string(), "cos(ROOT)/sqrt(ROOT)".to_string()),
    };
    let root_src = format!("root([t_osc => d({call}, t_osc), {at:.9}])");
    let root = cx.eval(b.call("to_expression", &[h.string(&root_src)]))?;
    // certified: the object has a numeric value
    if num(cx, b, root)?.is_none() {
        return Ok(None);
    }
    let src = outer.replace("ROOT", &root_src);
    let v = cx.eval(b.call("to_expression", &[h.string(&src)]))?;
    let Some(val) = num(cx, b, v)? else { return Ok(None) };
    Ok(Some((v, val)))
}

/// The range of `c g(u) + d` (`g` of the families above, `u` affine in `x`, `c`, `d` free of `x`) as a condition on `y`.
pub fn osc_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if !spec::params(b, &[f], &[x]).is_empty() {
        return Ok(None);
    }
    if let Some((c, cos)) = trig_sqrt_coefficient(cx, b, f, x)? {
        return if cos { cos_sqrt_range(cx, b, x, y, c) } else { sin_sqrt_range(cx, b, x, y, c) };
    }
    let (g, c, d) = outer_affine(cx, b, f, x)?;
    let View::App { head, args } = h.view(g) else { return Ok(None) };
    let Some(s) = b.sym_of(head) else { return Ok(None) };
    let name = h.sym_name(s).to_string();
    let (fam, u) = match (name.as_str(), args) {
        ("bessel_j" | "anger_j", &[o, u]) if crate::common::is_free(b, o, x) => match small_order(cx, b, o)? {
            Some(n) => (Fam::J(n), u),
            None => return Ok(None),
        },
        ("bessel_y", &[o, u]) if crate::common::is_free(b, o, x) => match small_order(cx, b, o)? {
            Some(n) => (Fam::Y(n), u),
            None => return Ok(None),
        },
        ("airy_ai", &[u]) => (Fam::Ai, u),
        ("airy_bi", &[u]) => (Fam::Bi, u),
        ("dawson_f", &[u]) => {
            if !affine_in(cx, b, u, x)? {
                return Ok(None);
            }
            let Some(v) = dawson_max(cx, b, x)? else { return Ok(None) };
            let lo = cx.eval(b.neg(v))?;
            return finish(cx, b, y, c, d, Some((lo, true)), Some((v, true)));
        }
        _ => return Ok(None),
    };
    if !affine_in(cx, b, u, x)? {
        return Ok(None);
    }
    let Some(cv) = num(cx, b, c)? else { return Ok(None) };
    if cv == 0.0 {
        return Ok(None);
    }
    // the bounds of g over the line: (lower, upper), `None` for no bound
    let (lo, hi): (Option<T<'h>>, Option<T<'h>>) = match fam {
        Fam::J(n) => {
            // the critical points beyond n: the first is the maximum, the second the minimum
            let Some(cs) = scan(cx, b, b.call("bessel_j", &[b.int(n), x]), x, n as f64, 1.0, 3.0 * (n as f64).cbrt() + 12.0, 2)? else { return Ok(None) };
            let [c1, c2] = cs.as_slice() else { return Ok(None) };
            if !c1.max || c2.max {
                return Ok(None);
            }
            let (Some((v1, n1)), Some((v2, n2))) = (value_at_crit(cx, b, fam, c1.at)?, value_at_crit(cx, b, fam, c2.at)?) else { return Ok(None) };
            if n1 <= 0.0 || n2 >= 0.0 || n2 <= -n1 {
                return Ok(None);
            }
            if n % 2 == 1 {
                (Some(cx.eval(b.neg(v1))?), Some(v1))
            } else {
                (Some(v2), Some(v1))
            }
        }
        Fam::Y(n) => {
            // the first critical point with a positive value beyond n is the largest value
            let Some(cs) = scan(cx, b, b.call("bessel_y", &[b.int(n), x]), x, n as f64, 1.0, 3.0 * (n as f64).cbrt() + 12.0, 3)? else { return Ok(None) };
            let mut top = None;
            for c1 in cs.iter().filter(|c| c.max) {
                if let Some((v, val)) = value_at_crit(cx, b, fam, c1.at)? {
                    if val > 0.0 {
                        top = Some(v);
                        break;
                    }
                }
            }
            let Some(v) = top else { return Ok(None) };
            (None, Some(v))
        }
        Fam::Ai => {
            let Some(cs) = scan(cx, b, b.call("airy_ai", &[x]), x, 0.0, -1.0, 8.0, 2)? else { return Ok(None) };
            let [c1, c2] = cs.as_slice() else { return Ok(None) };
            // going left from 0: the first critical point is a maximum, the second a minimum
            if !c1.max || c2.max {
                return Ok(None);
            }
            let (Some((v1, n1)), Some((v2, n2))) = (value_at_crit(cx, b, fam, c1.at)?, value_at_crit(cx, b, fam, c2.at)?) else { return Ok(None) };
            if n1 <= 0.0 || n2 >= 0.0 || n2 <= -n1 {
                return Ok(None);
            }
            (Some(v2), Some(v1))
        }
        Fam::SinSqrt | Fam::CosSqrt => return Ok(None),
        Fam::Bi => {
            let Some(cs) = scan(cx, b, b.call("airy_bi", &[x]), x, 0.0, -1.0, 8.0, 1)? else { return Ok(None) };
            let [c1] = cs.as_slice() else { return Ok(None) };
            if c1.max {
                return Ok(None);
            }
            let Some((v1, n1)) = value_at_crit(cx, b, fam, c1.at)? else { return Ok(None) };
            if n1 >= 0.0 {
                return Ok(None);
            }
            (Some(v1), None)
        }
    };
    finish(cx, b, y, c, d, lo.map(|v| (v, true)), hi.map(|v| (v, true)))
}

/// The largest value `v` of Dawson's integral `F(x) = e^(-x^2) int_0^x e^(t^2) dt` on the real line, as a root object.
/// `F` is odd and positive for `x > 0` (the integral), `F' = 1 - 2 x F` (DLMF 7.5.1 with 7.2.5). With `g = 1 - 2 x F`,
/// `g(0) = 1` and at every zero of `g` on `x > 0`, `g' = -2 F - 2 x g = -1/x < 0`: every zero is a downward crossing,
/// so there is at most one, and there is one (`2 x F -> 1` from above, DLMF 7.12.1: `F ~ 1/(2x) + 1/(4x^3)`), found as
/// a sign change. At it, `F(c) = 1/(2c)`, so `v = F(c)` is the root of `t -> F(1/(2t)) - t` near `1/(2c)` (the map
/// `t = 1/(2x)` is one to one, so the equation has no other root with `1/(2t)` a critical point) and the range is
/// `[-v, v]`, both ends taken (continuity, oddness).
fn dawson_max<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let Some(cs) = scan(cx, b, b.call("dawson_f", &[x]), x, 0.0, 1.0, 4.0, 1)? else { return Ok(None) };
    let [c1] = cs.as_slice() else { return Ok(None) };
    if !c1.max {
        return Ok(None);
    }
    let seed = 0.5 / c1.at;
    let v = cx.eval(b.call("to_expression", &[h.string(&format!("root([t_osc => dawson_f(1/(2*t_osc)) - t_osc, {seed:.12}])"))]))?;
    // certified: the object has a numeric value, and it agrees with F at the critical point
    let Some(vn) = num(cx, b, v)? else { return Ok(None) };
    if (vn - seed).abs() > 1e-6 {
        return Ok(None);
    }
    Ok(Some(v))
}

/// `(c, cos)` when `f = c sin(x)/sqrt(x)` (`cos` false) or `f = c cos(x)/sqrt(x)` (`cos` true) with `c` free of `x`; the
/// root may be written `(k x)^(-1/2)` with a constant `k` proved positive (`(pi x)^(-1/2)` of the Bessel functions of
/// half-integer order), which is `k^(-1/2) x^(-1/2)` for `x > 0`.
fn trig_sqrt_coefficient<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, bool)>> {
    let h = b.heap();
    let View::App { head, args } = h.view(f) else { return Ok(None) };
    if b.sym_of(head) != Some(b.hd.mul) {
        return Ok(None);
    }
    let (mut free, dep): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| crate::common::is_free(b, a, x));
    let trig = |t: T<'h>| match h.view(t) {
        View::App { head, args: &[u] } if u == x => b.sym_of(head).and_then(|s| match h.sym_name(s).as_str() {
            "sin" => Some(false),
            "cos" => Some(true),
            _ => None,
        }),
        _ => None,
    };
    // the base of a power with the exponent -1/2: x itself (factor 1), or k x with k free of x
    let rsqrt = |t: T<'h>| -> Option<Option<T<'h>>> {
        let View::App { head, args: &[base, ex] } = h.view(t) else { return None };
        if b.sym_of(head) != Some(b.hd.pow) || ex != b.rat(-1, 2) {
            return None;
        }
        if base == x {
            return Some(None);
        }
        let View::App { head: mh, args: margs } = h.view(base) else { return None };
        if b.sym_of(mh) != Some(b.hd.mul) {
            return None;
        }
        let (kf, kd): (Vec<T<'h>>, Vec<T<'h>>) = margs.iter().partition(|&&a| crate::common::is_free(b, a, x));
        (kd.as_slice() == [x] && !kf.is_empty()).then(|| Some(b.mul(&kf)))
    };
    let (cos, k) = match dep.as_slice() {
        [p, q] => match (trig(*p), rsqrt(*q), trig(*q), rsqrt(*p)) {
            (Some(c), Some(k), _, _) | (_, _, Some(c), Some(k)) => (c, k),
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    if let Some(k) = k {
        // k > 0 proved: (k x)^(-1/2) = k^(-1/2) x^(-1/2) on x > 0
        if super::proof::closed_sign(cx, b, k)? != Some(1) {
            return Ok(None);
        }
        free.push(b.pow(k, b.rat(-1, 2)));
    }
    let c = cx.eval(b.mul(&free))?;
    Ok(num(cx, b, c)?.is_some_and(|v| v != 0.0).then_some((c, cos)))
}

/// The range of `c sin(x)/sqrt(x)` on `x > 0`: at a critical point `tan x = 2 x`, so `g^2 = sin^2 x/x = 4 x/(1 + 4
/// x^2)`, which decreases for `x > 1/2`; `g' = (x cos x - sin(x)/2)/x^(3/2)` has no zero on `(0, 1/2]` (`N = x cos x -
/// sin(x)/2` has `N(0) = 0`, `N' = cos(x)/2 - x sin x > 0` there), `g -> 0` at both ends, so the extreme values are
/// those at the first (maximum) and second (minimum) critical points.
fn sin_sqrt_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, x: T<'h>, y: T<'h>, c: T<'h>) -> Res<Option<T<'h>>> {
    let g = cx.eval(b.mul(&[b.call("sin", &[x]), b.pow(x, b.rat(-1, 2))]))?;
    let Some(cs) = scan(cx, b, g, x, 0.5, 1.0, 8.0, 2)? else { return Ok(None) };
    let [c1, c2] = cs.as_slice() else { return Ok(None) };
    if !c1.max || c2.max {
        return Ok(None);
    }
    let (Some((v1, n1)), Some((v2, n2))) = (value_at_crit(cx, b, Fam::SinSqrt, c1.at)?, value_at_crit(cx, b, Fam::SinSqrt, c2.at)?) else { return Ok(None) };
    if n1 <= 0.0 || n2 >= 0.0 || n2 <= -n1 {
        return Ok(None);
    }
    finish(cx, b, y, c, b.zero(), Some((v2, true)), Some((v1, true)))
}

/// The range of `c cos(x)/sqrt(x)` on `x > 0`: `g = cos(x)/sqrt(x)` tends to `inf` at `0+` and to 0 at `inf`; `g' =
/// -(x sin x + cos(x)/2)/x^(3/2)` is negative on `(0, pi/2]` (both terms of the numerator positive there) and its
/// numerator `N` changes sign exactly once in `(pi/2, pi)` (`-pi/2` at `pi/2`, `1/2` at `pi`; `N' = -(sin(x)/2 + x
/// cos x)` vanishes once there, where `tan x = -2 x`, so `N` falls and then rises), so the first critical point `c1`
/// is the only one in `(pi/2, pi)`, a minimum with `g(c1) < 0` (the certified root object near the scanned point in that
/// interval is `c1`). At every critical point `tan x = -1/(2 x)`, so `g^2 =
/// cos^2 x/x = 4 x/(1 + 4 x^2)`, which decreases for `x > 1/2`: the values `|g|` at the later critical points are
/// smaller, and the range is `[g(c1), inf)` (continuity on the interval `x > 0`).
fn cos_sqrt_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, x: T<'h>, y: T<'h>, c: T<'h>) -> Res<Option<T<'h>>> {
    let g = cx.eval(b.mul(&[b.call("cos", &[x]), b.pow(x, b.rat(-1, 2))]))?;
    let Some(cs) = scan(cx, b, g, x, 0.5, 1.0, 4.0, 1)? else { return Ok(None) };
    let [c1] = cs.as_slice() else { return Ok(None) };
    if c1.max || !(std::f64::consts::FRAC_PI_2..std::f64::consts::PI).contains(&c1.at) {
        return Ok(None);
    }
    let Some((v1, n1)) = value_at_crit(cx, b, Fam::CosSqrt, c1.at)? else { return Ok(None) };
    if n1 >= 0.0 {
        return Ok(None);
    }
    finish(cx, b, y, c, b.zero(), Some((v1, true)), None)
}

/// An end of a range: the value and whether it is taken.
type End<'h> = Option<(T<'h>, bool)>;

/// The condition on `y` for `c g + d` when `g` has the range between `lo` and `hi`; a negative `c` swaps the ends.
fn finish<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, y: T<'h>, c: T<'h>, d: T<'h>, lo: End<'h>, hi: End<'h>) -> Res<Option<T<'h>>> {
    let Some(cv) = num(cx, b, c)? else { return Ok(None) };
    let (lo, hi) = if cv > 0.0 { (lo, hi) } else { (hi, lo) };
    let mut parts = vec![];
    if let Some((l, closed)) = lo {
        let v = cx.eval(b.add(&[b.mul(&[c, l]), d]))?;
        parts.push(b.call(if closed { "greater_equal" } else { "greater" }, &[y, v]));
    }
    if let Some((u, closed)) = hi {
        let v = cx.eval(b.add(&[b.mul(&[c, u]), d]))?;
        parts.push(b.call(if closed { "less_equal" } else { "less" }, &[y, v]));
    }
    let r = match (lo, hi) {
        (Some((l, lc)), Some((u, uc))) => {
            let (l, u) = (cx.eval(b.add(&[b.mul(&[c, l]), d]))?, cx.eval(b.add(&[b.mul(&[c, u]), d]))?);
            b.call("and", &[b.call(if lc { "less_equal" } else { "less" }, &[l, y]), b.call(if uc { "less_equal" } else { "less" }, &[y, u])])
        }
        _ => match parts.as_slice() {
            [a] => *a,
            _ => b.call("and", &parts),
        },
    };
    Ok(Some(cx.eval(r)?))
}

/// The Struve functions of a half-integer order, which evaluate to elementary forms: `L_nu` for `nu = 1/2, 3/2, ...`
/// and `H_nu` for `nu = 3/2, 5/2, ...` take every positive value on `x > 0` and no other (their real domain). `L_nu(x)
/// = (x/2)^(nu + 1) sum_k (x/2)^(2k)/(Gamma(k + 3/2) Gamma(k + nu + 3/2))` has positive terms and tends to 0 at `0+`
/// and to `inf` (DLMF 11.2.2, 11.6.2: `L_nu ~ I_(-nu)`); `H_nu(x) = 2 (x/2)^nu/(sqrt(pi) Gamma(nu + 1/2))
/// int_0^1 (1 - t^2)^(nu - 1/2) sin(x t) dt` (DLMF 11.5.1) has a weight that decreases for `nu > 1/2`, so by the
/// integration by parts `int w(t) sin(x t) dt = [w(1)(1 - cos x) + int (-w'(t))(1 - cos(x t)) dt]/x >= 0` it is positive
/// (strictly: `1 - cos(x t)` is positive off a null set), it tends to 0 at `0+`, and `H_nu - Y_nu` grows like
/// `(x/2)^(nu - 1)/(sqrt(pi) Gamma(nu + 1/2))` (DLMF 11.6.1) for `nu > 1`. The function is matched by its evaluated
/// form `c struve(nu, x) + d`.
pub fn struve_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if !spec::params(b, &[f], &[x]).is_empty() || !contains_half_power(b, f, x) {
        return Ok(None);
    }
    let (g, c, d) = outer_affine(cx, b, f, x)?;
    for (name, first) in [("struve_l", 1), ("struve_h", 3)] {
        for k in 0..6 {
            let nu = b.rat(first + 2 * k, 2);
            let s = cx.eval(b.call(name, &[nu, x]))?;
            if s == g {
                // a check at high precision (the elementary form cancels at small x in machine arithmetic)
                for v in [b.rat(1, 100), b.rat(1, 2), b.int(2), b.int(15)] {
                    let w = cx.eval(h.call(h.sym("rewrite"), &[s, h.call(h.sym(mc_api::names::RULE), &[x, v])]))?;
                    let w60 = cx.eval(b.call("n", &[w, b.int(60)]))?;
                    match num(cx, b, w60)? {
                        Some(t) if t > 0.0 => {}
                        _ => return Ok(None),
                    }
                }
                let lo = Some((b.zero(), false));
                return finish(cx, b, y, c, d, lo, None);
            }
        }
    }
    Ok(None)
}

/// Does `t` contain a power of `x` with an exponent that is a half of an odd integer (the elementary form of a Struve
/// function of half-integer order)?
fn contains_half_power<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if b.sym_of(head) == Some(b.hd.pow) {
                if let [base, ex] = args {
                    if *base == x && matches!(h.view(*ex), View::Rational { .. }) {
                        return true;
                    }
                }
            }
            stack.extend(args.iter().copied());
        }
    }
    false
}
