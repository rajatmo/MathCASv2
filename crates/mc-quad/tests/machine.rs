//! Machine-precision integrals: classic problems with known values, every argument form, methods, failures.
mod common;
use common::{close, Sys};

/// Hard classic cases against their exact values (tolerance relative to max(1, |value|)).
#[test]
fn classic_cases() {
    let mut s = Sys::new();
    let pi = std::f64::consts::PI;
    let cases: &[(&str, f64, f64)] = &[
        // endpoint singularities (the double-exponential rule, chosen because f is not numeric at 0)
        ("n_integrate(1/sqrt(x), [x, 0, 1])", 2.0, 1e-14),
        ("n_integrate(log(x), [x, 0, 1])", -1.0, 1e-14),
        ("n_integrate(x^(-0.9), [x, 0, 1])", 10.0, 1e-9),
        // the real line and half-lines (substitution)
        ("n_integrate(exp(-x^2), [x, -inf, inf])", pi.sqrt(), 1e-14),
        ("n_integrate(exp(-x), [x, 0, inf])", 1.0, 1e-13),
        ("n_integrate(1/(1 + x^2), [x, -inf, 0, inf])", pi, 1e-14),
        ("n_integrate(exp(x), [x, -inf, 0])", 1.0, 1e-13),
        ("n_integrate(1/x^2, [x, 1, inf])", 1.0, 1e-13),
        // sin(x)/x is 0/0 at 0: Si(100)
        ("n_integrate(sin(x)/x, [x, 0, 100])", 1.562_225_466_889_056_3, 1e-13),
        // a near-singular peak of width 1e-4
        ("n_integrate(1/((x - 0.3)^2 + 1e-8), [x, 0, 1])", 31_411.164_631_269_203, 1e-13),
        // an interior singularity at a splitting point
        ("n_integrate(log(abs(x - 0.3)), [x, 0, 0.3, 1])", -1.610_864_302_054_893_4, 1e-13),
        // oscillatory: about 500 periods
        ("n_integrate(exp(-x)*cos(1000*x), [x, 0, pi])", 9.567_851_249_511_027e-7, 1e-15),
        ("n_integrate(x^2, [x, 0, 3])", 9.0, 1e-15),
        ("n_integrate(x^2, [x, 3, 0])", -9.0, 1e-15),
        ("n_integrate(cos(x), [x, 0, pi/2])", 1.0, 1e-15),
        // square-root zeros at the ends: finite there, so Gauss–Kronrod, within the default goal
        ("n_integrate(sqrt(1 - x^2), [x, -1, 1])", pi / 2.0, 1e-9),
    ];
    for &(src, want, tol) in cases {
        let got = s.real(src);
        close(src, got, want, tol);
    }
}

#[test]
fn methods_agree_on_smooth_integrands() {
    let mut s = Sys::new();
    for f in ["exp(x)", "1/(1 + x^2)", "sin(x)^2"] {
        let a = s.real(&format!("n_integrate({f}, [x, 0, 2], method: gauss_kronrod)"));
        let b = s.real(&format!("n_integrate({f}, [x, 0, 2], method: double_exponential)"));
        let c = s.real(&format!("n_integrate({f}, [x, 0, 2])"));
        close(f, a, b, 1e-13);
        close(f, a, c, 1e-15);
    }
    // the Gauss–Kronrod method alone on an endpoint singularity reaches its depth limit near the goal (the
    // automatic choice takes the double-exponential rule here); the estimate is usable, so it is the value (with a
    // printed warning)
    let out = s.run("n_integrate(1/sqrt(x), [x, 0, 1], method: gauss_kronrod)");
    assert!(out.starts_with("1.99999999"), "{out}");
}

#[test]
fn goals_and_limits() {
    let mut s = Sys::new();
    let tight = s.real("n_integrate(exp(-x)*cos(50*x), [x, 0, 10], precision_goal: 14)");
    let want = (1.0 - (-10f64).exp() * (500f64.cos() - 50.0 * 500f64.sin())) / (1.0 + 2500.0);
    close("tight", tight, want, 1e-14);
    // a subdivision limit too small for 3000 oscillations: the estimate as a failure
    let out = s.run("n_integrate(cos(10000*x), [x, 0, pi], max_subdivisions: 4, method: gauss_kronrod)");
    assert!(out.starts_with("failure(not_converged, n_integrate, "), "{out}");
    let out = s.run("n_integrate(cos(10000*x)^2, [x, 0, pi], max_recursion: 2, method: gauss_kronrod)");
    assert!(out.starts_with("failure(not_converged, n_integrate, "), "{out}");
}

