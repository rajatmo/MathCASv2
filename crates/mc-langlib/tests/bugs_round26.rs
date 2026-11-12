//! Round 26: engine bugs found in this round that concern this crate, one test each (ids B26-k); a test that still
//! fails is kept ignored until its bug is fixed, and removing the `ignore` is the proof of a fix.
use mc_api::{Engine, StopFlag};

fn ev(src: &str) -> String {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn b26_12_counting_function_with_a_single_count() {
    // exactly one of two is true
    assert_eq!(
        ev("boolean_table(boolean_counting_function([1], 2)(a, b), [a, b])"),
        "[false, true, true, false]"
    );
    // exactly two of three
    assert_eq!(
        ev("boolean_table(boolean_counting_function([2], 3)(a, b, c), [a, b, c])"),
        "[false, true, true, false, true, false, false, false]"
    );
    // none of two
    assert_eq!(
        ev("boolean_table(boolean_counting_function([0], 2)(a, b), [a, b])"),
        "[false, false, false, true]"
    );
}
