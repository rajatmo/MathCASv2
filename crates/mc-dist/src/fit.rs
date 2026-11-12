//! `estimated_distribution(xs, d)`: the parameters of d that are not numbers estimated from the data xs
//! (decision DI.9).
//!
//! Maximum likelihood by default; `method: moments` matches the mean and the sample variance (divisor n - 1)
//! instead.
//! Closed forms are used where they exist and stay exact for exact data: the normal distribution (sigma from the
//! divisor n), log-normal (on the logarithms), uniform ([min, max]), exponential (1/mean), Poisson (the mean),
//! geometric (1/(1 + mean)), binomial with a known number of trials (mean/n), and every family by moments. The
//! likelihood equations without closed forms are solved numerically on machine reals: gamma (Newton on
//! ln k - psi(k) = ln mean - mean(ln x), from the Choi-Wette start), chi-square (Newton on psi(k/2) = mean(ln x) -
//! ln 2), beta (two-dimensional Newton on the digamma equations from the moment estimates, steps halved to keep
//! the parameters positive), Student t (a golden-section search of the log-likelihood over ln n).
//!
//! Of the normal distribution either parameter may be known; of the binomial distribution the number of trials
//! must be known; other families need all their parameters estimated.
use crate::build::B;
use crate::data::{self, Data};
use crate::family::{Dist, Family};
use crate::special::{digamma, lgamma, trigamma};
use mc_api::{names, Store, View, T};
use mc_arith::{Arith, Num};
use mc_eval::{NativeCx, Reply, Res};

/// The estimation method.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Method {
    Likelihood,
    Moments,
}

/// The method of a trailing `options(method -> m)` argument (`None`: an unknown option or value).
fn method<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Method> {
    let h = cx.heap();
    let View::App { head, args } = h.view(t) else { return None };
    if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "options") {
        return None;
    }
    let mut m = Method::Likelihood;
    for &r in args {
        let View::App { head, args: &[k, v] } = h.view(r) else { return None };
        let (View::Sym(rs), View::Sym(ks), View::Sym(vs)) = (h.view(head), h.view(k), h.view(v)) else { return None };
        if h.sym_name(rs) != names::RULE || h.sym_name(ks) != "method" {
            return None;
        }
        m = match h.sym_name(vs).as_str() {
            "moments" => Method::Moments,
            "likelihood" | "maximum_likelihood" => Method::Likelihood,
            _ => return None,
        };
    }
    Some(m)
}

/// `estimated_distribution(xs, d)`, `estimated_distribution(xs, d, method: m)`.
pub fn estimated<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let b = B::new(h);
    let fail = |m: &str| Ok(Reply::Done(b.bad(name, m)));
    let (xs, dt, how) = match *args {
        [xs, d] => (xs, d, Method::Likelihood),
        [xs, d, o] => match method(cx, o) {
            Some(m) => (xs, d, m),
            None => return fail("the option is method: likelihood or method: moments"),
        },
        _ => return Ok(Reply::Decline),
    };
    let Some(d) = Dist::read(h, dt) else {
        return fail("the second argument must be a distribution whose unknown parameters are symbols");
    };
    let Some(dat) = data::read(cx, xs) else { return fail("the data must be a list of real numbers") };
    let ar = Arith::new(h);
    let known: Vec<bool> = d.p.iter().map(|&p| ar.num(p).is_some()).collect();
    if known.iter().all(|&k| k) {
        return Ok(Reply::Again(dt));
    }
    let n = match &dat {
        Data::Machine(v) => v.len(),
        Data::Terms(v) => {
            if v.iter().any(|&x| !matches!(ar.num(x), Some(Num::Real(_)))) {
                return fail("the data must be a list of real numbers");
            }
            v.len()
        }
    };
    if n < 2 {
        return fail("the data needs at least two values");
    }
    cx.tick(n as u64)?;
    let partial_ok = matches!(d.fam, Family::Normal) || (d.fam == Family::Binomial && known[0] && !known[1]);
    if known.iter().any(|&k| k) && !partial_ok {
        return fail("only all parameters of this family can be estimated together (the trials of a binomial distribution must be known)");
    }
    if d.fam == Family::Binomial && !known[0] {
        return fail("the number of trials of a binomial distribution must be given");
    }
    let closed = how == Method::Moments
        || matches!(
            d.fam,
            Family::Normal | Family::LogNormal | Family::Uniform | Family::Exponential | Family::Poisson | Family::Geometric | Family::Binomial
        );
    let exact = matches!(dat, Data::Terms(_));
    if closed && exact {
        return closed_form(cx, &d, &known, xs, n, how);
    }
    // machine data (or exact data for a numeric likelihood)
    let v: Vec<f64> = match dat {
        Data::Machine(v) => v,
        Data::Terms(items) => items
            .iter()
            .map(|&x| match ar.num(x) {
                Some(Num::Real(r)) => r.to_f64(),
                _ => f64::NAN,
            })
            .collect(),
    };
    let ps = match numeric(&d, &known, &v, how, &|t| match ar.num(t) {
        Some(Num::Real(r)) => r.to_f64(),
        _ => f64::NAN,
    }) {
        Ok(ps) => ps,
        Err(m) => return fail(m),
    };
    let p: Vec<T<'h>> = d.p.iter().zip(&ps).zip(&known).map(|((&t, &v), &k)| if k { t } else { h.real(v) }).collect();
    Ok(Reply::Again(Dist { fam: d.fam, p }.term(h)))
}