/// The real and imaginary parts of a printed complex number.
fn parts(out: &str) -> (f64, f64) {
    let inner = out.strip_prefix("complex(").and_then(|s| s.strip_suffix(')')).unwrap_or_else(|| panic!("{out}"));
    let (re, im) = inner.split_once(", ").unwrap();
    (re.parse().unwrap(), im.parse().unwrap())
}

#[test]
fn complex_integrands() {
    let mut s = Sys::new();
    let (re, im) = parts(&s.run("n_integrate(exp(im*x), [x, 0, 1])"));
    close("re", re, 1f64.sin(), 1e-15);
    close("im", im, 1.0 - 1f64.cos(), 1e-15);
    // real on part of the range only: sqrt(x - 1/2) = i sqrt(1/2 - x) below 1/2
    let (re, im) = parts(&s.run("n_integrate(sqrt(x - 0.5), [x, 0, 1])"));
    let w = (2.0 / 3.0) * 0.5f64.powf(1.5);
    close("re", re, w, 1e-9);
    close("im", im, w, 1e-9);
}

#[test]
fn multiple_integrals() {
    let mut s = Sys::new();
    close("triangle", s.real("n_integrate(x*y, [x, 0, 1], [y, 0, x])"), 0.125, 1e-14);
    close("square", s.real("n_integrate(exp(x + y), [x, 0, 1], [y, 0, 1])"), (1f64.exp() - 1.0).powi(2), 1e-13);
    // the area of the unit disc, the inner bounds depending on x
    let disc = s.real("n_integrate(1, [x, -1, 1], [y, -sqrt(1 - x^2), sqrt(1 - x^2)])");
    close("disc", disc, std::f64::consts::PI, 1e-9);
    close("cube", s.real("n_integrate(x*y*z, [x, 0, 1], [y, 0, 1], [z, 0, 2])"), 0.5, 1e-14);
    let g = s.real("n_integrate(exp(-x^2 - y^2), [x, -inf, inf], [y, -inf, inf])");
    // adaptive cubature (the default for 2 to 7 variables) meets the default goal of 8 digits with room to spare
    close("gauss 2d", g, std::f64::consts::PI, 1e-10);
}

#[test]
fn compiled_and_interpreted_agree_bitwise() {
    // the compiled backend's results are the interpreter's (mc-numfn decision N0.2), so the integrals are equal
    let mut s = Sys::new();
    for f in ["exp(-x)*cos(37*x)", "x^3 - 2*x + sin(x)", "1/(1 + x^4)", "log(1 + x)"] {
        let a = s.run(&format!("n_integrate({f}, [x, 0, 3], compile: true)"));
        let b = s.run(&format!("n_integrate({f}, [x, 0, 3], compile: false)"));
        let c = s.run(&format!("n_integrate({f}, [x, 0, 3])"));
        assert_eq!(a, b, "{f}");
        assert_eq!(a, c, "{f}");
    }
}

