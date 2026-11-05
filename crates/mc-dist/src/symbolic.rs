//! Exact and symbolic forms: densities, distribution functions, closed-form quantiles and moments as terms in
//! the parameters and the argument, which the evaluator then brings to canonical form (exact numbers give exact
//! values, symbols give formulas).
//!
//! Non-elementary distribution functions are written with the functions of package 3C (decision DI.3):
//! `erfc(z)`, the upper incomplete gamma function `gamma(a, x)` and the incomplete beta function `beta(z, a, b)`
//! (B_z(a, b), not regularized), with `inverse_erfc` for the normal quantile. Elementary cases avoid them: a
//! gamma or chi-square distribution with an integer shape has the finite Erlang sum, a beta distribution with
//! integer parameters a polynomial distribution function, Student t with 1 or 2 degrees of freedom closed forms,
//! and discrete distributions at a numeric argument finite sums (up to [`MAX_TERMS`] terms).
use crate::build::B;
use crate::family::{Dist, Family};
use mc_api::T;
use mc_arith::num::{Int, Rat};

/// The most terms of a finite sum written out (Erlang sums, binomial and Poisson distribution functions,
/// polynomial beta distribution functions, moments of discrete distributions); beyond, the incomplete-function
/// form.
pub const MAX_TERMS: i64 = 200;

/// A small exact integer value of an optional rational.
fn small_int(q: Option<&Rat>) -> Option<i64> {
    q.filter(|q| q.is_int()).and_then(|q| q.num.to_i64())
}

/// The moment statistics with closed forms.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stat {
    /// the mean
    Mean,
    /// the variance
    Variance,
    /// the skewness E[(X - mu)^3]/sigma^3
    Skewness,
    /// the kurtosis E[(X - mu)^4]/sigma^4 (3 for a normal distribution, not the excess)
    Kurtosis,
}

/// The exact parameters of a distribution, where they are rational numbers.
pub type Exact = [Option<Rat>; 2];

/// The density at x (the probability of x for a discrete family).
pub fn pdf<'h>(b: B<'h>, d: &Dist<'h>, ex: &Exact, x: T<'h>, xq: Option<&Rat>) -> T<'h> {
    let p = &d.p;
    let (zero, one) = (b.int(0), b.int(1));
    let half = b.rat(1, 2);
    match d.fam {
        Family::Normal => normal_pdf(b, p[0], p[1], x),
        Family::LogNormal | Family::Exponential | Family::Gamma | Family::ChiSquare => {
            let (v, _, _) = density(b, d, x).expect("continuous");
            let cond = if d.fam == Family::Exponential { b.le(zero, x) } else { b.lt(zero, x) };
            b.piecewise(&[(v, cond)], zero)
        }
        Family::Uniform => {
            let v = b.powi(b.sub(p[1], p[0]), -1);
            b.piecewise(&[(v, b.and(b.le(p[0], x), b.le(x, p[1])))], zero)
        }
        Family::Beta => {
            let (v, _, _) = density(b, d, x).expect("continuous");
            b.piecewise(&[(v, b.and(b.lt(zero, x), b.lt(x, one)))], zero)
        }
        Family::StudentT => {
            let n = p[0];
            // (n/(n + x^2))^((n+1)/2) / (sqrt(n) B(n/2, 1/2))
            let base = b.div(n, b.add(&[n, b.powi(x, 2)]));
            b.div(
                b.pow(base, b.mul(&[half, b.add(&[n, one])])),
                b.mul(&[b.sqrt(n), b.call("beta", &[b.mul(&[half, n]), half])]),
            )
        }
        Family::Binomial | Family::Poisson | Family::Geometric => {
            // off the integers of the support the probability is 0; a symbolic x is taken as an integer
            if let Some(q) = xq {
                if !q.is_int() || q.num.neg {
                    return zero;
                }
                if d.fam == Family::Binomial {
                    if let Some(n) = ex[0].as_ref() {
                        if q.cmp_value(n).is_gt() {
                            return zero;
                        }
                    }
                }
            }
            let v = pmf(b, d, x);
            match (xq, d.fam) {
                (Some(_), _) => v,
                (None, Family::Binomial) => b.piecewise(&[(v, b.and(b.le(zero, x), b.le(x, p[0])))], zero),
                (None, _) => b.piecewise(&[(v, b.le(zero, x))], zero),
            }
        }
    }
}