/// The closed-form estimates as formulas over the data list, evaluated exactly.
fn closed_form<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    d: &Dist<'h>,
    known: &[bool],
    xs: T<'h>,
    n: usize,
    how: Method,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    let b = B::new(h);
    let nn = b.int(n as i64);
    let one = b.int(1);
    let mean_of = |t: T<'h>| b.div(b.call("sum", &[t]), nn);
    let m = cx.eval(mean_of(xs))?;
    // the variance: divisor n for the likelihood (its estimate), n - 1 for the method of moments
    let div = if how == Method::Moments { b.int(n as i64 - 1) } else { nn };
    let v = cx.eval(b.div(b.call("sum", &[b.powi(b.sub(xs, m), 2)]), div))?;
    let p = &d.p;
    let est: Vec<T<'h>> = match (d.fam, how) {
        (Family::Normal, _) => match (known[0], known[1]) {
            (true, false) => vec![p[0], b.sqrt(mean_of(b.powi(b.sub(xs, p[0]), 2)))],
            (false, true) => vec![m, p[1]],
            _ => vec![m, b.sqrt(v)],
        },
        (Family::LogNormal, Method::Likelihood) => {
            let ls = b.call("log", &[xs]);
            let lm = cx.eval(mean_of(ls))?;
            vec![lm, b.sqrt(mean_of(b.powi(b.sub(ls, lm), 2)))]
        }
        (Family::LogNormal, Method::Moments) => {
            let s2 = cx.eval(b.log(b.add(&[one, b.div(v, b.powi(m, 2))])))?;
            vec![b.sub(b.log(m), b.mul(&[b.rat(1, 2), s2])), b.sqrt(s2)]
        }
        (Family::Uniform, Method::Likelihood) => vec![b.call("min", &[xs]), b.call("max", &[xs])],
        (Family::Uniform, Method::Moments) => {
            let w = b.sqrt(b.mul(&[b.int(3), v]));
            vec![b.sub(m, w), b.add(&[m, w])]
        }
        (Family::Exponential, _) => vec![b.powi(m, -1)],
        (Family::Poisson, _) => vec![m],
        (Family::Geometric, _) => vec![b.powi(b.add(&[one, m]), -1)],
        (Family::Binomial, _) => vec![p[0], b.div(m, p[0])],
        (Family::Gamma, _) => vec![b.div(b.powi(m, 2), v), b.div(v, m)],
        (Family::ChiSquare, _) => vec![m],
        (Family::Beta, _) => {
            let c = b.sub(b.div(b.mul(&[m, b.sub(one, m)]), v), one);
            vec![b.mul(&[m, c]), b.mul(&[b.sub(one, m), c])]
        }
        (Family::StudentT, _) => {
            // the variance n/(n - 2) of a t distribution is v: n = 2v/(v - 1)
            vec![b.div(b.mul(&[b.int(2), v]), b.sub(v, one))]
        }
    };
    Ok(Reply::Again(Dist { fam: d.fam, p: est }.term(h)))
}

