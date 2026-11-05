//! Machine-real densities, distribution functions and quantiles.
//!
//! [`Mach`] is a distribution with machine parameters. Its distribution function returns both tails,
//! (P(X <= x), P(X > x)), each computed directly where it is small (`erfc`, the continued fractions of the
//! incomplete functions), so survival probabilities far in the upper tail keep their relative accuracy. Quantiles
//! are closed forms where the family has one (normal, log-normal, uniform, exponential, Student t with n = 1, 2),
//! else the inversion of the smaller tail by safeguarded Newton steps inside a bracket that shrinks with every
//! evaluation (continuous), or the least k with P(X <= k) >= q by a galloping search (discrete).
use crate::family::Family;
use crate::special::{self as sp, beta_i, gamma_pq, norm_quantile};
use mc_special::dd::DD;

/// A distribution with machine-real parameters (already checked valid).
#[derive(Clone, Copy, Debug)]
pub struct Mach {
    /// the family
    pub fam: Family,
    /// the parameters ([`Family::arity`] of them; the second is unused for one-parameter families)
    pub p: [f64; 2],
}

/// A distribution function given as both tails.
type Tails = dyn Fn(f64) -> (f64, f64);
/// A density.
type Density = dyn Fn(f64) -> f64;

/// The most iterations of the inversions (bisection alone needs at most about 2100 on the whole double range).
const MAX_STEPS: usize = 3000;

impl Mach {
    /// The distribution with these parameters, when they are finite and valid for the family.
    pub fn new(fam: Family, p: &[f64]) -> Option<Mach> {
        let a = *p.first()?;
        let b = p.get(1).copied().unwrap_or(0.0);
        if !a.is_finite() || !b.is_finite() {
            return None;
        }
        let ok = match fam {
            Family::Normal | Family::LogNormal => b > 0.0,
            Family::Uniform => a < b,
            Family::Exponential | Family::ChiSquare | Family::StudentT | Family::Poisson => a > 0.0,
            Family::Gamma | Family::Beta => a > 0.0 && b > 0.0,
            Family::Binomial => a >= 0.0 && a == a.floor() && (0.0..=1.0).contains(&b),
            Family::Geometric => a > 0.0 && a <= 1.0,
        };
        ok.then_some(Mach { fam, p: [a, b] })
    }

    /// The density at x (for a discrete family the probability of x, 0 off the integers).
    pub fn pdf(&self, x: f64) -> f64 {
        let [a, b] = self.p;
        if x.is_nan() {
            return x;
        }
        match self.fam {
            Family::Normal => sp::norm_pdf((x - a) / b) / b,
            Family::LogNormal => {
                if x <= 0.0 || x == f64::INFINITY {
                    return 0.0;
                }
                sp::norm_pdf((x.ln() - a) / b) / (b * x)
            }
            Family::Uniform => {
                if a <= x && x <= b {
                    1.0 / (b - a)
                } else {
                    0.0
                }
            }
            Family::Exponential => {
                if x < 0.0 {
                    0.0
                } else {
                    a * (-a * x).exp()
                }
            }
            Family::Gamma => gamma_density(a, x / b) / b,
            Family::ChiSquare => gamma_density(a / 2.0, x / 2.0) / 2.0,
            Family::Beta => {
                if !(0.0..=1.0).contains(&x) {
                    return 0.0;
                }
                if x == 0.0 || x == 1.0 {
                    let e = if x == 0.0 { a } else { b };
                    return if e < 1.0 {
                        f64::INFINITY
                    } else if e > 1.0 {
                        0.0
                    } else {
                        // a = 1 at 0 (b = 1 at 1): the density there is 1/B(1, b) = b
                        if x == 0.0 {
                            b
                        } else {
                            a
                        }
                    };
                }
                let v = sp::ln(x).mul_f(a - 1.0) + sp::ln_1p(-x).mul_f(b - 1.0) - sp::lbeta(a, b);
                sp::exp_of(v)
            }
            Family::StudentT => {
                let n = a;
                // Gamma((n+1)/2) / (sqrt(n pi) Gamma(n/2)) (1 + x^2/n)^(-(n+1)/2), the logarithm in double-double
                let v = sp::lgamma((n + 1.0) / 2.0) - sp::lgamma(n / 2.0)
                    - (sp::ln(n) + mc_special::dd::LN_PI).mul_f(0.5)
                    - sp::ln_1p(x * x / n).mul_f((n + 1.0) / 2.0);
                sp::exp_of(v)
            }
            Family::Binomial | Family::Poisson | Family::Geometric => match as_count(x) {
                Some(k) => self.pmf(k),
                None => 0.0,
            },
        }
    }