/// exp(-(x - mu)^2/(2 sigma^2))/(sqrt(2 pi) sigma).
fn normal_pdf<'h>(b: B<'h>, mu: T<'h>, sigma: T<'h>, x: T<'h>) -> T<'h> {
    let e = b.div(b.powi(b.sub(x, mu), 2), b.mul(&[b.int(2), b.powi(sigma, 2)]));
    b.div(b.exp(b.neg(e)), b.mul(&[b.sqrt(b.mul(&[b.int(2), b.pi()])), sigma]))
}

/// The probability formula of a discrete family at an integer k.
fn pmf<'h>(b: B<'h>, d: &Dist<'h>, k: T<'h>) -> T<'h> {
    let p = &d.p;
    let one = b.int(1);
    match d.fam {
        Family::Binomial => b.mul(&[
            b.call("binomial", &[p[0], k]),
            b.pow(p[1], k),
            b.pow(b.sub(one, p[1]), b.sub(p[0], k)),
        ]),
        Family::Poisson => b.div(b.mul(&[b.exp(b.neg(p[0])), b.pow(p[0], k)]), b.call("factorial", &[k])),
        _ => b.mul(&[p[0], b.pow(b.sub(one, p[0]), k)]),
    }
}

/// The distribution function P(X <= x).
pub fn cdf<'h>(b: B<'h>, d: &Dist<'h>, ex: &Exact, x: T<'h>, xq: Option<&Rat>, int: &dyn Fn(&Int) -> T<'h>) -> T<'h> {
    let p = &d.p;
    let (zero, one, two) = (b.int(0), b.int(1), b.int(2));
    let half = b.rat(1, 2);
    match d.fam {
        Family::Normal => normal_cdf(b, p[0], p[1], x),
        Family::LogNormal => b.piecewise(&[(normal_cdf(b, p[0], p[1], b.log(x)), b.lt(zero, x))], zero),
        Family::Uniform => {
            let v = b.div(b.sub(x, p[0]), b.sub(p[1], p[0]));
            b.piecewise(&[(v, b.and(b.le(p[0], x), b.le(x, p[1]))), (one, b.lt(p[1], x))], zero)
        }
        Family::Exponential => {
            let v = b.sub(one, b.exp(b.neg(b.mul(&[p[0], x]))));
            b.piecewise(&[(v, b.le(zero, x))], zero)
        }
        Family::Gamma => b.piecewise(&[(gamma_p(b, p[0], ex[0].as_ref(), b.div(x, p[1])), b.lt(zero, x))], zero),
        Family::ChiSquare => {
            let k = b.mul(&[half, p[0]]);
            let kq = ex[0].as_ref().map(|q| q.mul(&Rat::new(Int::from_i64(1), Int::from_i64(2))));
            b.piecewise(&[(gamma_p(b, k, kq.as_ref(), b.mul(&[half, x])), b.lt(zero, x))], zero)
        }
        Family::Beta => {
            let v = beta_cdf(b, p[0], p[1], small_int(ex[0].as_ref()), small_int(ex[1].as_ref()), x);
            b.piecewise(&[(v, b.and(b.lt(zero, x), b.lt(x, one))), (one, b.le(one, x))], zero)
        }
        Family::StudentT => match small_int(ex[0].as_ref()) {
            Some(1) => b.add(&[half, b.div(b.call("atan", &[x]), b.pi())]),
            Some(2) => b.add(&[half, b.div(x, b.mul(&[two, b.sqrt(b.add(&[two, b.powi(x, 2)]))]))]),
            _ => {
                let n = p[0];
                let z = b.div(n, b.add(&[n, b.powi(x, 2)]));
                let tail = b.div(
                    b.call("beta", &[z, b.mul(&[half, n]), half]),
                    b.mul(&[two, b.call("beta", &[b.mul(&[half, n]), half])]),
                );
                b.piecewise(&[(tail, b.le(x, zero))], b.sub(one, tail))
            }
        },
        Family::Binomial | Family::Poisson | Family::Geometric => match xq {
            Some(q) => {
                let (k, _) = q.floor_fract();
                if k.neg {
                    return zero;
                }
                discrete_cdf_at(b, d, ex, int(&k), k.to_i64())
            }
            None => {
                let k = b.call("floor", &[x]);
                let v = match d.fam {
                    Family::Binomial => {
                        let (n, pp) = (p[0], p[1]);
                        let nk = b.sub(n, k);
                        let k1 = b.add(&[one, k]);
                        b.div(b.call("beta", &[b.sub(one, pp), nk, k1]), b.call("beta", &[nk, k1]))
                    }
                    Family::Poisson => {
                        b.div(b.call("gamma", &[b.add(&[one, k]), p[0]]), b.call("factorial", &[k]))
                    }
                    _ => b.sub(one, b.pow(b.sub(one, p[0]), b.add(&[one, k]))),
                };
                match d.fam {
                    Family::Binomial => {
                        b.piecewise(&[(v, b.and(b.le(zero, x), b.lt(x, p[0]))), (one, b.le(p[0], x))], zero)
                    }
                    _ => b.piecewise(&[(v, b.le(zero, x))], zero),
                }
            }
        },
    }
}

