//! Moments of a symbolic order, quantiles at a symbolic probability, the estimate of an unknown number of
//! binomial trials and the elementwise skewness and kurtosis of a Wishart distribution, each checked against a
//! computation made here that does not share the library's route.
mod common;
use common::Sys;

/// ln C(n, k) by a sum of logarithms.
fn ln_choose(n: u64, k: u64) -> f64 {
    (0..k).map(|i| ((n - i) as f64).ln() - ((i + 1) as f64).ln()).sum()
}

/// The binomial log-likelihood of the counts.
fn loglik(xs: &[u64], n: u64, p: f64) -> f64 {
    xs.iter().map(|&x| ln_choose(n, x) + x as f64 * p.ln() + (n - x) as f64 * (1.0 - p).ln()).sum()
}

const COUNTS: [u64; 20] = [23, 16, 24, 17, 16, 17, 18, 17, 19, 23, 19, 13, 19, 18, 22, 21, 19, 22, 19, 23];

#[test]
fn binomial_trials_by_likelihood() {
    let mut s = Sys::new();
    s.run("bdata = [23, 16, 24, 17, 16, 17, 18, 17, 19, 23, 19, 13, 19, 18, 22, 21, 19, 22, 19, 23]");
    // p known: the n of the largest likelihood over a range searched directly
    let best_known = (24..400).max_by(|&a, &b| loglik(&COUNTS, a, 0.4).total_cmp(&loglik(&COUNTS, b, 0.4))).unwrap();
    assert_eq!(s.run("estimated_distribution(bdata, binomial_distribution(n, 2/5))"), format!("binomial_distribution({best_known}, 2/5)"));
    // both unknown: the profile likelihood with p = mean/n
    let mean = COUNTS.iter().sum::<u64>() as f64 / COUNTS.len() as f64;
    let best = (24..2000).max_by(|&a, &b| loglik(&COUNTS, a, mean / a as f64).total_cmp(&loglik(&COUNTS, b, mean / b as f64))).unwrap();
    let out = s.run("estimated_distribution(bdata, binomial_distribution(n, p))");
    assert!(out.starts_with(&format!("binomial_distribution({best}, ")), "{out}");
    let p = common::numbers(&s.run("estimated_distribution(bdata, binomial_distribution(n, p))[2]")).unwrap()[0];
    assert!((p - mean / best as f64).abs() < 1e-15, "{p}");
    // counts whose variance reaches their mean: the likelihood grows towards the Poisson limit, no estimate
    let out = s.run("estimated_distribution([0, 2, 0, 2, 1, 5], binomial_distribution(n, p))");
    assert!(out.starts_with("failure(argument, estimated_distribution"), "{out}");
    // an end for each side of the search: a single repeated count, and a known p near 0 and near 1
    assert_eq!(s.run("estimated_distribution([3, 3, 3], binomial_distribution(n, p))"), "binomial_distribution(3, 1.0)");
    assert!(s.run("estimated_distribution([1, 2, 1], binomial_distribution(n, 1/1000))").starts_with("binomial_distribution("));
    assert_eq!(s.run("estimated_distribution([1, 2, 1], binomial_distribution(n, 999/1000))"), "binomial_distribution(2, 999/1000)");
}

#[test]
fn chi_square_central_moment_of_symbolic_order() {
    let mut s = Sys::new();
    s.run("cm = central_moment(chi_square_distribution(nu), r)");
    // central moments of chi-square with nu = 5 from its raw moments E[X^j] = 2^j (5/2)_j, summed here
    let nu = 5.0f64;
    let raw = |j: u32| (0..j).fold(2f64.powi(j as i32), |a, i| a * (nu / 2.0 + i as f64));
    for r in 0..7u32 {
        let mut c = 0.0;
        for j in 0..=r {
            let binom = (0..j).fold(1.0, |a, i| a * (r - i) as f64 / (i + 1) as f64);
            c += binom * raw(j) * (-nu).powi((r - j) as i32);
        }
        let got = s.number(&format!("n(rewrite(cm, [nu -> 5, r -> {r}]), 30)"));
        assert!((got - c).abs() <= 1e-12 * c.abs().max(1.0), "r = {r}: {got} against {c}");
    }
}

