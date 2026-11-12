//! Round 26: engine bugs found in this round that concern this crate, one test each (ids B26-k); a test that still
//! fails is kept ignored until its bug is fixed, and removing the `ignore` is the proof of a fix.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig {
        threads: 1,
        ..EngineConfig::default()
    });
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err}"))
}

#[test]
fn b26_13_n_product_overflow() {
    // the product is about 2^5050; n(exp(1000)) is inf, so the same convention applies
    for src in [
        "n_product(1 + 2^k, [k, 1, 100])",
        "n_product(1 + 0.999^k, [k, 1, 5000])",
    ] {
        let out = ev(src);
        assert_eq!(out, "inf", "{src}: {out}");
    }
}