/// erfc((mu - x)/(sqrt(2) sigma))/2.
fn normal_cdf<'h>(b: B<'h>, mu: T<'h>, sigma: T<'h>, x: T<'h>) -> T<'h> {
    let z = b.div(b.sub(mu, x), b.mul(&[b.sqrt(b.int(2)), sigma]));
    b.mul(&[b.rat(1, 2), b.call("erfc", &[z])])
}

/// The regularized lower incomplete gamma function P(k, y): the Erlang sum 1 - e^-y sum_{j<k} y^j/j! for a small
/// integer k, else 1 - gamma(k, y)/gamma(k).
fn gamma_p<'h>(b: B<'h>, k: T<'h>, kq: Option<&Rat>, y: T<'h>) -> T<'h> {
    let one = b.int(1);
    match small_int(kq) {
        Some(n) if (1..=MAX_TERMS).contains(&n) => {
            let terms: Vec<T<'h>> =
                (0..n).map(|j| b.div(b.powi(y, j), b.call("factorial", &[b.int(j)]))).collect();
            b.sub(one, b.mul(&[b.exp(b.neg(y)), b.add(&terms)]))
        }
        _ => b.sub(one, b.div(b.call("gamma", &[k, y]), b.call("gamma", &[k]))),
    }
}

/// I_x(a, b): for integer a, b the polynomial sum_{j=a}^{a+b-1} C(a+b-1, j) x^j (1-x)^(a+b-1-j), else
/// beta(x, a, b)/beta(a, b).
fn beta_cdf<'h>(b: B<'h>, a: T<'h>, bb: T<'h>, ai: Option<i64>, bi: Option<i64>, x: T<'h>) -> T<'h> {
    match (ai, bi) {
        (Some(ai), Some(bi)) if ai >= 1 && bi >= 1 && ai + bi - 1 <= MAX_TERMS => {
            let m = ai + bi - 1;
            let one = b.int(1);
            let terms: Vec<T<'h>> = (ai..=m)
                .map(|j| {
                    b.mul(&[b.call("binomial", &[b.int(m), b.int(j)]), b.powi(x, j), b.powi(b.sub(one, x), m - j)])
                })
                .collect();
            b.add(&terms)
        }
        _ => b.div(b.call("beta", &[x, a, bb]), b.call("beta", &[a, bb])),
    }
}

