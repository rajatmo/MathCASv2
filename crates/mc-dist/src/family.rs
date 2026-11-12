//! The distribution families: names, parameters, validity, and the reading of a distribution term.
//!
//! A distribution is an inert term of the session, `normal_distribution(mu, sigma)`, ..., with the parameters in
//! the order of the table below (`uniform_distribution([a, b])` keeps its interval as a list). Parameters may be
//! numbers of any kind or symbolic expressions; numeric parameters are checked when the term is evaluated
//! (decision DI.1), symbolic ones are taken as valid.
//!
//! | family | parameters | support |
//! |---|---|---|
//! | `normal_distribution(mu, sigma)` | sigma > 0 | reals |
//! | `log_normal_distribution(mu, sigma)` | sigma > 0 | x > 0 |
//! | `uniform_distribution([a, b])` | a < b | [a, b] |
//! | `exponential_distribution(l)` | rate l > 0 | x >= 0 |
//! | `gamma_distribution(k, theta)` | shape k > 0, scale theta > 0 | x > 0 |
//! | `beta_distribution(a, b)` | a, b > 0 | [0, 1] |
//! | `chi_square_distribution(k)` | k > 0 | x > 0 |
//! | `student_t_distribution(n)` | n > 0 | reals |
//! | `binomial_distribution(n, p)` | integer n >= 0, 0 <= p <= 1 | 0, 1, ..., n |
//! | `poisson_distribution(m)` | m > 0 | 0, 1, 2, ... |
//! | `geometric_distribution(p)` | 0 < p <= 1 | 0, 1, 2, ... (failures before the first success) |
use mc_api::{names, Store, View, T};
use mc_arith::number::compare;
use mc_arith::{Arith, Cmp3, Num, Real};
use mc_core::Heap;

/// A family of distributions.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Family {
    /// `normal_distribution(mu, sigma)`
    Normal,
    /// `log_normal_distribution(mu, sigma)`
    LogNormal,
    /// `uniform_distribution([a, b])`
    Uniform,
    /// `exponential_distribution(l)`
    Exponential,
    /// `gamma_distribution(k, theta)`
    Gamma,
    /// `beta_distribution(a, b)`
    Beta,
    /// `chi_square_distribution(k)`
    ChiSquare,
    /// `student_t_distribution(n)`
    StudentT,
    /// `binomial_distribution(n, p)`
    Binomial,
    /// `poisson_distribution(m)`
    Poisson,
    /// `geometric_distribution(p)`
    Geometric,
}

impl Family {
    /// Every family, in the order of [`crate::FUNCTIONS`].
    pub const ALL: [Family; 11] = [
        Family::Normal,
        Family::LogNormal,
        Family::Uniform,
        Family::Exponential,
        Family::Gamma,
        Family::Beta,
        Family::ChiSquare,
        Family::StudentT,
        Family::Binomial,
        Family::Poisson,
        Family::Geometric,
    ];

