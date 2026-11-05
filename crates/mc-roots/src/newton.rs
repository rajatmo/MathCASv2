//! Damped Newton's method for n equations in n unknowns, real or complex, with Broyden's method as the fallback
//! when no Jacobian can be evaluated; one variable is the case n = 1.
//!
//! Each iteration solves `J d = -f` (the regularised least-squares step where `J` is singular), then backtracks on
//! the merit |f|^2 from the full step, halving it until the Armijo condition holds (decision R.2). Without a
//! symbolic Jacobian, or where it is not numeric, the Jacobian is a forward-difference one (evaluated as a
//! speculative group) and is then updated by Broyden's rank-one formula; a failed line search on an updated
//! Jacobian recomputes it once. Optional bounds (one interval per variable, real scalars) project every trial point
//! into the box: Newton's method constrained to a bracket.
use crate::func::NumFn;
use crate::options::Goals;
use crate::scalar::{all_finite, norm, norm2, Scalar};
use crate::{linalg, Outcome};
use mc_eval::{NativeCx, Res};

/// Halvings of the line search before the direction is given up.
const MAX_HALVINGS: u32 = 40;

/// The Armijo constant: a step of length t must reduce |f|^2 by the factor (1 - 2 ALPHA t) at least.
const ALPHA: f64 = 1e-4;

/// How a residual is accepted as zero: within the accuracy goal (but no tighter than about half the machine
/// digits) relative to the larger of 1 and the residual at the start (decision R.3).
pub(crate) fn residual_ok(residual: f64, scale: f64, goals: &Goals) -> bool {
    residual <= goals.abs.max(f64::EPSILON.sqrt()) * scale.max(1.0)
}

/// What the line search gives for the Jacobian.
enum Jacobian<'f, 'h, S: Scalar> {
    /// the symbolic Jacobian
    Exact(&'f mut dyn NumFn<'h, S>),
    /// Broyden's approximation; `fresh` when it is a finite-difference Jacobian not updated since
    Broyden { fresh: bool },
}

/// Newton's method from `x0` on the residual `f` (n outputs) with the Jacobian `jac` (n·n outputs, row-major), or
/// Broyden's method when `jac` is `None`. `bounds` constrain real variables to intervals.
pub(crate) fn solve<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn NumFn<'h, S>,
    jac: Option<&mut dyn NumFn<'h, S>>,
    x0: Vec<S>,
    goals: &Goals,
    bounds: Option<&[(f64, f64)]>,
) -> Res<Outcome<S>> {
    let n = x0.len();
    let mut x = x0;
    project(&mut x, bounds);
    let mut fx = vec![S::real(0.0); n];
    if !f.eval(cx, &x, &mut fx)? {
        return Ok(Outcome::NotNumeric(x));
    }
    let scale = norm(&fx);
    let mut jac = match jac {
        Some(j) => Jacobian::Exact(j),
        None => Jacobian::Broyden { fresh: false },
    };
    let mut j = vec![S::real(0.0); n * n];
    // whether `j` holds a Broyden approximation to update
    let mut have_j = false;
    let (mut err, mut prev) = (f64::INFINITY, f64::INFINITY);
    let mut converged = false;
    let mut iterations = 0;
    while iterations < goals.max_iterations {
        iterations += 1;
        cx.tick(1)?;
        let phi = norm2(&fx);
        if phi == 0.0 {
            (err, converged) = (0.0, true);
            break;
        }
        // the Jacobian at x: symbolic, else a fresh finite-difference one (from then on Broyden)
        let need_differences = match &mut jac {
            Jacobian::Exact(jf) => !jf.eval(cx, &x, &mut j)?,
            Jacobian::Broyden { .. } => !have_j,
        };
        if need_differences {
            if !finite_differences(cx, f, &x, &fx, &mut j)? {
                break;
            }
            jac = Jacobian::Broyden { fresh: true };
            have_j = true;
        }
        let neg: Vec<S> = fx.iter().map(|&v| S::real(0.0) - v).collect();
        let Some(d) = linalg::solve(n, &j, &neg).or_else(|| linalg::damped_least_squares(n, &j, &fx)) else {
            break;
        };
        match line_search(cx, f, &x, &d, phi, bounds)? {
            Some((xt, ft)) => {
                let s: Vec<S> = xt.iter().zip(&x).map(|(&a, &b)| a - b).collect();
                if let Jacobian::Broyden { fresh } = &mut jac {
                    broyden_update(n, &mut j, &s, &fx, &ft);
                    *fresh = false;
                }
                let step = norm(&s);
                x = xt;
                fx = ft;
                err = step;
                let xn = norm(&x);
                if step <= 4.0 * f64::EPSILON * xn || step == 0.0 {
                    converged = true;
                    break;
                }
                // at the noise level the steps stop shrinking: within the goals, that is convergence
                if step <= goals.tol(xn) && step > 0.25 * prev {
                    converged = true;
                    break;
                }
                prev = step;
            }
            None => {
                // no decrease along the direction: an updated Broyden matrix is recomputed once; otherwise the
                // point is a root at the noise level (a direction within the goals) or a stationary point of |f|
                if let Jacobian::Broyden { fresh: false } = jac {
                    if !finite_differences(cx, f, &x, &fx, &mut j)? {
                        break;
                    }
                    jac = Jacobian::Broyden { fresh: true };
                    continue;
                }
                let dn = norm(&d);
                if dn <= goals.tol(norm(&x)) {
                    (err, converged) = (dn, true);
                } else if err.is_infinite() {
                    err = dn;
                }
                break;
            }
        }
    }
    let residual = norm(&fx);
    let ok = converged && err <= goals.tol(norm(&x)) && residual_ok(residual, scale, goals);
    Ok(Outcome::Done { x, err, converged: ok, iterations })
}

