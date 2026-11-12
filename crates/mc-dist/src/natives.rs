//! The natives: the family constructors (checks), the functions of a distribution (density, distribution
//! function, quantile, moments, variates) and their data-list forms.
//!
//! The arguments of `pdf(d, x)`, `cdf`, `survival` and `quantile` decide the path (decision DI.4):
//!
//! - a machine real among the parameters and the argument, all of them numeric (closed expressions such as
//!   `sqrt(2)` are made numeric by `n`): the machine path of [`crate::machine`];
//! - otherwise the exact or symbolic form of [`crate::symbolic`], handed back to the evaluator, which evaluates it
//!   exactly, symbolically or in balls (a ball among the arguments); a quantile without a closed form is inverted
//!   in balls by Newton steps on the distribution function, the precision doubling per step ([`ball_quantile`]);
//!   a discrete quantile at an exact probability is the least integer whose exact distribution function reaches
//!   it (a machine candidate confirmed by exact comparisons).
//!
//! A list argument maps (`pdf(d, [x1, x2])`); a packed array of machine reals maps in machine arithmetic into a
//! packed array.
use crate::build::B;
use crate::data::{self, DataStat};
use crate::family::{Dist, Family};
use crate::machine::Mach;
use crate::symbolic::{self, Exact, Stat};
use crate::{expect, fit};
use mc_api::packed::{build_nested, Packed};
use mc_api::{names, Store, View, T};
use mc_arith::num::{Int, Nat, Rat};
use mc_arith::{Arith, Num, Real};
use mc_core::Heap;
use mc_eval::{Native, NativeCx, Reply, Res};
use mc_random::Rng;

/// The functions of this crate other than the family constructors.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Fun {
    Pdf,
    Cdf,
    Survival,
    Quantile,
    Mean,
    Variance,
    StdDev,
    Skewness,
    Kurtosis,
    Moment,
    CentralMoment,
    Median,
    RandomVariate,
    Expectation,
    Estimated,
    Histogram,
}

/// Name, function, description.
pub(crate) const TABLE: &[(&str, Fun, &str)] = &[
    ("pdf", Fun::Pdf, "pdf(d, x): the probability density of the distribution d at x (the probability of x for a discrete d); a list x maps"),
    ("cdf", Fun::Cdf, "cdf(d, x): the distribution function P(X <= x) of the distribution d; a list x maps"),
    ("survival", Fun::Survival, "survival(d, x): the survival function P(X > x) = 1 - cdf(d, x), accurate in the upper tail"),
    (
        "quantile",
        Fun::Quantile,
        "quantile(d, q): the least x with cdf(d, x) >= q, 0 <= q <= 1; quantile(xs, q): the element of rank max(1, ceil(n q)) of the sorted data list xs; a list q maps",
    ),
    ("mean", Fun::Mean, "mean(d): the mean of the distribution d; mean(xs): the mean of a data list"),
    ("variance", Fun::Variance, "variance(d): the variance of the distribution d; variance(xs): the sample variance of a data list (divisor n - 1)"),
    ("std_dev", Fun::StdDev, "std_dev(d): the standard deviation of the distribution d; std_dev(xs): the sample standard deviation of a data list"),
    ("skewness", Fun::Skewness, "skewness(d): the skewness E[(X - mu)^3]/sigma^3 of a distribution, or m3/m2^(3/2) of a data list"),
    ("kurtosis", Fun::Kurtosis, "kurtosis(d): the kurtosis E[(X - mu)^4]/sigma^4 of a distribution (3 for a normal one), or m4/m2^2 of a data list"),
    ("moment", Fun::Moment, "moment(d, k): the k-th raw moment E[X^k] of a distribution; moment(xs, k): sum(xs^k)/n of a data list"),
    (
        "central_moment",
        Fun::CentralMoment,
        "central_moment(d, k): the k-th central moment E[(X - mu)^k] of a distribution; central_moment(xs, k): sum((xs - mean)^k)/n of a data list",
    ),
    ("median", Fun::Median, "median(d): quantile(d, 1/2); median(xs): the middle element of the sorted data list, or the mean of the two middle ones"),
    (
        "random_variate",
        Fun::RandomVariate,
        "random_variate(d): a random value of the distribution d from the session generator; random_variate(d, n) or random_variate(d, [n1, ...]): a list or nested lists",
    ),
    (
        "expectation",
        Fun::Expectation,
        "expectation(f, d): E[f(X)] for X distributed as d, f a function; expectation(e, x, d): E[e] for an expression e in x",
    ),
    (
        "estimated_distribution",
        Fun::Estimated,
        "estimated_distribution(xs, d): d with its symbolic parameters estimated from the data xs by maximum likelihood; method: moments for the method of moments",
    ),
    (
        "histogram_list",
        Fun::Histogram,
        "histogram_list(xs), histogram_list(xs, n), histogram_list(xs, [w]), histogram_list(xs, [a, b, w]), histogram_list(xs, [[e1, ...]]): [edges, counts] of the data xs in bins [e_i, e_{i+1})",
    ),
];

