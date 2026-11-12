//! Local minimisation of a real function: Brent's method in one variable (after bracketing a minimum from the
//! start), BFGS with the symbolic gradient (forward differences without one) in several. Where a symbolic Hessian
//! exists, the minimiser's point is polished by Newton's method on the gradient ([`crate::newton`]), which finds the
//! location to machine precision instead of the half precision function values allow (decision R.12).
use crate::func::NumFn;
use crate::options::Goals;
use crate::scalar::norm;
use crate::Outcome;
use mc_eval::{NativeCx, Res};

/// The golden ratio, for expanding a bracket.
const GOLD: f64 = std::f64::consts::GOLDEN_RATIO;
/// 1 - 1/golden ratio, the golden-section fraction.
const CGOLD: f64 = 2.0 - std::f64::consts::GOLDEN_RATIO;
/// Expansions of a bracket before the function counts as unbounded below.
const MAX_EXPANSIONS: usize = 80;

/// The value of a scalar function at a point, `None` when not a finite real.
fn value<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut dyn NumFn<'h, f64>, x: &[f64]) -> Res<Option<f64>> {
    let mut out = [0.0];
    Ok(f.eval(cx, x, &mut out)?.then_some(out[0]))
}

/// The location tolerance of a derivative-free search: the goals, but no tighter than function values can resolve
/// (about half the machine digits).
fn location_tol(goals: &Goals, x: f64) -> f64 {
    goals.tol(x).max(2.0 * f64::EPSILON.sqrt() * x.abs().max(f64::MIN_POSITIVE))
}

/// What the bracketing found.
enum Bracket {
    /// a < b < c (or c < b < a) with f(b) = fb below both ends
    Found(f64, f64, f64, f64),
    /// no bracket within the expansion limit (unbounded below, or monotone): the last point and expansion step
    Unbounded(f64, f64),
}

/// A bracket around a minimum, expanding from `x0` by the golden ratio. `Err(point)` when a value is not numeric.
fn bracket<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut dyn NumFn<'h, f64>, x0: f64) -> Res<Result<Bracket, f64>> {
    let h = 0.1 * x0.abs().max(1.0);
    let Some(fa) = value(cx, f, &[x0])? else { return Ok(Err(x0)) };
    let mut a = x0;
    let mut b = x0 + h;
    let mut fb = match value(cx, f, &[b])? {
        Some(v) => v,
        None => {
            b = x0 - h;
            match value(cx, f, &[b])? {
                Some(v) => v,
                None => return Ok(Err(b)),
            }
        }
    };
    if fb > fa {
        (a, b, fb) = (b, a, fa);
    }
    let mut c = b + GOLD * (b - a);
    for _ in 0..MAX_EXPANSIONS {
        cx.tick(1)?;
        // a point without a value (outside the domain) shortens the expansion
        let mut fc = None;
        for _ in 0..40 {
            fc = value(cx, f, &[c])?;
            if fc.is_some() {
                break;
            }
            c = b + 0.5 * (c - b);
        }
        let Some(fc) = fc else { return Ok(Err(c)) };
        if fc > fb {
            return Ok(Ok(Bracket::Found(a, b, c, fb)));
        }
        (a, b, fb) = (b, c, fc);
        c = b + GOLD * (b - a);
        if !c.is_finite() {
            break;
        }
    }
    Ok(Ok(Bracket::Unbounded(b, (b - a).abs())))
}

/// The minimum of a function of one variable from `x0`: a bracket, then Brent's method (golden section and
/// parabolic interpolation, Brent's `localmin`, 1973).
pub(crate) fn brent_min<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn NumFn<'h, f64>,
    x0: f64,
    goals: &Goals,
) -> Res<Outcome<f64>> {
    let (a0, mut x, c0, mut fx) = match bracket(cx, f, x0)? {
        Err(p) => return Ok(Outcome::NotNumeric(vec![p])),
        Ok(Bracket::Unbounded(x, step)) => {
            return Ok(Outcome::Done { x: vec![x], err: step, converged: false, iterations: MAX_EXPANSIONS })
        }
        Ok(Bracket::Found(a, b, c, fb)) => (a, b, c, fb),
    };
    let (mut a, mut b) = if a0 < c0 { (a0, c0) } else { (c0, a0) };
    let (mut v, mut w) = (x, x);
    let (mut fv, mut fw) = (fx, fx);
    let (mut d, mut e) = (0.0f64, 0.0f64);
    let mut iterations = 0;
    let mut converged = false;
    while iterations < goals.max_iterations {
        iterations += 1;
        cx.tick(1)?;
        let xm = 0.5 * (a + b);
        let tol1 = location_tol(goals, x) / 3.0;
        let tol2 = 2.0 * tol1;
        if (x - xm).abs() <= tol2 - 0.5 * (b - a) {
            converged = true;
            break;
        }
        let mut golden = true;
        if e.abs() > tol1 {
            let r = (x - w) * (fx - fv);
            let mut q = (x - v) * (fx - fw);
            let mut p = (x - v) * q - (x - w) * r;
            q = 2.0 * (q - r);
            if q > 0.0 {
                p = -p;
            } else {
                q = -q;
            }
            let etemp = e;
            e = d;
            if p.abs() < (0.5 * q * etemp).abs() && p > q * (a - x) && p < q * (b - x) {
                d = p / q;
                let u = x + d;
                if u - a < tol2 || b - u < tol2 {
                    d = tol1.copysign(xm - x);
                }
                golden = false;
            }
        }
        if golden {
            e = if x >= xm { a - x } else { b - x };
            d = CGOLD * e;
        }
        let u = if d.abs() >= tol1 { x + d } else { x + tol1.copysign(d) };
        let Some(fu) = value(cx, f, &[u])? else { return Ok(Outcome::NotNumeric(vec![u])) };
        if fu <= fx {
            if u >= x {
                a = x;
            } else {
                b = x;
            }
            (v, fv, w, fw, x, fx) = (w, fw, x, fx, u, fu);
        } else {
            if u < x {
                a = u;
            } else {
                b = u;
            }
            if fu <= fw || w == x {
                (v, fv, w, fw) = (w, fw, u, fu);
            } else if fu <= fv || v == x || v == w {
                (v, fv) = (u, fu);
            }
        }
    }
    let err = 0.5 * (b - a);
    Ok(Outcome::Done { x: vec![x], err, converged: converged || err <= location_tol(goals, x), iterations })
}

