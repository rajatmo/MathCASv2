//! The Meijer G-function (round 22): exact reductions, the precision contract (30 digits agree
//! with 60 and the 30-digit ball contains the 60-digit value) on both sides of |z| = 1, at complex and negative
//! arguments and with coincident poles, values against named functions, threading and the derivative.
mod common;
use common::Sys;

fn contract_src(e: &str) -> String {
    format!(
        "{{ let a = n({e}, 30); let b = n({e}, 60); \
         [precision(a) >= 29, abs(re(a) - re(b)) + abs(im_part(a) - im_part(b)) <= 10^-29*abs(b), \
          abs(set_precision(re(a), inf) - re(b)) <= 10^-min(accuracy(re(a)), 10000), \
          abs(set_precision(im_part(a), inf) - im_part(b)) <= 10^-min(accuracy(im_part(a)), 10000)] }}"
    )
}

fn contract(exprs: &[&str]) {
    let mut s = Sys::new();
    let mut bad = vec![];
    for e in exprs {
        let got = s.run(&contract_src(e));
        if got != "[true, true, true, true]" {
            bad.push(format!("{e}: {got}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn precision_contract() {
    contract(&[
        // p < q, simple poles, the series in z
        "meijer_g([[1], [2]], [[2/3, 1/5], [4]], 3)",
        "meijer_g([[1/3], []], [[1/2, 1/7, 3/2], []], 5/2)",
        // p < q with coincident poles (perturbation)
        "meijer_g([[], []], [[0, 1], [1/2]], 2)",
        "meijer_g([[1/2], []], [[0, 0, 1], []], 3/2)",
        // p > q: the series in 1/z, with integer differences there
        "meijer_g([[31/3, 4], [2, 3]], [[5, 3/2], [1/10]], 3)",
        "meijer_g([[1/2, 1/3], [1/5]], [[0], []], 7/2)",
        // p = q on both sides of the unit circle
        "meijer_g([[1/2, 1/3], [1/5]], [[0, 1/7], [1/9]], 1/3)",
        "meijer_g([[1/2, 1/3], [1/5]], [[0, 1/7], [1/9]], 3)",
        // negative and complex arguments, complex parameters
        "meijer_g([[1/3], []], [[1/2, 1, 3/2], []], -1/2)",
        "meijer_g([[3 + im], [2]], [[2/3, 1/5], [4]], 3)",
        "meijer_g([[1/3], [1/4]], [[1/2, 1/5], [1/6]], 1 + 2*im)",
        // the generalized function
        "meijer_g([[1/3], []], [[1/2], [1]], 3/10, 1/2)",
    ]);
}

#[test]
fn named_functions() {
    let mut s = Sys::new();
    for (g, f) in [
        ("meijer_g([[], []], [[v], [-v]], z)", "bessel_j(2*v, 2*z^(1/2))"),
        ("meijer_g([[], []], [[b], []], -x)", "(-x)^b*exp(x)"),
        ("meijer_g([[1/2], []], [[0], [0]], x)", "pi^(1/2)*exp(-x/2)*bessel_i(0, x/2)"),
        ("meijer_g([[1/2, 3/2], []], [[0], [0]], x)", "-4*elliptic_e(-x)"),
        ("meijer_g([[], []], [[5], [4]], 5)", "625*5^(1/2)*bessel_j(1, 2*5^(1/2))"),
        ("meijer_g([[1/2], []], [[1], []], 0)", "0"),
        ("meijer_g([[1/2], []], [[0], [0]], 0)", "pi^(1/2)"),
    ] {
        assert_eq!(s.run(&format!("simplify({g} - ({f}))")), "0", "{g}");
    }
    assert_eq!(s.run("meijer_g([[1/2], [1/2]], [[], []], 0)"), "complex_infinity");
    assert_eq!(s.run("meijer_g([[1/2, 1/2, 1/2], []], [[0], [-3/2, -3/2]], -x)"), s.run("(16/9)*pi^(1/2)*hypergeometric_pfq([1/2, 1/2, 1/2], [5/2, 5/2], x)"));
    assert_eq!(s.run("meijer_g([[], []], [[b], [b - 1/2]], x)"), s.run("x^(b - 1/2)*sin(2*x^(1/2))/pi^(1/2)"));
}

#[test]
fn values_against_named_functions() {
    let mut s = Sys::new();
    for (g, f) in [
        // confluent: 2 K_0(2 sqrt z), 2 z^(1/2) K_1(2 sqrt z)
        ("meijer_g([[], []], [[0, 0], []], 1.5~40)", "2*bessel_k(0, 2*sqrt(1.5~40))"),
        ("meijer_g([[], []], [[1, 0], []], 0.7~40)", "2*sqrt(0.7~40)*bessel_k(1, 2*sqrt(0.7~40))"),
        // G^{1,1}_{1,1} on both sides of 1
        ("meijer_g([[1/2], []], [[1/4], []], 3.~40)", "gamma(3/4)*3.~40^(1/4)*4.~40^(-3/4)"),
        ("meijer_g([[1/2], []], [[1/4], []], 0.3~40)", "gamma(3/4)*0.3~40^(1/4)*1.3~40^(-3/4)"),
        // z^b e^-z at a negative argument (principal power)
        ("meijer_g([[], []], [[1/3], []], -2.~40)", "(-2.~40)^(1/3)*exp(2.~40)"),
    ] {
        assert_eq!(s.run(&format!("{{ let d = {g} - {f}; abs(re(d)) + abs(im_part(d)) < 10^-35 }}")), "true", "{g}");
    }
    // machine values
    assert_eq!(s.run("abs(meijer_g([[], []], [[0, 0], []], 2.) - 2*bessel_k(0, 2*sqrt(2.))) < 10^-14"), "true");
    assert_eq!(s.run("abs(meijer_g([[1], [2]], [[2.3, 1.5], [4]], 3) + 1.8225742114495949) < 10^-13"), "true");
}

#[test]
fn threading_and_derivative() {
    let mut s = Sys::new();
    assert_eq!(s.run("length(meijer_g([[1/3], []], [[1/2], [1]], [0.1, 0.3, 0.5], 1/2))"), "3");
    assert_eq!(s.run("abs(meijer_g([[1/3], []], [[1/2], [1]], 0.1, 1/2) - 0.05112845270810049) < 10^-13"), "true");
    // the derivative rule z G'(z) = G^{m,n+1}_{p+1,q+1}(z | 0, a; b, 1) against a central difference
    assert_eq!(
        s.run("{ let h = 10^-15; let z = n(3/2, 40); abs(meijer_g([[0, 1/3], []], [[1/2, 1/4], [2, 1]], z)/z - (meijer_g([[1/3], []], [[1/2, 1/4], [2]], z + h) - meijer_g([[1/3], []], [[1/2, 1/4], [2]], z - h))/(2*h)) < 10^-20 }"),
        "true"
    );
    // a reduced function differentiates through its named form
    assert_eq!(s.run("rewrite(d(meijer_g([[], []], [[1/2], [3/2]], x), x), x -> 2.0) < 0"), "false");
}
