//! Sums, products, recurrences and their checks (item 13B).
mod common;
use common::{check, Sys};
use std::time::Duration;

#[test]
fn gosper() {
    check(&[
        // indefinite sums: Gosper's antidifference, a telescoping difference, digamma for a rational term
        ("sum(1/(k*(k + 1)), k)", "-k^-1"),
        ("sum(k*2^k, k)", "(-2 + k)*2^k"),
        ("sum(k/2^k, k)", "-1*(1 + k)*2^(1 - k)"),
        ("sum(f(k + 1) - f(k), k)", "f(k)"),
        ("sum(1/k, k)", "digamma(k)"),
        // definite sums with symbolic bounds
        ("sum(k*2^k, [k, 0, m])", "2 - 2^(1 + m) + m*2^(1 + m)"),
        ("sum(1/(k*(k + 1)), [k, 1, m])", "m/(1 + m)"),
        ("sum(k*factorial(k), [k, 0, m])", "-1 + factorial(m) + m*factorial(m)"),
        ("sum(binomial(2*k, k)/4^k, [k, 0, m])", "(1 + 2*m)*binomial(2*m, m)*4^-m"),
        ("sum(1/k, [k, 1, m])", "harmonic_number(m)"),
        ("sum(1/(k*(k + 2)), [k, 1, m])", "(1/4)*m*(5 + 3*m)/(1 + m)/(2 + m)"),
        ("product(a^(1/(k*(k + 1))), k)", "a^((-1 + k)/k)"),
    ]);
}

#[test]
fn zeilberger() {
    check(&[
        ("sum(binomial(m, k), [k, 0, m])", "2^m"),
        ("sum(binomial(m, k)^2, [k, 0, m])", "binomial(2*m, m)"),
        ("sum(k*binomial(m, k), [k, 0, m])", "m*2^(-1 + m)"),
        ("sum((-1)^k*binomial(m, k), [k, 0, m])", "0"),
        ("sum(binomial(m, k)*binomial(p, j - k), [k, 0, j])", "binomial(m + p, j)"),
        ("sum(binomial(m, k)*x^k, [k, 0, m])", "(1 + x)^m"),
        // Apery's numbers have no hypergeometric closed form: unevaluated
        ("sum(binomial(m, k)^2*binomial(m + k, k), [k, 0, m])", "sum(binomial(k + m, k)*binomial(m, k)^2, [k, 0, m])"),
    ]);
}

#[test]
fn infinite_sums() {
    check(&[
        ("sum(1/(k^2 + 1), [k, -inf, inf])", "pi*coth(pi)"),
        ("sum(1/(k^2 + 1), [k, 1, inf])", "(1/2)*(-1 + pi*coth(pi))"),
        ("sum(1/((k + 1)*(k + 2)), [k, 0, inf])", "1"),
        ("sum(1/(4*k^2 - 1), [k, 1, inf])", "1/2"),
        ("sum(1/k^2, [k, 3, inf])", "-5/4 + (1/6)*pi^2"),
        ("sum(x^k/factorial(k), [k, 0, inf])", "exp(x)"),
        ("sum(1/factorial(k), [k, 0, inf])", "e"),
        ("sum(a^k, [k, 0, inf])", "(1 - a)^-1"),
        ("sum(k/2^k, [k, 1, inf])", "2"),
        // divergent: unevaluated
        ("sum(k*2^k, [k, 0, inf])", "sum(k*2^k, [k, 0, inf])"),
        ("sum((-1)^k, [k, 1, inf])", "sum((-1)^k, [k, 1, inf])"),
    ]);
}

