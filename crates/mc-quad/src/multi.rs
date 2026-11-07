//! The multidimensional methods behind `n_integrate` (plan 10, sections 2.2, 2.3 and 2.5): the choice of method,
//! the goals of each, the integrand in blocks, and the results and messages.
//!
//! | method | default goal (digits) | result |
//! |---|---|---|
//! | `cubature` (default for 2 to 7 variables) | precision 8 (6 from 4 variables) | a number |
//! | `lattice` (default above 7 variables, smooth integrands) | precision 4 | `around(value, error)` |
//! | `sobol` | precision 4 | `around(value, error)` |
//! | `vegas` (default above 7 variables otherwise) | precision 3 | `around(value, error)` |
//! | `monte_carlo` | precision 3 | `around(value, error)` |
//!
//! `accuracy_goal` defaults to `inf` (none) for every method. `max_points` caps the evaluations (default
//! [`MAX_POINTS`]). A result that misses its goal prints a message saying how far it got (and, for cubature, where
//! the largest error is left; for VEGAS, whether the iterations agree).
use crate::domain::{Domain, Sampler};
use crate::nested::Bound;
use crate::options::{Goals, Method, Options, MACHINE_DIGITS};
use crate::{cubature, mc, qmc};
use mc_core::{Heap, Store, Sym, T};
use mc_eval::{NativeCx, Res};
use mc_numfn::{NumFn, Policy, Var};

/// The default cap on evaluations of the multidimensional methods.
pub(crate) const MAX_POINTS: u64 = 10_000_000;

/// The most variables of a cubature (the rule has 2^d points).
pub(crate) const MAX_CUBATURE_DIMS: usize = 15;

/// Deterministic estimates that miss the goal but whose error is below this fraction of the value are returned
/// with a message; worse ones are `not_converged` failures (divergent integrals give large relative errors).
const USABLE: f64 = 1e-3;

/// The estimate of a multidimensional method.
#[derive(Clone, Debug)]
pub(crate) struct Estimate {
    /// the integral
    pub value: f64,
    /// its error estimate (a standard error for the statistical methods)
    pub err: f64,
    /// the goals were met
    pub converged: bool,
    /// evaluations of the integrand
    pub calls: u64,
    /// cubature: the variables' values at the centre of the region with the largest error, and that error
    pub worst: Option<(Vec<f64>, f64)>,
    /// VEGAS: χ² per degree of freedom of the averaged iterations, and the degrees of freedom
    pub chi2: Option<(f64, usize)>,
}

/// A multidimensional method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Genz–Malik adaptive cubature
    Cubature,
    /// VEGAS+
    Vegas,
    /// randomised lattice rule
    Lattice,
    /// scrambled Sobol points
    Sobol,
    /// plain Monte Carlo
    MonteCarlo,
}

impl Kind {
    /// The method of `method:`, if it is one of these.
    pub fn of(m: Method) -> Option<Kind> {
        Some(match m {
            Method::Cubature => Kind::Cubature,
            Method::Vegas => Kind::Vegas,
            Method::Lattice => Kind::Lattice,
            Method::Sobol => Kind::Sobol,
            Method::MonteCarlo => Kind::MonteCarlo,
            Method::GaussKronrod | Method::DoubleExponential | Method::Nested => return None,
        })
    }

    /// Does the method give a statistical error (an `around`)?
    pub fn statistical(self) -> bool {
        self != Kind::Cubature
    }

    /// The default precision goal in digits for `d` variables (decision quad-1.4): cubature as one-dimensional
    /// integration (half the machine digits) up to 3 variables, 6 digits from 4 (|I7 - I5| overstates the error
    /// of smooth integrands by orders of magnitude, and in 4 variables the 8-digit goal took the whole default
    /// budget of 10^7 points for errors actually near 10^-11).
    fn precision_goal(self, d: usize) -> f64 {
        match self {
            Kind::Cubature if d <= 3 => MACHINE_DIGITS / 2.0,
            Kind::Cubature => 6.0,
            Kind::Lattice | Kind::Sobol => 4.0,
            Kind::Vegas | Kind::MonteCarlo => 3.0,
        }
    }

    /// The most variables.
    pub fn max_dims(self) -> usize {
        match self {
            Kind::Cubature => MAX_CUBATURE_DIMS,
            Kind::Lattice | Kind::Sobol => qmc::MAX_DIMS,
            Kind::Vegas | Kind::MonteCarlo => mc::MAX_DIMS,
        }
    }

    /// The name of `method:`.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Cubature => "cubature",
            Kind::Vegas => "vegas",
            Kind::Lattice => "lattice",
            Kind::Sobol => "sobol",
            Kind::MonteCarlo => "monte_carlo",
        }
    }
}

/// The goals of a method: the options' goals, or the method's defaults.
pub(crate) fn goals(kind: Kind, o: &Options, d: usize) -> Goals {
    let pg = o.precision_goal.unwrap_or(kind.precision_goal(d));
    let pow10 = |g: f64| if g.is_infinite() { 0.0 } else { 10f64.powf(-g) };
    Goals { abs: pow10(o.accuracy_goal.unwrap_or(f64::INFINITY)), rel: pow10(pg) }
}

