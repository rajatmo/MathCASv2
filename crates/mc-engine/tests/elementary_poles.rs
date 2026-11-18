//! The circular and hyperbolic functions at their exact poles answer as the special functions do at theirs
//! (`gamma(0)`): `failure(pole, call)` where the function is unbounded on both sides; one-sided infinities stay the
//! infinities, symbolic multiples stay unevaluated unless proved integers, machine numbers give machine values, and
//! plots through the poles still draw (the samplers use machine numbers, and a failure among listed values is a gap).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn exact_poles_are_pole_failures() {
    let mut e = CoreEngine::new();
    for src in [
        "tan(pi/2)",
        "tan(3*pi/2)",
        "tan(-pi/2)",
        "cot(0)",
        "cot(pi)",
        "sec(pi/2)",
        "sec(-pi/2)",
        "csc(0)",
        "csc(2*pi)",
        "coth(0)",
        "csch(0)",
        "csch(im*pi)",
        "acsch(0)",
    ] {
        let r = ev(&mut e, src);
        assert!(r.starts_with("failure(pole, "), "{src}: {r}");
    }
    // the values beside them, and cot at the poles of tan
    for (src, v) in [("tan(pi/4)", "1"), ("cot(pi/2)", "0"), ("cot(3*pi/2)", "0"), ("sec(pi/3)", "2"), ("csc(pi/6)", "2"), ("coth(im*pi/2)", "0")] {
        assert_eq!(ev(&mut e, src), v, "{src}");
    }
    // one-sided infinities stay infinities
    for (src, v) in [("atanh(1)", "inf"), ("acoth(1)", "inf"), ("acoth(-1)", "-inf"), ("log(0)", "-inf")] {
        assert_eq!(ev(&mut e, src), v, "{src}");
    }
    // symbolic arguments stay; machine numbers give machine values
    assert_eq!(ev(&mut e, "tan(pi/2 + n*pi)"), "tan((1/2)*pi + n*pi)");
    assert_eq!(ev(&mut e, "sec(x)"), "sec(x)");
    assert_eq!(ev(&mut e, "cot(0.0)"), "machine_real(\"inf\")");
    assert_eq!(ev(&mut e, "is_number(tan(1.5707963267948966))"), "true");
    // a list keeps the failure in place
    assert_eq!(ev(&mut e, "table(tan(k*pi/4), [k, 0, 4])"), "[0, 1, failure(pole, tan((1/2)*pi)), -1, 0]");
}

#[test]
fn plots_through_poles_still_draw() {
    let mut e = CoreEngine::new();
    for src in [
        "plot(tan(x), [x, 0, pi])",
        "plot(csc(x), [x, 0, 2*pi])",
        "plot(sec(x), [x, -pi/2, pi/2])",
        "plot([tan(x), cot(x)], [x, 0, pi])",
        "plot(acsch(x), [x, -1, 1])",
        "list_plot(table(tan(k*pi/4), [k, 0, 8]))",
        "discrete_plot(tan(k*pi/4), [k, 0, 8])",
    ] {
        let r = ev(&mut e, src);
        assert!(r.starts_with("image("), "{src}: {r}");
    }
}