/// Backtracking from the full step `d`: the first point x + t d (t = 1, 1/2, ...) with a numeric residual and
/// |f|^2 <= (1 - 2 ALPHA t) phi; none once the step no longer moves x.
fn line_search<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn NumFn<'h, S>,
    x: &[S],
    d: &[S],
    phi: f64,
    bounds: Option<&[(f64, f64)]>,
) -> Res<Option<(Vec<S>, Vec<S>)>> {
    let mut ft = vec![S::real(0.0); x.len()];
    let mut t = 1.0;
    for _ in 0..MAX_HALVINGS {
        cx.tick(1)?;
        let mut xt: Vec<S> = x.iter().zip(d).map(|(&a, &b)| a + S::real(t) * b).collect();
        project(&mut xt, bounds);
        // a step too small to move x (at a root, at the noise level of f): halving it further cannot help
        if xt.iter().zip(x).all(|(&a, &b)| (a - b).abs() == 0.0) {
            break;
        }
        if all_finite(&xt) && f.eval(cx, &xt, &mut ft)? && norm2(&ft) <= (1.0 - 2.0 * ALPHA * t) * phi {
            return Ok(Some((xt, ft)));
        }
        t *= 0.5;
    }
    Ok(None)
}

/// The forward-difference Jacobian at `x` (with `fx = f(x)`), its columns evaluated as one batch. `false` when a
/// perturbed point is not numeric.
pub(crate) fn finite_differences<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &mut dyn NumFn<'h, S>,
    x: &[S],
    fx: &[S],
    j: &mut [S],
) -> Res<bool> {
    let n = x.len();
    let m = fx.len();
    let steps: Vec<f64> = x.iter().map(|v| f64::EPSILON.sqrt() * v.abs().max(1.0)).collect();
    let points: Vec<Vec<S>> = (0..n)
        .map(|c| {
            let mut p = x.to_vec();
            p[c] = p[c] + S::real(steps[c]);
            p
        })
        .collect();
    let mut cols = vec![vec![S::real(0.0); m]; n];
    if !f.eval_batch(cx, &points, &mut cols)? {
        return Ok(false);
    }
    for c in 0..n {
        for r in 0..m {
            j[r * n + c] = (cols[c][r] - fx[r]) / S::real(steps[c]);
        }
    }
    Ok(all_finite(j))
}

/// Broyden's update J += ((f1 - f0) - J s) s^H / (s^H s).
fn broyden_update<S: Scalar>(n: usize, j: &mut [S], s: &[S], f0: &[S], f1: &[S]) {
    let ss: f64 = s.iter().map(|v| v.abs() * v.abs()).sum();
    if ss == 0.0 || !ss.is_finite() {
        return;
    }
    for r in 0..n {
        let mut js = S::real(0.0);
        for c in 0..n {
            js = js + j[r * n + c] * s[c];
        }
        let u = (f1[r] - f0[r] - js) / S::real(ss);
        for c in 0..n {
            j[r * n + c] = j[r * n + c] + u * s[c].conj();
        }
    }
}

/// Clamp each real variable into its interval.
fn project<S: Scalar>(x: &mut [S], bounds: Option<&[(f64, f64)]>) {
    if let Some(b) = bounds {
        for (v, &(lo, hi)) in x.iter_mut().zip(b) {
            *v = v.clamp_real(lo, hi);
        }
    }
}
