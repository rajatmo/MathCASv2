//! The constant attribute and the context patterns of `names`, the derivative of an inert sum, empty runs of
//! `sequence_cases` with `overlaps: all`, the bounds of a general ellipsoid and its picture, and large sparse
//! arrays. Expected values come from a second computation of the same quantity (the active sum, a count of the
//! runs, samples of the boundary, the defining formula of the entries).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread (one worker thread, fresh session); `TIMEOUT` after 60 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn built_in_constants_carry_the_constant_attribute() {
    assert_eq!(ev("attributes(pi)"), "[constant]");
    assert_eq!(ev("attributes(\"pi\")"), "[constant]");
    assert_eq!(ev("attributes(sin)"), "[listable]");
    // a string naming no known symbol: no answer
    assert_eq!(ev("attributes(\"NoSuchSymbolHere\")"), "attributes(\"NoSuchSymbolHere\")");
    assert_eq!(
        ev("map(to_expression, select(names(\"System`*\"), &(member_of(attributes(&1), constant))))"),
        "[catalan, champernowne_number, degree, e, euler_gamma, glaisher, pi*(3 - 5^(1/2)), golden_ratio, khinchin, 15.954589770191003, pi]"
    );
    // the values against digits from the literature (OEIS A006752, A001620, A074962, A002210)
    assert_eq!(ev("abs(n(catalan, 25) - 0.9159655941772190150546035) < 10^-24"), "true");
    assert_eq!(ev("abs(n(euler_gamma, 25) - 0.5772156649015328606065121) < 10^-24"), "true");
    assert_eq!(ev("abs(n(glaisher, 25) - 1.282427129100622636875343) < 10^-23"), "true");
    assert_eq!(ev("abs(n(khinchin, 25) - 2.685452001065306445309715) < 10^-23"), "true");
    // a user symbol may have it too, and lose it
    assert_eq!(ev("set_attributes(c0, constant); a = attributes(c0); clear_attributes(c0, constant); [a, attributes(c0)]"), "[[constant], []]");
}

#[test]
fn names_with_a_context_split_built_in_and_session_symbols() {
    let defs = "rule f(x_)(y_) := x + y; rule f1(x_)(y_)(z_) := x + y + z; ";
    assert_eq!(ev(&format!("{defs}names(\"Global`f*\")")), "[\"f\", \"f1\"]");
    assert_eq!(ev(&format!("{defs}map(sub_values, names(\"Global`f*\"))")), "[[f(x_)(y_) :> x + y], [f1(x_)(y_)(z_) :> x + y + z]]");
    assert_eq!(ev("member_of(names(\"System`*\"), \"factor\")"), "true");
    assert_eq!(ev("member_of(names(\"Global`*\"), \"factor\")"), "false");
    assert_eq!(ev("zzq = 1; [member_of(names(\"Global`*\"), \"zzq\"), member_of(names(\"System`*\"), \"zzq\")]"), "[true, false]");
}

#[test]
fn derivative_of_an_inert_sum_is_the_inert_sum_of_derivatives() {
    assert_eq!(ev("d(inactive(sum)(f(k, y), [k, a, b]), y)"), "inactive(sum)(derivative(0, 1)(f)(k, y), [k, a, b])");
    // the active sum differentiated agrees with the inert derivative activated
    assert_eq!(
        ev("expand(activate(d(inactive(sum)(sin(k*y)*y^k, [k, 1, 4]), y)) - d(sum(sin(k*y)*y^k, [k, 1, 4]), y))"),
        "0"
    );
    assert_eq!(ev("d(inactive(sum)(k^2*y, [k, 1, n], [j, 1, m]), y)"), "inactive(sum)(k^2, [k, 1, n], [j, 1, m])");
    // the index is bound: no dependence on it
    assert_eq!(ev("d(inactive(sum)(f(k), [k, 1, 3]), k)"), "0");
    assert_eq!(ev("d(inactive(sum)(k^2, [k, 1, 3]), y)"), "0");
    // a bound that varies: no derivative is claimed
    assert_eq!(ev("d(inactive(sum)(f(k, y), [k, a, y]), y)"), "d(inactive(sum)(f(k, y), [k, a, y]), y)");
}