/// P(X <= k) of a discrete family at an integer k >= 0: a finite sum (the shorter side for a binomial) up to
/// [`MAX_TERMS`] terms, else the incomplete-function form.
fn discrete_cdf_at<'h>(b: B<'h>, d: &Dist<'h>, ex: &Exact, kt: T<'h>, ki: Option<i64>) -> T<'h> {
    let (zero, one) = (b.int(0), b.int(1));
    let p = &d.p;
    let Some(ki) = ki else {
        // beyond the machine integers: the closed forms in k
        return match d.fam {
            Family::Binomial => one,
            Family::Poisson => b.div(b.call("gamma", &[b.add(&[one, kt]), p[0]]), b.call("factorial", &[kt])),
            _ => b.sub(one, b.pow(b.sub(one, p[0]), b.add(&[one, kt]))),
        };
    };
    match d.fam {
        Family::Binomial => {
            let n = small_int(ex[0].as_ref());
            if let Some(n) = n {
                if ki >= n {
                    return one;
                }
            }
            let term = |j: i64| pmf(b, d, b.int(j));
            match n {
                // the shorter of the two sums
                Some(n) if n - ki - 1 < ki && n - ki <= MAX_TERMS => {
                    let upper: Vec<T<'h>> = (ki + 1..=n).map(term).collect();
                    b.sub(one, b.add(&upper))
                }
                _ if ki < MAX_TERMS => b.add(&(0..=ki).map(term).collect::<Vec<_>>()),
                _ => {
                    let nk = b.sub(p[0], b.int(ki));
                    let k1 = b.int(ki + 1);
                    b.div(b.call("beta", &[b.sub(one, p[1]), nk, k1]), b.call("beta", &[nk, k1]))
                }
            }
        }
        Family::Poisson => {
            if ki < MAX_TERMS {
                let terms: Vec<T<'h>> =
                    (0..=ki).map(|j| b.div(b.powi(p[0], j), b.call("factorial", &[b.int(j)]))).collect();
                b.mul(&[b.exp(b.neg(p[0])), b.add(&terms)])
            } else {
                b.div(b.call("gamma", &[b.int(ki + 1), p[0]]), b.call("factorial", &[b.int(ki)]))
            }
        }
        Family::Geometric => b.sub(one, b.pow(b.sub(one, p[0]), b.int(ki + 1))),
        _ => zero,
    }
}

/// The survival function P(X > x): direct forms where they are simpler than 1 - cdf.
pub fn survival<'h>(b: B<'h>, d: &Dist<'h>, ex: &Exact, x: T<'h>, xq: Option<&Rat>, int: &dyn Fn(&Int) -> T<'h>) -> T<'h> {
    let p = &d.p;
    let (zero, one) = (b.int(0), b.int(1));
    match d.fam {
        Family::Normal => normal_cdf(b, b.neg(p[0]), p[1], b.neg(x)),
        Family::Exponential => b.piecewise(&[(b.exp(b.neg(b.mul(&[p[0], x]))), b.le(zero, x))], one),
        Family::Geometric if xq.is_none() => {
            let k = b.call("floor", &[x]);
            b.piecewise(&[(b.pow(b.sub(one, p[0]), b.add(&[one, k])), b.le(zero, x))], one)
        }
        _ => b.sub(one, cdf(b, d, ex, x, xq, int)),
    }
}