/// The native of a family constructor: checks numeric parameters, gives the standard member for no arguments.
pub(crate) struct Constructor {
    pub(crate) fam: Family,
}

impl Native for Constructor {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let b = B::new(h);
        let fam = self.fam;
        if args.is_empty() {
            return Ok(match fam.standard() {
                Some([a, z]) => {
                    let d = Dist { fam, p: vec![b.int(a), b.int(z)] };
                    Reply::Done(d.term(h))
                }
                None => Reply::Decline,
            });
        }
        let Some(d) = Dist::read(h, t) else {
            return Ok(match fam {
                Family::Uniform => Reply::Done(b.bad(fam.name(), "the argument must be an interval [a, b]")),
                _ => Reply::Decline,
            });
        };
        let ar = Arith::new(h);
        let nums: Vec<Option<Num>> = d.p.iter().map(|&p| ar.num(p)).collect();
        // a non-numeric parameter that cannot stand for a real (a list, a string)
        if d.p.iter().any(|&p| matches!(h.view(p), View::Str(_)) || cx.as_list(p).is_some()) {
            return Ok(Reply::Done(b.bad(fam.name(), "the parameters must be real numbers or symbolic expressions")));
        }
        Ok(match fam.check(&nums) {
            Ok(()) => Reply::Decline,
            Err(m) => Reply::Done(b.bad(fam.name(), m)),
        })
    }
}

/// The native of the other functions.
pub(crate) struct DistNative {
    pub(crate) f: Fun,
    pub(crate) name: &'static str,
    pub(crate) rng: Rng,
}

/// How the numeric arguments of a call are evaluated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// all exact numbers
    Exact,
    /// numbers, a ball among them (the least precision), no machine real
    Ball(u64),
    /// a machine real among the arguments
    Machine,
    /// some argument is not a number
    Symbolic,
}

fn mode(nums: &[Option<Num>]) -> Mode {
    let mut m = Mode::Exact;
    let mut symbolic = false;
    for n in nums {
        match n {
            Some(Num::Real(Real::Machine(_))) => return Mode::Machine,
            Some(Num::Real(Real::Ball(_, p))) => {
                m = match m {
                    Mode::Ball(q) => Mode::Ball(q.min(*p)),
                    _ => Mode::Ball(*p),
                }
            }
            Some(Num::Real(Real::Exact(_))) => {}
            _ => symbolic = true,
        }
    }
    if symbolic {
        Mode::Symbolic
    } else {
        m
    }
}

/// The value of t as a machine real: a real number, or a closed expression made numeric by `n`.
fn machine_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let num = match ar.num(t) {
        Some(v) => Some(v),
        None => {
            let v = cx.eval(h.call(h.sym("n"), &[t]))?;
            ar.num(v)
        }
    };
    Ok(match num {
        Some(Num::Real(r)) => Some(r.to_f64()).filter(|x| !x.is_nan()),
        _ => None,
    })
}

/// The distribution with machine parameters, when all parameters have machine values.
fn machine_dist<'h>(cx: &mut NativeCx<'_, '_, 'h>, d: &Dist<'h>) -> Res<Option<Mach>> {
    let mut p = Vec::with_capacity(2);
    for &t in &d.p {
        match machine_value(cx, t)? {
            Some(v) => p.push(v),
            None => return Ok(None),
        }
    }
    Ok(Mach::new(d.fam, &p))
}

/// The exact rational values of the parameters (also of balls with radius 0, such as `n(2, 30)`, so that an
/// integer shape keeps its elementary distribution function).
fn exact_params<'h>(ar: &Arith<'h, Heap>, d: &Dist<'h>) -> Exact {
    let q = |i: usize| match d.p.get(i).and_then(|&t| ar.num(t)) {
        Some(Num::Real(Real::Exact(q))) => Some(q),
        Some(Num::Real(Real::Ball(b, _))) if b.rad.is_zero() => Some(mc_numeric::value::float_rat(&b.mid)),
        _ => None,
    };
    [q(0), q(1)]
}

/// An exact integer as a term.
fn int_term<'a, 'h>(ar: &'a Arith<'h, Heap>) -> impl Fn(&Int) -> T<'h> + 'a {
    move |v: &Int| ar.integer(v)
}

/// The value of a machine count result: an integer, or `inf`.
fn count_term<'h>(b: B<'h>, k: f64) -> T<'h> {
    if k.is_finite() && k.abs() < 9.0e15 {
        b.int(k as i64)
    } else if k == f64::INFINITY {
        b.inf()
    } else {
        b.h.real(k)
    }
}

