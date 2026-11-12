//! Real root isolation of integer polynomials in one variable (decision 3B.7), and of polynomials with
//! algebraic-number coefficients with exact signs at algebraic points (item F2).
//!
//! The integer isolation (Descartes' rule with bisection) lives in `mc_poly::descartes` since item F2, so that
//! the algebraic-number layer below this crate can use it; this module keeps the names the solvers here use and
//! re-exports the algebraic real roots of `mc_algebra::realroot` (isolation over number fields with validated
//! ball intervals first and exact arithmetic as the fallback, real algebraic numbers with exact comparison and
//! signs, sample points between roots) for the phases above.
pub(crate) use mc_poly::descartes::{halve, isolate, refine, sign_at, Interval};
pub use mc_algebra::countroots::{count_rectangle, real_alg_of};
pub use mc_algebra::realroot::{
    isolate_balls, rational_real_roots, real_roots, sample_points, sign_at_algebraic, simplest_between, RealAlg, Sample,
};