/// A closed-form quantile, where the family has one: normal and log-normal (through `inverse_erfc`), uniform,
/// exponential, Student t with 1 or 2 degrees of freedom; the ends q = 0, 1 and the median of symmetric
/// families exactly.
pub fn quantile<'h>(b: B<'h>, d: &Dist<'h>, ex: &Exact, q: T<'h>, qq: Option<&Rat>) -> Option<T<'h>> {
    let p = &d.p;
    let (zero, one, two) = (b.int(0), b.int(1), b.int(2));
    let half = Rat::new(Int::from_i64(1), Int::from_i64(2));
    let is = |v: &Rat| qq.is_some_and(|q| q.cmp_value(v).is_eq());
    let (q0, q1, qh) = (is(&Rat::from_i64(0)), is(&Rat::from_i64(1)), is(&half));
    let neg_inf = b.neg(b.inf());
    Some(match d.fam {
        Family::Normal => {
            if q0 || q1 {
                return Some(if q0 { neg_inf } else { b.inf() });
            }
            if qh {
                return Some(p[0]);
            }
            normal_quantile(b, p[0], p[1], q)
        }
        Family::LogNormal => {
            if q0 || q1 {
                return Some(if q0 { zero } else { b.inf() });
            }
            if qh {
                return Some(b.exp(p[0]));
            }
            b.exp(normal_quantile(b, p[0], p[1], q))
        }
        Family::Uniform => b.add(&[p[0], b.mul(&[q, b.sub(p[1], p[0])])]),
        Family::Exponential => {
            if q1 {
                return Some(b.inf());
            }
            b.neg(b.div(b.log(b.sub(one, q)), p[0]))
        }
        Family::StudentT => {
            if q0 || q1 {
                return Some(if q0 { neg_inf } else { b.inf() });
            }
            if qh {
                return Some(zero);
            }
            match small_int(ex[0].as_ref()) {
                Some(1) => b.call("tan", &[b.mul(&[b.pi(), b.sub(q, b.rat(1, 2))])]),
                Some(2) => b.div(b.sub(b.mul(&[two, q]), one), b.sqrt(b.mul(&[two, q, b.sub(one, q)]))),
                _ => return None,
            }
        }
        Family::Gamma | Family::ChiSquare | Family::Beta => {
            if q0 {
                return Some(zero);
            }
            if q1 {
                return Some(if d.fam == Family::Beta { one } else { b.inf() });
            }
            return None;
        }
        Family::Binomial | Family::Poisson | Family::Geometric => {
            if q0 {
                return Some(zero);
            }
            if q1 {
                // the least k with P(X <= k) = 1
                return match d.fam {
                    Family::Binomial => Some(p[0]),
                    Family::Geometric if small_int(ex[0].as_ref()) == Some(1) => Some(zero),
                    _ => Some(b.inf()),
                };
            }
            return None;
        }
    })
}

/// The standard deviation where it is simpler than the square root of the variance (the parameters are positive
/// by the family's constraints): sigma, (b - a)/sqrt(12), 1/l, sqrt(k) theta.
pub fn std_dev<'h>(b: B<'h>, d: &Dist<'h>) -> Option<T<'h>> {
    let p = &d.p;
    match d.fam {
        Family::Normal => Some(p[1]),
        Family::Uniform => Some(b.div(b.sub(p[1], p[0]), b.sqrt(b.int(12)))),
        Family::Exponential => Some(b.powi(p[0], -1)),
        Family::Gamma => Some(b.mul(&[b.sqrt(p[0]), p[1]])),
        _ => None,
    }
}

/// mu - sqrt(2) sigma inverse_erfc(2q).
fn normal_quantile<'h>(b: B<'h>, mu: T<'h>, sigma: T<'h>, q: T<'h>) -> T<'h> {
    let two = b.int(2);
    b.sub(mu, b.mul(&[b.sqrt(two), sigma, b.call("inverse_erfc", &[b.mul(&[two, q])])]))
}