#[test]
fn products() {
    check(&[
        ("product(1 + 1/k^2, [k, 1, inf])", "pi^-1*sinh(pi)"),
        ("product(1 - 1/k^4, [k, 2, inf])", "(1/4)/pi*sinh(pi)"),
        ("product(4*k^2/(4*k^2 - 1), [k, 1, inf])", "(1/2)*pi"),
        ("product(1 + 1/(k^2 + 5), [k, 0, inf])", "(1/5)*csch(pi*5^(1/2))*30^(1/2)*sinh(pi*6^(1/2))"),
        ("product(k/(k + 1), [k, 1, m])", "(1 + m)^-1"),
        ("product(1 - 1/k^2, [k, 2, m])", "(1/2)*(1 + m)/m"),
        ("product((k^2 - 1/4)/(k + 1), [k, 1, m - 1])", "pochhammer(1/2, -1 + m)*pochhammer(3/2, -1 + m)/pochhammer(2, -1 + m)"),
        ("product(k, k)", "factorial(-1 + k)"),
        ("product(f(k + 1)/f(k), k)", "f(k)"),
    ]);
}

#[test]
fn numeric() {
    let mut s = Sys::new();
    for (src, want) in [
        ("n_sum(1/k^2, [k, 1, inf]) - pi^2/6", 1e-12),
        ("n_sum(1/k^(11/10), [k, 1, inf]) - n(zeta(11/10))", 1e-9),
        ("n_sum((-1)^k/k, [k, 1, inf]) + log(2)", 1e-12),
        ("n_sum(1/2^i, [i, 0, inf, 2]) - 4/3", 1e-12),
        ("n_sum(1/i^2, [i, 100, 10^6]) - 0.010049166663833597", 1e-12),
        ("n_product(1 + 1/k^2, [k, 1, inf]) - sinh(pi)/pi", 1e-9),
        ("n(sum(exp(-k^5), [k, 1, inf])) - 0.367879441171455", 1e-12),
    ] {
        let v = s.run(&format!("abs(n({src}))"));
        let x: f64 = v.parse().unwrap_or_else(|_| panic!("{src}: {v}"));
        assert!(x < want, "{src}: {v}");
    }
}

/// Every r_solve result satisfies its recurrence: substituted at n = 2..7 with the constants c(i) given values.
#[test]
fn recurrences_checked_by_substitution() {
    let mut s = Sys::new();
    let cases = [
        ("A(m + 1) - 2*A(m) - 1", "[]"),
        ("A(m + 1) - 2*A(m) - 1", "[a(0) == 1]"),
        ("A(m + 1) - 3*A(m)", "[]"),
        ("A(m + 1) - 4*A(m) - k*exp(m)", "[a(0) == 3]"),
        ("A(m + 2) - 5*A(m + 1) + 6*A(m) - m", "[]"),
        ("A(m + 2) - 5*A(m + 1) + 6*A(m) - m", "[a(0) == 1, a(1) == 3]"),
        ("A(m + 2) - 7*A(m + 1) + 12*A(m) - 1", "[]"),
        ("A(m + 2) - 4*A(m + 1) + 3*A(m) - m*4^m", "[]"),
        ("A(m + 2) - 3*A(m + 1) + 2*A(m) - 2^m", "[]"),
        ("A(m + 1) - (m^2 - 1/4)/(m + 1)*A(m)", "[]"),
        ("A(m + 1) - (m + 1)*A(m)", "[a(0) == 1]"),
        ("(m + 4)*A(m + 2) - A(m + 1) - (m + 1)*A(m)", "[]"),
        ("2*A(m) - 2*m*(-A(m) + A(m + 1)) + m*(1 + m)*(A(m) - 2*A(m + 1) + A(m + 2))", "[]"),
        ("A(m + 2) - A(m + 1) - A(m)", "[a(0) == 0, a(1) == 1]"),
    ];
    for (rec, inits) in cases {
        let eq = rec.replace("A(", "a(");
        s.run(&format!("sol = r_solve_value(join([{eq} == 0], {inits}), a(m), m)"));
        let sol = s.run("sol");
        assert!(!sol.starts_with("r_solve_value"), "{rec}: unsolved");
        let fixed = s.run("rewrite(sol, [c(1) -> 3/7, c(2) -> 5/11, k -> 2])");
        let fsrc = rec.replace("A(", "ff(").replace('k', "2");
        let v = s.run(&format!("ff = (m => {fixed}); max(table(abs(n({fsrc})), m, range(2, 7)))"));
        let x: f64 = v.parse().unwrap_or_else(|_| panic!("{rec}: {v} for {sol}"));
        assert!(x < 1e-9, "{rec} {inits}: residual {v} for {sol}");
    }
}