/// `failure(undefined, f, d)`: a moment that does not exist.
fn undefined<'h>(b: B<'h>, f: &str, d: T<'h>) -> T<'h> {
    b.call(names::FAILURE, &[b.sym("undefined"), b.sym(f), d])
}

/// The point functions of a distribution.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Point {
    Pdf,
    Cdf,
    Survival,
    Quantile,
}

/// The most elements of a `random_variate` result (as for the generator's own functions).
const MAX_ELEMENTS: u64 = 1 << 32;

impl DistNative {
    /// `pdf(d, x)`, `cdf(d, x)`, `survival(d, x)`, `quantile(d, q)`.
    fn point<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, which: Point, dt: T<'h>, x: T<'h>) -> Res<Reply<'h>> {
        let h = cx.heap();
        let b = B::new(h);
        let ar = Arith::new(h);
        let Some(d) = Dist::read(h, dt) else { return Ok(Reply::Decline) };
        // a packed array of machine reals: mapped in machine arithmetic
        if let Some(Packed::Reals { dims, data }) = h.packed(x) {
            if let Some(m) = machine_dist(cx, &d)? {
                let dims: Vec<usize> = dims.iter().map(|&n| n as usize).collect();
                if which == Point::Quantile && data.iter().any(|q| !(0.0..=1.0).contains(q)) {
                    return Ok(Reply::Done(b.bad(self.name, "the probability must be a real number in [0, 1]")));
                }
                cx.tick(data.len() as u64)?;
                let out: Vec<f64> = data.iter().map(|&v| self.machine_point(&m, which, v)).collect();
                if which == Point::Quantile && d.fam.is_discrete() {
                    return Ok(Reply::Done(build_nested(h, &dims, out.len(), |i| count_term(b, out[i]))));
                }
                return Ok(Reply::Done(h.real_array(&dims, &out)));
            }
        }
        if let Some(xs) = cx.as_list(x) {
            cx.tick(xs.len() as u64)?;
            let head = h.sym(self.name);
            let items: Vec<T<'h>> = xs.iter().map(|&xi| h.call(head, &[dt, xi])).collect();
            return Ok(Reply::Again(cx.list(&items)));
        }
        let mut nums: Vec<Option<Num>> = d.p.iter().map(|&p| ar.num(p)).collect();
        nums.push(ar.num(x));
        let md = mode(&nums);
        let xr = match nums.last() {
            Some(Some(Num::Real(r))) => Some(r.clone()),
            _ => None,
        };
        if which == Point::Quantile {
            if let Some(r) = &xr {
                let v = r.to_f64();
                if !(0.0..=1.0).contains(&v) {
                    return Ok(Reply::Done(b.bad(self.name, "the probability must be a real number in [0, 1]")));
                }
            }
        }
        // a machine real somewhere: the machine path when everything has a machine value
        let any_machine = md == Mode::Machine
            || (md == Mode::Symbolic && nums.iter().any(|n| matches!(n, Some(Num::Real(Real::Machine(_))))));
        if any_machine {
            if let (Some(m), Some(xv)) = (machine_dist(cx, &d)?, machine_value(cx, x)?) {
                cx.tick(1)?;
                let v = self.machine_point(&m, which, xv);
                return Ok(Reply::Done(if which == Point::Quantile && d.fam.is_discrete() {
                    count_term(b, v)
                } else {
                    h.real(v)
                }));
            }
        }
        let ex = exact_params(&ar, &d);
        let xq = match &xr {
            Some(Real::Exact(q)) => Some(q.clone()),
            _ => None,
        };
        let int = int_term(&ar);
        Ok(match which {
            Point::Pdf => Reply::Again(symbolic::pdf(b, &d, &ex, x, xq.as_ref())),
            Point::Cdf => Reply::Again(symbolic::cdf(b, &d, &ex, x, xq.as_ref(), &int)),
            Point::Survival => Reply::Again(symbolic::survival(b, &d, &ex, x, xq.as_ref(), &int)),
            Point::Quantile => {
                if let Some(t) = symbolic::quantile(b, &d, &ex, x, xq.as_ref()) {
                    return Ok(Reply::Again(t));
                }
                match md {
                    Mode::Exact if d.fam.is_discrete() => exact_count_quantile(cx, &d, &ex, x)?,
                    Mode::Ball(p) if !d.fam.is_discrete() => ball_quantile(cx, &d, &ex, x, p)?,
                    Mode::Ball(_) => match machine_dist(cx, &d)? {
                        // a discrete quantile is an integer: the machine candidate confirmed by the ball comparisons
                        Some(_) => exact_count_quantile(cx, &d, &ex, x)?,
                        None => Reply::Decline,
                    },
                    _ => Reply::Decline,
                }
            }
        })
    }

    fn machine_point(&self, m: &Mach, which: Point, x: f64) -> f64 {
        match which {
            Point::Pdf => m.pdf(x),
            Point::Cdf => m.cdf(x).0,
            Point::Survival => m.cdf(x).1,
            Point::Quantile => m.quantile(x),
        }
    }

    /// `mean(d)`, ... of a distribution.
    fn moment_stat<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, dt: T<'h>, s: Option<Stat>, k: Option<T<'h>>) -> Res<Reply<'h>> {
        let h = cx.heap();
        let b = B::new(h);
        let ar = Arith::new(h);
        let Some(d) = Dist::read(h, dt) else { return Ok(Reply::Decline) };
        // the degrees of freedom of Student's t, when numeric: which moments exist
        let n = match (d.fam, d.p.first().and_then(|&t| ar.num(t))) {
            (Family::StudentT, Some(Num::Real(r))) => Some(r.to_f64()),
            _ => None,
        };
        if self.f == Fun::StdDev {
            if let Some(t) = symbolic::std_dev(b, &d) {
                return Ok(Reply::Again(t));
            }
        }
        let r = match (s, k) {
            (Some(s), _) => symbolic::stat(b, &d, s, n),
            (None, Some(k)) => {
                let Some(k) = (match h.view(k) {
                    View::Int(v) if (0..=symbolic::MAX_TERMS).contains(&v) => Some(v),
                    _ => None,
                }) else {
                    return Ok(match h.view(k) {
                        View::Int(_) => Reply::Done(b.bad(self.name, "the order must be an integer from 0 to 200")),
                        // a symbolic order: the closed forms of the families that have one
                        _ => match symbolic_moment(b, &d, k) {
                            Some(t) => Reply::Again(t),
                            None => Reply::Decline,
                        },
                    });
                };
                symbolic::raw_moment(b, &d, k, n, &int_term(&ar))
            }
            _ => return Ok(Reply::Decline),
        };
        Ok(match r {
            Ok(t) => Reply::Again(if self.f == Fun::StdDev { b.sqrt(t) } else { t }),
            Err(true) => Reply::Done(b.inf()),
            Err(false) => Reply::Done(undefined(b, self.name, dt)),
        })
    }

    /// `central_moment(d, k)`: 1, 0, the variance, and for k >= 3 the binomial expansion in the raw moments
    /// (expanded), through the moment statistics for k = 3 and 4.
    fn central<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, dt: T<'h>, k: T<'h>) -> Res<Reply<'h>> {
        let h = cx.heap();
        let b = B::new(h);
        if Dist::read(h, dt).is_none() {
            return Ok(Reply::Decline);
        }
        let k = match h.view(k) {
            View::Int(v) if (0..=symbolic::MAX_TERMS).contains(&v) => v,
            View::Int(_) => return Ok(Reply::Done(b.bad(self.name, "the order must be an integer from 0 to 200"))),
            _ => return Ok(Reply::Decline),
        };
        let var = || b.call("variance", &[dt]);
        Ok(Reply::Again(match k {
            0 => b.int(1),
            1 => b.int(0),
            2 => var(),
            3 => b.mul(&[b.call("skewness", &[dt]), b.pow(var(), b.rat(3, 2))]),
            4 => b.mul(&[b.call("kurtosis", &[dt]), b.powi(var(), 2)]),
            _ => {
                // sum_j C(k, j) E[X^j] (-mu)^(k - j)
                let mu = b.call("mean", &[dt]);
                let terms: Vec<T<'h>> = (0..=k)
                    .map(|j| {
                        b.mul(&[
                            b.call("binomial", &[b.int(k), b.int(j)]),
                            b.call("moment", &[dt, b.int(j)]),
                            b.powi(b.neg(mu), k - j),
                        ])
                    })
                    .collect();
                b.call("expand", &[b.add(&terms)])
            }
        }))
    }

    /// `random_variate(d)`, `random_variate(d, n)`, `random_variate(d, [n1, ...])`.
    fn variate<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let b = B::new(h);
        let (dt, count) = match args {
            [d] => (*d, None),
            [d, n] => (*d, Some(*n)),
            _ => return Ok(Reply::Decline),
        };
        let Some(d) = Dist::read(h, dt) else { return Ok(Reply::Decline) };
        let Some(m) = machine_dist(cx, &d)? else {
            return Ok(Reply::Done(b.bad(self.name, "the parameters of the distribution must be numeric")));
        };
        let dims = match count {
            None => None,
            Some(c) => match shape(cx, c) {
                Ok(s) => Some(s),
                Err(msg) => return Ok(Reply::Done(b.bad(self.name, msg))),
            },
        };
        let n: usize = dims.as_ref().map_or(1, |d| d.iter().product());
        // decision DI.6: a draw is an effect, announced before the generator is touched
        cx.effect()?;
        let mut v = Vec::new();
        if v.try_reserve_exact(n).is_err() {
            v.reserve(n.min(1 << 20));
        }
        {
            let mut g = self.rng.lock();
            let mut draws = crate::sample::Draws::new(g.stream());
            for _ in 0..n {
                cx.tick(1)?;
                v.push(draws.variate(&m));
            }
        }
        let discrete = d.fam.is_discrete();
        Ok(Reply::Done(match (&dims, discrete) {
            (None, false) => h.real(v[0]),
            (None, true) => count_term(b, v[0]),
            (Some(dd), false) => h.real_array(dd, &v),
            (Some(dd), true) => {
                if v.iter().all(|x| x.abs() < 9.0e15) {
                    let ints: Vec<i64> = v.iter().map(|&x| x as i64).collect();
                    h.int_array(dd, &ints)
                } else {
                    build_nested(h, dd, n, |i| count_term(b, v[i]))
                }
            }
        }))
    }
}

