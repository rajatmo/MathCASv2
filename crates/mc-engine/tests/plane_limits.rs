//! Limits from every direction of the complex plane for functions continuous at the point (the value
//! there). Checked against the one-sided limits along several rays, a separate method.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn continuous_functions_in_the_plane() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "max_limit(abs(x), x -> 0, direction: complexes)"), "0");
    assert_eq!(ev(&mut e, "min_limit(abs(x), x -> 0, direction: complexes)"), "0");
    // variations: a complex-valued function at a complex point, a real-valued one for max_limit; each equals its
    // limits along three rays
    for (f, a, kind) in [("exp(z)*z^2 + conjugate(z)", "1 + im", "limit"), ("re(z)^2 + abs(z - 2)", "im", "max_limit"), ("abs(z)^2/(z - 3)", "1/2", "limit")] {
        let v = ev(&mut e, &format!("v = {kind}({f}, z -> {a}, direction: complexes); v"));
        assert!(!v.contains("limit"), "{f}: {v}");
        for d in ["im", "-1 - im", "exp(im*pi/5)"] {
            let r = ev(&mut e, &format!("simplify(limit({f}, z -> {a}, direction: {d}) - v)"));
            assert_eq!(r, "0", "{f} along {d}: {v}");
        }
    }
}

#[test]
fn not_continuous_stays() {
    let mut e = CoreEngine::new();
    // a pole stays; on a branch cut the two sides differ, so there is no limit; a value that is not real for max_limit
    assert!(ev(&mut e, "limit(1/z, z -> 0, direction: complexes)").starts_with("limit("));
    assert!(ev(&mut e, "limit(sqrt(z), z -> -1, direction: complexes)").starts_with("failure(no_limit"));
    // beside the cut the function is continuous
    assert_eq!(ev(&mut e, "limit(abs(z), z -> -1, direction: complexes)"), "1");
    assert!(ev(&mut e, "max_limit(z, z -> im, direction: complexes)").starts_with("max_limit("));
}