/// The method chosen automatically for `dims` variables (decision quad-1.2): one variable is not handled here;
/// 2 to 7 adaptive cubature; above, the lattice rule for integrands built from analytic functions only, VEGAS for
/// the others (piecewise, `abs`, user functions, ...). When the automatic lattice rule misses its goal, VEGAS
/// runs too and the smaller error estimate is kept; when the automatic cubature misses it in 2 or 3 variables,
/// the nested rules do.
pub(crate) fn automatic<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, dims: usize) -> Res<Kind> {
    if dims <= 7 {
        return Ok(Kind::Cubature);
    }
    Ok(if crate::rigorous::analytic(cx, f)? { Kind::Lattice } else { Kind::Vegas })
}

/// The integrand of all variables, evaluated in blocks: compiled code split across the session's threads, with
/// the interpreter for what does not compile (`compile: false`: the interpreter only).
pub(crate) fn integrand<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    vars: &[Sym],
    policy: Policy,
) -> Res<Box<dyn NumFn<'h, f64> + 'h>> {
    let all: Vec<Var> = vars.iter().map(|&s| Var::scalar(s)).collect();
    // the multidimensional methods evaluate many points: compile at once (the cache may already hold the code)
    let policy = match policy {
        Policy::Interpreted => Policy::Interpreted,
        _ => Policy::Compiled,
    };
    let opts = mc_numfn::Options { policy, ..mc_numfn::Options::default() };
    Ok(Box::new(mc_numfn::par_expr(cx, f, &all, &opts)?))
}

/// Run `kind` on the integrand `f` over `ranges`. `Err(point)`: the integrand or an end is not numeric there.
pub(crate) fn run<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: Box<dyn NumFn<'h, f64> + 'h>,
    ranges: Vec<Vec<Bound<'h>>>,
    kind: Kind,
    o: &Options,
) -> Res<Result<Estimate, Vec<f64>>> {
    let mut s = Sampler { f, dom: Domain { ranges }, calls: 0 };
    let g = goals(kind, o, s.dim());
    let max = o.max_points.unwrap_or(MAX_POINTS);
    let seed = o.seed.unwrap_or(0);
    match kind {
        Kind::Cubature => cubature::integrate(cx, &mut s, g, max),
        Kind::Lattice => qmc::integrate(cx, &mut s, qmc::Kind::Lattice, g, max, o.points, seed),
        Kind::Sobol => qmc::integrate(cx, &mut s, qmc::Kind::Sobol, g, max, o.points, seed),
        Kind::MonteCarlo => mc::plain(cx, &mut s, g, max, o.points, seed),
        Kind::Vegas => {
            // enough points per iteration for the training and three averaged iterations within the budget
            let rounds = (mc::WARMUP + 3) as u64;
            let points = o.points.unwrap_or(mc::VEGAS_POINTS.min((max / rounds).max(64)));
            let plan = mc::VegasPlan { points, iterations: o.iterations, max_points: max, seed };
            mc::vegas(cx, &mut s, g, &plan)
        }
    }
}

/// `around(value, error)`.
pub(crate) fn around<'h>(h: &'h Heap, value: f64, err: f64) -> T<'h> {
    h.call(h.sym("around"), &[h.real(value), h.real(err)])
}

/// A short form of a machine real for messages.
fn short(v: f64) -> String {
    format!("{v:.3e}")
}

/// The messages for an estimate (none when the goals were met and the iterations agree).
pub(crate) fn messages(h: &Heap, est: &Estimate, kind: Kind, vars: &[Sym]) -> Vec<String> {
    let mut out = vec![];
    let rel = if est.value != 0.0 { est.err / est.value.abs() } else { f64::INFINITY };
    if !est.converged {
        let mut m = format!(
            "{}: the goals were not met after {} evaluations; the error estimate is {} (relative {})",
            kind.name(),
            est.calls,
            short(est.err),
            short(rel)
        );
        if let Some((x, e)) = &est.worst {
            let at: Vec<String> =
                vars.iter().zip(x).map(|(&s, v)| format!("{} = {}", h.sym_name(s), short(*v))).collect();
            m.push_str(&format!(", the largest part ({}) in the region around {}", short(*e), at.join(", ")));
        }
        m.push_str("; raise max_points: or lower precision_goal:");
        out.push(m);
    }
    if let Some((c, dof)) = est.chi2 {
        if dof >= 2 && c > 1.0 + 3.0 * (2.0 / dof as f64).sqrt() {
            out.push(format!(
                "vegas: the iterations disagree (chi^2 per degree of freedom {:.2} over {} iterations); the error estimate is unreliable",
                c,
                dof + 1
            ));
        }
    }
    out
}

/// Is the estimate fit to be returned (with a message when the goals were not met)?
pub(crate) fn usable(est: &Estimate, kind: Kind) -> bool {
    if est.converged || kind.statistical() {
        return est.value.is_finite() && est.err.is_finite();
    }
    est.err.is_finite() && est.value.is_finite() && est.err <= USABLE * est.value.abs()
}