/// The shape of a count argument: `n` or `[n1, ..., nk]`, natural numbers with at most 2^32 elements in all.
fn shape<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Result<Vec<usize>, &'static str> {
    let h = cx.heap();
    let one = |t: T<'h>| match h.view(t) {
        View::Int(v) if v >= 0 && (v as u64) <= MAX_ELEMENTS => Ok(v as usize),
        _ => Err("the count must be a natural number, at most 2^32, or a list of them"),
    };
    let dims: Vec<usize> = match cx.as_list(t) {
        Some(items) if !items.is_empty() => items.iter().map(|&x| one(x)).collect::<Result<_, _>>()?,
        Some(_) => return Err("the list of dimensions is empty"),
        None => vec![one(t)?],
    };
    let total = dims.iter().fold(1u64, |a, &d| a.saturating_mul(d as u64));
    if total > MAX_ELEMENTS {
        return Err("more than 2^32 elements");
    }
    Ok(dims)
}

/// The least integer k with cdf(d, k) >= q for exact (or ball) arguments: the machine quantile as a candidate,
/// then exact comparisons of the exact distribution function, stepping k by one until it is the least.
fn exact_count_quantile<'h>(cx: &mut NativeCx<'_, '_, 'h>, d: &Dist<'h>, ex: &Exact, q: T<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let b = B::new(h);
    let ar = Arith::new(h);
    let Some(m) = machine_dist(cx, d)? else { return Ok(Reply::Decline) };
    let Some(qv) = machine_value(cx, q)? else { return Ok(Reply::Decline) };
    let k0 = m.quantile(qv);
    if !k0.is_finite() || k0 > 1e15 {
        return Ok(Reply::Decline);
    }
    let int = int_term(&ar);
    // cdf(d, k) >= q, decided exactly (`None`: undecided)
    let reached = |cx: &mut NativeCx<'_, '_, 'h>, k: i64| -> Res<Option<bool>> {
        let kq = Rat::from_i64(k);
        let c = symbolic::cdf(b, d, ex, b.int(k), Some(&kq), &int);
        let v = cx.eval(b.call("greater_equal", &[c, q]))?;
        Ok(if cx.is_true(v) {
            Some(true)
        } else if cx.is_false(v) {
            Some(false)
        } else {
            None
        })
    };
    let mut k = k0 as i64;
    for _ in 0..64 {
        cx.tick(1)?;
        match reached(cx, k)? {
            None => return Ok(Reply::Decline),
            Some(true) => {
                if k == 0 {
                    return Ok(Reply::Done(b.int(0)));
                }
                match reached(cx, k - 1)? {
                    None => return Ok(Reply::Decline),
                    Some(true) => k -= 1,
                    Some(false) => return Ok(Reply::Done(b.int(k))),
                }
            }
            Some(false) => k += 1,
        }
    }
    Ok(Reply::Decline)
}