/// Mean and variance of machine data (divisor n, or n - 1 when `sample`).
fn mean_var(v: &[f64], sample: bool) -> (f64, f64) {
    let m = data::mean(v);
    let div = if sample { v.len() as f64 - 1.0 } else { v.len() as f64 };
    let var = v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / div;
    (m, var)
}

/// The estimates in machine arithmetic: the parameters in order (known ones from `value`).
fn numeric<'h>(
    d: &Dist<'h>,
    known: &[bool],
    v: &[f64],
    how: Method,
    value: &dyn Fn(T<'h>) -> f64,
) -> Result<Vec<f64>, &'static str> {
    let n = v.len() as f64;
    let (m, var) = mean_var(v, how == Method::Moments);
    let positive = || if v.iter().all(|&x| x > 0.0) { Ok(()) } else { Err("the data must be positive for this family") };
    let mean_ln = || v.iter().map(|x| x.ln()).sum::<f64>() / n;
    Ok(match (d.fam, how) {
        (Family::Normal, _) => match (known[0], known[1]) {
            (true, false) => {
                let mu = value(d.p[0]);
                vec![mu, (v.iter().map(|x| (x - mu) * (x - mu)).sum::<f64>() / n).sqrt()]
            }
            (false, true) => vec![m, value(d.p[1])],
            _ => vec![m, var.sqrt()],
        },
        (Family::LogNormal, Method::Likelihood) => {
            positive()?;
            let lm = mean_ln();
            let lv = v.iter().map(|x| (x.ln() - lm).powi(2)).sum::<f64>() / n;
            vec![lm, lv.sqrt()]
        }
        (Family::LogNormal, Method::Moments) => {
            let s2 = (1.0 + var / (m * m)).ln();
            vec![m.ln() - s2 / 2.0, s2.sqrt()]
        }
        (Family::Uniform, Method::Likelihood) => {
            vec![v.iter().copied().fold(f64::INFINITY, f64::min), v.iter().copied().fold(f64::NEG_INFINITY, f64::max)]
        }
        (Family::Uniform, Method::Moments) => vec![m - (3.0 * var).sqrt(), m + (3.0 * var).sqrt()],
        (Family::Exponential, _) => vec![1.0 / m],
        (Family::Poisson, _) => vec![m],
        (Family::Geometric, _) => vec![1.0 / (1.0 + m)],
        (Family::Binomial, _) => {
            let trials = value(d.p[0]);
            vec![trials, m / trials]
        }
        (Family::Gamma, Method::Moments) => vec![m * m / var, var / m],
        (Family::Gamma, Method::Likelihood) => {
            positive()?;
            let s = m.ln() - mean_ln();
            if s.is_nan() || s <= 0.0 {
                return Err("the data must not all be equal");
            }
            // Choi-Wette start, then Newton on ln k - psi(k) = s
            let mut k = (3.0 - s + ((s - 3.0) * (s - 3.0) + 24.0 * s).sqrt()) / (12.0 * s);
            for _ in 0..100 {
                let g = k.ln() - digamma(k) - s;
                let dg = 1.0 / k - trigamma(k);
                let next = k - g / dg;
                let next = if next > 0.0 { next } else { k / 2.0 };
                if (next - k).abs() <= 1e-15 * k {
                    k = next;
                    break;
                }
                k = next;
            }
            vec![k, m / k]
        }
        (Family::ChiSquare, Method::Moments) => vec![m],
        (Family::ChiSquare, Method::Likelihood) => {
            positive()?;
            // psi(h) = mean(ln x) - ln 2, h = k/2
            let c = mean_ln() - std::f64::consts::LN_2;
            let mut hk = c.exp() + 0.5;
            for _ in 0..200 {
                let next = hk - (digamma(hk) - c) / trigamma(hk);
                let next = if next > 0.0 { next } else { hk / 2.0 };
                if (next - hk).abs() <= 1e-15 * hk {
                    hk = next;
                    break;
                }
                hk = next;
            }
            vec![2.0 * hk]
        }
        (Family::Beta, how) => {
            if !v.iter().all(|&x| x > 0.0 && x < 1.0) {
                return Err("the data must lie in (0, 1) for a beta distribution");
            }
            let c = m * (1.0 - m) / var - 1.0;
            let (mut a, mut bb) = if c > 0.0 { (m * c, (1.0 - m) * c) } else { (1.0, 1.0) };
            if how == Method::Likelihood {
                let l1 = mean_ln();
                let l2 = v.iter().map(|x| (-x).ln_1p()).sum::<f64>() / n;
                for _ in 0..200 {
                    let (pab, tab) = (digamma(a + bb), trigamma(a + bb));
                    let g1 = digamma(a) - pab - l1;
                    let g2 = digamma(bb) - pab - l2;
                    let (j11, j12, j22) = (trigamma(a) - tab, -tab, trigamma(bb) - tab);
                    let det = j11 * j22 - j12 * j12;
                    let da = (g1 * j22 - g2 * j12) / det;
                    let db = (g2 * j11 - g1 * j12) / det;
                    // halve the step until both stay positive
                    let mut t = 1.0;
                    while (a - t * da <= 0.0 || bb - t * db <= 0.0) && t > 1e-10 {
                        t /= 2.0;
                    }
                    let (na, nb) = (a - t * da, bb - t * db);
                    let done = (na - a).abs() <= 1e-15 * a && (nb - bb).abs() <= 1e-15 * bb;
                    a = na;
                    bb = nb;
                    if done {
                        break;
                    }
                }
            } else if c <= 0.0 {
                return Err("the data variance is too large for a beta distribution");
            }
            vec![a, bb]
        }
        (Family::StudentT, Method::Moments) => {
            if var <= 1.0 {
                return Err("the data variance must exceed 1 for the moment estimate of a t distribution");
            }
            vec![2.0 * var / (var - 1.0)]
        }
        (Family::StudentT, Method::Likelihood) => {
            // the log-likelihood in u = ln n, maximised by golden-section search on [-3, 14]
            let ll = |u: f64| {
                let nu = u.exp();
                let c = (lgamma((nu + 1.0) / 2.0) - lgamma(nu / 2.0)).to_f64() - 0.5 * (nu * std::f64::consts::PI).ln();
                n * c - (nu + 1.0) / 2.0 * v.iter().map(|x| (x * x / nu).ln_1p()).sum::<f64>()
            };
            let g = (5f64.sqrt() - 1.0) / 2.0;
            let (mut lo, mut hi) = (-3.0f64, 14.0f64);
            let (mut x1, mut x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
            let (mut f1, mut f2) = (ll(x1), ll(x2));
            for _ in 0..200 {
                if f1 < f2 {
                    lo = x1;
                    x1 = x2;
                    f1 = f2;
                    x2 = lo + g * (hi - lo);
                    f2 = ll(x2);
                } else {
                    hi = x2;
                    x2 = x1;
                    f2 = f1;
                    x1 = hi - g * (hi - lo);
                    f1 = ll(x1);
                }
                if hi - lo < 1e-12 {
                    break;
                }
            }
            // polish by secant steps on the score dl/dnu (the likelihood is flat near its maximum, so the search
            // alone leaves about half the digits)
            let score = |nu: f64| {
                let s1: f64 = v.iter().map(|x| (x * x / nu).ln_1p()).sum();
                let s2: f64 = v.iter().map(|x| x * x / (nu * nu) / (1.0 + x * x / nu)).sum();
                n / 2.0 * (digamma((nu + 1.0) / 2.0) - digamma(nu / 2.0) - 1.0 / nu) - s1 / 2.0 + (nu + 1.0) / 2.0 * s2
            };
            let mut a = ((lo + hi) / 2.0).exp();
            let mut b2 = a * (1.0 + 1e-6);
            let (mut fa, mut fb) = (score(a), score(b2));
            for _ in 0..30 {
                if fb == fa {
                    break;
                }
                let c = b2 - fb * (b2 - a) / (fb - fa);
                if c.is_nan() || c <= 0.0 || !c.is_finite() {
                    break;
                }
                a = b2;
                fa = fb;
                b2 = c;
                fb = score(b2);
                if (b2 - a).abs() <= 1e-15 * b2 {
                    break;
                }
            }
            vec![b2]
        }
    })
}
