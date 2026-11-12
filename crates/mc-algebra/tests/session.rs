//! The natives through an engine session: minimal polynomials, number fields, factorisation over extensions
//! (checked by expansion) and sums over roots.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

struct Sys {
    engine: CoreEngine,
    stop: StopFlag,
}

impl Sys {
    fn new() -> Sys {
        Sys { engine: CoreEngine::new(), stop: StopFlag::new() }
    }
    fn run(&mut self, src: &str) -> String {
        match self.engine.eval(src, &self.stop) {
            Ok(e) => e.printed,
            Err(err) => panic!("{src}: {err}"),
        }
    }
}

#[test]
fn minimal_polynomials() {
    let mut s = Sys::new();
    let cases = [
        ("minimal_polynomial(sqrt(2) + sqrt(3), x)", "1 + -10*x^2 + x^4"),
        ("minimal_polynomial((1 + sqrt(5))/2)", "&(-1 - &1 + &1^2)"),
        ("minimal_polynomial(3/4, x)", "-3 + 4*x"),
        ("minimal_polynomial(im, x)", "1 + x^2"),
        ("minimal_polynomial(sqrt(2) + sqrt(3) - sqrt(5 + 2*sqrt(6)), x)", "x"),
        ("minimal_polynomial(root(x => x^3 - x - 1, 1)^2, y)", "-1 + y + -2*y^2 + y^3"),
        ("minimal_polynomial(pi, x)", "minimal_polynomial(pi, x)"),
    ];
    for (src, want) in cases {
        assert_eq!(s.run(src), want, "{src}");
    }
    // the polynomial vanishes at the number: n at 50 digits finds no nonzero digit, or only far below 10^-40
    for a in ["2^(1/3) + im", "sqrt(2 + sqrt(2))", "root(x => x^5 - x - 1, 3) + 1/2", "(-8)^(1/3)", "3^(1/3)*5^(1/2)", "2^(1/4) - sqrt(2)"] {
        let v = s.run(&format!("n(rewrite(minimal_polynomial({a}, z), [z -> {a}]), 50)"));
        assert!(tiny(&v), "minimal_polynomial({a}) at the number: {v}");
    }
}

/// A printed numeric value that is zero to more than 40 digits: no correct digit (`precision_not_reached`), or
/// balls whose exponents are all below -40.
fn tiny(v: &str) -> bool {
    if v.starts_with("failure(precision_not_reached") || v.trim_end_matches('.') == "0" {
        return true;
    }
    let exps: Vec<i64> = v.split("e-").skip(1).filter_map(|s| s.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()).collect();
    !exps.is_empty() && exps.iter().all(|&e| e > 40)
}

#[test]
fn number_fields() {
    let mut s = Sys::new();
    // the result is an algebraic_number term: the generator and the coefficient list (round 36; it was the list alone)
    assert_eq!(s.run("to_number_field(sqrt(2), sqrt(2) + sqrt(3))"), "algebraic_number(root(&(1 + -10*&1^2 + &1^4), 4), [0, -9/2, 0, 1/2])");
    assert_eq!(s.run("to_number_field(sqrt(2), 2^(1/4))"), "algebraic_number(root(&(-2 + &1^4), 2), [0, 0, 1, 0])");
    assert!(s.run("to_number_field(sqrt(3), sqrt(2))").starts_with("failure(not_in_field"));
    let r = s.run("to_number_field([sqrt(2), sqrt(3)])");
    assert!(r.starts_with("[algebraic_number(") && r.contains("[0, -9/2, 0, 1/2]") && r.contains("[0, 11/2, 0, -1/2]"), "{r}");
}

#[test]
fn factor_over_extensions() {
    let mut s = Sys::new();
    let cases = [
        ("factor(x^2 - 2, extension: sqrt(2))", "(x - 2^(1/2))*(x + 2^(1/2))"),
        ("factor(x^4 + 1, extension: sqrt(2))", "(1 + x*2^(1/2) + x^2)*(1 + -1*x*2^(1/2) + x^2)"),
        ("factor(x^2 + 1, extension: im)", "(-1im + x)*(1im + x)"),
        ("factor(x^3 - 2, extension: 2^(1/3))", "(x - 2^(1/3))*(x*2^(1/3) + 2^(2/3) + x^2)"),
        ("factor(x^4 + 1, extension: automatic)", "1 + x^4"),
        ("factor(x^2 - sqrt(2)*x, extension: automatic)", "x*(x - 2^(1/2))"),
    ];
    for (src, want) in cases {
        assert_eq!(s.run(src), want, "{src}");
    }
    // every factorisation expands back to the input
    for (p, ext) in [
        ("x^4 - 10*x^2 + 1", "[sqrt(2), sqrt(3)]"),
        ("x^6 + 3*x^4 + 3*x^2 - 1", "2^(1/3)"),
        ("2*x^4 - 8", "sqrt(2)"),
        ("x^8 - 1", "[sqrt(2), im]"),
        ("x^5 - x - 1", "root(x => x^5 - x - 1, 1)"),
        ("x^4 + x^3 + x^2 + x + 1", "sqrt(5)"),
        ("(x^2 - 3)^2*(x + 1)", "sqrt(3)"),
    ] {
        let f = s.run(&format!("ff = factor({p}, extension: {ext}); ff"));
        assert!(f.contains('*') || f.contains('^'), "factor({p}, extension: {ext}) = {f}");
        let back = s.run(&format!("expand(ff - ({p}))"));
        // root objects are not reduced by expand: their difference is decided by its minimal polynomial
        let zero = back == "0" || s.run(&format!("minimal_polynomial({back}, x)")) == "x";
        assert!(zero, "factor({p}, extension: {ext}) = {f}: {back}");
    }
}