#[test]
fn every_run_with_overlaps_all_includes_the_empty_ones() {
    // n elements: (n + 1)(n + 2)/2 runs [i, j) with 0 <= i <= j <= n, longest first at each start
    assert_eq!(ev("sequence_cases([a, b, c], _, overlaps: all)"), "[[a, b, c], [a, b], [a], [], [b, c], [b], [], [c], [], []]");
    assert_eq!(ev("length(sequence_cases(range(6), [___], overlaps: all)) == 7*8/2"), "true");
    // patterns that need an element are unchanged, and so are the other overlap settings
    assert_eq!(
        ev("sequence_cases([a, b, c, d], [__], overlaps: all)"),
        "[[a, b, c, d], [a, b, c], [a, b], [a], [b, c, d], [b, c], [b], [c, d], [c], [d]]"
    );
    assert_eq!(ev("sequence_cases([a, b, c], [___], overlaps: true)"), "[[a, b, c], [b, c], [c]]");
    assert_eq!(ev("sequence_count([a, b, c], _)"), "1");
}

#[test]
fn bounds_and_picture_of_an_ellipsoid_with_a_matrix() {
    assert_eq!(ev("region_bounds(ellipsoid([0, 0], [[5, 2], [2, 5]]), \"Minimal\")"), "[[-5^(1/2), 5^(1/2)], [-5^(1/2), 5^(1/2)]]");
    assert_eq!(ev("region_bounds(ellipsoid([1, 2, 3], [[4, 1, 0], [1, 9, 0], [0, 0, 1]]))"), "[[-1, 3], [-1, 5], [2, 4]]");
    // samples of the boundary c + L (cos t, sin t), L L^T = m (Cholesky), reach the bounds and stay inside them
    let sample = "l = [[5^(1/2), 0], [2/5^(1/2), (21/5)^(1/2)]]; \
                  ps = table(n(dot(l, [cos(t), sin(t)])), t, range(0, 2 pi, pi/2000)); \
                  [max(map(first, ps)) - n(5^(1/2)), max(map(last, ps)) - n(5^(1/2))]";
    let r = ev(sample);
    let v: Vec<f64> = r.trim_matches(['[', ']']).split(", ").map(|x| x.parse().unwrap()).collect();
    assert!(v.iter().all(|d| d.abs() < 1e-5 && *d <= 1e-12), "{r}");
    // not positive definite: no ellipsoid
    assert_eq!(ev("region_bounds(ellipsoid([0, 0], [[1, 2], [2, 1]]))"), "region_bounds(ellipsoid([0, 0], [[1, 2], [2, 1]]))");
    assert_eq!(ev("is_failure(show(graphics([rectangle([0, 0], [1, 1])]), ellipsoid([0, 0], [[5, 2], [2, 5]])))"), "false");
    assert_eq!(ev("is_failure(show(graphics([rectangle([0, 0], [1, 1])]), discretize_region(ellipsoid([0, 0], [[5, 2], [2, 5]]))))"), "false");
}

#[test]
fn sparse_arrays_past_a_million_entries() {
    assert_eq!(ev("identity_matrix(3, sparse_array) == identity_matrix(3)"), "true");
    // 2^20 entries and more stay as written
    assert_eq!(ev("head(sparse_array([[1] -> 1.0], [2^20]))"), "sparse_array");
    assert_eq!(ev("length(sparse_array([[1] -> 1.0], [2^20 - 1]))"), "1048575");
    let m = "id = identity_matrix(1001, sparse_array); \
             j = 1000^2*sparse_array([[i_, i_] -> -2., [i_, j_] when abs(i - j) == 1 -> 1.], [1001, 1001], 0.); \
             m = id - 0.001*j; ";
    // the entries by their formula: 1 + 2000 on the diagonal, -1000 next to it, 0 elsewhere
    assert_eq!(ev(&format!("{m}[dimensions(m), take(m[1], 3), take(m[500], [498, 502]), total(m[1001])]")), "[[1001, 1001], [2001.0, -1000.0, 0.0], [0.0, -1000.0, 2001.0, -1000.0, 0.0], 1001.0]");
}