/// The mean, variance, skewness or kurtosis: `Ok(term)`, `Err(true)` for an infinite value (`inf`), `Err(false)`
/// for a moment that does not exist. `n` is the numeric value of the degrees of freedom of a Student t
/// distribution (symbolic degrees of freedom are taken to be large enough).
pub fn stat<'h>(b: B<'h>, d: &Dist<'h>, s: Stat, n: Option<f64>) -> Result<T<'h>, bool> {
    let p = &d.p;
    let (zero, one, two, three) = (b.int(0), b.int(1), b.int(2), b.int(3));
    let half = b.rat(1, 2);
    let sq = |t| b.powi(t, 2);
    Ok(match (d.fam, s) {
        (Family::Normal, Stat::Mean) => p[0],
        (Family::Normal, Stat::Variance) => sq(p[1]),
        (Family::Normal, Stat::Skewness) => zero,
        (Family::Normal, Stat::Kurtosis) => three,
        (Family::LogNormal, s) => {
            let s2 = sq(p[1]);
            let es2 = b.exp(s2);
            match s {
                Stat::Mean => b.exp(b.add(&[p[0], b.mul(&[half, s2])])),
                Stat::Variance => b.mul(&[b.sub(es2, one), b.exp(b.add(&[b.mul(&[two, p[0]]), s2]))]),
                Stat::Skewness => b.mul(&[b.add(&[es2, two]), b.sqrt(b.sub(es2, one))]),
                Stat::Kurtosis => b.add(&[
                    b.exp(b.mul(&[b.int(4), s2])),
                    b.mul(&[two, b.exp(b.mul(&[three, s2]))]),
                    b.mul(&[three, b.exp(b.mul(&[two, s2]))]),
                    b.int(-3),
                ]),
            }
        }
        (Family::Uniform, Stat::Mean) => b.mul(&[half, b.add(&[p[0], p[1]])]),
        (Family::Uniform, Stat::Variance) => b.div(sq(b.sub(p[1], p[0])), b.int(12)),
        (Family::Uniform, Stat::Skewness) => zero,
        (Family::Uniform, Stat::Kurtosis) => b.rat(9, 5),
        (Family::Exponential, Stat::Mean) => b.powi(p[0], -1),
        (Family::Exponential, Stat::Variance) => b.powi(p[0], -2),
        (Family::Exponential, Stat::Skewness) => two,
        (Family::Exponential, Stat::Kurtosis) => b.int(9),
        (Family::Gamma, Stat::Mean) => b.mul(&[p[0], p[1]]),
        (Family::Gamma, Stat::Variance) => b.mul(&[p[0], sq(p[1])]),
        (Family::Gamma, Stat::Skewness) => b.div(two, b.sqrt(p[0])),
        (Family::Gamma, Stat::Kurtosis) => b.add(&[three, b.div(b.int(6), p[0])]),
        (Family::ChiSquare, Stat::Mean) => p[0],
        (Family::ChiSquare, Stat::Variance) => b.mul(&[two, p[0]]),
        (Family::ChiSquare, Stat::Skewness) => b.mul(&[two, b.sqrt(b.div(two, p[0]))]),
        (Family::ChiSquare, Stat::Kurtosis) => b.add(&[three, b.div(b.int(12), p[0])]),
        (Family::Beta, s) => {
            let (a, bb) = (p[0], p[1]);
            let ab = b.add(&[a, bb]);
            match s {
                Stat::Mean => b.div(a, ab),
                Stat::Variance => b.div(b.mul(&[a, bb]), b.mul(&[sq(ab), b.add(&[ab, one])])),
                Stat::Skewness => b.div(
                    b.mul(&[two, b.sub(bb, a), b.sqrt(b.add(&[ab, one]))]),
                    b.mul(&[b.add(&[ab, two]), b.sqrt(b.mul(&[a, bb]))]),
                ),
                Stat::Kurtosis => {
                    let num = b.sub(
                        b.mul(&[sq(b.sub(a, bb)), b.add(&[ab, one])]),
                        b.mul(&[a, bb, b.add(&[ab, two])]),
                    );
                    let den = b.mul(&[a, bb, b.add(&[ab, two]), b.add(&[ab, three])]);
                    b.add(&[three, b.div(b.mul(&[b.int(6), num]), den)])
                }
            }
        }
        (Family::StudentT, s) => {
            let nn = p[0];
            match s {
                Stat::Mean => match n {
                    Some(v) if v <= 1.0 => return Err(false),
                    _ => zero,
                },
                Stat::Variance => match n {
                    Some(v) if v <= 1.0 => return Err(false),
                    Some(v) if v <= 2.0 => return Err(true),
                    _ => b.div(nn, b.sub(nn, two)),
                },
                Stat::Skewness => match n {
                    Some(v) if v <= 3.0 => return Err(false),
                    _ => zero,
                },
                Stat::Kurtosis => match n {
                    Some(v) if v <= 2.0 => return Err(false),
                    Some(v) if v <= 4.0 => return Err(true),
                    _ => b.add(&[three, b.div(b.int(6), b.sub(nn, b.int(4)))]),
                },
            }
        }
        (Family::Binomial, s) => {
            let (nn, pp) = (p[0], p[1]);
            let npq = b.mul(&[nn, pp, b.sub(one, pp)]);
            match s {
                Stat::Mean => b.mul(&[nn, pp]),
                Stat::Variance => npq,
                Stat::Skewness => b.div(b.sub(one, b.mul(&[two, pp])), b.sqrt(npq)),
                Stat::Kurtosis => {
                    b.add(&[three, b.div(b.sub(one, b.mul(&[b.int(6), pp, b.sub(one, pp)])), npq)])
                }
            }
        }
        (Family::Poisson, s) => match s {
            Stat::Mean | Stat::Variance => p[0],
            Stat::Skewness => b.powi(b.sqrt(p[0]), -1),
            Stat::Kurtosis => b.add(&[three, b.powi(p[0], -1)]),
        },
        (Family::Geometric, s) => {
            let pp = p[0];
            let q = b.sub(one, pp);
            match s {
                Stat::Mean => b.div(q, pp),
                Stat::Variance => b.div(q, sq(pp)),
                Stat::Skewness => b.div(b.sub(two, pp), b.sqrt(q)),
                Stat::Kurtosis => b.add(&[b.int(9), b.div(sq(pp), q)]),
            }
        }
    })
}

