//! Round 39: a plot whose epilog holds a primitive with a coordinate that is not a number (an undefined symbol)
//! is drawn without that primitive, with a message; drawable primitives beside it stay, and content that is not
//! graphics still fails.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn symbolic_epilog_points_are_left_out() {
    let mut e = CoreEngine::new();
    ev(&mut e, "rule f(m_) := elliptic_e(m) - 2;");
    let r = ev(&mut e, "plot(f(m), [m, -2, 2], epilog: style(point([xzero, f(xzero)]), point_size(large), red))");
    assert!(r.starts_with("image("), "{r}");
    // variations: a list with a drawable point, a prolog, a failure term as a coordinate
    let r = ev(&mut e, "plot(sin(x), [x, 0, 1], epilog: [point([0.5, 0.5]), point([q, 1])])");
    assert!(r.starts_with("image("), "{r}");
    let r = ev(&mut e, "plot(sin(x), [x, 0, 1], prolog: [red, line([[0, 0], [w, 1]])])");
    assert!(r.starts_with("image("), "{r}");
    let r = ev(&mut e, "plot(sin(x), [x, 0, 1], epilog: point([failure(argument, g, \"no value\"), 1]))");
    assert!(r.starts_with("image("), "{r}");
    // numeric coordinates given by closed forms are drawn as before
    let r = ev(&mut e, "plot(sin(x), [x, 0, 1], epilog: point([sqrt(2)/2, sin(sqrt(2)/2)]))");
    assert!(r.starts_with("image("), "{r}");
}
