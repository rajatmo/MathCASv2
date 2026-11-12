//! MathCAS v2 probability distributions and data statistics (item 3D).
//!
//! - **Distributions** are inert terms: `normal_distribution(mu, sigma)`, `log_normal_distribution(mu, sigma)`,
//!   `uniform_distribution([a, b])`, `exponential_distribution(l)`, `gamma_distribution(k, theta)`,
//!   `beta_distribution(a, b)`, `chi_square_distribution(k)`, `student_t_distribution(n)`,
//!   `binomial_distribution(n, p)`, `poisson_distribution(m)`, `geometric_distribution(p)` ([`family`]).
//! - **Functions of a distribution**: `pdf`, `cdf`, `survival`, `quantile`, `mean`, `variance`, `std_dev`,
//!   `skewness`, `kurtosis`, `moment`, `central_moment`, `median`, `expectation`, `random_variate`. Exact and symbolic arguments
//!   give exact and symbolic results ([`symbolic`]); machine arguments the machine algorithms of [`machine`];
//!   balls the exact forms evaluated in balls, and quantiles without closed forms by Newton steps in balls.
//! - **Data**: `mean`, `variance`, `std_dev`, `skewness`, `kurtosis`, `moment`, `central_moment`, `median`, `quantile` of a data
//!   list, `histogram_list`, and `estimated_distribution` (maximum likelihood or moments) ([`data`], [`fit`]).
//!   The module `stats` (`lib/stats.mc`) adds `covariance` and `correlation`.
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! let v = e.eval("variance(binomial_distribution(10, 1/4))", &stop).unwrap().printed;
//! assert_eq!(v, "15/8");
//! ```
//!
//! Decisions of this crate are numbered DI.1, DI.2, ...:
//!
//! - **DI.1** A distribution is the call of its family head; the family's native checks numeric parameters
//!   (`failure(argument, normal_distribution, message)` for sigma <= 0 and the like), takes symbolic ones as
//!   valid, and rewrites `normal_distribution()` and `uniform_distribution()` to their standard members.
//! - **DI.2** The machine erf family and the incomplete gamma and beta functions are private to this crate
//!   ([`special`]) until item 3C's versions in `mc-special` are merged.
//! - **DI.3** Exact distribution functions are written with `erfc`, `gamma(a, x)` (upper incomplete),
//!   `beta(z, a, b)` (incomplete, not regularized) and `inverse_erfc`, the names of item 3C; elementary cases
//!   (integer shapes, small t degrees of freedom, discrete sums) avoid them.
//! - **DI.4** A machine real among the arguments selects the machine path (closed numeric expressions are made
//!   numeric with `n`); exact and symbolic arguments the exact forms.
//! - **DI.5** Quantiles at ball probabilities without a closed form are Newton iterations with the precision
//!   doubling; the radius is the size of the last step (an estimate, not a proven bound).
//! - **DI.6** `random_variate` draws from the session generator of `mc-random` as an effect, so a seed reproduces
//!   the values at any thread count; the algorithms are exact for their distributions ([`sample`]).
//! - **DI.7** The data functions share their names with the distribution functions (one native per name, which
//!   tells a list from a distribution); data lists with a machine real use machine arithmetic, others formulas.
//! - **DI.8** `expectation(f, d)` takes a function (the syntax has no `x ~ d`), `expectation(e, x, d)` an
//!   expression in x.
//! - **DI.9** `estimated_distribution` estimates the parameters that are not numbers, by maximum likelihood unless
//!   `method: moments`.
//! - **DI.10** Kurtosis is E[(X - mu)^4]/sigma^4 (3 for a normal distribution), not the excess; geometric
//!   distributions count the failures before the first success (support 0, 1, ...); the gamma distribution takes
//!   shape and scale.
//! - **DI.11** A moment that does not exist is `failure(undefined, f, d)`; an infinite one is `inf`. Symbolic
//!   parameters are assumed to make the moment exist.
#![warn(missing_docs)]
pub mod build;
pub mod data;
mod expect;
pub mod family;
mod findfit;
mod fit;
pub mod machine;
mod natives;
pub mod sample;
pub mod special;
pub mod symbolic;

use family::Family;
use mc_api::Store;
use mc_core::Heap;
use mc_eval::{Attributes, Kernel};
use mc_random::Rng;

/// Every function of this crate with a one-line description: the family constructors, then the functions.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (Family::Normal.name(), Family::Normal.description()),
    (Family::LogNormal.name(), Family::LogNormal.description()),
    (Family::Uniform.name(), Family::Uniform.description()),
    (Family::Exponential.name(), Family::Exponential.description()),
    (Family::Gamma.name(), Family::Gamma.description()),
    (Family::Beta.name(), Family::Beta.description()),
    (Family::ChiSquare.name(), Family::ChiSquare.description()),
    (Family::StudentT.name(), Family::StudentT.description()),
    (Family::Binomial.name(), Family::Binomial.description()),
    (Family::Poisson.name(), Family::Poisson.description()),
    (Family::Geometric.name(), Family::Geometric.description()),
    (natives::TABLE[0].0, natives::TABLE[0].2),
    (natives::TABLE[1].0, natives::TABLE[1].2),
    (natives::TABLE[2].0, natives::TABLE[2].2),
    (natives::TABLE[3].0, natives::TABLE[3].2),
    (natives::TABLE[4].0, natives::TABLE[4].2),
    (natives::TABLE[5].0, natives::TABLE[5].2),
    (natives::TABLE[6].0, natives::TABLE[6].2),
    (natives::TABLE[7].0, natives::TABLE[7].2),
    (natives::TABLE[8].0, natives::TABLE[8].2),
    (natives::TABLE[9].0, natives::TABLE[9].2),
    (natives::TABLE[10].0, natives::TABLE[10].2),
    (natives::TABLE[11].0, natives::TABLE[11].2),
    (natives::TABLE[12].0, natives::TABLE[12].2),
    (natives::TABLE[13].0, natives::TABLE[13].2),
    (natives::TABLE[14].0, natives::TABLE[14].2),
    (natives::TABLE[15].0, natives::TABLE[15].2),
    ("find_fit", "find_fit(data, model, params, x): the parameter values minimizing the squared residuals of model (Levenberg-Marquardt)"),
];

/// Register the natives of [`FUNCTIONS`] with `k`. `rng` is the session generator (`mc_random::install`'s
/// result), which `random_variate` draws from.
pub fn install(k: &mut Kernel, h: &Heap, rng: Rng) {
    for fam in Family::ALL {
        k.register(h.sym(fam.name()), Attributes::default(), Box::new(natives::Constructor { fam }));
    }
    for &(name, f, _) in natives::TABLE {
        // a draw is an effect (decision DI.6)
        let a = Attributes { effect: f == natives::Fun::RandomVariate, ..Attributes::default() };
        k.register(h.sym(name), a, Box::new(natives::DistNative { f, name, rng: rng.clone() }));
    }
    mc_numeric::formula::install(k, h, findfit::DEFS);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_table_lists_every_function() {
        let listed: Vec<&str> = super::FUNCTIONS.iter().map(|e| e.0).collect();
        let mut want: Vec<&str> = super::Family::ALL.iter().map(|f| f.name()).collect();
        want.extend(super::natives::TABLE.iter().map(|e| e.0));
        want.extend(super::findfit::DEFS.iter().map(|d| d.name));
        assert_eq!(listed, want);
    }
}