/// Stirling numbers of the second kind S(k, 0..=k).
fn stirling2(k: usize) -> Vec<Int> {
    let mut row = vec![Int::from_i64(1)];
    for n in 1..=k {
        let mut next = vec![Int::from_i64(0); n + 1];
        for j in 1..=n {
            let stay = if j < n { row[j].mul(&Int::from_i64(j as i64)) } else { Int::from_i64(0) };
            next[j] = stay.add(&row[j - 1]);
        }
        row = next;
    }
    row
}

/// The k-th raw moment E[X^k] (k >= 0 at most [`MAX_TERMS`]): `Ok(term)`, `Err(true)` when infinite, `Err(false)`
/// when it does not exist. `n` as in [`stat`].
pub fn raw_moment<'h>(b: B<'h>, d: &Dist<'h>, k: i64, n: Option<f64>, int: &dyn Fn(&Int) -> T<'h>) -> Result<T<'h>, bool> {
    let p = &d.p;
    let one = b.int(1);
    let prod = |f: &dyn Fn(i64) -> T<'h>| -> T<'h> {
        if k == 0 {
            one
        } else {
            b.mul(&(0..k).map(f).collect::<Vec<_>>())
        }
    };
    Ok(match d.fam {
        Family::Normal => {
            // sum over even j of C(k, j) mu^(k-j) sigma^j (j-1)!!
            let mut terms = vec![];
            let mut dfact = Int::from_i64(1);
            for j in (0..=k).step_by(2) {
                if j >= 2 {
                    dfact = dfact.mul(&Int::from_i64(j - 1));
                }
                terms.push(b.mul(&[
                    b.call("binomial", &[b.int(k), b.int(j)]),
                    b.powi(p[0], k - j),
                    b.powi(p[1], j),
                    int(&dfact),
                ]));
            }
            b.add(&terms)
        }
        Family::LogNormal => {
            let kt = b.int(k);
            b.exp(b.add(&[b.mul(&[kt, p[0]]), b.mul(&[b.rat(1, 2), b.powi(kt, 2), b.powi(p[1], 2)])]))
        }
        Family::Uniform => b.div(
            b.sub(b.powi(p[1], k + 1), b.powi(p[0], k + 1)),
            b.mul(&[b.int(k + 1), b.sub(p[1], p[0])]),
        ),
        Family::Exponential => b.div(b.call("factorial", &[b.int(k)]), b.powi(p[0], k)),
        Family::Gamma => b.mul(&[b.powi(p[1], k), prod(&|j| b.add(&[p[0], b.int(j)]))]),
        Family::ChiSquare => prod(&|j| b.add(&[p[0], b.int(2 * j)])),
        Family::Beta => prod(&|j| b.div(b.add(&[p[0], b.int(j)]), b.add(&[p[0], p[1], b.int(j)]))),
        Family::StudentT => {
            let odd = k % 2 == 1;
            if let Some(nv) = n {
                if nv <= k as f64 {
                    return Err(!odd);
                }
            }
            if odd {
                b.int(0)
            } else {
                // n^(k/2) prod_{j=1}^{k/2} (2j - 1)/(n - 2j)
                let m = k / 2;
                let mut f = vec![b.powi(p[0], m)];
                for j in 1..=m {
                    f.push(b.div(b.int(2 * j - 1), b.sub(p[0], b.int(2 * j))));
                }
                b.mul(&f)
            }
        }
        Family::Binomial | Family::Poisson | Family::Geometric => {
            // sum_j S(k, j) E[falling factorial of order j]
            let s = stirling2(k as usize);
            let mut terms = vec![];
            for (j, sj) in s.iter().enumerate() {
                if sj.is_zero() {
                    continue;
                }
                let j = j as i64;
                let falling = match d.fam {
                    Family::Binomial => {
                        let mut f: Vec<T<'h>> = (0..j).map(|i| b.sub(p[0], b.int(i))).collect();
                        f.push(b.powi(p[1], j));
                        b.mul(&f)
                    }
                    Family::Poisson => b.powi(p[0], j),
                    _ => b.mul(&[
                        b.call("factorial", &[b.int(j)]),
                        b.powi(b.div(b.sub(one, p[0]), p[0]), j),
                    ]),
                };
                terms.push(b.mul(&[int(sj), falling]));
            }
            if terms.is_empty() {
                one
            } else {
                b.add(&terms)
            }
        }
    })
}