    /// The probability of the integer k (discrete families).
    pub fn pmf(&self, k: f64) -> f64 {
        let [a, b] = self.p;
        if k < 0.0 {
            return 0.0;
        }
        match self.fam {
            Family::Binomial => {
                let (n, p) = (a, b);
                if k > n {
                    return 0.0;
                }
                if p == 0.0 || p == 1.0 {
                    let at = if p == 0.0 { 0.0 } else { n };
                    return if k == at { 1.0 } else { 0.0 };
                }
                // C(n, k) p^k (1-p)^(n-k), in double-double logarithms
                let v = sp::lgamma(n + 1.0) - sp::lgamma(k + 1.0) - sp::lgamma(n - k + 1.0) + sp::ln(p).mul_f(k)
                    + sp::ln_1p(-p).mul_f(n - k);
                sp::exp_of(v)
            }
            Family::Poisson => {
                let v = sp::ln(a).mul_f(k) - DD::new(a) - sp::lgamma(k + 1.0);
                sp::exp_of(v)
            }
            Family::Geometric => {
                if a == 1.0 {
                    return if k == 0.0 { 1.0 } else { 0.0 };
                }
                a * sp::exp_of(sp::ln_1p(-a).mul_f(k))
            }
            _ => f64::NAN,
        }
    }

    /// (P(X <= x), P(X > x)).
    pub fn cdf(&self, x: f64) -> (f64, f64) {
        let [a, b] = self.p;
        if x.is_nan() {
            return (x, x);
        }
        match self.fam {
            Family::Normal => {
                // the standardized value in double-double: (x - mu)/sigma without a rounding of the difference
                let z = DD::sum(x, -a) / DD::new(b);
                let lo = sp::erfc_dd(-z * DD::new(0.5).sqrt()) / 2.0;
                let hi = sp::erfc_dd(z * DD::new(0.5).sqrt()) / 2.0;
                (lo, hi)
            }
            Family::LogNormal => {
                if x <= 0.0 {
                    return (0.0, 1.0);
                }
                Mach { fam: Family::Normal, p: [a, b] }.cdf(x.ln())
            }
            Family::Uniform => {
                if x <= a {
                    (0.0, 1.0)
                } else if x >= b {
                    (1.0, 0.0)
                } else {
                    ((x - a) / (b - a), (b - x) / (b - a))
                }
            }
            Family::Exponential => {
                if x <= 0.0 {
                    (0.0, 1.0)
                } else {
                    (-(-a * x).exp_m1(), (-a * x).exp())
                }
            }
            Family::Gamma => gamma_pq(a, (x / b).max(0.0)),
            Family::ChiSquare => gamma_pq(a / 2.0, (x / 2.0).max(0.0)),
            Family::Beta => {
                if x <= 0.0 {
                    (0.0, 1.0)
                } else if x >= 1.0 {
                    (1.0, 0.0)
                } else {
                    beta_i(a, b, x, 1.0 - x)
                }
            }
            Family::StudentT => {
                let n = a;
                if x.is_infinite() {
                    return if x > 0.0 { (1.0, 0.0) } else { (0.0, 1.0) };
                }
                // the tail beyond |x| is I_{n/(n+x^2)}(n/2, 1/2)/2
                let x2 = x * x;
                let (z, w) = (n / (n + x2), x2 / (n + x2));
                let tail = beta_i(n / 2.0, 0.5, z, w).0 / 2.0;
                if x < 0.0 {
                    (tail, 1.0 - tail)
                } else {
                    (1.0 - tail, tail)
                }
            }
            Family::Binomial | Family::Poisson | Family::Geometric => {
                if x < 0.0 {
                    return (0.0, 1.0);
                }
                self.cdf_count(x.floor())
            }
        }
    }

