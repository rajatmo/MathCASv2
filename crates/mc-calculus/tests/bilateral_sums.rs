//! Sums over all integers of quotients of Pochhammer symbols times a power: the bilateral hypergeometric series.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn run(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn pochhammer_quotients_over_all_integers() {
    assert_eq!(run("sum((pochhammer(a, k)/pochhammer(b, k))*z^k, [k, -inf, inf])"), "bilateral_hypergeometric_pfq([a], [b], z)");
    assert_eq!(
        run("sum(pochhammer(a, k)*pochhammer(c, k)/(pochhammer(b, k)*pochhammer(d, k)), [k, -inf, inf])"),
        "bilateral_hypergeometric_pfq([a, c], [b, d], 1)"
    );
    // no lower parameter (the negative side has no series of that kind), a parameter in k, a half-line: left alone
    assert_eq!(run("sum(pochhammer(a, k)*z^k, [k, -inf, inf])"), "sum(pochhammer(a, k)*z^k, [k, -inf, inf])");
    assert_eq!(run("sum(z^k/pochhammer(k, k), [k, -inf, inf])"), "sum(z^k/pochhammer(k, k), [k, -inf, inf])");
    assert!(!run("sum((pochhammer(a, k)/pochhammer(b, k))*z^k, [k, 0, inf])").contains("bilateral"));
}
