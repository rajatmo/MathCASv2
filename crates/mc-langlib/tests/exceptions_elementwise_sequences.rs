//! Exceptions with a string type, elementwise operations on associations with different keys, and longest
//! common sequences that keep their values.
use mc_api::{Engine, StopFlag};

fn ev(src: &str) -> String {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn exception_with_a_string_type() {
    assert_eq!(ev("exception(\"s\")"), "exception(\"s\", assoc())");
    assert_eq!(ev("exception(1)"), "exception(1, assoc())");
    assert_eq!(ev("exception(\"s\", 5)"), "exception(\"s\", assoc(payload -> 5))");
}

#[test]
fn elementwise_on_associations_with_different_keys() {
    assert_eq!(ev("elementwise(plus)(assoc(a -> 1, b -> 2), assoc(a -> 3, c -> 4))"), "failure(shape_mismatch)");
    assert_eq!(ev("elementwise(plus)(assoc(a -> 1, b -> 2), assoc(a -> 3, b -> 4))"), "assoc(a -> plus(1, 3), b -> plus(2, 4))");
}

#[test]
fn longest_common_sequences_keep_their_values() {
    assert_eq!(ev("longest_common_sequence([1, 3, 2, 5, 4], [3, 5, 2, 4])"), "[3, 2, 4]");
    assert_eq!(ev("longest_common_sequence(\"AGGTAB\", \"GXTXAYB\")"), "\"GTAB\"");
    assert_eq!(ev("longest_common_sequence([], [1])"), "[]");
    assert_eq!(ev("longest_common_subsequence([1, 2, 3, 4, 2, 3], [9, 2, 3, 4, 5])"), "[2, 3, 4]");
    assert_eq!(ev("longest_common_subsequence([1, 1, 1], [1, 1])"), "[1, 1]");
    assert_eq!(ev("longest_common_sequence(range(1500), range(1500)) === range(1500)"), "true");
}
