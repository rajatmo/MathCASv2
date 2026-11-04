//! What the machine-precision methods share: the pieces of a range (finite, or infinite and mapped onto a finite
//! interval), the integrand as the methods see it, and their outcome.
use mc_eval::{NativeCx, Res};

/// How a piece of the range is parametrised by t (decision N2.5): the identity on a finite interval, or a map of
/// a finite t-interval onto an infinite one. `x'(t)` is signed, so a piece integrated from t0 to t1 is the
/// integral from x(t0) to x(t1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Map {
    /// x = t
    Id,
    /// x = a + t / (1 - t), t in [0, 1): [a, inf)
    Upper(f64),
    /// x = b - t / (1 - t), t in [0, 1): [b, -inf)
    Lower(f64),
    /// x = t / (1 - t^2), t in (-1, 1): the real line
    Whole,
}

impl Map {
    /// x(t) and x'(t); `None` at an end of the t-interval that maps to infinity (the integrand is taken to vanish
    /// there).
    pub fn apply(self, t: f64) -> Option<(f64, f64)> {
        match self {
            Map::Id => Some((t, 1.0)),
            Map::Upper(a) | Map::Lower(a) => {
                if t >= 1.0 {
                    return None;
                }
                let s = 1.0 - t;
                let (x, d) = (t / s, 1.0 / (s * s));
                Some(if matches!(self, Map::Upper(_)) { (a + x, d) } else { (a - x, -d) })
            }
            Map::Whole => {
                if t.abs() >= 1.0 {
                    return None;
                }
                let s = 1.0 - t * t;
                Some((t / s, (1.0 + t * t) / (s * s)))
            }
        }
    }
}

/// A piece of the range: the integral of f(x(t)) x'(t) from t0 to t1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Piece {
    /// the parametrisation
    pub map: Map,
    /// the start in t
    pub t0: f64,
    /// the end in t
    pub t1: f64,
}

/// An end of a piece: a finite number or an infinity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum End {
    /// a real number
    At(f64),
    /// +inf (`true`) or -inf
    Inf(bool),
}

/// The piece from `p` to `q`; `None` when both are the same infinity (an empty piece).
pub(crate) fn piece(p: End, q: End) -> Option<Piece> {
    let pc = |map, t0, t1| Some(Piece { map, t0, t1 });
    match (p, q) {
        (End::At(a), End::At(b)) => pc(Map::Id, a, b),
        (End::At(a), End::Inf(true)) => pc(Map::Upper(a), 0.0, 1.0),
        (End::At(a), End::Inf(false)) => pc(Map::Lower(a), 0.0, 1.0),
        (End::Inf(true), End::At(b)) => pc(Map::Upper(b), 1.0, 0.0),
        (End::Inf(false), End::At(b)) => pc(Map::Lower(b), 1.0, 0.0),
        (End::Inf(false), End::Inf(true)) => pc(Map::Whole, -1.0, 1.0),
        (End::Inf(true), End::Inf(false)) => pc(Map::Whole, 1.0, -1.0),
        (End::Inf(_), End::Inf(_)) => None,
    }
}

/// The integrand as the methods see it: values at points of the original variable.
pub(crate) trait Integrand<'h, S> {
    /// The values at `xs` into `out` and the error estimates of the values into `err` (zero for a function; the
    /// errors of the inner integrals of a multiple integral). `Err(point)`: a value is not numeric at this point
    /// (every variable's value, outer ones first).
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, xs: &[f64], out: &mut [S], err: &mut [f64]) -> Res<Result<(), Vec<f64>>>;
}

/// The result of a machine-precision method.
#[derive(Clone, Debug)]
pub(crate) enum Outcome<S> {
    /// The estimate, its error estimate, and whether the estimate meets the goals.
    Done {
        /// the integral
        value: S,
        /// its error estimate
        err: f64,
        /// the goals were met
        converged: bool,
    },
    /// A value at this point is not numeric.
    NotNumeric(Vec<f64>),
}

impl<S> Outcome<S> {
    /// The error estimate (infinite for a point that is not numeric).
    pub fn err(&self) -> f64 {
        match self {
            Outcome::Done { err, .. } => *err,
            Outcome::NotNumeric(_) => f64::INFINITY,
        }
    }

    /// Converged.
    pub fn converged(&self) -> bool {
        matches!(self, Outcome::Done { converged: true, .. })
    }
}

/// The roundoff floor of the adaptive methods: the error goal is never below this multiple of the machine epsilon
/// times the integral of |f| (decision N2.6). An integral that cancels to far below the integral of |f| cannot be
/// computed to more relative accuracy in machine arithmetic.
pub(crate) const ROUNDOFF: f64 = 200.0 * f64::EPSILON;