/// A continuous quantile without a closed form at a ball probability (precision p bits): Newton steps
/// x <- x - (F(x) - q)/f(x) with F and f the exact forms evaluated by `n` at an exact x, starting from the machine
/// quantile, the working precision doubling per step (quadratic convergence) up to p + 32 bits. The result is
/// a ball around the last iterate with the size of the last step as its radius (an error estimate, not a proven
/// bound: decision DI.5). When `n` cannot evaluate the distribution function (an incomplete function not yet
/// numeric) the call stays.
fn ball_quantile<'h>(cx: &mut NativeCx<'_, '_, 'h>, d: &Dist<'h>, ex: &Exact, q: T<'h>, p: u64) -> Res<Reply<'h>> {
    let h = cx.heap();
    let b = B::new(h);
    let ar = Arith::new(h);
    let Some(m) = machine_dist(cx, d)? else { return Ok(Reply::Decline) };
    let Some(qv) = machine_value(cx, q)? else { return Ok(Reply::Decline) };
    let x0 = m.quantile(qv);
    if !x0.is_finite() {
        return Ok(Reply::Decline);
    }
    let Some(Num::Real(qr)) = ar.num(q) else { return Ok(Reply::Decline) };
    let Some((neg, man, e)) = mc_arith::num::float::f64_to_dyadic(x0) else { return Ok(Reply::Decline) };
    let mut x = dyadic_rat(neg, man, e);
    let int = int_term(&ar);
    // ball parameters carry their own precision (n cannot raise it): evaluate the forms as they are, at p bits
    let inexact = d.p.iter().any(|&t| matches!(ar.num(t), Some(Num::Real(Real::Ball(..)))));
    let target = if inexact { p } else { p + 32 };
    let mut bits = if inexact { p } else { 60u64 };
    let mut last_step = None;
    for _ in 0..40 {
        cx.tick(1 + bits / 64)?;
        let digits = (bits as f64 * std::f64::consts::LOG10_2).ceil() as i64 + 10;
        let xt = ar.number(&Num::Real(Real::Exact(x.clone())));
        let f = symbolic::cdf(b, d, ex, xt, Some(&x), &int);
        let dens = symbolic::pdf(b, d, ex, xt, Some(&x));
        let numeric = |t: T<'h>| if inexact { t } else { b.call("n", &[t, b.int(digits)]) };
        let fv = cx.eval(numeric(f))?;
        let dv = cx.eval(numeric(dens))?;
        let (Some(Num::Real(fr)), Some(Num::Real(dr))) = (ar.num(fv), ar.num(dv)) else { return Ok(Reply::Decline) };
        let resid = fr.sub(&qr.to_kind(mc_arith::Kind::Ball, bits.max(qr.prec().unwrap_or(bits))));
        let Ok(step) = resid.div(&dr) else { return Ok(Reply::Decline) };
        let step_q = match &step {
            Real::Ball(s, _) => mc_numeric::value::float_rat(&s.mid),
            Real::Exact(s) => s.clone(),
            Real::Machine(_) => return Ok(Reply::Decline),
        };
        x = x.sub(&step_q);
        let size = step.to_f64().abs();
        last_step = Some(size);
        let scale = x_scale(&x);
        if bits >= target && size <= scale * 2f64.powi(-(target as i32 - 8)) {
            break;
        }
        bits = (bits * 2).min(target);
    }
    let rad = last_step.unwrap_or(0.0) * 4.0 + x_scale(&x) * 2f64.powi(-(p as i32));
    let mid = ar.number(&Num::Real(Real::Exact(x)));
    let Some((rn, rm, re)) = mc_arith::num::float::f64_to_dyadic(rad) else { return Ok(Reply::Decline) };
    let radt = ar.number(&Num::Real(Real::Exact(dyadic_rat(rn, rm, re))));
    Ok(match ar.ball_with_radius(mid, radt, p) {
        Some(t) => Reply::Done(t),
        None => Reply::Decline,
    })
}

