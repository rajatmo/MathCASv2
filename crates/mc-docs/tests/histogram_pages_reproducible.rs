//! A page example that is meant to be reproducible gives the same value twice.
//! A test that still fails is kept ignored with the reason in its `ignore`; removing the `ignore` is the proof of a fix.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn val(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|x| format!("error: {x}"))
}

#[test]
fn histogram_page_examples_are_reproducible() {
    let page = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/functions/histogram.mcn")).unwrap();
    assert!(!page.contains("random_seed("), "histogram.mcn seeds with random_seed(n), which does not seed");
    assert!(page.contains("seed(3);"), "the page seeds with seed(n)");
    // `random_seed(3)` (one argument) is mc-random's to reject with a message; the page uses `seed`
    let a = val("seed(3); random_real()");
    let b = val("seed(3); random_real()");
    assert_eq!(a, b, "seed(3) seeds the generator");
}