    /// (P(X <= k), P(X > k)) of a discrete family at an integer k >= 0.
    fn cdf_count(&self, k: f64) -> (f64, f64) {
        let [a, b] = self.p;
        match self.fam {
            Family::Binomial => {
                let (n, p) = (a, b);
                if k >= n {
                    return (1.0, 0.0);
                }
                if p == 0.0 {
                    return (1.0, 0.0);
                }
                if p == 1.0 {
                    return (0.0, 1.0);
                }
                // P(X <= k) = I_{1-p}(n - k, k + 1)
                beta_i(n - k, k + 1.0, 1.0 - p, p)
            }
            // P(X <= k) = Q(k + 1, m)
            Family::Poisson => {
                if k == f64::INFINITY {
                    return (1.0, 0.0);
                }
                let (p, q) = gamma_pq(k + 1.0, a);
                (q, p)
            }
            Family::Geometric => {
                // P(X > k) = (1 - p)^(k + 1)
                let up = if a == 1.0 { 0.0 } else { sp::exp_of(sp::ln_1p(-a).mul_f(k + 1.0)) };
                (1.0 - up, up)
            }
            _ => (f64::NAN, f64::NAN),
        }
    }

    /// The quantile: the least x with P(X <= x) >= q, for 0 <= q <= 1 (the ends of the support at 0 and 1).
    pub fn quantile(&self, q: f64) -> f64 {
        let [a, b] = self.p;
        if !(0.0..=1.0).contains(&q) {
            return f64::NAN;
        }
        match self.fam {
            Family::Normal => a + b * norm_quantile(q),
            Family::LogNormal => (a + b * norm_quantile(q)).exp(),
            Family::Uniform => a + q * (b - a),
            Family::Exponential => -sp::ln_1p(-q).to_f64() / a,
            Family::Gamma => b * self.invert(q),
            Family::ChiSquare | Family::Beta => self.invert(q),
            Family::StudentT => {
                let n = a;
                if q == 0.0 || q == 1.0 {
                    return if q == 0.0 { f64::NEG_INFINITY } else { f64::INFINITY };
                }
                if q == 0.5 {
                    return 0.0;
                }
                if n == 1.0 {
                    // the Cauchy quantile tan(pi (q - 1/2)), written through the smaller tail (pi q keeps its
                    // relative accuracy where tan has its pole)
                    let t = q.min(1.0 - q);
                    let x = if t == 0.5 { 0.0 } else { 1.0 / (std::f64::consts::PI * t).tan() };
                    return if q < 0.5 { -x } else { x };
                }
                if n == 2.0 {
                    return (2.0 * q - 1.0) / (2.0 * q * (1.0 - q)).sqrt();
                }
                // the tail probability t = min(q, 1 - q) is I_z(n/2, 1/2)/2 at z = n/(n + x^2)
                let t = q.min(1.0 - q);
                let beta = Mach { fam: Family::Beta, p: [n / 2.0, 0.5] };
                let z = beta.invert(2.0 * t);
                let x = if z > 0.5 {
                    // near the centre: w = x^2/(n + x^2) = 1 - z from its own inversion (the upper tail of
                    // I_w(1/2, n/2) is 2t), without the cancellation of 1 - z
                    let w = Mach { fam: Family::Beta, p: [0.5, n / 2.0] }.invert_tail(false, 2.0 * t);
                    (n * w / (1.0 - w)).sqrt()
                } else {
                    (n * (1.0 - z) / z).sqrt()
                };
                if q < 0.5 {
                    -x
                } else {
                    x
                }
            }
            Family::Binomial | Family::Poisson | Family::Geometric => self.quantile_count(q),
        }
    }

    /// The inverse of the distribution function of a gamma (unit scale), chi-square or beta distribution.
    fn invert(&self, q: f64) -> f64 {
        if q <= 0.5 {
            self.invert_tail(true, q)
        } else {
            self.invert_tail(false, 1.0 - q)
        }
    }