/// The magnitude of a rational (at least the smallest normal double), for relative tolerances.
fn x_scale(x: &Rat) -> f64 {
    let v = mc_arith::num::float::ratio_to_f64(x.num.neg, &x.num.mag, &x.den).abs();
    v.max(f64::MIN_POSITIVE)
}

/// The rational value of the dyadic (-1)^neg man 2^e.
fn dyadic_rat(neg: bool, man: Nat, e: i64) -> Rat {
    let num = Int::new(neg, man);
    if e >= 0 {
        Rat::from_int(num.shl(e as u64))
    } else {
        Rat::new(num, Int::from_i64(1).shl(e.unsigned_abs()))
    }
}

/// The columns of a data matrix: a list of rows that are lists of one length (at least 1), or a packed array of
/// rank 2. `None` for anything else (a flat data list among them).
fn columns<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<T<'h>>> {
    let h = cx.heap();
    if let Some(Packed::Reals { dims: &[n, m], data }) = h.packed(t) {
        let (n, m) = (n as usize, m as usize);
        return Some((0..m).map(|j| h.reals(&(0..n).map(|i| data[i * m + j]).collect::<Vec<f64>>())).collect());
    }
    let rows = cx.as_list(t)?;
    let first = cx.as_list(*rows.first()?)?;
    let m = first.len();
    if m == 0 {
        return None;
    }
    let mut cols: Vec<Vec<T<'h>>> = vec![Vec::with_capacity(rows.len()); m];
    for &r in rows {
        let r = cx.as_list(r).filter(|r| r.len() == m)?;
        for (c, &x) in cols.iter_mut().zip(r) {
            c.push(x);
        }
    }
    Some(cols.iter().map(|c| cx.list(c)).collect())
}

