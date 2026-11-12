//! Trigonometric substitutions for one quadratic radical, the alternatives to Euler's (scored by size).
//!
//! With `Q = a (x + h)^2 + k` (`h = b/(2a)`, `k = c - b^2/(4a)`):
//!
//! - `a < 0 < k`: `x + h = s sin(th)`, `s = sqrt(-k/a)`, `sqrt(Q) = sqrt(k) cos(th)`;
//! - `a, k > 0`: `x + h = s tan(th)`, `s = sqrt(k/a)`, `sqrt(Q) = sqrt(k)/cos(th)`;
//! - `a > 0 > k`: `x + h = s/cos(th)`, `s = sqrt(-k/a)`, `sqrt(Q) = sqrt(-k) tan(th)`.
//!
//! The integrand becomes a rational function of `sin(th)` and `cos(th)` for the trigonometric rules; the answer is
//! expanded (`trig_expand`) and written back with `sin(th)`, `cos(th)` and `th` as functions of `x` (`asin`, `atan`,
//! `acos` forms; `tan(th/2) = sin(th)/(1 + cos(th))`). An answer in which `th` is left inside another function is
//! dropped.
use super::{change, Cands, Level, Radical};
use crate::elementary::{deriv, rewrite, sign, unary};
use crate::{apply, classic, is_free};
use mc_calculus::term::Build;
use mc_api::T;
use mc_eval::{NativeCx, Res};

/// The trigonometric functions written back.
const TRIG: [&str; 6] = ["sin", "cos", "tan", "cot", "sec", "csc"];

/// The candidates of the trigonometric substitutions (see the module).
pub fn run<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &mut Cands<'h>, rads: &[Radical<'h>], lv: Level) -> Res<()> {
    let (f, x) = (c.f, c.x);
    let Some(r0) = rads.first() else { return Ok(()) };
    if rads.iter().any(|r| r.base != r0.base || r.den != 2) {
        return Ok(());
    }
    let Some(cs) = mc_integrate::terms::coeffs_in(&b.ar, r0.base, x) else { return Ok(()) };
    let [qc, qb, qa] = cs.as_slice() else { return Ok(()) };
    let (qa, qb, qc) = (*qa, *qb, *qc);
    let h = b.div(qb, b.mul(&[b.int(2), qa]));
    let k = apply(cx, b, "together", &[b.sub(qc, b.div(b.powi(qb, 2), b.mul(&[b.int(4), qa])))])?;
    let (sa, sk) = (sign(cx, b, qa)?, sign(cx, b, k)?);
    let th = super::super::fresh(b, "th", lv.depth);
    let (sin, cos) = (b.call("sin", &[th]), b.call("cos", &[th]));
    let half = b.rat(1, 2);
    let y = b.pow(r0.base, half);
    let xh = b.add(&[x, h]);
    // (x(th), sqrt(Q)(th), sin(th)(x), cos(th)(x), th(x))
    let plan = match (sa, sk) {
        (Some(-1), Some(1)) => {
            let s = b.pow(b.neg(b.div(k, qa)), half);
            let rk = b.pow(k, half);
            Some((b.sub(b.mul(&[s, sin]), h), b.mul(&[rk, cos]), b.div(xh, s), b.div(y, rk), b.call("asin", &[b.div(xh, s)])))
        }
        (Some(1), Some(1)) => {
            let s = b.pow(b.div(k, qa), half);
            let rk = b.pow(k, half);
            let cx_ = b.div(rk, y);
            Some((b.sub(b.mul(&[s, b.div(sin, cos)]), h), b.div(rk, cos), b.mul(&[b.div(xh, s), cx_]), cx_, b.call("atan", &[b.div(xh, s)])))
        }
        (Some(1), Some(-1)) => {
            let s = b.pow(b.neg(b.div(k, qa)), half);
            let rk = b.pow(b.neg(k), half);
            let cx_ = b.div(s, xh);
            Some((b.sub(b.div(s, cos), h), b.mul(&[rk, b.div(sin, cos)]), b.mul(&[b.div(y, rk), cx_]), cx_, b.call("acos", &[cx_])))
        }
        _ => None,
    };
    let Some((xt, yt, sx, cx_x, thx)) = plan else { return Ok(()) };
    let Some(dx) = deriv(cx, b, xt, th)? else { return Ok(()) };
    let reps: Vec<(T<'h>, T<'h>)> = rads.iter().map(|r| (r.term, b.powi(yt, r.num))).collect();
    let g = b.mul(&[change(b, f, x, &reps, xt), dx]);
    if !is_free(b, g, x) || !super::radicals(b, g, th).is_empty() {
        return Ok(());
    }
    let g = apply(cx, b, "together", &[g])?;
    let Some(rt) = classic(cx, b, g, th, lv.depth + 1)? else { return Ok(()) };
    let rt = apply(cx, b, "trig_expand", &[rt])?;
    let half_th = b.mul(&[half, th]);
    let mut bad = false;
    let back = rewrite(b, rt, &mut |s| {
        if s == th {
            return Some(thx);
        }
        let (name, arg) = unary(b, s)?;
        if !TRIG.contains(&name.as_str()) {
            return None;
        }
        if arg == half_th {
            // tan(th/2) = sin(th)/(1 + cos(th)) (Weierstrass's substitution of the trigonometric rules)
            return match name.as_str() {
                "tan" => Some(b.div(sx, b.add(&[b.one(), cx_x]))),
                "cot" => Some(b.div(b.add(&[b.one(), cx_x]), sx)),
                _ => {
                    bad = true;
                    None
                }
            };
        }
        if arg != th {
            if !is_free(b, arg, th) {
                bad = true;
            }
            return None;
        }
        Some(match name.as_str() {
            "sin" => sx,
            "cos" => cx_x,
            "tan" => b.div(sx, cx_x),
            "cot" => b.div(cx_x, sx),
            "sec" => b.div(b.one(), cx_x),
            "csc" => b.div(b.one(), sx),
            _ => return None,
        })
    });
    if bad || !is_free(b, back, th) {
        return Ok(());
    }
    c.offer(cx, b, back)?;
    Ok(())
}