/// The density of a continuous family on its support, without the piecewise wrapper, and the ends of the support
/// (for integrals): `None` for a discrete family.
pub fn density<'h>(b: B<'h>, d: &Dist<'h>, x: T<'h>) -> Option<(T<'h>, T<'h>, T<'h>)> {
    let p = &d.p;
    let (zero, one, two) = (b.int(0), b.int(1), b.int(2));
    let half = b.rat(1, 2);
    let (inf, ninf) = (b.inf(), b.neg(b.inf()));
    Some(match d.fam {
        Family::Normal => (normal_pdf(b, p[0], p[1], x), ninf, inf),
        Family::LogNormal => (b.div(normal_pdf(b, p[0], p[1], b.log(x)), x), zero, inf),
        Family::Uniform => (b.powi(b.sub(p[1], p[0]), -1), p[0], p[1]),
        Family::Exponential => (b.mul(&[p[0], b.exp(b.neg(b.mul(&[p[0], x])))]), zero, inf),
        Family::Gamma => {
            let (k, th) = (p[0], p[1]);
            let v = b.div(
                b.mul(&[b.pow(x, b.sub(k, one)), b.exp(b.neg(b.div(x, th)))]),
                b.mul(&[b.call("gamma", &[k]), b.pow(th, k)]),
            );
            (v, zero, inf)
        }
        Family::ChiSquare => {
            let h = b.mul(&[half, p[0]]);
            let v = b.div(
                b.mul(&[b.pow(x, b.sub(h, one)), b.exp(b.neg(b.mul(&[half, x])))]),
                b.mul(&[b.pow(two, h), b.call("gamma", &[h])]),
            );
            (v, zero, inf)
        }
        Family::Beta => {
            let (a, bb) = (p[0], p[1]);
            let v = b.div(
                b.mul(&[b.pow(x, b.sub(a, one)), b.pow(b.sub(one, x), b.sub(bb, one))]),
                b.call("beta", &[a, bb]),
            );
            (v, zero, one)
        }
        Family::StudentT => (pdf(b, d, &[None, None], x, None), ninf, inf),
        Family::Binomial | Family::Poisson | Family::Geometric => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stirling_numbers() {
        let s: Vec<i64> = stirling2(5).iter().map(|v| v.to_i64().unwrap()).collect();
        assert_eq!(s, [0, 1, 15, 25, 10, 1]);
        assert_eq!(stirling2(0)[0].to_i64(), Some(1));
    }
}
