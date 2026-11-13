//! Round 39: series and limits of the non-analytic functions (abs, sign, arg, re, im_part, conjugate) of a
//! complex-valued argument, limits along complex directions, correctly rounded machine logarithms to a base, areas
//! of polygons with symbolic vertices, roots of functions with complex values on the real line, and cos, sin, tan of
//! the two-argument arctangent.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn num(src: &str) -> f64 {
    let s = ev(src);
    s.trim().parse::<f64>().unwrap_or_else(|_| panic!("{src} gave {s}"))
}

#[test]
fn series_of_non_analytic_functions() {
    // sign(1 - i t) = (1 - i t)/sqrt(1 + t^2) for real t
    assert_eq!(ev("series(sign(1 - im*t), t, 0, 2)"), "1 + -1im*t - t^2/2 + O(t^3)");
    assert_eq!(ev("normal(series(sign(1 - im*t), t, 0, 4)) - normal(series((1 - im*t)/sqrt(1 + t^2), t, 0, 4))"), "0");
    // abs(2 + i t) = sqrt(4 + t^2), arg(1 + i t) = atan(t), re and im_part of exp(i t) are cos and sin, the
    // conjugate of exp(i t) is exp(-i t): each against the series of the analytic form
    for (lhs, rhs) in [
        ("abs(2 + im*t)", "sqrt(4 + t^2)"),
        ("arg(1 + im*t)", "atan(t)"),
        ("arg(3 + 4*im + (1 + im)*t)", "atan(4/3) - atan(t/(25 + 7*t))"),
        ("re(exp(im*t))", "cos(t)"),
        ("im_part(exp(im*t))", "sin(t)"),
        ("conjugate(exp(im*t))", "exp(-im*t)"),
        ("abs(sin(t))", "sin(t)"),
    ] {
        // abs(sin(t)) at 2, where sin is positive; the others at 0
        let point = if lhs.contains("sin(t)") { "t, 2" } else { "t, 0" };
        let d = ev(&format!("expand(normal(series({lhs}, {point}, 5)) - normal(series({rhs}, {point}, 5)))"));
        assert_eq!(d, "0", "{lhs}");
    }
    // a value of the series checked against the function at t = 1/10 (truncation error below 10^-6)
    let v = num("n(normal(series(abs(1 + im*t + t^2), t, 0, 6)) /. t -> 1/10)");
    let w = num("n(abs(1 + im/10 + 1/100))");
    assert!((v - w).abs() < 1e-6, "{v} {w}");
    // declined: abs of a real argument through 0 without a side (it was O(x^4)), arg across its cut, a parameter
    // of unknown realness; with the side given, abs(x) = x
    assert_eq!(ev("series(abs(x), x, 0, 3)"), "series(abs(x), x, 0, 3)");
    assert_eq!(ev("series(arg(-1 + im*t), t, 0, 3)"), "series(arg(-1 + 1im*t), t, 0, 3)");
    assert_eq!(ev("series(abs(a + x), x, 0, 2)"), "series(abs(a + x), x, 0, 2)");
    assert_eq!(ev("assuming(x > 0, series(abs(x), x, 0, 3))"), "x + O(x^4)");
    // real arguments as before
    assert_eq!(ev("series(abs(x), x, 1, 3)"), "1 + (x - 1) + O((x - 1)^4)");
}

#[test]
fn limits_of_non_analytic_functions() {
    // (sign(1 + i t) - 1)/t -> i (it was 0), (abs(1 + i t) - 1)/t -> 0 (it was i), against the analytic forms
    assert_eq!(ev("limit((sign(1 + im*t) - 1)/t, t -> 0)"), "1im");
    assert_eq!(ev("limit(((1 + im*t)/sqrt(1 + t^2) - 1)/t, t -> 0)"), "1im");
    assert_eq!(ev("limit((abs(1 + im*t) - 1)/t, t -> 0)"), "0");
    assert_eq!(ev("limit((abs(1 + im*t) - 1)/t^2, t -> 0)"), "1/2");
    assert_eq!(ev("limit((sqrt(1 + t^2) - 1)/t^2, t -> 0)"), "1/2");
    assert_eq!(ev("limit(arg(1 + im*t)/t, t -> 0)"), "1");
}