    /// The head of the family's terms.
    pub const fn name(self) -> &'static str {
        match self {
            Family::Normal => "normal_distribution",
            Family::LogNormal => "log_normal_distribution",
            Family::Uniform => "uniform_distribution",
            Family::Exponential => "exponential_distribution",
            Family::Gamma => "gamma_distribution",
            Family::Beta => "beta_distribution",
            Family::ChiSquare => "chi_square_distribution",
            Family::StudentT => "student_t_distribution",
            Family::Binomial => "binomial_distribution",
            Family::Poisson => "poisson_distribution",
            Family::Geometric => "geometric_distribution",
        }
    }

    /// The one-line description of [`crate::FUNCTIONS`].
    pub const fn description(self) -> &'static str {
        match self {
            Family::Normal => "normal_distribution(mu, sigma): the normal distribution with mean mu and standard deviation sigma > 0; normal_distribution() is normal_distribution(0, 1)",
            Family::LogNormal => "log_normal_distribution(mu, sigma): the distribution of exp(X) for X normal with mean mu and standard deviation sigma > 0",
            Family::Uniform => "uniform_distribution([a, b]): the continuous uniform distribution on [a, b], a < b; uniform_distribution() is uniform_distribution([0, 1])",
            Family::Exponential => "exponential_distribution(l): the exponential distribution with rate l > 0 (mean 1/l)",
            Family::Gamma => "gamma_distribution(k, theta): the gamma distribution with shape k > 0 and scale theta > 0 (mean k theta)",
            Family::Beta => "beta_distribution(a, b): the beta distribution on [0, 1] with shape parameters a > 0 and b > 0",
            Family::ChiSquare => "chi_square_distribution(k): the chi-square distribution with k > 0 degrees of freedom",
            Family::StudentT => "student_t_distribution(n): Student's t distribution with n > 0 degrees of freedom",
            Family::Binomial => "binomial_distribution(n, p): the number of successes in n independent trials of success probability p",
            Family::Poisson => "poisson_distribution(m): the Poisson distribution with mean m > 0",
            Family::Geometric => "geometric_distribution(p): the number of failures before the first success in trials of success probability p, 0 < p <= 1",
        }
    }

    /// The family of a head name.
    pub fn from_name(name: &str) -> Option<Family> {
        Family::ALL.into_iter().find(|f| f.name() == name)
    }

    /// Whether the support is a set of integers (the density is a probability mass function).
    pub const fn is_discrete(self) -> bool {
        matches!(self, Family::Binomial | Family::Poisson | Family::Geometric)
    }

    /// The number of parameters (the two ends of a uniform interval count as two).
    pub const fn arity(self) -> usize {
        match self {
            Family::Normal | Family::LogNormal | Family::Uniform | Family::Gamma | Family::Beta | Family::Binomial => 2,
            _ => 1,
        }
    }

    /// The parameters of the call without arguments, when the family has a standard member.
    pub const fn standard(self) -> Option<[i64; 2]> {
        match self {
            Family::Normal | Family::Uniform => Some([0, 1]),
            _ => None,
        }
    }

    /// Whether the parameters, numbers or symbolic, are valid: `Err(message)` when numbers violate the
    /// constraints of the family. Symbolic parameters pass (they are assumed to satisfy them).
    pub fn check(self, nums: &[Option<Num>]) -> Result<(), &'static str> {
        // every numeric parameter must be real
        let reals: Vec<Option<&Real>> = nums
            .iter()
            .map(|n| match n {
                Some(Num::Real(r)) => Ok(Some(r)),
                Some(Num::Complex(..)) => Err("the parameters must be real"),
                None => Ok(None),
            })
            .collect::<Result<_, _>>()?;
        let pos = |r: Option<&Real>| r.is_none_or(|r| decided(compare(r, &Real::int(0))) == Some(Cmp3::Greater));
        let nonneg = |r: Option<&Real>| r.is_none_or(|r| matches!(decided(compare(r, &Real::int(0))), Some(Cmp3::Greater | Cmp3::Equal)));
        let at_most_1 = |r: Option<&Real>| r.is_none_or(|r| matches!(decided(compare(r, &Real::int(1))), Some(Cmp3::Less | Cmp3::Equal)));
        let ok = |c: bool, m: &'static str| if c { Ok(()) } else { Err(m) };
        match self {
            Family::Normal | Family::LogNormal => ok(pos(reals[1]), "the standard deviation sigma must be positive"),
            Family::Uniform => ok(
                match (reals[0], reals[1]) {
                    (Some(a), Some(b)) => decided(compare(a, b)) == Some(Cmp3::Less),
                    _ => true,
                },
                "the interval [a, b] must have a < b",
            ),
            Family::Exponential => ok(pos(reals[0]), "the rate must be positive"),
            Family::Gamma => ok(pos(reals[0]) && pos(reals[1]), "the shape and the scale must be positive"),
            Family::Beta => ok(pos(reals[0]) && pos(reals[1]), "the shape parameters must be positive"),
            Family::ChiSquare | Family::StudentT => ok(pos(reals[0]), "the degrees of freedom must be positive"),
            Family::Binomial => {
                let n_ok = match reals[0] {
                    None => true,
                    Some(Real::Exact(q)) => q.is_int() && !q.num.neg,
                    Some(_) => false,
                };
                ok(n_ok, "the number of trials must be an exact integer n >= 0")?;
                ok(nonneg(reals[1]) && at_most_1(reals[1]), "the probability p must be in [0, 1]")
            }
            Family::Poisson => ok(pos(reals[0]), "the mean must be positive"),
            Family::Geometric => ok(pos(reals[0]) && at_most_1(reals[0]), "the probability p must be in (0, 1]"),
        }
    }
}

/// A comparison that counts: undecided (overlapping balls) and unordered (NaN) give `None`; an undecided
/// comparison of a ball parameter is accepted by the callers above only where `None` passes.
fn decided(c: Cmp3) -> Option<Cmp3> {
    match c {
        Cmp3::Undecided | Cmp3::Unordered => None,
        c => Some(c),
    }
}

/// A distribution read from its term: the family and the parameters (the interval of a uniform distribution
/// unpacked into two).
#[derive(Clone, Debug)]
pub struct Dist<'h> {
    /// the family
    pub fam: Family,
    /// the parameters, [`Family::arity`] of them
    pub p: Vec<T<'h>>,
}

impl<'h> Dist<'h> {
    /// The distribution a term denotes: a call of a family head with its full parameter list (the canonical form
    /// the family's native leaves after its checks).
    pub fn read(h: &'h Heap, t: T<'h>) -> Option<Dist<'h>> {
        let View::App { head, args } = h.view(t) else { return None };
        let View::Sym(s) = h.view(head) else { return None };
        let fam = Family::from_name(&h.sym_name(s))?;
        let p = match (fam, args) {
            (Family::Uniform, &[ab]) => match h.view(ab) {
                View::App { head, args: &[a, b] } if is_list(h, head) => vec![a, b],
                _ => return None,
            },
            (Family::Uniform, _) => return None,
            (_, args) if args.len() == fam.arity() => args.to_vec(),
            _ => return None,
        };
        Some(Dist { fam, p })
    }

    /// The term of the distribution.
    pub fn term(&self, h: &'h Heap) -> T<'h> {
        let head = h.sym(self.fam.name());
        match self.fam {
            Family::Uniform => h.call(head, &[h.call(h.sym(names::LIST), &self.p)]),
            _ => h.call(head, &self.p),
        }
    }

    /// The parameters as numbers, when all are real numbers.
    pub fn reals(&self, ar: &Arith<'h, Heap>) -> Option<Vec<Real>> {
        self.p
            .iter()
            .map(|&t| match ar.num(t) {
                Some(Num::Real(r)) => Some(r),
                _ => None,
            })
            .collect()
    }
}

fn is_list(h: &Heap, head: T<'_>) -> bool {
    matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::LIST)
}
