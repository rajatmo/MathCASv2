//! What the integration loop asks of a one-step method, and the pieces both machine methods share: the error
//! norm and the initial step size.
use crate::model::Model;
use crate::options::Tolerance;
use mc_eval::{NativeCx, Res};

/// The outcome of one attempted step.
pub(crate) enum Attempt {
    /// the step is accepted; the next step size, and whether the method found the problem stiff
    Accepted { h_next: f64, stiff: bool },
    /// the step is rejected; the size to try instead
    Rejected { h_next: f64 },
    /// the right-hand side is not a finite real at this point
    NotNumeric(f64, Vec<f64>),
}

/// A one-step method with step size control and a continuous extension in the form of `crate::dense`.
pub(crate) trait Stepper<'h> {
    /// Prepare a step from `(t, y)`: evaluate what the method needs there; `false` when `F(t, y)` is not numeric.
    fn start(&mut self, cx: &mut NativeCx<'_, '_, 'h>, m: &mut Model<'h>, t: f64, y: &[f64]) -> Res<bool>;

    /// Try a step of size `h` from `(t, y)` (after [`Stepper::start`] at that point).
    fn attempt(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        m: &mut Model<'h>,
        t: f64,
        y: &[f64],
        h: f64,
        tol: &Tolerance,
    ) -> Res<Attempt>;

    /// After an accepted attempt: the new state into `y` and the step's dense coefficients appended to `dense`
    /// (`dense::COEFS * n` values); the method is then started at the new point.
    fn accept(&mut self, y: &mut [f64], h: f64, dense: &mut Vec<f64>);

    /// `F(t, y)` at the point of the last [`Stepper::start`].
    fn f0(&self) -> &[f64];

    /// The order of the method's error estimate (for the initial step size).
    fn order(&self) -> i32;
}

/// The root-mean-square of the error `e` relative to the tolerance at the values `a` and `b` (Hairer's norm).
pub(crate) fn error_norm(e: &[f64], a: &[f64], b: &[f64], tol: &Tolerance) -> f64 {
    let n = e.len().max(1);
    let s: f64 = e
        .iter()
        .zip(a.iter().zip(b))
        .map(|(&ei, (&ai, &bi))| {
            let sk = tol.abs + tol.rel * ai.abs().max(bi.abs());
            (ei / sk).powi(2)
        })
        .sum();
    (s / n as f64).sqrt()
}

/// The size of the first step (Hairer–Nørsett–Wanner's `hinit`): an explicit Euler step estimates the second
/// derivative, and the step makes the local error of a method of order `order` about the tolerance. Signed like
/// `dir`; at most `hmax`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn initial_step<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    m: &mut Model<'h>,
    t: f64,
    y: &[f64],
    f0: &[f64],
    order: i32,
    dir: f64,
    hmax: f64,
    tol: &Tolerance,
) -> Res<f64> {
    let sk: Vec<f64> = y.iter().map(|v| tol.abs + tol.rel * v.abs()).collect();
    let dnf: f64 = f0.iter().zip(&sk).map(|(f, s)| (f / s).powi(2)).sum();
    let dny: f64 = y.iter().zip(&sk).map(|(v, s)| (v / s).powi(2)).sum();
    let mut h = if dnf <= 1e-10 || dny <= 1e-10 { 1e-6 } else { (dny / dnf).sqrt() * 0.01 };
    h = h.min(hmax);
    let y1: Vec<f64> = y.iter().zip(f0).map(|(v, f)| v + dir * h * f).collect();
    let mut f1 = vec![0.0; y.len()];
    if !m.f(cx, t + dir * h, &y1, &mut f1)? {
        // the Euler step left the domain: start small and let the step control grow it
        return Ok(dir * (h * 1e-3).max(1e-12 * t.abs().max(1.0)));
    }
    let der2 = (f1.iter().zip(f0).zip(&sk).map(|((a, b), s)| ((a - b) / s).powi(2)).sum::<f64>()).sqrt() / h;
    let der12 = der2.abs().max(dnf.sqrt());
    let h1 = if der12 <= 1e-15 { (h * 1e-3).max(1e-6) } else { (0.01 / der12).powf(1.0 / f64::from(order)) };
    Ok(dir * (100.0 * h).min(h1).min(hmax))
}