#[test]
fn limits_along_complex_directions() {
    // x = a - d s with s -> 0 from above: direction im approaches from below the real axis
    assert_eq!(ev("limit(sqrt(-2 + e1), e1 -> 0, direction: im)"), "-1im*2^(1/2)");
    assert_eq!(ev("limit(sqrt(-2 + e1), e1 -> 0, direction: -im)"), "1im*2^(1/2)");
    // the values just beside the cut agree
    assert!((num("im_part(n(sqrt(-2 - 10^-8*im)))") + 2f64.sqrt()).abs() < 1e-6);
    assert_eq!(ev("limit(exp(im/x), x -> 0, direction: im)"), "0");
    // the derivative of the conjugate along the imaginary axis is -1, along the real axis 1
    assert_eq!(ev("limit((conjugate(1 + h) - conjugate(1))/h, h -> 0, direction: -im)"), "-1");
    assert_eq!(ev("limit((conjugate(1 + h) - conjugate(1))/h, h -> 0, direction: exp(im*pi/4))"), "-1im");
    assert_eq!(ev("limit((exp(x) - 1)/x, x -> 0, direction: 1 + im)"), "1");
    // every direction at once is not decided: unevaluated, not a failure
    assert_eq!(ev("limit(abs(x), x -> 0, direction: complexes)"), "limit(abs(x), x -> 0, direction: complexes)");
}

#[test]
fn machine_logarithms_to_a_base_are_correctly_rounded() {
    assert_eq!(ev("log(2.0, 8.0)"), "3.0");
    assert_eq!(ev("log_base(2.0, 8.0)"), "3.0");
    assert_eq!(ev("[log2(8.0), log10(1000.0), log10(0.001), log(3.0, 81.0), log(2.0, 1.0)]"), "[3.0, 3.0, -3.0, 4.0, 0.0]");
    // exact powers b^k (all exactly representable) give exactly k
    for b in [2i64, 3, 5, 6, 7, 10, 12, 17] {
        let mut p = 1i64;
        for k in 0..40 {
            if p > (1 << 53) {
                break;
            }
            assert_eq!(num(&format!("log({b}.0, {p}.0)")), k as f64, "{b}^{k}");
            p = match p.checked_mul(b) {
                Some(q) => q,
                None => break,
            };
        }
    }
    // other arguments: within half an ulp of the quotient of the exact logarithms, measured at 50 digits
    for (b, x) in [("2.0", "10.0"), ("10.0", "5.0"), ("3.0", "7.0"), ("1.5", "100.0"), ("0.5", "3.0"), ("7.0", "0.001"), ("10.0", "2.0")] {
        let r = num(&format!("log({b}, {x})"));
        let ulp = 2f64.powi(r.abs().log2().floor() as i32 - 52);
        let err = num(&format!(
            "n(abs(set_precision({r:?}, inf) - log(set_precision({x}, inf))/log(set_precision({b}, inf))), 20)"
        ));
        assert!(err <= ulp / 2.0, "log({b}, {x}) = {r}: error {err}, ulp {ulp}");
    }
    // a complex value: real part exactly 3, imaginary part pi/log(2)
    assert_eq!(ev("re(log(2.0, -8.0))"), "3.0");
    assert!((num("im_part(log(2.0, -8.0))") - std::f64::consts::PI / std::f64::consts::LN_2).abs() < 1e-15);
}

#[test]
fn areas_of_polygons_with_symbolic_vertices() {
    assert_eq!(ev("area(parallelogram([p_x, p_y], [[u_x, u_y], [v_x, v_y]]))"), "abs(u_x*v_y + -1*u_y*v_x)");
    // a numeric parallelogram against the determinant, and a translated rectangle
    assert_eq!(ev("area(parallelogram([1, 2], [[3, 0], [1, 4]]))"), "12");
    assert_eq!(ev("area(polygon([[a, b], [a + 2, b], [a + 2, b + 3], [a, b + 3]]))"), "6");
    // a triangle: half the cross product of two edges (checked at a point)
    assert_eq!(ev("area(triangle([[x1, y1], [x2, y2], [x3, y3]])) /. [x1 -> 1, y1 -> 1, x2 -> 4, y2 -> 2, x3 -> 2, y3 -> 5]"), "11/2");
    // the centroid of a parallelogram is p + (u + v)/2
    assert_eq!(
        ev("simplify(region_centroid(parallelogram([p_x, p_y], [[u_x, u_y], [v_x, v_y]])) - [p_x + (u_x + v_x)/2, p_y + (u_y + v_y)/2])"),
        "[0, 0]"
    );
}