    /// The x with P(X <= x) = target (`lower`) or P(X > x) = target, for the families of [`Mach::invert`].
    fn invert_tail(&self, lower: bool, target: f64) -> f64 {
        let q = if lower { target } else { 1.0 - target };
        let [a, b] = self.p;
        let (shape, lo, hi) = match self.fam {
            Family::Gamma => (a, 0.0, f64::INFINITY),
            Family::ChiSquare => (a / 2.0, 0.0, f64::INFINITY),
            Family::Beta => (a, 0.0, 1.0),
            _ => return f64::NAN,
        };
        if target == 0.0 {
            return if lower { lo } else { hi };
        }
        // the unit-scale problem: P(shape, x) = q for gamma and chi-square (x/2), I_x(a, b) = q for beta
        // the normalising logarithms once for all the iterations
        let (f, d): (Box<Tails>, Box<Density>) = match self.fam {
            Family::Beta => {
                let lb = sp::lbeta(a, b);
                (
                    Box::new(move |x| sp::beta_i_l(a, b, x, 1.0 - x, lb)),
                    Box::new(move |x| {
                        if x <= 0.0 || x >= 1.0 {
                            return Mach { fam: Family::Beta, p: [a, b] }.pdf(x);
                        }
                        sp::exp_of(sp::ln(x).mul_f(a - 1.0) + sp::ln_1p(-x).mul_f(b - 1.0) - lb)
                    }),
                )
            }
            _ => {
                let lga = sp::lgamma(shape);
                (
                    Box::new(move |x| sp::gamma_pq_l(shape, x, lga)),
                    Box::new(move |x| if x > 0.0 && x.is_finite() { sp::gamma_prefactor_l(shape, x, lga) / x } else { gamma_density(shape, x) }),
                )
            }
        };
        let x0 = match self.fam {
            Family::Beta => beta_start(a, b, lower, target),
            _ => gamma_start(shape, q),
        };
        let x = invert(&*f, &*d, lower, target, lo, hi, x0);
        match self.fam {
            Family::ChiSquare => 2.0 * x,
            _ => x,
        }
    }

