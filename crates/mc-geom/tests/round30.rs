//! Round 30: crossing counts as the least number of ray crossings, winding counts of polygons as regions.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::new();
    for (src, want) in cases {
        let got = e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"));
        assert_eq!(got, *want, "{src}");
    }
}

#[test]
fn crossing_and_winding_counts() {
    check(&[
        // the arrowhead: below its notch no ray needs to cross; on the notch the curve itself counts
        ("crossing_count(line([[1, 0], [0, 2], [-1, 0], [0, 1], [1, 0]]), [0, 1/2])", "0"),
        ("crossing_count(line([[1, 0], [0, 2], [-1, 0], [0, 1], [1, 0]]), [0, 1])", "1"),
        ("crossing_count(polygon([[0.35, 0.2], [0.9, 0.75], [0.1, 0.55], [0.9, 0.35], [0.42, 0.9]], [1, 2, 3, 4, 5]), [0.5, 0.7])", "1"),
        ("crossing_count(line([[0.35, 0.2], [0.42, 0.9], [0.9, 0.35], [0.1, 0.55], [0.9, 0.75], [0.35, 0.2]]), [0.5, 0.5])", "2"),
        // a polygon traced twice clockwise is the triangle once; a line keeps direction and multiplicity
        ("winding_count(polygon([[1/2, 1], [1, 0], [0, 0], [1/2, 1], [1, 0], [0, 0]]), [1/2, 1/2])", "1"),
        ("winding_count(line([[1/2, 1], [1, 0], [0, 0], [1/2, 1], [1, 0], [0, 0], [1/2, 1]]), [1/2, 1/2])", "-2"),
    ]);
}
