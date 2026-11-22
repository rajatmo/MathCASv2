//! Numeric integrals over regions, `n_integrate(f, element(vars, region))`, checked against values computed
//! independently: closed-form measures and a nested one-dimensional quadrature of the same solid.
use mc_api::{Engine, StopFlag};

fn value(src: &str) -> f64 {
    let mut e = mc_engine::CoreEngine::new();
    let got = e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"));
    got.parse().unwrap_or_else(|_| panic!("{src}: not a number: {got}"))
}

#[test]
fn implicit_solid_is_integrated_with_error_control() {
    // x^2 + 2 y^2 + 3 z^2 < 1 with -1 < x < y < z < 1: the volume by nested quadrature in z (the interval
    // max(y, -s) < z < min(1, s), s = sqrt((1 - x^2 - 2 y^2)/3)), then y > x, then x, is 0.30132771462 to 5e-8;
    // the numeric measure of the solid (a rough estimate) gave 0.30040
    let v = value("n_integrate(1, element([x, y, z], implicit_region(x^2 + 2*y^2 + 3*z^2 < 1 and -1 < x and x < y and y < z and z < 1, [x, y, z])))");
    assert!((v - 0.301_327_714_6).abs() < 1e-7, "{v}");
    // the region's own variable names differ from the integration variables
    let v = value("n_integrate(u^2, element([u, w], implicit_region(a^2 + b^2 < 1, [a, b])))");
    assert!((v - std::f64::consts::PI / 4.0).abs() < 1e-7, "{v}");
}

#[test]
fn constant_over_a_cone_is_its_volume() {
    // the default cone: radius 1, height 2, volume 2 pi/3
    let v = value("n_integrate(1, element([x, y, z], cone()))");
    assert!((v - 2.0 * std::f64::consts::PI / 3.0).abs() < 1e-12, "{v}");
}