#[test]
fn failures() {
    let mut s = Sys::new();
    let cases: &[(&str, &str)] = &[
        ("n_integrate(x, [x, 0, 1], foo: 3)", "failure(unknown_option, foo)"),
        ("n_integrate(x, [x, 0, 1], method: simpson)", "failure(argument, n_integrate, method)"),
        ("n_integrate(x, [x, 0, 1], digits: 0)", "failure(argument, n_integrate, digits)"),
        ("n_integrate(x, [x, 0, 1], rigorous: maybe)", "failure(argument, n_integrate, rigorous)"),
        ("n_integrate(x, [x, 0])", "failure(argument, n_integrate, 2)"),
        ("n_integrate(x, [2, 0, 1])", "failure(argument, n_integrate, 2)"),
        ("n_integrate(x, [x, 0, a])", "failure(argument, n_integrate, 2)"),
        ("n_integrate(x, [x, 0, inf, 1])", "failure(argument, n_integrate, 2)"),
        // a complex end: the straight path (decision quad-2.1); a list integrand: each element
        ("im_part(n_integrate(2*x, [x, 0, 1 + im]))", "2.0000000000000004"),
        ("n_integrate(x, [x, 0, y])", "failure(argument, n_integrate, 2)"),
        ("n_integrate(x*y, [x, 0, 1], [x, 0, 1])", "failure(argument, n_integrate, 3)"),
        ("n_integrate(x*y, [x, 0, y], [y, 0, 1])", "failure(argument, n_integrate, 2)"),
        ("n_integrate(x*y, [x, 0, 1], [y, 0, y])", "failure(argument, n_integrate, 3)"),
        ("n_integrate([x, 3*x^2], [x, 0, 1])", "[0.5000000000000001, 1.0000000000000002]"),
        ("n_integrate(x*y, [x, 0, 1], [y, 0, 1], digits: 20)", "failure(argument, n_integrate, digits)"),
        ("n_integrate(x, [x, 0, 1], digits: 20, method: gauss_kronrod)", "failure(argument, n_integrate, method)"),
        ("n_integrate(g(x), [x, 0, 1])", "failure(not_numeric, g(x), [x -> 0.5])"),
        ("n_integrate(x)", "n_integrate(x)"),
    ];
    for (src, want) in cases {
        assert_eq!(s.run(src), *want, "{src}");
    }
    // divergent integrals
    for src in ["n_integrate(1/x, [x, 0, 1])", "n_integrate(1/x, [x, 1, inf])"] {
        let out = s.run(src);
        assert!(out.starts_with("failure(not_converged, n_integrate, "), "{src}: {out}");
    }
}

/// log(1 + u) and exp(u) - 1 are evaluated in their stable forms (log1p, expm1): the thermal integrand
/// log(1 - exp(-r)) converges at large r, where the plain form loses five digits.
#[test]
fn stable_forms() {
    let mut s = Sys::new();
    let jb = s.run("n_integrate(y^2*log(1 - exp(-sqrt(y^2 + 625))), [y, 0, inf])");
    let v: f64 = jb.parse().unwrap_or_else(|_| panic!("{jb}"));
    assert!((v + 2.3417398800487363e-9).abs() < 1e-17, "{jb}");
    assert_eq!(s.run("log1p(1.0e-20)"), "1e-20");
    assert_eq!(s.run("expm1(1.0e-20)"), "1e-20");
}

/// G50: the reciprocal and inverse hyperbolic functions in an integrand are compiled, not interpreted at every point
/// (an oscillatory integrand with `atanh` took seconds).
#[test]
fn inverse_hyperbolic_integrands_compile() {
    let mut s = Sys::new();
    s.run("m = 1.05; kappa = 0.1; nr = 100");
    s.run("fn hh(r) = (1 + 4*r - 2*r^2)*atanh(sqrt(1 - r)) - (1 + 2*r)*sqrt(1 - r)");
    s.run("fn ww(q) = kappa^4/(q^2 - kappa^2)^2*cos(q/2*(2*nr + 1)*pi/kappa)^2");
    let start = std::time::Instant::now();
    let v = s.run("n_integrate(ww(2*m^2/(100.0*r))*hh(r), [r, 0, 1])");
    let w = s.run("n_integrate(ww(2*m^2/(10.0*r))*hh(r), [r, 0, 1])");
    let took = start.elapsed();
    assert!(v.starts_with("66.175558929"), "{v}");
    assert!(w.starts_with("0.00112177459"), "{w}");
    assert!(took < std::time::Duration::from_secs(3), "took {took:?}");
    for (src, want) in [
        ("n_integrate(atanh(x), [x, 0, 1/2])", 0.5 * 0.5f64.atanh() + 0.5 * (0.75f64).ln()),
        ("n_integrate(sec(x)^2, [x, 0, 1])", 1f64.tan()),
        ("n_integrate(asinh(x), [x, 0, 1])", 1f64.asinh() - 2f64.sqrt() + 1.0),
    ] {
        let got: f64 = s.run(src).parse().unwrap_or(f64::NAN);
        close(src, got, want, 1e-12);
    }
}
