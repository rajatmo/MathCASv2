//! Round 21: tests for the bugs found by running the pages' claims against the engine; a test that still fails is
//! kept ignored until its bug is fixed. The crates the fixes belong to are named in each ignore reason; the tests
//! live here because the page runner they use (`runner`) does. Each runs one input in a fresh session, as a page does.
use mc_docs::runner;
use mc_nbfile::{Notebook, OutputKind};

/// The output lines of one input: (kind, text).
fn eval(src: &str) -> Vec<(OutputKind, String)> {
    let mut nb = Notebook::parse(&format!("#! mathcas 2 notebook\n\n#% cell\n{src}\n")).expect("a notebook");
    let dir = std::env::temp_dir();
    let ran = runner::run("bugs_round21", &mut nb, &dir, &mut |_, _| {});
    ran.into_iter().flat_map(|r| r.lines).map(|l| (l.kind, l.text)).collect()
}

fn value(src: &str) -> String {
    let lines = eval(src);
    lines.iter().filter(|(k, _)| *k == OutputKind::Value).map(|(_, t)| t.clone()).collect::<Vec<_>>().join("\n")
}

fn finite_number(s: &str) -> bool {
    s.trim().parse::<f64>().is_ok_and(f64::is_finite)
}

#[test]
fn divergent_sums_and_products_are_not_numbers() {
    for src in [
        "n_sum(1, [k, 1, inf])",
        "n_sum(k, [k, 1, inf])",
        "n_sum(1/k, [k, 1, inf])",
        "n_sum(1/sqrt(k), [k, 1, inf])",
        "n_product(1 + 1/k, [k, 1, inf])",
    ] {
        let v = value(src);
        assert!(!finite_number(&v), "{src} gave the number {v}");
    }
    for src in ["n_sum(2^k, [k, 0, inf])", "n_product(2, [k, 1, inf])"] {
        let v = value(src);
        assert!(!v.contains("nan"), "{src} gave {v}");
    }
}

#[test]
fn kronecker_delta_compares_numbers_by_value() {
    assert_eq!(value("1.0 == 1"), "true");
    assert_eq!(value("kronecker_delta(1.0, 1)"), "1");
    assert_eq!(value("kronecker_delta(2.5, 5/2)"), "1");
}

#[test]
fn kronecker_delta_of_different_values_is_zero() {
    assert_eq!(value("kronecker_delta(pi, 3)"), "0");
    assert_eq!(value("kronecker_delta(x, x + 1)"), "0");
}

#[test]
fn solve_applies_assumptions_to_an_expression_without_an_equals_sign() {
    assert_eq!(value("solve(x^2 - 4 == 0, x, assumptions: x < 0)"), "[[x -> -2]]");
    assert_eq!(value("solve(x^2 - 4, x, assumptions: x < 0)"), "[[x -> -2]]");
    assert_eq!(value("solve(x^2 - 1, x, assumptions: x > 0)"), "[[x -> 1]]");
}

#[test]
fn hypergeometric_1f1_of_an_exact_complex_argument_has_digits() {
    for src in [
        "n(hypergeometric_1f1(1, 2, 3 + im), 30)",
        "n(hypergeometric_1f1(1/2, 3/2, 1 + im), 30)",
        "n(hypergeometric_2f1(1, 2, 3, 1/2 + im/2), 30)",
    ] {
        let lines = eval(src);
        assert!(lines.iter().all(|(k, _)| *k == OutputKind::Value), "{src}: {lines:?}");
        assert!(lines.iter().any(|(_, t)| t.contains("im")), "{src}: {lines:?}");
    }
}

#[test]
fn infinities_and_nans_print_without_the_internal_constructor() {
    for src in ["1.0e300*1.0e300", "log(0.0)", "laplace_transform(t, t, 0)", "n_integrate(t, [t, 0, inf])", "sin(1.0e300*1.0e300)"] {
        let v = value(src);
        assert!(!v.contains("machine_real"), "{src} printed {v}");
    }
}

#[test]
#[ignore = "B21-14 (mc-plot): bar_legend is documented as the colour bar value of plot_legends, but density_plot says it does not use it"]
fn bar_legend_works_in_the_plots_it_is_documented_for() {
    let lines = eval("density_plot(x*y, [x, 0, 1], [y, 0, 1], plot_legends: bar_legend(\"viridis\"))");
    assert!(lines.iter().all(|(_, t)| !t.contains("options not used")), "{lines:?}");
}