#[test]
fn normal_moment_of_symbolic_order() {
    let mut s = Sys::new();
    s.run("m = moment(normal_distribution(mu, sigma), r)");
    // E[X^r] = sum_k C(r, k) mu^(r - k) sigma^k E[Z^k], E[Z^k] = (k - 1)!! for even k, 0 for odd k
    let (mu, sigma) = (1.3f64, 0.7f64);
    for r in 0..9u32 {
        let mut want = 0.0;
        for k in (0..=r).step_by(2) {
            let binom = (0..k).fold(1.0, |a, i| a * (r - i) as f64 / (i + 1) as f64);
            let dfact = (1..k).step_by(2).fold(1.0, |a, i| a * i as f64);
            want += binom * mu.powi((r - k) as i32) * sigma.powi(k as i32) * dfact;
        }
        let got = s.number(&format!("n(rewrite(m, [mu -> 13/10, sigma -> 7/10, r -> {r}]), 30)"));
        assert!((got - want).abs() <= 1e-12 * want.abs().max(1.0), "r = {r}: {got} against {want}");
    }
}

#[test]
fn gamma_family_quantile_of_symbolic_probability() {
    let mut s = Sys::new();
    assert_eq!(s.run("quantile(chi_square_distribution(nu), q)"), "2*inverse_gamma_regularized((1/2)*nu, 0, q)");
    assert_eq!(s.run("median(chi_square_distribution(nu))"), "2*inverse_gamma_regularized((1/2)*nu, 0, 1/2)");
    // the quantile put into the distribution function gives the probability back, inside and at both ends
    // (the symbolic forms with the parameters put in afterwards)
    for (sym, d, rules, q) in [
        ("chi_square_distribution(nu)", "chi_square_distribution(5)", "nu -> 5", "3/10"),
        ("gamma_distribution(k, 2)", "gamma_distribution(3/2, 2)", "k -> 3/2", "9/10"),
        ("chi_square_distribution(nu)", "chi_square_distribution(1)", "nu -> 1", "1/2"),
    ] {
        let x = s.run(&format!("n(rewrite(quantile({sym}, qq), [{rules}, qq -> {q}]))"));
        let back = s.number(&format!("n(cdf({d}, {x}))"));
        let want = s.number(&format!("n({q})"));
        assert!((back - want).abs() < 1e-10, "{d} {q}: {x} gives {back}");
    }
    assert_eq!(s.run("quantile(chi_square_distribution(nu), 0)"), "0");
    assert_eq!(s.run("quantile(chi_square_distribution(nu), 1)"), "inf");
}

#[test]
fn wishart_skewness_and_kurtosis() {
    let mut s = Sys::new();
    // the entry sum of nu products of a normal pair: its moments by Isserlis' theorem, then the cumulants
    let (nu, sg) = (3.0f64, [[2.0f64, 1.0], [1.0, 3.0]]);
    let skew = s.numbers("n(skewness(wishart_matrix_distribution(3, [[2, 1], [1, 3]])))");
    let kurt = s.numbers("n(kurtosis(wishart_matrix_distribution(3, [[2, 1], [1, 3]])))");
    for i in 0..2 {
        for j in 0..2 {
            let (a, b, c) = (sg[i][i], sg[j][j], sg[i][j]);
            let m1 = c;
            let m2 = a * b + 2.0 * c * c;
            let m3 = 9.0 * a * b * c + 6.0 * c.powi(3);
            let m4 = 9.0 * a * a * b * b + 72.0 * a * b * c * c + 24.0 * c.powi(4);
            let k2 = m2 - m1 * m1;
            let k3 = m3 - 3.0 * m2 * m1 + 2.0 * m1.powi(3);
            let k4 = m4 - 4.0 * m3 * m1 - 3.0 * m2 * m2 + 12.0 * m2 * m1 * m1 - 6.0 * m1.powi(4);
            let want_s = nu * k3 / (nu * k2).powf(1.5);
            let want_k = 3.0 + nu * k4 / (nu * k2).powi(2);
            assert!((skew[2 * i + j] - want_s).abs() < 1e-12, "skewness {i} {j}");
            assert!((kurt[2 * i + j] - want_k).abs() < 1e-12, "kurtosis {i} {j}");
        }
    }
}

#[test]
fn weights_given_as_a_function() {
    let mut s = Sys::new();
    // weights from a function equal the weights listed
    assert_eq!(
        s.run("variance(weighted_data([8, 3, 5, 3], w => w + 1)) == variance(weighted_data([8, 3, 5, 3], [9, 4, 6, 4]))"),
        "true"
    );
    assert_eq!(
        s.run("variance(weighted_data([8, 3, 5], wt))"),
        "(4*wt(3)*wt(5) + 9*wt(5)*wt(8) + 25*wt(3)*wt(8))*(wt(3) + wt(5) + wt(8))^-2"
    );
    assert_eq!(s.run("weighted_data([1, 2], automatic)"), "weighted_data([1, 2], automatic)");
}