    /// The least integer k in the support with P(X <= k) >= q.
    fn quantile_count(&self, q: f64) -> f64 {
        let [a, b] = self.p;
        let top = match self.fam {
            Family::Binomial => a,
            _ => f64::INFINITY,
        };
        if q == 0.0 {
            return 0.0;
        }
        if q == 1.0 {
            // the least k with P(X <= k) = 1: n for a binomial (0 when p = 0), unbounded otherwise
            return match self.fam {
                Family::Binomial if b == 0.0 => 0.0,
                Family::Geometric if a == 1.0 => 0.0,
                _ => top,
            };
        }
        // the predicate P(X <= k) >= q, on the smaller tail
        let reached = |k: f64| {
            let (lo, up) = self.cdf_count(k);
            if q <= 0.5 {
                lo >= q
            } else {
                up <= 1.0 - q
            }
        };
        // a start from the normal approximation
        let (m, s) = match self.fam {
            Family::Binomial => (a * b, (a * b * (1.0 - b)).sqrt()),
            Family::Poisson => (a, a.sqrt()),
            _ => ((1.0 - a) / a, (1.0 - a).sqrt() / a),
        };
        let mut k = (m + s * norm_quantile(q)).floor().clamp(0.0, top);
        if !k.is_finite() {
            k = 0.0;
        }
        // gallop to a bracket (lo, hi]: reached(hi), not reached(lo)
        let (mut lo, mut hi);
        if reached(k) {
            hi = k;
            let mut step = 1.0;
            loop {
                let c = hi - step;
                if c < 0.0 {
                    lo = -1.0;
                    break;
                }
                if !reached(c) {
                    lo = c;
                    break;
                }
                hi = c;
                step *= 2.0;
            }
        } else {
            lo = k;
            let mut step = 1.0;
            loop {
                let c = (lo + step).min(top);
                if reached(c) || c >= top {
                    hi = c;
                    break;
                }
                lo = c;
                step *= 2.0;
                if !step.is_finite() || step > 1e300 {
                    return f64::INFINITY;
                }
            }
        }
        // bisect on the integers
        for _ in 0..MAX_STEPS {
            if hi - lo <= 1.0 {
                break;
            }
            let mid = ((lo + hi) / 2.0).floor();
            if reached(mid) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        hi
    }
}

/// The integer value of x, if it is one (a count at which a probability mass sits).
fn as_count(x: f64) -> Option<f64> {
    (x >= 0.0 && x == x.floor() && x.is_finite()).then_some(x)
}

/// The gamma density with unit scale: x^(k-1) e^-x / Gamma(k).
fn gamma_density(k: f64, x: f64) -> f64 {
    if x < 0.0 || x == f64::INFINITY {
        return 0.0;
    }
    if x == 0.0 {
        return if k < 1.0 {
            f64::INFINITY
        } else if k == 1.0 {
            1.0
        } else {
            0.0
        };
    }
    sp::gamma_prefactor(k, x) / x
}

/// A start for the gamma inversion: the Wilson-Hilferty approximation, or the small-x behaviour
/// P(k, x) ~ x^k / Gamma(k + 1) in the far lower tail.
fn gamma_start(k: f64, q: f64) -> f64 {
    let z = norm_quantile(q);
    let c = 1.0 / (9.0 * k);
    let wh = k * (1.0 - c + z * c.sqrt()).powi(3);
    let small = ((q.ln() + sp::lgamma(k + 1.0).to_f64()) / k).exp();
    if wh > 0.0 && (k >= 1.0 || q > 0.05) {
        wh
    } else {
        small
    }
}

/// A start for the beta inversion at the lower-tail probability `target` (`lower`) or the upper-tail one: the
/// small-x and small-(1-x) behaviours in the tails, the mean otherwise.
fn beta_start(a: f64, b: f64, lower: bool, target: f64) -> f64 {
    let lb = sp::lbeta(a, b).to_f64();
    let (lq, uq) = if lower { (target.ln(), (-target).ln_1p()) } else { ((-target).ln_1p(), target.ln()) };
    let lo = ((lq + a.ln() + lb) / a).exp();
    let hi = 1.0 - ((uq + b.ln() + lb) / b).exp();
    let mean = a / (a + b);
    if lo < mean {
        lo
    } else if hi > mean {
        hi
    } else {
        mean
    }
}

/// The x in (lo, hi) with F(x) = target (`lower`) or 1 - F(x) = target, for a continuous increasing distribution
/// function F given as both tails (`f`) and its density `d`: safeguarded Newton steps on the smaller tail inside a bracket that every evaluation
/// shrinks, bisection (geometric for brackets spanning orders of magnitude) when a step leaves it.
pub fn invert(
    f: &dyn Fn(f64) -> (f64, f64),
    d: &dyn Fn(f64) -> f64,
    lower: bool,
    target: f64,
    lo: f64,
    hi: f64,
    x0: f64,
) -> f64 {
    let (mut a, mut b) = (lo, hi);
    let mut x = if x0.is_finite() && x0 > lo && x0 < hi { x0 } else { bisect(lo, hi, (lo + hi) / 2.0) };
    let mut prev_step = f64::INFINITY;
    for _ in 0..MAX_STEPS {
        let (l, u) = f(x);
        // g(x) = F(x) - q, from the tail that is small
        let g = if lower { l - target } else { target - u };
        if g == 0.0 {
            return x;
        }
        if g < 0.0 {
            a = x;
        } else {
            b = x;
        }
        let dens = d(x);
        let newton = x - g / dens;
        let inside = dens > 0.0 && newton.is_finite() && newton > a && newton < b;
        // a Newton step below the resolution of x that leaves the bracket: noise in the last bits of F(x) set
        // the bracket's end at x, and x is the answer
        if !inside && dens > 0.0 && (g / dens).abs() <= 4e-16 * x.abs() {
            return x;
        }
        let next = if inside { newton } else { bisect(a, b, x) };
        let step = (next - x).abs();
        // converged: a step below the resolution, or a small Newton step in the quadratic regime (the error after
        // it is about step^2/x, far below the resolution, so no further evaluation is needed)
        let scale = x.abs().max(f64::MIN_POSITIVE);
        let quadratic = inside && step <= 1e-9 * scale && step <= 10.0 * prev_step * prev_step / scale;
        if step <= 4e-16 * x.abs() || quadratic || next == a || next == b {
            return next;
        }
        prev_step = step;
        x = next;
    }
    x
}

/// A point strictly inside (a, b) (either end may be infinite), near the middle in the scale that suits the
/// bracket: geometric for positive brackets spanning orders of magnitude, else arithmetic, and a step away from
/// x toward an infinite end.
fn bisect(a: f64, b: f64, x: f64) -> f64 {
    match (a.is_finite(), b.is_finite()) {
        (true, true) => {
            if a > 0.0 && b > 4.0 * a {
                (a * b).sqrt()
            } else if a == 0.0 && b > 0.0 {
                b / 4.0
            } else {
                a + (b - a) / 2.0
            }
        }
        (true, false) => (2.0 * x.abs()).max(a + 1.0).max(x + 1.0),
        (false, true) => (-2.0 * x.abs()).min(b - 1.0).min(x - 1.0),
        (false, false) => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(a: f64, b: f64) -> f64 {
        if a == b {
            0.0
        } else {
            (a - b).abs() / b.abs()
        }
    }

    #[test]
    fn quantiles_invert_the_distribution_functions() {
        let ds = [
            Mach::new(Family::Gamma, &[0.3, 2.0]).unwrap(),
            Mach::new(Family::Gamma, &[7.5, 0.5]).unwrap(),
            Mach::new(Family::Gamma, &[400.0, 1.0]).unwrap(),
            Mach::new(Family::ChiSquare, &[3.0]).unwrap(),
            Mach::new(Family::Beta, &[0.5, 0.5]).unwrap(),
            Mach::new(Family::Beta, &[2.0, 9.0]).unwrap(),
            Mach::new(Family::Beta, &[30.0, 0.7]).unwrap(),
            Mach::new(Family::StudentT, &[3.0]).unwrap(),
            Mach::new(Family::StudentT, &[0.5]).unwrap(),
            Mach::new(Family::StudentT, &[50.0]).unwrap(),
            Mach::new(Family::LogNormal, &[1.0, 0.5]).unwrap(),
        ];
        for d in ds {
            for &q in &[1e-12, 1e-4, 0.05, 0.3, 0.5, 0.77, 0.99, 1.0 - 1e-9] {
                let x = d.quantile(q);
                let (l, u) = d.cdf(x);
                let err = if q <= 0.5 { rel(l, q) } else { rel(u, 1.0 - q) };
                // or q lies between F at the neighbours of x (the exact quantile is not a double)
                let (below, above) = (d.cdf(x - x.abs() * 4e-16).0, d.cdf(x + x.abs() * 4e-16).0);
                let bracketed = below <= q && q <= above;
                assert!(err < 1e-12 || bracketed, "{d:?} q = {q}: x = {x}, F = {l}, error {err}");
            }
        }
    }

    #[test]
    fn discrete_quantiles_are_least_counts() {
        let ds = [
            Mach::new(Family::Binomial, &[20.0, 0.3]).unwrap(),
            Mach::new(Family::Binomial, &[1e6, 0.001]).unwrap(),
            Mach::new(Family::Poisson, &[3.5]).unwrap(),
            Mach::new(Family::Poisson, &[1e5]).unwrap(),
            Mach::new(Family::Geometric, &[0.2]).unwrap(),
        ];
        for d in ds {
            for &q in &[1e-9, 0.01, 0.25, 0.5, 0.9, 0.999] {
                let k = d.quantile(q);
                assert!(d.cdf(k).0 >= q * (1.0 - 1e-12), "{d:?} {q}: {k}");
                assert!(k == 0.0 || d.cdf(k - 1.0).0 < q, "{d:?} {q}: {k}");
            }
        }
    }

    #[test]
    fn discrete_masses_sum_to_one() {
        for d in [
            Mach::new(Family::Binomial, &[30.0, 0.35]).unwrap(),
            Mach::new(Family::Poisson, &[7.0]).unwrap(),
            Mach::new(Family::Geometric, &[0.4]).unwrap(),
        ] {
            let s: f64 = (0..200).map(|k| d.pdf(k as f64)).sum();
            assert!((s - 1.0).abs() < 1e-13, "{d:?}: {s}");
            // and the distribution function is the partial sums
            let partial: f64 = (0..=5).map(|k| d.pdf(k as f64)).sum();
            assert!(rel(d.cdf(5.5).0, partial) < 1e-13, "{d:?}");
        }
    }
}
