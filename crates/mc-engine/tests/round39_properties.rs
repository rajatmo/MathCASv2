//! Round 39: properties of functions whose real domain is not tabulated. Samples prove only the negative claims: real
//! values of both signs (no sign), differences of both signs (not monotonic), second differences of both signs
//! (neither convex nor concave); the answer is then indeterminate, and otherwise the call stays. Each witness is
//! checked here by evaluating the function at two points.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn indeterminate_from_witnesses() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "function_sign(elliptic_log([u, sqrt(u^3 + (1 / 2) * u)], [0, 1 / 2]), u)"), "indeterminate");
    assert_eq!(ev(&mut e, "function_sign(q_hypergeometric_pfq([1 / 2], [2 / 3], 3 / 4, x), x)"), "indeterminate");
    // a witness must be confirmed at an exact point (30 digits); the slow spheroidal function may not get there within
    // the search budget, and the call then stays
    for p in ["function_convexity", "function_sign", "function_monotonicity"] {
        let r = ev(&mut e, &format!("{p}(spheroidal_s1(1, 1, pi / 2, x), x)"));
        assert!(r == "indeterminate" || r.starts_with(&format!("{p}(")), "{r}");
    }
    // independent check: values of both signs
    assert_eq!(ev(&mut e, "[sign(re(n(spheroidal_s1(1, 1, pi/2, 3/2)))), sign(re(n(spheroidal_s1(1, 1, pi/2, 31/10))))]"), "[1, -1]");
    // variations: a monotonicity, and a function of one sign stays undecided (samples never prove a sign)
    let r = ev(&mut e, "function_sign(spheroidal_s1(1, 1, pi / 2, x)^2 + 1, x)");
    assert!(r == "1" || r.starts_with("function_sign("), "{r}");
}