#[test]
fn root_sums() {
    let mut s = Sys::new();
    let cases = [
        ("root_sum(x => x^3 + x + 1, x => x^2)", "-2"),
        ("root_sum(x => x^3 + x + 1, x => x^5 + 1/x)", "4"),
        ("root_sum(x => (x^3 + x + 1)^2, x => x^2)", "-4"),
        ("root_sum(x => x^3 + x + 1, x => 1/(z - x))", "(1 + 3*z^2)/(1 + z + z^3)"),
        ("root_sum(x => x^2 - 2, x => x*log(z - x))", "log(z - 2^(1/2))*2^(1/2) + -1*log(z + 2^(1/2))*2^(1/2)"),
        ("root_sum(x => x^3 + x + 1, x => log(2 - x))", "root_sum(x => 1 + x + x^3, x => log(2 - x))"),
        ("root_sum(x => 2*x^3 + 2*x + 2, x => log(2 - x))", "root_sum(x => 1 + x + x^3, x => log(2 - x))"),
        ("root_sum(x => (x - 1)*(x^3 + x + 1), x => log(2 - x))", "root_sum(x => 1 + x + x^3, x => log(2 - x))"),
        ("root_sum(x => x^3 + x + 1, x => 7)", "21"),
        ("root_sum(x => x^4 - 1, x => x^3)", "0"),
    ];
    for (src, want) in cases {
        assert_eq!(s.run(src), want, "{src}");
    }
    // numerics: a sum of logarithms over the roots, to 30 digits
    let v = s.run("n(root_sum(x => x^3 + x + 1, x => log(2 - x)), 30)");
    assert!(v.starts_with("2.39789527279837054406194357796"), "{v}");
    // the derivative of a root sum
    assert_eq!(s.run("d(root_sum(t => t^3 - 2, t => t*log(x - t)), x)"), "6/(-2 + x^3)");
    // to radicals: the same number
    let a = s.run("n(to_radicals(root_sum(x => x^3 + x + 1, x => log(2 - x))), 20)");
    assert!(a.starts_with("complex(2.3978952727983705"), "{a}");
}

#[test]
fn root_reduce_norms_and_extension_options() {
    let mut s = Sys::new();
    let cases = [
        ("root_reduce(sqrt(2) + sqrt(3))", "root(&(1 + -10*&1^2 + &1^4), 4)"),
        ("root_reduce(root(x => x^3 + x + 1, 1)^2)", "root(&(-1 + &1 + 2*&1^2 + &1^3), 1)"),
        ("root_reduce(sqrt(2)*sqrt(8))", "4"),
        ("[algebraic_number_norm(1 + sqrt(2)), algebraic_number_trace(1 + sqrt(2)), algebraic_number_norm(2^(1/3) + 1)]", "[-1, 2, 3]"),
        ("algebraic_number_trace(sqrt(2) + sqrt(3))", "0"),
        ("polynomial_gcd(x^2 - 2*sqrt(3)*x + 3, x^2 - 3, extension: automatic)", "x - 3^(1/2)"),
        ("factor_square_free(x^2 + 2*sqrt(2)*x + 2, extension: automatic)", "(x + 2^(1/2))^2"),
    ];
    for (src, want) in cases {
        assert_eq!(s.run(src), want, "{src}");
    }
    // the root object is the same number
    let v = s.run("n(root_reduce(2^(1/3) + im) - (2^(1/3) + im), 30)");
    assert!(v.starts_with("failure(precision_not_reached") || v.contains("e-"), "{v}");
    // lists thread
    assert_eq!(s.run("minimal_polynomial([sqrt(3), 2^(1/3) + 1], x)"), "[-3 + x^2, -3 + -3*x^2 + 3*x + x^3]");
    // roots of unity as exponentials, power sums with parameters
    assert_eq!(s.run("algebraic_number_norm(exp(pi*im/8))"), "1");
    assert_eq!(s.run("root_sum(x => x^2*0 + x^3 + a*x + b, x => x^2)"), "-2*a");
    // normal writes a sum over roots with root objects
    let r = s.run("normal(root_sum(&(&1^3 + &1 + 7), sin))");
    assert!(r.matches("root(").count() == 3 && !r.contains("root_sum"), "{r}");
}
