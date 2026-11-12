//! The precision contract (roadmap 16B) for `n_integrate`: `working_precision: 30` agrees with
//! `working_precision: 60` to 30 digits and its ball contains the 60-digit value, including the oscillatory stage
//! (P19-02) and the precision of `n` reaching an integral without a closed form (P19-01).
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
fn oscillatory_tails_at_working_precision() {
    contract(&[
        "n_integrate(sin(x)/x, [x, 0, inf], working_precision: {d})",
        "n_integrate(cos(x)/(1 + x^2), [x, -inf, inf], working_precision: {d})",
        "n_integrate(exp(im*x^2), [x, -inf, inf], working_precision: {d})",
        "n_integrate(sin(x)/x, [x, 1, inf], working_precision: {d})",
    ]);
}

#[test]
fn n_of_an_integral_without_closed_form() {
    contract(&["n(integrate(1/(x^5 + x + 1), [x, 0, 1]), {d})", "n_integrate(1/k^3 + exp(-k), [k, 17, inf], working_precision: {d})"]);
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    let got = e.eval("abs(n(integrate(1/(x^5 + x + 1), [x, 0, 1]), 40) - 0.6538920078155722543779450308759496825509) < 10^-39", &StopFlag::new()).unwrap();
    assert_eq!(got.printed, "true");
    let got = e.eval("abs(n_integrate(sin(x)/x, [x, 0, inf], working_precision: 60) - pi/2) < 10^-59", &StopFlag::new()).unwrap();
    assert_eq!(got.printed, "true");
}
