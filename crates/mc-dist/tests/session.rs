//! The functions in a session: distribution terms and their checks, exact and symbolic forms, failures, list
//! threading, data statistics, histograms, estimation, expectations, the module `stats`, exact values against
//! expected values, balls, identities (densities integrate to 1, moments by integration, round trips), and the
//! independence of the results from the thread count.
// the expected values are written as computed (some are close to library constants)
#![allow(clippy::approx_constant, clippy::excessive_precision)]
mod common;
use common::{rel, Sys};

/// Inputs and their expected printed results.
const CASES: &[(&str, &str)] = &[
    ("normal_distribution()", "normal_distribution(0, 1)"),
    ("uniform_distribution()", "uniform_distribution([0, 1])"),
    (r#"normal_distribution(0, -1)"#, r#"failure(argument, normal_distribution, "the standard deviation sigma must be positive")"#),
    (r#"normal_distribution(0, 0)"#, r#"failure(argument, normal_distribution, "the standard deviation sigma must be positive")"#),
    (r#"uniform_distribution([2, 1])"#, r#"failure(argument, uniform_distribution, "the interval [a, b] must have a < b")"#),
    (r#"uniform_distribution(3)"#, r#"failure(argument, uniform_distribution, "the argument must be an interval [a, b]")"#),
    (r#"gamma_distribution(-1, 2)"#, r#"failure(argument, gamma_distribution, "the shape and the scale must be positive")"#),
    (r#"beta_distribution(1, 0)"#, r#"failure(argument, beta_distribution, "the shape parameters must be positive")"#),
    (r#"binomial_distribution(2.5, 0.5)"#, r#"failure(argument, binomial_distribution, "the number of trials must be an exact integer n >= 0")"#),
    (r#"binomial_distribution(10, 3/2)"#, r#"failure(argument, binomial_distribution, "the probability p must be in [0, 1]")"#),
    (r#"poisson_distribution(0)"#, r#"failure(argument, poisson_distribution, "the mean must be positive")"#),
    (r#"geometric_distribution(0)"#, r#"failure(argument, geometric_distribution, "the probability p must be in (0, 1]")"#),
    ("geometric_distribution(1)", "geometric_distribution(1)"),
    (r#"student_t_distribution(0)"#, r#"failure(argument, student_t_distribution, "the degrees of freedom must be positive")"#),
    (r#"chi_square_distribution(-3)"#, r#"failure(argument, chi_square_distribution, "the degrees of freedom must be positive")"#),
    (r#"exponential_distribution(0)"#, r#"failure(argument, exponential_distribution, "the rate must be positive")"#),
    (r#"normal_distribution("a", 1)"#, r#"failure(argument, normal_distribution, "the parameters must be real numbers or symbolic expressions")"#),
    ("normal_distribution(mu, sigma)", "normal_distribution(mu, sigma)"),
    ("pdf(normal_distribution(mu, sigma), x)", "(1/2)*2^(1/2)*exp((-1/2)*sigma^-2*(x - mu)^2)*pi^(-1/2)/sigma"),
    ("cdf(normal_distribution(mu, sigma), x)", "(1/2)*erfc((1/2)*(mu - x)*2^(1/2)/sigma)"),
    ("survival(normal_distribution(mu, sigma), x)", "(1/2)*erfc((1/2)*(x - mu)*2^(1/2)/sigma)"),
    ("pdf(log_normal_distribution(mu, sigma), x)", "piecewise([[(1/2)*2^(1/2)*exp((-1/2)*sigma^-2*(log(x) - mu)^2)*pi^(-1/2)/sigma/x, x > 0]], 0)"),
    ("pdf(uniform_distribution([a, b]), x)", "piecewise([[(b - a)^-1, a <= x and x <= b]], 0)"),
    ("cdf(uniform_distribution([0, 4]), 1)", "1/4"),
    ("cdf(uniform_distribution([0, 4]), 5)", "1"),
    ("pdf(exponential_distribution(l), x)", "piecewise([[l*exp(-1*l*x), x >= 0]], 0)"),
    ("pdf(gamma_distribution(k, t), x)", "piecewise([[exp(-1*x/t)*t^-k*x^(-1 + k)/gamma(k), x > 0]], 0)"),
    ("pdf(beta_distribution(a, b), x)", "piecewise([[x^(-1 + a)*(1 - x)^(-1 + b)/beta(a, b), 0 < x and x < 1]], 0)"),
    ("pdf(chi_square_distribution(k), x)", "piecewise([[2^((-1/2)*k)*exp((-1/2)*x)*x^(-1 + (1/2)*k)/gamma((1/2)*k), x > 0]], 0)"),
    ("pdf(student_t_distribution(n), x)", "n^(-1/2)/beta((1/2)*n, 1/2)*(n/(n + x^2))^((1/2)*(1 + n))"),
    ("pdf(binomial_distribution(n, p), k)", "piecewise([[binomial(n, k)*p^k*(1 - p)^(n - k), 0 <= k and k <= n]], 0)"),
    ("pdf(poisson_distribution(m), k)", "piecewise([[exp(-m)*m^k/factorial(k), k >= 0]], 0)"),
    ("pdf(geometric_distribution(p), k)", "piecewise([[p*(1 - p)^k, k >= 0]], 0)"),
    ("pdf(binomial_distribution(4, 1/2), 5/2)", "0"),
    ("pdf(binomial_distribution(4, 1/2), 5)", "0"),
    ("pdf(poisson_distribution(m), -1)", "0"),
    ("cdf(exponential_distribution(l), x)", "piecewise([[1 - exp(-1*l*x), x >= 0]], 0)"),
    ("cdf(gamma_distribution(2, t), x)", "piecewise([[1 + -1*(1 + x/t)*exp(-1*x/t), x > 0]], 0)"),
    ("cdf(beta_distribution(2, 2), x)", "piecewise([[3*(1 - x)*x^2 + x^3, 0 < x and x < 1], [1, 1 <= x]], 0)"),
    ("cdf(student_t_distribution(n), x)", "piecewise([[(1/2)*beta(n/(n + x^2), (1/2)*n, 1/2)/beta((1/2)*n, 1/2), x <= 0]], 1 + (-1/2)*beta(n/(n + x^2), (1/2)*n, 1/2)/beta((1/2)*n, 1/2))"),
    ("cdf(poisson_distribution(m), x)", "piecewise([[gamma_regularized(1 + floor(x), m), x >= 0]], 0)"),
    ("cdf(geometric_distribution(p), x)", "piecewise([[1 - (1 - p)^(1 + floor(x)), x >= 0]], 0)"),
    ("cdf(binomial_distribution(n, p), x)", "piecewise([[beta(1 - p, n - floor(x), 1 + floor(x))/beta(n - floor(x), 1 + floor(x)), 0 <= x and x < n], [1, n <= x]], 0)"),
    ("cdf(poisson_distribution(2), -1)", "0"),
    ("cdf(binomial_distribution(5, 1/2), 7)", "1"),
    ("survival(exponential_distribution(l), x)", "piecewise([[exp(-1*l*x), x >= 0]], 1)"),
    ("survival(geometric_distribution(p), x)", "piecewise([[(1 - p)^(1 + floor(x)), x >= 0]], 1)"),
    ("quantile(normal_distribution(mu, sigma), q)", "mu + -1*sigma*inverse_erfc(2*q)*2^(1/2)"),
    ("quantile(normal_distribution(mu, sigma), 1/2)", "mu"),
    ("quantile(normal_distribution(mu, sigma), 0)", "-inf"),
    ("quantile(normal_distribution(mu, sigma), 1)", "inf"),
    ("quantile(log_normal_distribution(mu, sigma), q)", "exp(mu + -1*sigma*inverse_erfc(2*q)*2^(1/2))"),
    ("quantile(uniform_distribution([a, b]), q)", "a + q*(b - a)"),
    ("quantile(exponential_distribution(l), q)", "-1*log(1 - q)/l"),
    ("quantile(exponential_distribution(l), 1)", "inf"),
    ("quantile(student_t_distribution(1), q)", "tan(pi*(-1/2 + q))"),
    ("quantile(gamma_distribution(2, 3), 0)", "0"),
    ("quantile(beta_distribution(2, 3), 1)", "1"),
    ("quantile(gamma_distribution(2, 3), 1/2)", "quantile(gamma_distribution(2, 3), 1/2)"),
    (r#"quantile(normal_distribution(0, 1), 3/2)"#, r#"failure(argument, quantile, "the probability must be a real number in [0, 1]")"#),
    (r#"quantile(normal_distribution(0, 1), -0.5)"#, r#"failure(argument, quantile, "the probability must be a real number in [0, 1]")"#),
    ("quantile(poisson_distribution(4), 1)", "inf"),
    ("quantile(binomial_distribution(8, 0.25), 1.0)", "8"),
    ("mean(normal_distribution(mu, sigma))", "mu"),
    ("variance(normal_distribution(mu, sigma))", "sigma^2"),
    ("std_dev(normal_distribution(mu, sigma))", "sigma"),
    ("skewness(gamma_distribution(k, t))", "2*k^(-1/2)"),
    ("kurtosis(exponential_distribution(l))", "9"),
    ("variance(binomial_distribution(n, p))", "n*p*(1 - p)"),
    ("mean(geometric_distribution(p))", "(1 - p)/p"),
    ("variance(beta_distribution(a, b))", "a*b*(a + b)^-2/(1 + a + b)"),
    ("mean(student_t_distribution(1))", "failure(undefined, mean, student_t_distribution(1))"),
    ("variance(student_t_distribution(2))", "inf"),
    ("variance(student_t_distribution(1))", "failure(undefined, variance, student_t_distribution(1))"),
    ("kurtosis(student_t_distribution(3))", "inf"),
    ("skewness(student_t_distribution(3))", "failure(undefined, skewness, student_t_distribution(3))"),
    ("moment(student_t_distribution(4), 4)", "inf"),
    ("moment(student_t_distribution(4), 5)", "failure(undefined, moment, student_t_distribution(4))"),
    ("moment(normal_distribution(0, s), 6)", "15*s^6"),
    ("moment(gamma_distribution(k, t), 2)", "k*(1 + k)*t^2"),
    ("moment(exponential_distribution(l), k)", "factorial(k)*l^-k"),
    (r#"moment(exponential_distribution(l), -1)"#, r#"failure(argument, moment, "the order must be an integer from 0 to 200")"#),
    ("moment(exponential_distribution(l), 0)", "1"),
    ("median(exponential_distribution(l))", "log(2)/l"),
    ("median(binomial_distribution(9, 1/2))", "4"),
    ("median(gamma_distribution(2, 1.5))", "2.517520485024991"),
    ("mean(normal_distribution(1.5, 2))", "1.5"),
    ("std_dev(gamma_distribution(2.0, 3))", "4.242640687119286"),
    ("mean([1, 2, 3, 4])", "5/2"),
    ("mean([a, b])", "(1/2)*(a + b)"),
    ("variance([1, 2, 3, 4])", "5/3"),
    ("variance([x, y])", "(x + (-1/2)*(x + y))^2 + (y + (-1/2)*(x + y))^2"),
    ("std_dev([2, 4, 4, 4, 5, 5, 7, 9])", "(4/7)*14^(1/2)"),
    ("mean([1.5, 2, 3])", "2.1666666666666665"),
    // a data matrix: the statistic of each column (DI.12)
    ("mean([[1, 2], [3, 4]])", "[2, 3]"),
    ("mean([[1.0, 2], [3, 4], [5, 9]])", "[3.0, 5]"),
    ("variance([[1, 10], [3, 30], [5, 20]])", "[4, 100]"),
    ("std_dev([[1, 10], [3, 30], [5, 20]])", "[2, 10]"),
    ("median([[1, 10], [3, 30], [5, 20]])", "[3, 20]"),
    ("quantile([[1, 10], [3, 30], [5, 20]], [1/3, 1])", "[[1, 5], [10, 30]]"),
    ("moment([[1, 2], [3, 4]], 2)", "[5, 10]"),
    ("mean([[a, b], [c, d]])", "[(1/2)*(a + c), (1/2)*(b + d)]"),
    ("mean(table([k, 2.0*k], k, 100))", "[101/2, 101.0]"),
    ("mean(table([1.0*k, 2.0*k], k, 100))", "[50.5, 101.0]"),
    ("std_dev(table([1.0*k, -1.0*k], k, 100))", "[29.011491975882016, 29.011491975882016]"),
    ("mean([[1, 2], [3]])", "mean([[1, 2], [3]])"),
    ("variance([1.0, 2, 3, 4])", "1.6666666666666667"),
    ("skewness([1, 2, 3, 10])", "(18/25)*2^(1/2)"),
    ("kurtosis([1, 2, 3, 10])", "1394/625"),
    ("moment([1, 2, 3], 3)", "12"),
    ("central_moment([1, 2, 3, 6], 2)", "7/2"),
    ("median([3, 1, 2])", "2"),
    ("median([3, 1, 2, 10])", "5/2"),
    ("median([3.5, 1, 2, 10])", "2.75"),
    (r#"median([])"#, r#"failure(argument, median, "the list is empty")"#),
    (r#"mean([])"#, r#"failure(argument, mean, "the list is empty")"#),
    (r#"variance([1])"#, r#"failure(argument, variance, "the list needs at least two elements")"#),
    ("quantile([5, 1, 3, 2, 4], 1/2)", "3"),
    ("quantile([5, 1, 3, 2, 4], [0, 1/5, 0.21, 1])", "[1, 1, 2, 5]"),
    (r#"quantile([5, 1, 3, 2, 4], 2)"#, r#"failure(argument, quantile, "the probability must be a real number in [0, 1]")"#),
    ("quantile([a, 1], 1/2)", "quantile([a, 1], 1/2)"),
    ("histogram_list([1, 2, 2, 3, 3, 3, 4], 3)", "[[1, 2, 3, 4], [1, 2, 4]]"),
    ("histogram_list([1, 2, 2, 3, 3, 3, 4], [1])", "[[1, 2, 3, 4, 5], [1, 2, 3, 1]]"),
    ("histogram_list([1, 2, 2, 3, 3, 3, 4], [0, 4, 2])", "[[0, 2, 4], [1, 5]]"),
    ("histogram_list([1, 2, 2, 3, 3, 3, 4], [[0, 2.5, 10]])", "[[0, 2.5, 10], [3, 4]]"),
    ("histogram_list([0.1, 0.25, 0.3, 0.9, 1.7])", "[[0, 1/2, 1, 3/2, 2], [3, 1, 0, 1]]"),
    ("histogram_list([5, 5, 5], 2)", "[[5, 6, 7], [3, 0]]"),
    (r#"histogram_list([1, 2], [[2, 1]])"#, r#"failure(argument, histogram_list, "the edges must be at least two increasing real numbers")"#),
    (r#"histogram_list([1, a], 2)"#, r#"failure(argument, histogram_list, "the data must be a list of real numbers")"#),
    (r#"histogram_list([], 2)"#, r#"failure(argument, histogram_list, "the data list is empty")"#),
    ("pdf(normal_distribution(0, 1), [x, 0])", "[(1/2)*2^(1/2)*exp((-1/2)*x^2)*pi^(-1/2), (1/2)*2^(1/2)*pi^(-1/2)]"),
    ("cdf(exponential_distribution(1), [0, 1, 2])", "[0, 1 - exp(-1), 1 - exp(-2)]"),
    ("quantile(exponential_distribution(1), [1/2, 1/4])", "[log(2), -log(3/4)]"),
    (r#"random_variate(normal_distribution(mu, 1))"#, r#"failure(argument, random_variate, "the parameters of the distribution must be numeric")"#),
    (r#"random_variate(normal_distribution(0, 1), -1)"#, r#"failure(argument, random_variate, "the count must be a natural number, at most 2^32, or a list of them")"#),
    ("random_variate(normal_distribution(0, 1), 0)", "[]"),
    ("expectation(x => x^2 + 3, poisson_distribution(m))", "3 + m + m^2"),
    ("expectation(x^2, x, uniform_distribution([a, b]))", "(1/3)*(-a^3 + b^3)/(b - a)"),
    ("expectation(x => 5, normal_distribution(0, 1))", "5"),
    ("expectation(2*x + y, x, exponential_distribution(l))", "y + 2/l"),
    ("expectation(x => exp(x), exponential_distribution(3))", "3/2"),
    ("expectation(x => sin(x), exponential_distribution(1.0))", "0.4999999999999896"),
    ("expectation(x => sqrt(x), poisson_distribution(4.0))", "1.9220757266302173"),
    (r#"expectation(x^2, 3, normal_distribution(0, 1))"#, r#"failure(argument, expectation, "the variable must be a symbol")"#),
    ("estimated_distribution([1, 2, 3, 4, 6], normal_distribution(m, s))", "normal_distribution(16/5, (1/5)*74^(1/2))"),
    ("estimated_distribution([1, 2, 3, 4, 6], normal_distribution(0, s))", "normal_distribution(0, (1/5)*330^(1/2))"),
    ("estimated_distribution([1, 2, 3, 4, 6], normal_distribution(m, 1))", "normal_distribution(16/5, 1)"),
    ("estimated_distribution([1, 2, 3, 4, 6], normal_distribution(m, s), method: moments)", "normal_distribution(16/5, (1/10)*370^(1/2))"),
    ("estimated_distribution([1, 2, 3, 4, 6], uniform_distribution([a, b]))", "uniform_distribution([1, 6])"),
    ("estimated_distribution([1, 2, 3, 4, 6], exponential_distribution(l))", "exponential_distribution(5/16)"),
    ("estimated_distribution([3, 5, 4, 6], binomial_distribution(10, p))", "binomial_distribution(10, 9/20)"),
    (r#"estimated_distribution([3, 5, 4, 6], binomial_distribution(n, p))"#, r#"failure(argument, estimated_distribution, "the number of trials of a binomial distribution must be given")"#),
    (r#"estimated_distribution([3, 5, 4, 6], gamma_distribution(2, t))"#, r#"failure(argument, estimated_distribution, "only all parameters of this family can be estimated together (the trials of a binomial distribution must be known)")"#),
    (r#"estimated_distribution([1, 2], normal_distribution(m, s), method: magic)"#, r#"failure(argument, estimated_distribution, "the option is method: likelihood or method: moments")"#),
    (r#"estimated_distribution([1], normal_distribution(m, s))"#, r#"failure(argument, estimated_distribution, "the data needs at least two values")"#),
    ("estimated_distribution([1, 2, 3], normal_distribution(0, 1))", "normal_distribution(0, 1)"),
    (r#"estimated_distribution([-1.0, 2], gamma_distribution(k, t))"#, r#"failure(argument, estimated_distribution, "the data must be positive for this family")"#),
];

/// Exact results whose numerical value is compared with an expected value.
const EXACT: &[(&str, f64)] = &[
    ("cdf(gamma_distribution(3, 2), 5)", 0.45618688411667047),
    ("cdf(beta_distribution(2, 3), 1/3)", 0.4074074074074074),
    ("cdf(chi_square_distribution(4), 3)", 0.4421745996289254),
    ("cdf(student_t_distribution(1), 2)", 0.8524163823495667),
    ("cdf(student_t_distribution(2), -1)", 0.2113248654051871),
    ("cdf(binomial_distribution(12, 1/3), 5)", 0.8222775435090631),
    ("cdf(binomial_distribution(12, 1/3), 19/2)", 0.9994561955137071),
    ("cdf(poisson_distribution(5/2), 4)", 0.8911780189141513),
    ("cdf(geometric_distribution(1/5), 3)", 0.5904),
    ("pdf(poisson_distribution(5/2), 3)", 0.21376301724973645),
    ("pdf(gamma_distribution(5/2, 3), 2)", 0.07007708083523508),
    ("pdf(beta_distribution(1/2, 3/2), 1/4)", 1.1026577908435842),
    ("pdf(student_t_distribution(3), 1)", 0.206748335783172),
    ("pdf(chi_square_distribution(3), 2)", 0.20755374871029736),
    ("pdf(log_normal_distribution(0, 1), 2)", 0.1568740192789811),
    ("survival(exponential_distribution(3), 2)", 0.0024787521766663585),
    ("quantile(binomial_distribution(10, 1/3), 9/10)", 5.0),
    ("quantile(binomial_distribution(10, 1/3), 1/10)", 1.0),
    ("quantile(poisson_distribution(3), 1/2)", 3.0),
    ("quantile(poisson_distribution(3), 99/100)", 8.0),
    ("quantile(geometric_distribution(1/4), 1/2)", 2.0),
    ("quantile(geometric_distribution(1/4), 3/4)", 4.0),
    ("quantile(exponential_distribution(2), 3/4)", 0.6931471805599453),
    ("quantile(student_t_distribution(2), 9/10)", 1.8856180831641267),
    ("mean(log_normal_distribution(1, 2))", 20.085536923187668),
    ("variance(log_normal_distribution(1, 2))", 21623.03700131398),
    ("skewness(beta_distribution(2, 5))", 0.5962847939999439),
    ("kurtosis(beta_distribution(2, 5))", 2.88),
    ("kurtosis(binomial_distribution(10, 1/4))", 2.933333333333333),
    ("skewness(geometric_distribution(1/3))", 2.041241452319315),
    ("kurtosis(geometric_distribution(1/3))", 9.166666666666666),
    ("kurtosis(student_t_distribution(7))", 5.0),
    ("variance(student_t_distribution(7/2))", 2.3333333333333335),
    ("skewness(chi_square_distribution(5))", 1.2649110640673518),
    ("kurtosis(uniform_distribution([2, 7]))", 1.8),
    ("moment(gamma_distribution(5/2, 3), 3)", 1063.125),
    ("moment(student_t_distribution(9), 6)", 104.14285714285714),
    ("moment(binomial_distribution(7, 2/5), 4)", 151.984),
    ("moment(geometric_distribution(1/3), 3)", 74.0),
    ("moment(uniform_distribution([-1, 3]), 5)", 30.333333333333332),
    ("moment(beta_distribution(2, 3), 4)", 0.07142857142857142),
    ("moment(normal_distribution(1, 2), 5)", 281.0),
    ("moment(log_normal_distribution(0, 1/2), 3)", 3.080216848918031),
    ("moment(chi_square_distribution(3), 3)", 105.0),
    ("moment(exponential_distribution(3), 4)", 0.2962962962962963),
    ("moment(poisson_distribution(2), 5)", 454.0),
    ("central_moment(gamma_distribution(3, 2), 5)", 8064.0),
    ("central_moment(poisson_distribution(4), 4)", 52.0),
    ("expectation(x => x^3 - 2*x, binomial_distribution(6, 1/2))", 34.5),
    ("expectation(x => exp(-x), exponential_distribution(2))", 0.6666666666666666),
    ("expectation(x => 1/(1 + x), binomial_distribution(5, 1/3))", 0.4561042524005487),
];

#[test]
fn cases() {
    let mut s = Sys::new();
    for (src, want) in CASES {
        assert_eq!(s.run(src), *want, "{src}");
    }
}

#[test]
fn exact_values_against_references() {
    let mut s = Sys::new();
    for &(src, want) in EXACT {
        // the exact result, then its value through `n` at 30 digits, read back as a machine real
        let exact = s.run(src);
        assert!(!exact.contains('.'), "{src} is not exact: {exact}");
        let v = s.number(&format!("n(n({src}, 30))"));
        assert!(rel(v, want) < 1e-15, "{src} = {exact} = {v}, want {want}");
    }
}

#[test]
fn ball_quantiles() {
    let mut s = Sys::new();
    // quantiles without closed forms at ball probabilities: every printed digit correct
    let cases = [
        ("quantile(gamma_distribution(2, 3), 0.5~40)", "5.03504097004998196023865353628356925447"),
        ("quantile(beta_distribution(2, 3), 0.3~50)", "0.27238394207510534744657937779948579307464854364124"),
        ("quantile(chi_square_distribution(4), 0.9~40)", "7.7794403397348581158079604498536140459"),
    ];
    for (src, want) in cases {
        assert_eq!(s.run(src), want, "{src}");
    }
    let got = s.run("n(quantile(gamma_distribution(2, 3), 1/2), 30)");
    assert!(got.starts_with("5.0350409700499819602386535362"), "{got}");
    // the machine value of an unevaluated exact quantile
    assert_eq!(s.run("n(quantile(gamma_distribution(2, 3), 1/2))"), "5.035040970049982");
}

#[test]
fn densities_integrate_to_one_and_give_the_moments() {
    let mut s = Sys::new();
    let ds = [
        ("normal_distribution(0.5, 1.5)", "-inf", "inf"),
        ("log_normal_distribution(0.2, 0.5)", "0", "inf"),
        ("uniform_distribution([-1.0, 2])", "-1", "2"),
        ("exponential_distribution(1.5)", "0", "inf"),
        ("gamma_distribution(2.5, 1.5)", "0", "inf"),
        ("beta_distribution(2.5, 3.5)", "0", "1"),
        ("chi_square_distribution(5.0)", "0", "inf"),
        ("student_t_distribution(6.0)", "-inf", "inf"),
    ];
    for (d, lo, hi) in ds {
        let total = s.number(&format!("n_integrate(pdf({d}, x), [x, {lo}, {hi}])"));
        assert!((total - 1.0).abs() < 1e-9, "{d}: {total}");
        let m = s.number(&format!("n_integrate(x*pdf({d}, x), [x, {lo}, {hi}])"));
        let want = s.number(&format!("mean({d})"));
        assert!((m - want).abs() < 1e-8 * (1.0 + want.abs()), "{d}: mean {m} vs {want}");
        let v = s.number(&format!("n_integrate((x - {want:?})^2*pdf({d}, x), [x, {lo}, {hi}])"));
        let want_v = s.number(&format!("variance({d})"));
        assert!(rel(v, want_v) < 1e-8, "{d}: variance {v} vs {want_v}");
    }
    // discrete: the masses sum to 1 and give the mean
    for (d, top) in [("binomial_distribution(30, 0.3)", 30), ("poisson_distribution(6.5)", 120), ("geometric_distribution(0.3)", 200)] {
        let total = s.number(&format!("sum(pdf({d}, range(0, {top})))"));
        assert!((total - 1.0).abs() < 1e-12, "{d}: {total}");
        let m = s.number(&format!("sum(range(0, {top})*pdf({d}, range(0, {top})))"));
        let want = s.number(&format!("mean({d})"));
        assert!(rel(m, want) < 1e-12, "{d}: mean {m} vs {want}");
    }
}

#[test]
fn exact_round_trips() {
    let mut s = Sys::new();
    for (d, q) in [
        ("exponential_distribution(2)", "3/4"),
        ("uniform_distribution([1, 5])", "1/3"),
        ("student_t_distribution(1)", "3/4"),
        ("student_t_distribution(2)", "9/10"),
        ("geometric_distribution(1/4)", "1/2"),
    ] {
        assert_eq!(s.run(&format!("cdf({d}, quantile({d}, {q})) >= {q}")), "true", "{d} {q}");
    }
    // discrete quantiles are the least such integers: one less does not reach q
    for (d, q) in [("binomial_distribution(10, 1/3)", "9/10"), ("poisson_distribution(3)", "99/100"), ("geometric_distribution(1/4)", "3/4")] {
        let k = s.run(&format!("quantile({d}, {q})"));
        assert_eq!(s.run(&format!("cdf({d}, {k}) >= {q}")), "true", "{d} {q}: {k}");
        assert_eq!(s.run(&format!("cdf({d}, {k} - 1) < {q}")), "true", "{d} {q}: {k}");
    }
}

#[test]
fn data_and_the_stats_module() {
    let mut s = Sys::new();
    let cases = [
        ("import stats\n[mean([1, 2, 3, 4]), variance([1, 2, 3, 4]), median([4, 1, 3, 2])]", "[5/2, 5/3, 5/2]"),
        (
            "[mean(normal_distribution(m, s)), variance(gamma_distribution(k, t)), std_dev(poisson_distribution(4)), median(normal_distribution(1, 2))]",
            "[m, k*t^2, 2, 1]",
        ),
        ("covariance([1, 2, 3, 4], [2, 4, 5, 9])", "11/3"),
        ("correlation([1, 2, 3, 4], [2, 4, 5, 9])", "(11/130)*130^(1/2)"),
        ("covariance([1, 2], [3, 1])", "-1"),
    ];
    for (src, want) in cases {
        assert_eq!(s.run(src), want, "{src}");
    }
    let c = s.number("correlation([1.0, 2, 3, 4], [2.0, 4, 5, 9])");
    assert!(rel(c, 0.964_763_821_237_732_1) < 1e-14, "{c}");
    // machine data on packed arrays: the exact computation, rounded
    let xs: Vec<String> = (1..=1000).map(|k| format!("{:?}", (k * 37 % 101) as f64 / 8.0)).collect();
    let list = xs.join(", ");
    for f in ["mean", "variance", "std_dev", "skewness", "kurtosis", "median"] {
        let m = s.number(&format!("{f}([{list}])"));
        let e = s.number(&format!("n({f}(rationalize([{list}], 0)))"));
        assert!((m - e).abs() < 1e-13 * e.abs().max(1.0), "{f}: {m} vs {e}");
    }
    let h = s.numbers(&format!("histogram_list([{list}], [0, 15, 5])"));
    let counts: f64 = h[4..].iter().sum();
    assert_eq!(h[..4], [0.0, 5.0, 10.0, 15.0]);
    assert_eq!(counts, 1000.0 - (1..=1000).filter(|k| (k * 37 % 101) as f64 / 8.0 >= 15.0).count() as f64);
}

#[test]
fn estimation_against_references() {
    let mut s = Sys::new();
    s.run("data = [1.2, 2.3, 0.7, 3.1, 1.9, 0.4, 2.8, 1.1]\nb = [0.2, 0.45, 0.5, 0.7, 0.33, 0.61]\nt = [1.0, -2, 3, 0.5, 1.5, -0.7, 0.1, -0.3]");
    let cases: [(&str, &[f64]); 7] = [
        ("estimated_distribution(data, gamma_distribution(k, th))", &[2.743_363_280_381_776_5, 0.615_120_867_173_363_1]),
        ("estimated_distribution(data, chi_square_distribution(k))", &[2.287_025_175_174_303_4]),
        ("estimated_distribution(data, log_normal_distribution(m, s))", &[0.330_056_324_820_643_6, 0.669_284_541_633_033]),
        ("estimated_distribution(b, beta_distribution(a, bb))", &[3.865_344_674_413_043_5, 4.480_110_286_890_946]),
        // the root of the score to 20 digits (a plain maximisation stops near 3.38806876)
        ("estimated_distribution(t, student_t_distribution(n))", &[3.388_069_302_431_242_8]),
        ("estimated_distribution(data, exponential_distribution(l))", &[0.592_592_592_592_592_6]),
        ("estimated_distribution(data, gamma_distribution(k, th), method: moments)", &[2.902_070_063_694_268_2, 0.581_481_481_481_481_4]),
    ];
    for (src, want) in cases {
        let out = s.run(src);
        let inner = &out[out.find('(').unwrap() + 1..out.len() - 1];
        let got = common::numbers(inner).unwrap_or_else(|| panic!("{src}: {out}"));
        assert_eq!(got.len(), want.len(), "{src}: {out}");
        for (g, w) in got.iter().zip(want) {
            assert!(rel(*g, *w) < 1e-12, "{src}: {out}");
        }
    }
}

#[test]
fn variates_reproduce_and_match_their_moments() {
    let mut s = Sys::new();
    let a = s.run("seed(5)\nrandom_variate(gamma_distribution(2.5, 1), 50)");
    assert_eq!(a, s.run("seed(5)\nrandom_variate(gamma_distribution(2.5, 1), 50)"));
    assert_ne!(a, s.run("seed(6)\nrandom_variate(gamma_distribution(2.5, 1), 50)"));
    assert_eq!(s.run("seed(1)\nv = random_variate(poisson_distribution(3), [2, 5])
[length(v), length(v[1])]"), "[2, 5]");
    assert!(s.run("seed(1)\nrandom_variate(binomial_distribution(10, 1/2))").parse::<i64>().is_ok());
    // a draw is an effect: two calls give two values
    let r = s.run("seed(2)\nr = [random_variate(normal_distribution()), random_variate(normal_distribution())]\nr[1] == r[2]");
    assert_eq!(r, "false");
    // sample means and variances within five standard errors (n = 10^5, fixed seeds)
    let ds = [
        "normal_distribution(1, 2)",
        "log_normal_distribution(0, 0.5)",
        "uniform_distribution([-1, 3])",
        "exponential_distribution(4)",
        "gamma_distribution(0.5, 2)",
        "beta_distribution(2, 5)",
        "chi_square_distribution(3)",
        "student_t_distribution(9)",
        "binomial_distribution(40, 0.25)",
        "poisson_distribution(120)",
        "geometric_distribution(0.2)",
    ];
    let n = 100_000.0;
    for (i, d) in ds.iter().enumerate() {
        let got = s.numbers(&format!("seed({})\nxs = random_variate({d}, 100000)\n[mean(xs), variance(xs)]", 100 + i));
        let want = s.numbers(&format!("n([mean({d}), variance({d}), central_moment({d}, 4)])"));
        let (m, v) = (got[0], got[1]);
        assert!((m - want[0]).abs() < 5.0 * (want[1] / n).sqrt(), "{d}: mean {m} vs {}", want[0]);
        // the standard error of the sample variance: sqrt((mu4 - sigma^4)/n)
        let se = ((want[2] - want[1] * want[1]) / n).sqrt();
        assert!((v - want[1]).abs() < 5.0 * se, "{d}: variance {v} vs {}", want[1]);
    }
}

#[test]
fn results_are_identical_at_1_and_4_threads() {
    let programs: Vec<String> = CASES
        .iter()
        .map(|c| c.0.to_string())
        .chain([
            "seed(9)\n[random_variate(normal_distribution(), 3), map(k => random_variate(poisson_distribution(k + 1)), range(5)), table(random_variate(beta_distribution(2, 2)), 4)]".to_string(),
            "seed(9)\nmean(random_variate(binomial_distribution(1000, 0.4), 2000))".to_string(),
        ])
        .collect();
    let run = |threads: usize, stress: bool| -> Vec<String> {
        let mut s = Sys::with_threads(threads, stress);
        programs.iter().map(|p| s.run(p)).collect()
    };
    let one = run(1, false);
    assert_eq!(run(4, false), one);
    assert_eq!(run(4, true), one);
}