#[test]
fn r_solve_forms() {
    check(&[
        ("r_solve(a(m + 1) == 3*a(m), a(m), m)", "[[a(m) -> c(1)*3^(-1 + m)]]"),
        ("r_solve([a(m + 1) == 2*a(m) + 5, a(0) == 3], a(m), m)", "[[a(m) -> -5 + 2^(3 + m)]]"),
        ("r_solve(a(m + 2) - 5*a(m + 1) + 6*a(m) == m, a(m), m)", "[[a(m) -> (1/4)*(3 + 2*m) + c(1)*2^m + c(2)*3^m]]"),
        ("r_solve(a(m + 2) - 7*a(m + 1) + 12*a(m) == 1, a(m), m, generated_parameters: f)", "[[a(m) -> 1/6 + f(1)*3^m + f(2)*4^m]]"),
        ("r_solve_value([a(m + 1) == c*a(m), a(0) == 1], a, m)", "m => c^m"),
        ("r_solve_value([a(m + 1) == 2*a(m), a(0) == 1], [a(3), a(4)], m)", "[8, 16]"),
        ("r_solve_value([x(m + 1) == (1 + x(m))^2, x(0) == 0], x(5), m)", "458329"),
        ("recurrence_table([a(m) == a(m - 1) + a(m - 2), a(1) == 1, a(2) == 1], a, [m, 10])", "[1, 1, 2, 3, 5, 8, 13, 21, 34, 55]"),
    ]);
}

#[test]
fn differences_and_ratios() {
    check(&[
        ("difference_delta((i^2 + i + 1)*2^i, i)", "(5 + 5*i + i^2)*2^i"),
        ("difference_delta(digamma(i), i)", "i^-1"),
        ("difference_delta(a^i, [i, p])", "a^i*(-1 + a)^p"),
        ("difference_delta(factorial_power(i, p), i)", "p*factorial_power(i, -1 + p)"),
        ("discrete_shift(gamma(i), [i, 3])", "(2*i + 3*i^2 + i^3)*gamma(i)"),
        ("discrete_shift(fibonacci(i), [i, 5])", "3*fibonacci(i) + 5*fibonacci(1 + i)"),
        ("discrete_ratio(binomial(p, x), x)", "(p - x)/(1 + x)"),
        ("discrete_ratio(pochhammer(a, x), x)", "a + x"),
    ]);
}

#[test]
fn generating_functions_and_convergence() {
    check(&[
        ("generating_function(a^m, m, z)", "(1 + -1*a*z)^-1"),
        ("generating_function(1/factorial(m)^2, m, x)", "bessel_i(0, 2*x^(1/2))"),
        ("generating_function(m*(m + 1)*(-1/2)^m, m, z)", "-8*z*(2 + z)^-3"),
        ("generating_function(f(m + 1), m, x)", "generating_function(f(m), m, x)/x + -1*f(0)/x"),
        ("sum_convergence(1/m, m)", "false"),
        ("sum_convergence(1/m^2, [m, 0, inf])", "false"),
        ("sum_convergence(3^m*m^2/factorial(m), m)", "true"),
        ("sum_convergence((-1)^m/(2*m + 1), m)", "true"),
        ("sum_convergence(1/m^b, m)", "re(b) > 1"),
        ("sum_convergence((x/2)^m, m)", "abs(x) < 2"),
    ]);
}

/// A long sum stops at the interrupt.
#[test]
fn interruptible() {
    let mut s = Sys::new();
    let (r, took) = s.interrupted("n_sum(1/k^2 + sin(k)/k^3, [k, 1, 10^9])", Duration::from_millis(300));
    assert!(took < Duration::from_secs(10), "took {took:?}: {r:?}");
}