/// E[X^k] for a symbolic order k, where the family has a closed form.
fn symbolic_moment<'h>(b: B<'h>, d: &Dist<'h>, k: T<'h>) -> Option<T<'h>> {
    let p = &d.p;
    let one = b.int(1);
    Some(match d.fam {
        Family::ChiSquare => b.mul(&[b.pow(b.int(2), k), b.call("pochhammer", &[b.mul(&[b.rat(1, 2), p[0]]), k])]),
        Family::Gamma => b.mul(&[b.pow(p[1], k), b.call("pochhammer", &[p[0], k])]),
        Family::Exponential => b.mul(&[b.call("factorial", &[k]), b.pow(p[0], b.neg(k))]),
        Family::Beta => b.div(b.call("pochhammer", &[p[0], k]), b.call("pochhammer", &[b.add(&[p[0], p[1]]), k])),
        Family::LogNormal => b.exp(b.add(&[b.mul(&[k, p[0]]), b.mul(&[b.rat(1, 2), b.powi(k, 2), b.powi(p[1], 2)])])),
        Family::Uniform => {
            let k1 = b.add(&[k, one]);
            b.div(b.sub(b.pow(p[1], k1), b.pow(p[0], k1)), b.mul(&[k1, b.sub(p[1], p[0])]))
        }
        _ => return None,
    })
}

/// The (key, column) pairs of a dataset term `dataset(assoc(key -> column, ...))` (`None` for anything else).
fn dataset_columns<'h>(h: &'h Heap, t: T<'h>) -> Option<Vec<(T<'h>, T<'h>)>> {
    let View::App { head, args: &[a] } = h.view(t) else { return None };
    if head != h.sym_term(h.sym("dataset")) {
        return None;
    }
    let View::App { head, args: rules } = h.view(a) else { return None };
    if head != h.sym_term(h.sym("assoc")) {
        return None;
    }
    let rule = h.sym_term(h.sym(names::RULE));
    rules
        .iter()
        .map(|&r| match h.view(r) {
            View::App { head, args: &[k, v] } if head == rule => Some((k, v)),
            _ => None,
        })
        .collect()
}

