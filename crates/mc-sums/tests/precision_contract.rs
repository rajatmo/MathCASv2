//! The precision contract (roadmap 16B) for numeric sums and products: `working_precision: 30` agrees with
//! `working_precision: 60` to 30 digits and its ball contains the 60-digit value; `n` of an unevaluated sum at d digits.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// The checks of one template (`{d}` is the precision) as a list of booleans in the language: the 30-digit result
/// has 29 digits or more, agrees with the 60-digit one to 29 digits, and its ball contains the 60-digit value.
fn contract_src(template: &str) -> String {
    let a = template.replace("{d}", "30");
    let b = template.replace("{d}", "60");
    format!(
        "{{ let a = {a}; let b = {b};          [precision(a) >= 29, abs(re(a) - re(b)) + abs(im_part(a) - im_part(b)) <= 10^-29*abs(b),           abs(set_precision(re(a), inf) - re(b)) <= 10^-min(accuracy(re(a)), 10000),           abs(set_precision(im_part(a), inf) - im_part(b)) <= 10^-min(accuracy(im_part(a)), 10000)] }}"
    )
}

/// Every template meets the precision contract.
fn contract(templates: &[&str]) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    let mut bad = vec![];
    for t in templates {
        let got = match e.eval(&contract_src(t), &StopFlag::new()) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        if got != "[true, true, true, true]" {
            bad.push(format!("{t}: {got}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("
"));
}

#[test]
fn n_sum_with_an_exponential_tail_at_working_precision() {
    contract(&["n_sum(1/k^3 + 1/2^k, [k, 1, inf], working_precision: {d})"]);
}

#[test]
fn n_sum_at_working_precision() {
    contract(&[
        "n_sum(1/k^2, [k, 1, inf], working_precision: {d})",
        "n_sum(1/(k^3 + 1), [k, 1, inf], working_precision: {d})",
        "n_sum(1/k^(3/2), [k, 1, inf], working_precision: {d})",
        "n_sum((-1)^k/k, [k, 1, inf], working_precision: {d})",
        "n_sum((-1)^k/sqrt(k), [k, 1, inf], working_precision: {d})",
        "n_sum(1/factorial(k), [k, 0, inf], working_precision: {d})",
        "n_sum(1/k^2, [k, 1, 1000], working_precision: {d})",
    ]);
}

#[test]
fn n_sum_values() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    for (src, want) in [
        ("abs(n_sum(1/k^2, [k, 1, inf], working_precision: 50) - pi^2/6) < 10^-49", "true"),
        ("abs(n_sum((-1)^k/k, [k, 1, inf], working_precision: 50) + log(2)) < 10^-49", "true"),
        ("abs(n_sum(1/factorial(k), [k, 0, inf], working_precision: 50) - e) < 10^-49", "true"),
    ] {
        assert_eq!(e.eval(src, &StopFlag::new()).unwrap().printed, want, "{src}");
    }
}

#[test]
fn leading_zero_terms_are_not_convergence() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    let got = e.eval("abs(n_sum((n - 3)*unit_step(n - 3)*5^-n, [n, 0, inf]) - 1/400) < 10^-15", &StopFlag::new()).unwrap();
    assert_eq!(got.printed, "true");
}