#[test]
fn roots_of_functions_complex_on_the_real_line() {
    // se(b_3(q), q, i/2) is imaginary for real q: Newton's method in complex arithmetic from the real start
    let q = num("q /. find_root(mathieu_s(mathieu_characteristic_b(3, q), q, im/2), [q, 15])[1]");
    let q30 = num("re(n(q /. find_root(mathieu_s(mathieu_characteristic_b(3, q), q, im/2), [q, 15], working_precision: 30)[1]))");
    assert!((q - 18.307764175196624).abs() < 1e-12 && (q - q30).abs() < 1e-13, "{q} {q30}");
    // the residual there is at the noise level, and the function changes sign along the real line
    assert!(num(&format!("abs(mathieu_s(mathieu_characteristic_b(3, {q:?}), {q:?}, im/2))")) < 1e-12);
    assert_eq!(ev("im_part(mathieu_s(mathieu_characteristic_b(3, 18.0), 18.0, im/2)) * im_part(mathieu_s(mathieu_characteristic_b(3, 18.5), 18.5, im/2)) < 0"), "true");
    // the even function on the imaginary axis is real; another order and the working precision
    let c = num("q /. find_root(mathieu_c(mathieu_characteristic_a(5, q), q, im/2), [q, 20])[1]");
    assert!((c - 22.666717080598346).abs() < 1e-10, "{c}");
    assert_eq!(ev("re(mathieu_s(2.0, 1.0, 0.3*im))"), "0.0");
    assert_eq!(ev("im_part(mathieu_c(2.0, 1.0, 0.3*im))"), "0");
    // machine values at a characteristic value (where the exponent is near an integer) against 30 digits
    for (f, a, q, z) in [("mathieu_s", "mathieu_characteristic_b(3, 15.3)", "15.3", "0.5*im"), ("mathieu_c", "mathieu_characteristic_a(2, 7.0)", "7.0", "0.4 + 0.3*im"), ("mathieu_s", "mathieu_characteristic_b(1, 40.0)", "40.0", "0.9")] {
        let m = num(&format!("a = {a}; abs({f}(a, {q}, {z}) - n({f}(set_precision(a, inf), set_precision({q}, inf), set_precision({z}, inf)), 30))/abs({f}(a, {q}, {z}))"));
        assert!(m < 1e-12, "{f} {a}: {m}");
    }
    // a polynomial times i: the real root sqrt(2)
    assert!((num("x /. find_root(im*(x^2 - 2), [x, 1])[1]") - 2f64.sqrt()).abs() < 1e-14);
}

#[test]
fn trigonometric_functions_of_the_two_argument_arctangent() {
    assert_eq!(ev("cos(atan(x, y))"), "x*(x^2 + y^2)^(-1/2)");
    assert_eq!(ev("sin(atan(x, y))"), "y*(x^2 + y^2)^(-1/2)");
    assert_eq!(ev("tan(atan(x, y))"), "y/x");
    assert_eq!(ev("from_polar_coordinates(to_polar_coordinates([x, y]))"), "[x, y]");
    // the forms at points of every quadrant against the numeric values
    for (x, y) in [(3.0, 4.0), (-3.0, 4.0), (-2.0, -5.0), (1.0, -7.0)] {
        let c = num(&format!("n(cos(atan(x, y)) /. [x -> {x:?}, y -> {y:?}])"));
        let s = num(&format!("n(sin(atan(x, y)) /. [x -> {x:?}, y -> {y:?}])"));
        let t = f64::atan2(y, x);
        assert!((c - t.cos()).abs() < 1e-15 && (s - t.sin()).abs() < 1e-15, "{x} {y}");
    }
    // the origin and a vanishing radius are left alone
    assert_eq!(ev("cos(atan(1, im))"), "cos(atan(1, 1im))");
    assert_eq!(ev("tan(atan(0, y))"), "tan(atan(0, y))");
}
