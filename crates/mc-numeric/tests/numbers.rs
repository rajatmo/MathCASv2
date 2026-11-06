//! The number functions of package 1C on the assembled engine (with `mc-numtheory`'s `abs` and `sign`): exact,
//! rational, ball and machine inputs, closed numeric terms, lists, failures, and properties on random inputs.
//! Expected values agree with the reference system where its meaning is the same (only values are recorded).
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn engine() -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() })
}

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn check(cases: &[(&str, &str)]) {
    let mut e = engine();
    let mut bad = vec![];
    for (src, want) in cases {
        let got = eval(&mut e, src);
        if got != *want {
            bad.push(format!("{src}\n    got  {got}\n    want {want}"));
        }
    }
    assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn complex_parts_of_numbers() {
    check(&[
        ("re(3 + 4im)", "3"),
        ("im_part(3 + 4im)", "4"),
        ("re(-7/2)", "-7/2"),
        ("im_part(-7/2)", "0"),
        ("im_part(2.5)", "0"),
        ("im_part(2.5~20)", "0"),
        ("re(complex(1.0, 2.0))", "1.0"),
        ("im_part(complex(1.0, 2.0))", "2.0"),
        ("im_part(complex(1.0, 0.0))", "0.0"),
        ("re(complex(1.5~20, 2.5~20))", "1.5000000000000000000"),
        ("im_part(complex(1.5~20, 2.5~20))", "2.5000000000000000000"),
        ("conjugate(1 + 2im)", "complex(1, -2)"),
        ("conjugate(1/2 - im/3)", "complex(1/2, 1/3)"),
        ("conjugate(complex(1.0, 2.0))", "complex(1.0, -2.0)"),
        ("conjugate(complex(1.5~20, 2.5~20))", "complex(1.5000000000000000000, -2.5000000000000000000)"),
        ("conjugate(-5)", "-5"),
        ("conjugate(2.5)", "2.5"),
        ("re([1 + im, 2, 3.0])", "[1, 2, 3.0]"),
        ("conjugate([1 + im, 2])", "[complex(1, -1), 2]"),
        ("im_part([[im], [2]])", "[[1], [0]]"),
    ]);
}

#[test]
fn complex_parts_of_terms() {
    check(&[
        ("re(3*pi + 2im*sqrt(2))", "3*pi"),
        ("im_part(3*pi + 2im*sqrt(2))", "2*2^(1/2)"),
        ("conjugate(3*pi + 2im*sqrt(2))", "-2im*2^(1/2) + 3*pi"),
        ("re((-1)^(1/3))", "1/2"),
        ("im_part((-1)^(1/3))", "(1/2)*3^(1/2)"),
        ("conjugate((-1)^(1/3))", "-1im*(-1)^(1/6)"),
        ("im_part(sqrt(-2))", "2^(1/2)"),
        ("re(exp(1) + log(2))", "e + log(2)"),
        ("re(x)", "re(x)"),
        ("re(x + 2)", "re(2 + x)"),
        ("re(re(x))", "re(x)"),
        ("im_part(re(x))", "0"),
        ("im_part(abs(x))", "0"),
        ("conjugate(abs(x))", "abs(x)"),
        ("conjugate(conjugate(x))", "x"),
        ("re(conjugate(x))", "re(x)"),
        ("im_part(conjugate(x))", "-im_part(x)"),
        ("re(2im*abs(x))", "0"),
        ("im_part(2im*abs(x))", "2*abs(x)"),
        ("re(log(-2))", "log(2)"),
    ]);
}

#[test]
fn arguments() {
    check(&[
        ("arg(1 + im)", "(1/4)*pi"),
        ("arg(-1 + 2im)", "pi - atan(2)"),
        ("arg(-3 - 4im)", "atan(4/3) - pi"),
        ("arg(3im)", "(1/2)*pi"),
        ("arg(-3im)", "(-1/2)*pi"),
        ("arg(1 - im*sqrt(3))", "arg(1 + -1im*3^(1/2))"),
        ("arg(0)", "0"),
        ("arg(5/2)", "0"),
        ("arg(-5/2)", "pi"),
        ("arg(-1.5)", "pi"),
        ("arg(1.0)", "0"),
        ("arg(-2.5~20)", "pi"),
        ("arg(complex(1.0, 1.0))", "0.7853981633974483"),
        ("arg(complex(-1.0, -1.0))", "-2.356194490192345"),
        ("arg(complex(0.0, 0.0))", "0.0"),
        ("arg(n(-1 + im, 30))", "2.356194490192344928846982537459627"),
        ("arg(complex(-1.0~30, -1.0~30))", "-2.35619449019234492884698253746"),
        ("arg(complex(0.0~20, 1.0~20))", "1.5707963267948966192"),
        // a ball around zero has no argument
        ("arg(1.0~3/3 - 1/3)", "arg(ball(0, 0.000733))"),
        ("arg(-pi)", "pi"),
        ("arg(2im*pi)", "(1/2)*pi"),
        ("arg(sqrt(-2)*pi)", "(1/2)*pi"),
        ("arg(abs(x))", "0"),
        ("arg(x)", "arg(x)"),
        ("arg([1, -1, im])", "[0, pi, (1/2)*pi]"),
    ]);
}

#[test]
fn abs_and_sign_of_closed_terms() {
    check(&[
        ("abs(3 + 4im)", "5"),
        ("abs(1 + im)", "2^(1/2)"),
        ("sign(3 + 4im)", "complex(3/5, 4/5)"),
        ("abs(-pi)", "pi"),
        ("abs(1 - pi)", "-1 + pi"),
        ("abs(3im*sqrt(2))", "3*2^(1/2)"),
        ("abs(sqrt(-2)*pi)", "pi*2^(1/2)"),
        ("abs((-1)^(1/3)*e)", "e"),
        ("abs(conjugate(x))", "abs(x)"),
        ("abs(-2*abs(x))", "2*abs(x)"),
        ("abs(x)", "abs(x)"),
        ("sign(-pi)", "-1"),
        ("sign(1 - sqrt(2))", "-1"),
        ("sign(sin(1))", "1"),
        ("sign(2im*pi)", "1im"),
        ("sign((3 + 4im)*pi)", "complex(3/5, 4/5)"),
        ("sign(x)", "sign(x)"),
    ]);
}

#[test]
fn precision_and_accuracy() {
    check(&[
        ("precision(1/3)", "inf"),
        ("accuracy(7)", "inf"),
        ("precision(pi)", "inf"),
        ("precision(x)", "inf"),
        ("precision(1.5)", "15.954589770191003"),
        ("accuracy(1.5)", "15.778498511135322"),
        ("accuracy(0.0)", "323.60724533877976"),
        ("precision(0.0)", "15.954589770191003"),
        ("precision(1.5~20)", "20.16900970948674"),
        ("accuracy(1.5~20)", "19.99291845043106"),
        ("precision(n(pi, 30))", "34.91947949702182"),
        ("accuracy(n(pi, 30))", "34.42232962432768"),
        // a three-digit ball divided by 3: the radius, not the working precision, limits it
        ("precision(1.0~3/3)", "2.959359440289832"),
        ("accuracy(1.0~3/3)", "3.436268688912093"),
        ("precision(1.0~3/3 - 1/3)", "0.0"),
        ("accuracy(1.0~3/3 - 1/3)", "3.010299956639812"),
        ("precision(0.0~30)", "0.0"),
        ("accuracy(0.0~30)", "30.10299956639812"),
        ("precision(complex(1.0, 2.0))", "15.954589770191003"),
        ("precision(complex(1.0~20, 1.0~30))", "20.16900970948674"),
        ("precision([1.5, n(pi, 30), x])", "15.954589770191003"),
        ("precision(n(pi, 10)*x + 2.5)", "15.05149978319906"),
    ]);
}

#[test]
fn set_precision() {
    check(&[
        ("set_precision(0.1, 20)", "0.10000000000000000555"),
        ("set_precision(1/3, 20)", "0.33333333333333333333"),
        ("set_precision(1.5~20, 40)", "1.500000000000000000000000000000000000000"),
        ("set_precision(n(pi, 50), 10)", "3.141592654"),
        ("set_precision(1.0~3/3, 20)", "0.33349609375000000000"),
        ("precision(set_precision(1.0~3/3, 20))", "20.16900970948674"),
        ("set_precision(complex(0.1, 0.2), 10)", "complex(0.100000000, 0.2000000000)"),
        // `pi + 0.1` is a machine real first (constants take a machine real's kind), then widened to 20 digits
        ("set_precision(pi + 0.1, 20)", "3.2415926535897932048"),
        ("set_precision(pi + 1/10, 20)", "3.24159265358979323846"),
        ("set_precision([1/2, x + 0.25], 10)", "[0.5000000000, 0.2500000000 + x]"),
        ("set_precision(0.1, inf)", "3602879701896397/36028797018963968"),
        ("set_precision(1/3, inf)", "1/3"),
        ("set_precision(1.0~3/3, inf)", "683/2048"),
        ("set_precision(complex(0.5, 0.25), inf)", "complex(1/2, 1/4)"),
        ("set_precision(0.1, 0)", "failure(invalid_argument, set_precision(0.1, 0))"),
        ("set_precision(0.1, 2.5)", "failure(invalid_argument, set_precision(0.1, 2.5))"),
        ("set_precision(0.1, d)", "set_precision(0.1, d)"),
        ("set_precision(0.1, 10^12)", "failure(limit, memory, set_precision(0.1, 1000000000000))"),
    ]);
}

#[test]
fn rationalize() {
    check(&[
        ("rationalize(3.14159)", "314159/100000"),
        ("rationalize(0.333333333333)", "0.333333333333"),
        ("rationalize(0.3333333333333333)", "1/3"),
        ("rationalize(0.1)", "1/10"),
        ("rationalize(-2.5)", "-5/2"),
        ("rationalize(0.0)", "0"),
        ("rationalize(n(pi, 20))", "3.14159265358979323846264"),
        ("rationalize(0.5~20)", "1/2"),
        ("rationalize(1.0~3/3)", "0.33"),
        ("rationalize(7/3)", "7/3"),
        ("rationalize(pi)", "pi"),
        ("rationalize(1.5*x + 0.25)", "1/4 + (3/2)*x"),
        ("rationalize(complex(0.5, 0.25))", "complex(1/2, 1/4)"),
        ("rationalize(complex(0.5, 0.333333333333))", "complex(0.5, 0.333333333333)"),
        ("rationalize(3.14159, 1/1000)", "201/64"),
        ("rationalize(3.14159, 0.01)", "22/7"),
        ("rationalize(0.1, 1/100)", "1/10"),
        ("rationalize(0.1, 0)", "1/10"),
        ("rationalize(355/113, 1/100)", "22/7"),
        ("rationalize(355/113, 0)", "355/113"),
        ("rationalize(pi, 1/100)", "22/7"),
        ("rationalize(pi, 1/10^7)", "75948/24175"),
        ("rationalize(pi, 3/10^7)", "355/113"),
        ("rationalize(sqrt(2), 1/1000)", "41/29"),
        ("rationalize([pi, e], 1/1000)", "[201/64, 87/32]"),
        ("rationalize(1.0~3/3, 1/100)", "1/3"),
        ("rationalize(pi, 0)", "pi"),
        ("rationalize(0.5, -1)", "failure(invalid_argument, rationalize(0.5, -1))"),
        ("rationalize(0.5, im)", "failure(invalid_argument, rationalize(0.5, 1im))"),
        ("rationalize(0.5, d)", "rationalize(0.5, d)"),
    ]);
}

#[test]
fn chop() {
    check(&[
        ("chop(1e-11)", "0"),
        ("chop(1e-9)", "1e-9"),
        ("chop(-1e-11)", "0"),
        ("chop(1/10^12)", "1/1000000000000"),
        ("chop([1e-12, 0.5, 1/10^12])", "[0, 0.5, 1/1000000000000]"),
        ("chop(1e-11 + 2im)", "complex(0.0, 2.0)"),
        ("chop(complex(3.0, 1e-12))", "3.0"),
        ("chop(complex(1e-12, 1e-12))", "0"),
        ("chop(0.001, 0.01)", "0"),
        ("chop(0.001, 1/1000)", "0.001"),
        ("chop(0.001, 0.002~10)", "0"),
        ("chop(n(sin(pi), 20))", "0"),
        ("chop((1.0~3/3 - 1/3)*10^-12)", "0"),
        ("chop(1.0~3/3 - 1/3)", "ball(0, 0.000733)"),
        ("chop(1.0~3/3 - 1/3, 1/100)", "0"),
        ("chop(1.5~20)", "1.5000000000000000000"),
        ("chop(x + 1e-12*y)", "x"),
        ("chop(0.5, 0)", "failure(invalid_argument, chop(0.5, 0))"),
        ("chop(0.5, -1.0)", "failure(invalid_argument, chop(0.5, -1.0))"),
        ("chop(0.5, d)", "chop(0.5, d)"),
    ]);
}

#[test]
fn clip_rescale_unit_step() {
    check(&[
        ("clip(5.0)", "1"),
        ("clip(-3/2)", "-1"),
        ("clip(1/2)", "1/2"),
        ("clip(1)", "1"),
        ("clip([-2, 0.5, 3])", "[-1, 0.5, 1]"),
        ("clip(-3, [0, 2])", "0"),
        ("clip(0.5, [0, 1], [-1, 2])", "0.5"),
        ("clip(5, [0, 1], [-1, 2])", "2"),
        ("clip(pi, [0, 3])", "3"),
        ("clip(pi, [0, 4])", "pi"),
        ("clip(2.5~20, [0, 1])", "1"),
        ("clip(1.0~3/3, [0, 1/3])", "clip(0.33, [0, 1/3])"),
        ("clip(x)", "clip(x)"),
        ("clip(1, [2, 0])", "failure(invalid_argument, clip(1, [2, 0]))"),
        ("clip(1, 2)", "failure(invalid_argument, clip(1, 2))"),
        ("clip(1, b)", "clip(1, b)"),
        ("rescale(-1, [0, 4])", "-1/4"),
        ("rescale(2, [0, 4], [10, 20])", "15"),
        ("rescale([1, 2, 4])", "[0, 1/3, 1]"),
        ("rescale([1, 1, 1])", "[0, 0, 0]"),
        ("rescale([2.0, 1, 3])", "[0.5, 0, 1]"),
        ("rescale([pi, 3, 4])", "[-3 + pi, 0, 1]"),
        ("rescale([x, 1])", "rescale([x, 1])"),
        ("rescale([1, 3], [0, 4])", "[1/4, 3/4]"),
        ("rescale(x, [0, 2])", "(1/2)*x"),
        ("rescale(0.5, [0, 2])", "0.25"),
        ("rescale(0.5~20, [0, 2])", "0.25000000000000000000"),
        ("rescale(1, [2, 2])", "failure(division_by_zero)"),
        ("unit_step(2.5)", "1"),
        ("unit_step(0)", "1"),
        ("unit_step(-0.0)", "1"),
        ("unit_step(-1/3)", "0"),
        ("unit_step(-2.5~20)", "0"),
        ("unit_step(1.0~3/3 - 1/3)", "unit_step(ball(0, 0.000733))"),
        ("unit_step(-1, 2)", "0"),
        ("unit_step(1, x)", "unit_step(x)"),
        ("unit_step(-1, x)", "0"),
        ("unit_step([-1, 0, 1])", "[0, 1, 1]"),
        ("unit_step(pi - 3)", "1"),
        ("unit_step(3 - pi)", "0"),
        ("unit_step(x)", "unit_step(x)"),
        ("unit_step(im)", "unit_step(1im)"),
    ]);
}

#[test]
fn n_with_the_digits_option() {
    check(&[
        ("n(pi, digits: 30)", "3.1415926535897932384626433832795029"),
        ("n(pi, 30)", "3.1415926535897932384626433832795029"),
        ("n(pi, 10, digits: 10)", "3.14159265358979"),
        ("n([1/3, x], digits: 5)", "[0.333333333, x]"),
        ("n(pi, 10, digits: 12)", "failure(invalid_argument, n(pi, 10, digits: 12))"),
        ("n(pi, digits: 0)", "failure(invalid_argument, n(pi, digits: 0))"),
        ("n(pi, digits: x)", "failure(invalid_argument, n(pi, digits: x))"),
        ("n(pi, foo: 3)", "failure(unknown_option, foo)"),
    ]);
}

/// `abs` is part of numerical evaluation (refined with the rest of an island, also around zero), and an exact
/// exponent outside an island stays exact, so a power of a ball is evaluated.
#[test]
fn n_through_abs_and_powers() {
    check(&[
        ("n(1/(1 + abs(1 - pi)), 25)", "0.318309886183790671537767526745"),
        ("n(abs(sin(1) - 1), 30)", "0.1585290151921034933474976783697010"),
        ("n(abs(1 - pi))", "2.141592653589793"),
        ("n(1/(1 + abs(v)), 25)", "(1.0000000000000000000000000000 + abs(v))^-1"),
        ("n(abs(x)^2 + 1/3, 10)", "0.33333333333333 + abs(x)^2"),
        ("n(sqrt(2)^-1 + x, 20)", "0.707106781186547524400844 + x"),
        ("n(abs(exp(-50) - exp(-50)*(1 + 10^-40)), 10)", "1.928749847964e-62"),
        ("n(1/(1 + abs(1.26~4)), 25)", "failure(precision_not_reached, 0.4425)"),
    ]);
}

/// A small deterministic generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

/// re(z) + im*im_part(z) == z, conjugate twice is the identity, and arg agrees with the machine atan2, on random
/// exact and machine complex numbers.
#[test]
fn parts_recompose_on_random_numbers() {
    let mut e = engine();
    let mut rng = Rng(7);
    let n = 60;
    for k in 0..n {
        let (a, b, c, d) = (rng.int(-9, 9), rng.int(1, 9), rng.int(-9, 9), rng.int(1, 9));
        let z = format!("({a}/{b} + ({c}/{d})*im)");
        assert_eq!(eval(&mut e, &format!("re({z}) + im*im_part({z}) - {z}")), "0", "{z}");
        assert_eq!(eval(&mut e, &format!("conjugate(conjugate({z})) - {z}")), "0", "{z}");
        assert_eq!(eval(&mut e, &format!("{z}*conjugate({z}) - abs({z})^2")), "0", "{z}");
        if a != 0 || c != 0 {
            let (x, y) = (a as f64 / b as f64, c as f64 / d as f64);
            let got: f64 = eval(&mut e, &format!("n(arg({z}))")).parse().unwrap_or(f64::NAN);
            assert!((got - y.atan2(x)).abs() < 1e-14, "arg{z} = {got}, atan2 {}", y.atan2(x));
        }
        if (k + 1) % 20 == 0 {
            println!("[{}/{n} {}%] random complex parts", k + 1, 100 * (k + 1) / n);
        }
    }
}

/// rationalize(x, dx) is within dx of x and no smaller denominator is (checked by brute force), on random machine
/// reals and tolerances.
#[test]
fn rationalize_is_the_simplest_on_random_inputs() {
    let mut e = engine();
    let mut rng = Rng(11);
    let n = 60;
    for k in 0..n {
        let x = rng.int(-100_000, 100_000) as f64 / 7919.0;
        let dx = 1.0 / rng.int(10, 5000) as f64;
        let got = eval(&mut e, &format!("rationalize({x:?}, {dx:?})"));
        let (p, q) = match got.split_once('/') {
            Some((p, q)) => (p.parse::<i64>().unwrap(), q.parse::<i64>().unwrap()),
            None => (got.parse::<i64>().unwrap(), 1),
        };
        // half an ulp of x widens the tolerance: allow 1e-12 slack in the brute force
        let within = |p: i64, q: i64| ((p as f64 / q as f64) - x).abs() <= dx + 1e-12;
        assert!(within(p, q), "rationalize({x}, {dx}) = {got}");
        for q2 in 1..q {
            let p2 = (x * q2 as f64).round() as i64;
            assert!(
                !((p2 - 1..=p2 + 1).any(|p2| ((p2 as f64 / q2 as f64) - x).abs() < dx - 1e-12)),
                "{x}, {dx}: {p2}/{q2} beats {got}"
            );
        }
        if (k + 1) % 20 == 0 {
            println!("[{}/{n} {}%] random rationalize", k + 1, 100 * (k + 1) / n);
        }
    }
}

/// The walks over terms are iterative: wide lists and deep nests.
#[test]
fn deep_and_wide_terms_are_walked_iteratively() {
    let mut e = engine();
    // a list of 10^4 inexact numbers and a 10^4-deep nest
    assert_eq!(eval(&mut e, "length(chop(map(k => 1e-15*k, range(10000))))"), "10000");
    assert_eq!(eval(&mut e, "total(chop(map(k => 1e-15*k, range(10000))))"), "0");
    assert_eq!(eval(&mut e, "precision(map(k => k/2.0, range(10000)))"), "15.954589770191003");
    eval(&mut e, "fn wrap(t, 0) = t\nfn wrap(t, k: Int) = wrap(f(t), k - 1)");
    assert_eq!(eval(&mut e, "rationalize(wrap(0.5, 3))"), "f(f(f(1/2)))");
    assert_eq!(eval(&mut e, "leaf_count(rationalize(wrap(0.5, 10000)))"), "10003");
    assert_eq!(eval(&mut e, "nest(first, rationalize(wrap(0.5, 10000)), 10000)"), "1/2");
    assert_eq!(eval(&mut e, "precision(wrap(0.5, 10000))"), "15.954589770191003");
}
