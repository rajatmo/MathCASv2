//! `digits: d`: roots refined with balls, printed to about d digits, every printed digit correct; simple real roots
//! verified by the interval Newton step (`verify: true` makes an unverified root a failure). The expected values
//! are exact values (radicals, pi) or given to 100 digits.
mod common;
use common::{complex, correct_digits, significant, Sys};

const SQRT2: &str = "1.4142135623730950488016887242096980785696718753769480731766797379907324784621070388503875343276415727";
const OMEGA: &str = "0.5671432904097838729999686622103555497538157871865125081351310792230457930866845666932194469617522946";
const DOTTIE: &str = "0.7390851332151606416553120876738734040134117589007574649656806357732846548835";
const SQRT_PI: &str = "1.7724538509055160272981674833411451827975494561223871282138077898529112845910321813749506567385446654";
const HALF_SQRT3: &str = "0.8660254037844386467637231707529361834714026269051903140279034897259665084544000185405730933786242878";
const PI_6: &str = "0.5235987755982988730771072305465838140328615665625176368291574320690976337";
const X_HYP: &str = "1.9318516525781365734994863994577947352678096780168091008046861526208464279597110326951234837161409037";
const Y_HYP: &str = "0.5176380902050415246977976752480966566981378026398610276280064146301139494976039938447359493884993310";
const PI: &str = "3.1415926535897932384626433832795028841971693993751058209749445923078164062862089986280348253421170679";

/// The single value of a printed `[x -> v]`.
fn one(s: &mut Sys, src: &str) -> String {
    let v = s.rules(src);
    assert_eq!(v.len(), 1, "{src}");
    v[0].clone()
}

/// The printed value has about `d` digits (at least d - 1, at most d + 6) and all are correct.
fn assert_digits(src: &str, got: &str, want: &str, d: usize) {
    let n = significant(got);
    assert!(n + 1 >= d && n <= d + 6, "{src}: {n} digits printed, {d} asked: {got}");
    assert!(correct_digits(got, want), "{src}: {got} against {want}");
}

#[test]
fn verified_real_roots() {
    let mut s = Sys::new();
    let cases: &[(&str, &str, usize)] = &[
        ("find_root(x^2 - 2, [x, 1], digits: 30, verify: true)", SQRT2, 30),
        ("find_root(x^2 == 2, [x, 1], digits: 100, verify: true)", SQRT2, 100),
        ("find_root(x*exp(x) == 1, [x, 1], digits: 100, verify: true)", OMEGA, 100),
        ("find_root(cos(x) == x, [x, 1], digits: 60, verify: true)", DOTTIE, 60),
        ("find_root(x^2 - pi, [x, 1], digits: 40, verify: true)", SQRT_PI, 40),
        ("find_root(sin(x) - 1/2, [x, 0], digits: 50, verify: true)", PI_6, 50),
        ("find_root(sin(x), [x, 3], digits: 80, verify: true)", PI, 80),
        // through the secant and Brent paths: the verification uses the symbolic derivative all the same
        ("find_root(x^2 - 2, [x, 1, 2], digits: 45, verify: true)", SQRT2, 45),
        ("find_root(x^2 - 2, [x, 1, 0, 2], digits: 45, verify: true)", SQRT2, 45),
        // only digits: no failure if unverified, the same ball here
        ("find_root(x^2 - 2, [x, 1], digits: 20)", SQRT2, 20),
    ];
    for &(src, want, d) in cases {
        let got = one(&mut s, src);
        assert_digits(src, &got, want, d);
    }
}

#[test]
fn refined_systems_and_complex_roots() {
    let mut s = Sys::new();
    let v = s.rules("find_root([x^2 + y^2 - 4, x*y - 1], [[x, 2], [y, 0.5]], digits: 40)");
    assert_digits("x", &v[0], X_HYP, 40);
    assert_digits("y", &v[1], Y_HYP, 40);
    // Broyden's machine root, refined with ball forward differences
    let v = s.rules("rule q(x_ : Real, y_ : Real) := x*y - 1; find_root([x^2 + y^2 - 4, q(x, y)], [[x, 2], [y, 0.5]], digits: 30)");
    assert_digits("x", &v[0], X_HYP, 30);
    assert_digits("y", &v[1], Y_HYP, 30);
    // a complex root: real part exactly -1/2
    let v = s.rules("find_root(x^3 - 1, [x, -1 + im], digits: 30)");
    let (re, im) = complex(&v[0]);
    assert!(re.starts_with("-0.50000000000000000000000000000"), "{re}");
    assert_digits("im", &im, HALF_SQRT3, 30);
    // without a symbolic derivative in one variable
    let got = one(&mut s, "rule g(x_ : Real) := x^2 - 2; find_root(g(x), [x, 1], digits: 30)");
    assert_digits("g", &got, SQRT2, 30);
}

#[test]
fn unverified_roots() {
    let mut s = Sys::new();
    // a double root: the derivative vanishes at the root, no interval Newton proof exists
    let r = s.run("find_root((x - 1)^2, [x, 3], digits: 30, verify: true)");
    assert!(r.starts_with("failure(not_verified, find_root, [x -> 1.0000000000"), "{r}");
    // the same root without verify: a ball whose radius reflects the linear convergence
    let r = one(&mut s, "find_root((x - 1)^2, [x, 3], digits: 30)");
    assert!(r.starts_with("1.0000000000") && significant(&r) < 30, "{r}");
    // complex roots and systems are refined but not verified
    let r = s.run("find_root(x^2 + 1, [x, 1 + im], digits: 20, verify: true)");
    assert!(r.starts_with("failure(not_verified, find_root, [x -> complex("), "{r}");
    let r = s.run("find_root([x - 1, y - 2], [[x, 0], [y, 0]], digits: 20, verify: true)");
    assert!(r.starts_with("failure(not_verified"), "{r}");
    // goals beyond the working precision: not met, or met because the iteration reached the exact root (the triple root
    // keeps converging linearly since round 36 and meets the default accuracy goal: a 25-digit ball around 1)
    let r = s.run("find_root((x - 1)^3, [x, 3], digits: 30, precision_goal: 30)");
    assert!(r.starts_with("failure(not_converged, find_root, [x -> 1.0") || r.starts_with("[x -> 1.0000000000"), "{r}");
    if r.starts_with("[x ->") {
        let v = s.run("abs(rewrite(x, find_root((x - 1)^3, [x, 3], digits: 30, precision_goal: 30)) - 1) < 1e-20");
        assert_eq!(v, "true");
    }
}
