//! The precision contract (roadmap 16B) for root finding and optimisation: `working_precision: 30` agrees with
//! `working_precision: 60` to 30 digits and its ball contains the 60-digit value (P19-03, P19-04).
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
fn find_root_at_working_precision() {
    contract(&[
        "rewrite(x, find_root(cos(x) == x, [x, 1], working_precision: {d}))",
        "rewrite(x, find_root(x^3 - 2*x - 5, [x, 2], working_precision: {d}))",
        "rewrite(x, find_root(exp(x) == 3*x, [x, 0, 1], working_precision: {d}))",
    ]);
}

#[test]
fn n_minimize_at_working_precision() {
    contract(&[
        "first(n_minimize(x^4 - 3*x^2 - x, x, working_precision: {d}))",
        "rewrite(x, last(n_minimize(x^4 - 3*x^2 - x, x, working_precision: {d})))",
        "first(n_maximize(-(x - 1/3)^2 + y - y^2, [x, y], working_precision: {d}))",
        "first(n_minimize([(x - 2)^2 + (y - 1)^2 + 1, x^2 + y^2 <= 9], [x, y], working_precision: {d}))",
    ]);
}
