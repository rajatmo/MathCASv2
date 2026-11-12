//! Derivative-free methods in one real variable: the secant method from two starts, and Brent's method in a
//! bracket with a sign change (inverse quadratic interpolation, secant and bisection steps).
use crate::func::NumFn;
use crate::newton::residual_ok;
use crate::options::Goals;
use crate::Outcome;
use mc_eval::{NativeCx, Res};

/// One value of a one-variable function: `None` when it is not a finite real.
fn value<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut dyn NumFn<'h, f64>, x: f64) -> Res<Option<f64>> {
    let mut out = [0.0];
    Ok(f.eval(cx, &[x], &mut out)?.then_some(out[0]))
}

/// The secant method from `x0` and `x1`, damped like Newton's method: a step that does not reduce |f| is halved
/// (at most 40 times) before the iteration gives up.
pub(crate) fn secant<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn NumFn<'h, f64>,
    x0: f64,
    x1: f64,
    goals: &Goals,
) -> Res<Outcome<f64>> {
    let Some(mut f0) = value(cx, f, x0)? else { return Ok(Outcome::NotNumeric(vec![x0])) };
    let Some(mut f1) = value(cx, f, x1)? else { return Ok(Outcome::NotNumeric(vec![x1])) };
    let (mut a, mut b) = (x0, x1);
    // the merit is the better of the two starts (on a tie the first, the caller's start)
    if f0.abs() <= f1.abs() {
        (a, b, f0, f1) = (b, a, f1, f0);
    }
    let scale = f0.abs().max(f1.abs());
    let (mut err, mut prev) = ((b - a).abs(), f64::INFINITY);
    let mut converged = false;
    let mut iterations = 0;
    'outer: while iterations < goals.max_iterations {
        iterations += 1;
        cx.tick(1)?;
        if f1 == 0.0 {
            (err, converged) = (0.0, true);
            break;
        }
        let slope = (f1 - f0) / (b - a);
        if slope == 0.0 || !slope.is_finite() {
            break;
        }
        let d = -f1 / slope;
        let mut t = 1.0;
        for _ in 0..40 {
            cx.tick(1)?;
            let xt = b + t * d;
            if let Some(ft) = value(cx, f, xt)? {
                if ft.abs() < f1.abs() || (t * d).abs() <= goals.tol(b.abs()) {
                    let step = (xt - b).abs();
                    (a, f0, b, f1) = (b, f1, xt, ft);
                    err = step;
                    if step <= 4.0 * f64::EPSILON * b.abs() || (step <= goals.tol(b.abs()) && step > 0.25 * prev) {
                        converged = true;
                        break 'outer;
                    }
                    prev = step;
                    continue 'outer;
                }
            }
            t *= 0.5;
        }
        // no step reduces |f|: a root at the noise level, or a stationary point
        converged = d.abs() <= goals.tol(b.abs());
        err = d.abs();
        break;
    }
    let ok = converged && err <= goals.tol(b.abs()) && residual_ok(f1.abs(), scale, goals);
    Ok(Outcome::Done { x: vec![b], err, converged: ok, iterations })
}

/// The values at the ends of a bracket, if both are numeric.
pub(crate) fn ends<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn NumFn<'h, f64>,
    a: f64,
    b: f64,
) -> Res<Result<(f64, f64), f64>> {
    let Some(fa) = value(cx, f, a)? else { return Ok(Err(a)) };
    let Some(fb) = value(cx, f, b)? else { return Ok(Err(b)) };
    Ok(Ok((fa, fb)))
}

/// Brent's method in [a, b] with the values fa, fb of opposite signs (or one of them zero), iterated to machine
/// precision (Brent's `zero`, 1973). A pole inside the bracket converges too, and is then refused by the residual
/// test.
pub(crate) fn brent<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn NumFn<'h, f64>,
    (mut a, mut b): (f64, f64),
    (mut fa, mut fb): (f64, f64),
    goals: &Goals,
) -> Res<Outcome<f64>> {
    let scale = fa.abs().max(fb.abs());
    let (mut c, mut fc) = (a, fa);
    let mut d = b - a;
    let mut e = d;
    let mut err = (b - a).abs();
    let mut converged = false;
    let mut iterations = 0;
    while iterations < goals.max_iterations {
        iterations += 1;
        cx.tick(1)?;
        if (fb > 0.0) == (fc > 0.0) && fb != 0.0 && fc != 0.0 {
            (c, fc) = (a, fa);
            d = b - a;
            e = d;
        }
        if fc.abs() < fb.abs() {
            (a, b, c) = (b, c, b);
            (fa, fb, fc) = (fb, fc, fb);
        }
        let tol = 2.0 * f64::EPSILON * b.abs() + f64::MIN_POSITIVE;
        let m = 0.5 * (c - b);
        err = m.abs();
        if m.abs() <= tol || fb == 0.0 {
            converged = true;
            if fb == 0.0 {
                err = 0.0;
            }
            break;
        }
        if e.abs() >= tol && fa.abs() > fb.abs() {
            let s = fb / fa;
            let (mut p, mut q);
            if a == c {
                p = 2.0 * m * s;
                q = 1.0 - s;
            } else {
                let (qq, r) = (fa / fc, fb / fc);
                p = s * (2.0 * m * qq * (qq - r) - (b - a) * (r - 1.0));
                q = (qq - 1.0) * (r - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            } else {
                p = -p;
            }
            if 2.0 * p < (3.0 * m * q - (tol * q).abs()).min((e * q).abs()) {
                e = d;
                d = p / q;
            } else {
                d = m;
                e = m;
            }
        } else {
            d = m;
            e = m;
        }
        // a bisection within the goals ends the search: interpolation failed, so the function is not smooth at
        // this scale (a kink, a jump, or noise from an inner solver) and further halvings only chase that; smooth
        // functions converge by interpolation steps to full precision as before
        if d == m && m.abs() <= goals.tol(b.abs()) {
            converged = true;
            break;
        }
        (a, fa) = (b, fb);
        b += if d.abs() > tol { d } else { tol.copysign(m) };
        match value(cx, f, b)? {
            Some(v) => fb = v,
            None => return Ok(Outcome::NotNumeric(vec![b])),
        }
    }
    // a sign change within a bracket narrower than the goals locates the root (or a jump of the function) to the
    // goals, so the residual test does not veto it
    let ok = converged && err <= goals.tol(b.abs()) && (residual_ok(fb.abs(), scale, goals) || fb.abs() <= scale);
    Ok(Outcome::Done { x: vec![b], err, converged: ok, iterations })
}
