//! Centered intervals of the Legendre functions P and Q, the radial Zernike polynomial of any degree and the Weber
//! function E: every value on a grid of the box (30 digits) lies inside the result, the result is tight to first
//! order (the mean value form), and arguments outside the regions of the formulas (the branch points 1 and -1, poles
//! of the Gamma factors, large arguments, boxes off the real line for the 2F1 forms) stay unevaluated or still enclose, within a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The result of `f` (a template in X) on centered_interval(c, d) and the grid points of the box whose values lie
/// outside it.
fn check(e: &mut CoreEngine, f: &str, c: &str, d: &str) -> (String, String) {
    let r = ev(e, &format!("rc = {}; rc", f.replace('X', &format!("centered_interval({c}, {d})"))));
    if !r.starts_with("centered_interval(") {
        return (r, "[]".into());
    }
    let fz = f.replace('X', "z");
    let bad = ev(
        e,
        &format!(
            "c0 = {c}; d0 = {d}; \
             pts = flatten(table(c0 + (2 i/6 - 1) re(d0) + (2 j/6 - 1) im_part(d0) im, [i, 0, 6], [j, 0, 6])); \
             select(pts, z => {{ let v = n({fz}, 30); \
                not(abs(re(v) - re(rc[1])) <= re(rc[2]) and abs(im_part(v) - im_part(rc[1])) <= im_part(rc[2])) }})"
        ),
    );
    (r, bad)
}

#[test]
fn boxes_enclose_and_are_tight() {
    let t0 = Instant::now();
    let mut e = CoreEngine::new();
    let mut failures = vec![];
    for (f, c, d) in [
        ("legendre_p(1/2, 2/3, X)", "2", "1/100"),
        ("legendre_p(1/2, 2/3, X)", "0.3", "1/100"),
        ("legendre_p(1/2, X)", "0.3", "1/100"),
        ("legendre_q(1/2, X)", "2", "1/100"),
        ("zernike_r(2*im, 1, X)", "2", "1/100"),
        ("zernike_r(5/2, 1/2, X)", "0.7", "1/100"),
        ("weber_e(1/3, X)", "2", "1/100"),
        ("weber_e(2*im, X)", "1 + im", "(1 + im)/100"),
    ] {
        let (r, bad) = check(&mut e, f, c, d);
        if !r.starts_with("centered_interval(") {
            failures.push(format!("{f} on {c} -+ {d} stays: {r}"));
        } else if bad != "[]" {
            failures.push(format!("{f} on {c} -+ {d} = {r}: outside at {bad}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // first order: |P'(2)| per part is about 0.0964 and 0.167 (radius 0.01), so the radii are below 0.0011 and 0.0018
    let r = ev(&mut e, "rc = legendre_p(1/2, 2/3, centered_interval(2, 1/100)); [re(rc[2]) < 0.0011, im_part(rc[2]) < 0.0019]");
    assert_eq!(r, "[true, true]");
    assert!(t0.elapsed() < Duration::from_secs(120), "took {:?}", t0.elapsed());
}

#[test]
fn boxes_outside_the_regions() {
    let t0 = Instant::now();
    let mut e = CoreEngine::new();
    // boxes that hold a branch point stay
    for src in [
        "legendre_p(1/2, 2/3, centered_interval(1, 1/100))",
        "legendre_p(1/2, 2/3, centered_interval(-1, 1/100))",
        "legendre_q(1/2, centered_interval(1, 1/100))",
        "legendre_q(1/2, centered_interval(-1, 1/100))",
        // boxes off the real line are not taken by the 2F1 forms
        "legendre_p(1/3, 1/4, centered_interval(1/2 + im, (1 + im)/100))",
        "legendre_q(1/3, centered_interval(1/2 + im/2, (1 + im)/100))",
        "zernike_r(5/2, 1/2, centered_interval(1/2 + im/2, (1 + im)/100))",
    ] {
        let t = Instant::now();
        let r = ev(&mut e, src);
        assert!(!r.starts_with("centered_interval("), "{src}: {r}");
        assert!(t.elapsed() < Duration::from_secs(10), "{src}: {:?}", t.elapsed());
    }
    // a large argument of the Weber series and an integer order (poles of the Gamma factors) end quickly and enclose
    // or stay; Q of an integer degree (sin(nu pi) = 0) is not taken by the combination
    for (f, c, d) in [
        ("weber_e(1/3, X)", "40", "1/100"),
        ("weber_e(2, X)", "2", "1/100"),
        ("weber_e(1/3, X)", "0", "1/100"),
        ("legendre_q(1, X)", "2", "1/100"),
        ("legendre_p(1/2, 2/3, X)", "1000", "1"),
        ("zernike_r(2*im, 1, X)", "50", "1/100"),
        ("zernike_r(2, 1, X)", "0", "1/100"),
    ] {
        let t = Instant::now();
        let (r, bad) = check(&mut e, f, c, d);
        assert!(t.elapsed() < Duration::from_secs(20), "{f} on {c}: {:?}", t.elapsed());
        assert_eq!(bad, "[]", "{f} on {c} -+ {d} = {r}");
    }
    assert!(t0.elapsed() < Duration::from_secs(120), "took {:?}", t0.elapsed());
}