/// The minimum of a function of several variables from `x0` by BFGS (inverse-Hessian updates, Armijo
/// backtracking), with the gradient `grad` (n outputs).
pub(crate) fn bfgs<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn NumFn<'h, f64>,
    grad: &mut dyn NumFn<'h, f64>,
    x0: Vec<f64>,
    goals: &Goals,
) -> Res<Outcome<f64>> {
    let n = x0.len();
    let mut x = x0;
    let Some(mut fx) = value(cx, f, &x)? else { return Ok(Outcome::NotNumeric(x)) };
    let mut g = vec![0.0; n];
    if !grad.eval(cx, &x, &mut g)? {
        return Ok(Outcome::NotNumeric(x));
    }
    // the inverse Hessian approximation, row-major; scaled after the first step
    let mut hm = identity(n);
    let mut first = true;
    let (mut err, mut iterations, mut converged) = (f64::INFINITY, 0, false);
    // the gradient relative to the scales of x and f: invariant under scaling either (decision R.12)
    let gscaled = |g: &[f64], x: &[f64], fx: f64| norm(g) * norm(x).max(1.0) / fx.abs().max(1.0);
    let gtol = goals.abs.max(1e-12);
    while iterations < goals.max_iterations {
        iterations += 1;
        cx.tick(1)?;
        if gscaled(&g, &x, fx) <= gtol * 1e-3 {
            converged = true;
            err = err.min(norm(&g));
            break;
        }
        let mut p: Vec<f64> = (0..n).map(|i| -(0..n).map(|j| hm[i * n + j] * g[j]).sum::<f64>()).collect();
        let mut slope: f64 = p.iter().zip(&g).map(|(a, b)| a * b).sum();
        if slope >= 0.0 || !slope.is_finite() {
            hm = identity(n);
            p = g.iter().map(|v| -v).collect();
            slope = -g.iter().map(|v| v * v).sum::<f64>();
        }
        let mut t = 1.0;
        let mut accepted = None;
        for _ in 0..50 {
            cx.tick(1)?;
            let xt: Vec<f64> = x.iter().zip(&p).map(|(a, b)| a + t * b).collect();
            if xt.iter().all(|v| v.is_finite()) {
                if let Some(ft) = value(cx, f, &xt)? {
                    if ft <= fx + 1e-4 * t * slope {
                        accepted = Some((xt, ft));
                        break;
                    }
                }
            }
            t *= 0.5;
        }
        let Some((xt, ft)) = accepted else {
            // no decrease: the minimum at the noise level of f, if the gradient is small
            converged = gscaled(&g, &x, fx) <= gtol.sqrt();
            err = err.min(norm(&p));
            break;
        };
        let mut gt = vec![0.0; n];
        if !grad.eval(cx, &xt, &mut gt)? {
            return Ok(Outcome::NotNumeric(xt));
        }
        let s: Vec<f64> = xt.iter().zip(&x).map(|(a, b)| a - b).collect();
        let y: Vec<f64> = gt.iter().zip(&g).map(|(a, b)| a - b).collect();
        let sy: f64 = s.iter().zip(&y).map(|(a, b)| a * b).sum();
        if sy > 1e-12 * norm(&s) * norm(&y) && sy.is_finite() {
            if first {
                let yy: f64 = y.iter().map(|v| v * v).sum();
                let gamma = sy / yy;
                hm.iter_mut().for_each(|v| *v *= gamma);
                first = false;
            }
            bfgs_update(n, &mut hm, &s, &y, sy);
        }
        err = norm(&s);
        x = xt;
        fx = ft;
        g = gt;
        if err <= 4.0 * f64::EPSILON * norm(&x) {
            converged = gscaled(&g, &x, fx) <= gtol.sqrt();
            break;
        }
    }
    let xn = norm(&x);
    let ok = converged || (err <= location_tol(goals, xn) && gscaled(&g, &x, fx) <= gtol.sqrt());
    Ok(Outcome::Done { x, err, converged: ok, iterations })
}

fn identity(n: usize) -> Vec<f64> {
    let mut m = vec![0.0; n * n];
    for i in 0..n {
        m[i * n + i] = 1.0;
    }
    m
}

/// H := (I - rho s y^T) H (I - rho y s^T) + rho s s^T with rho = 1 / (y^T s).
fn bfgs_update(n: usize, h: &mut [f64], s: &[f64], y: &[f64], sy: f64) {
    let rho = 1.0 / sy;
    let hy: Vec<f64> = (0..n).map(|i| (0..n).map(|j| h[i * n + j] * y[j]).sum()).collect();
    let yhy: f64 = y.iter().zip(&hy).map(|(a, b)| a * b).sum();
    for i in 0..n {
        for j in 0..n {
            h[i * n + j] += -rho * (hy[i] * s[j] + s[i] * hy[j]) + (rho * rho * yhy + rho) * s[i] * s[j];
        }
    }
}