impl Native for DistNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        // a statistic of a data matrix is the list of the statistics of its columns (DI.12)
        let column_wise = matches!(
            self.f,
            Fun::Mean | Fun::Variance | Fun::StdDev | Fun::Skewness | Fun::Kurtosis | Fun::Median | Fun::Quantile | Fun::Moment | Fun::CentralMoment
        );
        if let (true, Some(&xs)) = (column_wise, args.first()) {
            if let Some(cols) = columns(cx, xs) {
                cx.tick(cols.len() as u64)?;
                let b = B::new(h);
                let calls: Vec<T<'h>> = cols
                    .into_iter()
                    .map(|c| {
                        let mut a = vec![c];
                        a.extend_from_slice(&args[1..]);
                        b.call(self.name, &a)
                    })
                    .collect();
                return Ok(Reply::Again(cx.list(&calls)));
            }
            // rows of different lengths are not data
            if cx.as_list(xs).is_some_and(|items| items.iter().any(|&x| cx.as_list(x).is_some())) {
                return Ok(Reply::Decline);
            }
        }
        // a statistic of a dataset (columns): the association of the statistics of its columns
        if let (true, Some(&ds)) = (column_wise, args.first()) {
            if let Some(cols) = dataset_columns(h, ds) {
                cx.tick(cols.len() as u64)?;
                let b = B::new(h);
                let rule = h.sym(names::RULE);
                let parts: Vec<T<'h>> = cols
                    .iter()
                    .map(|&(k, col)| {
                        let mut a = vec![col];
                        a.extend_from_slice(&args[1..]);
                        h.call(rule, &[k, b.call(self.name, &a)])
                    })
                    .collect();
                return Ok(Reply::Again(b.call("assoc", &parts)));
            }
        }
        // a statistic of quantities: of their magnitudes in one unit, then in the unit to the statistic's power
        let power = match self.f {
            Fun::Mean | Fun::StdDev | Fun::Median | Fun::Quantile => Some(1),
            Fun::Variance => Some(2),
            Fun::Skewness | Fun::Kurtosis => Some(0),
            Fun::Moment | Fun::CentralMoment => match args.get(1).map(|&k| h.view(k)) {
                Some(View::Int(k)) => Some(k),
                _ => None,
            },
            _ => None,
        };
        if let (Some(p), Some(&xs)) = (power, args.first()) {
            if let Some((mags, u)) = data::quantities(cx, xs)? {
                let b = B::new(h);
                let unit = b.pow(b.call("quantity", &[b.int(1), u]), b.int(p));
                let qs: Vec<T<'h>> = match (self.f, args.get(1).and_then(|&q| cx.as_list(q))) {
                    (Fun::Quantile, Some(qs)) => qs.to_vec(),
                    _ => vec![args.get(1).copied().unwrap_or(mags)],
                };
                let one = |q: T<'h>| {
                    let mut a = vec![mags];
                    if args.len() > 1 {
                        a.push(q);
                    }
                    b.mul(&[b.call(self.name, &a), unit])
                };
                let out: Vec<T<'h>> = qs.iter().map(|&q| one(q)).collect();
                return Ok(Reply::Again(match (self.f, args.get(1).and_then(|&q| cx.as_list(q))) {
                    (Fun::Quantile, Some(_)) => cx.list(&out),
                    _ => out[0],
                }));
            }
        }
        let wd = h.sym_term(h.sym("weighted_data"));
        let is_data = |t: T<'h>| cx.as_list(t).is_some() || h.packed(t).is_some() || (self.f == Fun::Median && matches!(h.view(t), View::App { head, .. } if head == wd));
        match (self.f, args) {
            (Fun::Pdf, &[d, x]) => self.point(cx, Point::Pdf, d, x),
            (Fun::Cdf, &[d, x]) => self.point(cx, Point::Cdf, d, x),
            (Fun::Survival, &[d, x]) => self.point(cx, Point::Survival, d, x),
            (Fun::Quantile, &[xs, q]) if is_data(xs) => data::quantile(cx, self.name, xs, q),
            (Fun::Quantile, &[d, q]) => self.point(cx, Point::Quantile, d, q),
            (Fun::Median, &[xs]) if is_data(xs) => data::median(cx, self.name, xs),
            (Fun::Median, &[d]) => {
                if Dist::read(h, d).is_none() {
                    return Ok(Reply::Decline);
                }
                let b = B::new(h);
                Ok(Reply::Again(b.call("quantile", &[d, b.rat(1, 2)])))
            }
            (Fun::Mean | Fun::Variance | Fun::StdDev | Fun::Skewness | Fun::Kurtosis, &[xs]) if is_data(xs) => {
                let Some(dat) = data::read(cx, xs) else { return Ok(Reply::Decline) };
                let s = match self.f {
                    Fun::Mean => DataStat::Mean,
                    Fun::Variance => DataStat::Variance,
                    Fun::StdDev => DataStat::StdDev,
                    Fun::Skewness => DataStat::Skewness,
                    _ => DataStat::Kurtosis,
                };
                data::stat(cx, self.name, s, xs, dat)
            }
            (Fun::Moment, &[xs, k]) if is_data(xs) => {
                let View::Int(k) = h.view(k) else { return Ok(Reply::Decline) };
                if k < 0 {
                    return Ok(Reply::Done(B::new(h).bad(self.name, "the order must be a natural number")));
                }
                let Some(dat) = data::read(cx, xs) else { return Ok(Reply::Decline) };
                data::stat(cx, self.name, DataStat::Moment(k), xs, dat)
            }
            (Fun::CentralMoment, &[xs, k]) if is_data(xs) => {
                let View::Int(k) = h.view(k) else { return Ok(Reply::Decline) };
                if k < 0 {
                    return Ok(Reply::Done(B::new(h).bad(self.name, "the order must be a natural number")));
                }
                let Some(dat) = data::read(cx, xs) else { return Ok(Reply::Decline) };
                data::stat(cx, self.name, DataStat::Central(k), xs, dat)
            }
            (Fun::CentralMoment, &[d, k]) => self.central(cx, d, k),
            (Fun::Mean, &[d]) => self.moment_stat(cx, d, Some(Stat::Mean), None),
            (Fun::Variance | Fun::StdDev, &[d]) => self.moment_stat(cx, d, Some(Stat::Variance), None),
            (Fun::Skewness, &[d]) => self.moment_stat(cx, d, Some(Stat::Skewness), None),
            (Fun::Kurtosis, &[d]) => self.moment_stat(cx, d, Some(Stat::Kurtosis), None),
            (Fun::Moment, &[d, k]) => self.moment_stat(cx, d, None, Some(k)),
            (Fun::RandomVariate, _) => self.variate(cx, args),
            (Fun::Expectation, _) => expect::expectation(cx, self.name, args),
            (Fun::Estimated, _) => fit::estimated(cx, self.name, args),
            (Fun::Histogram, _) => data::histogram(cx, self.name, args),
            _ => Ok(Reply::Decline),
        }
    }
}
