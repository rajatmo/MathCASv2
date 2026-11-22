//! Norms of golden_ratio and in the field of an algebraic_number, the real parts and absolute value of a real
//! algebraic number, trigonometric values at rational multiples of pi in radicals, exact zero tests of algebraic
//! numbers, Horner forms with fractional powers and quotients, polynomial_mod over the integers, expand_all of
//! heads and in a pattern. Checked independently: products of conjugates, values at 30 digits, expansion.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| panic!("{src}: {err}"))
}

#[test]
fn norms() {
    // (1 + sqrt(5))/2 (1 - sqrt(5))/2 = -1
    assert_eq!(eval("algebraic_number_norm(golden_ratio)"), "-1");
    assert_eq!(eval("expand((1 + sqrt(5))/2*(1 - sqrt(5))/2)"), "-1");
    assert_eq!(eval("minimal_polynomial(golden_ratio, x)"), "-1 - x + x^2");
    // sqrt(2) in Q(sqrt(2) + sqrt(3)) of degree 4: its norm over Q(sqrt(2)) squared, (-2)^2
    assert_eq!(eval("{ a = to_number_field(sqrt(2), sqrt(2) + sqrt(3)); [algebraic_number_norm(a), algebraic_number_norm(a, extension: automatic)] }"), "[-2, 4]");
}

#[test]
fn parts_of_real_algebraic_numbers() {
    let a = "algebraic_number(root(&(&1^3 + &1 - 1), 1), [0, -1, 4])";
    assert_eq!(eval(&format!("{{ a = {a}; [re(a) === a, im_part(a), sign(a), conjugate(a) === a, abs(a) === a] }}")), "[true, 0, 1, true, true]");
    // the negative of it: abs is the number with the opposite sign, by its value
    assert_eq!(eval(&format!("{{ b = -{a}; [sign(b), n(abs(b) - {a}, 30) == 0] }}")), "[-1, true]");
    // a complex root stays
    assert_eq!(eval("re(algebraic_number(root(&(&1^3 + &1 + 1), 2), [0, 1]))"), "re(algebraic_number(root(&(1 + &1 + &1^3), 2), [0, 1, 0]))");
}

#[test]
fn trigonometric_radicals() {
    assert_eq!(eval("to_radicals(tan(pi/19))"), "-1im*(-1 + (-1)^(2/19))/(1 + (-1)^(2/19))");
    for e in ["sin(7*pi/16)", "tan(pi/19)", "cos(pi/17)", "cos(2*pi/7)", "sec(pi/8)", "csc(3*pi/32)", "cot(5*pi/12)", "sin(-11*pi/24)"] {
        let r = eval(&format!("to_radicals({e})"));
        assert!(!r.contains("sin") && !r.contains("cos") && !r.contains("tan"), "{e}: {r}");
        assert_eq!(eval(&format!("abs(n(to_radicals({e}), 30) - n({e}, 30)) < 10^(-28)")), "true", "{e}");
    }
    // the half angles give real radicals
    assert_eq!(eval("is_real(n(to_radicals(sin(7*pi/16)), 30))"), "true");
    assert_eq!(eval("to_radicals(sin(x) + 1)"), "1 + sin(x)");
}

#[test]
fn exact_algebraic_zero_tests() {
    let z = "sqrt(2) + sqrt(3) - root_reduce(sqrt(2) + sqrt(3))";
    assert_eq!(eval(&format!("is_possible_zero({z}, method: exact_algebraics)")), "true");
    assert_eq!(eval(&format!("is_possible_zero({z} + 10^(-10000), method: exact_algebraics)")), "false");
    assert_eq!(eval("is_possible_zero(pi - pi, method: exact_algebraics)"), "true");
    assert_eq!(eval("is_possible_zero(x - x, method: exact_algebraics)"), "true");
}

#[test]
fn horner_forms() {
    assert_eq!(eval("horner_form(x^(1/3) + x + x^(3/2))"), "(1 + (1 + x^(1/2))*x^(2/3))*x^(1/3)");
    assert_eq!(eval("expand(horner_form(x^(1/3) + x + x^(3/2)) - (x^(1/3) + x + x^(3/2)))"), "0");
    assert_eq!(eval("horner_form((x^3 - x^2*y + x*y - 3*y^3)/denom)"), "(-3*y^3 + x*(y + x*(x - y)))/denom");
    assert_eq!(eval("horner_form(1 + 2*x + 3*x^2)"), "1 + x*(2 + 3*x)");
    // numerator and denominator in their own variable orders
    assert_eq!(eval("{ f = (y + 2*x)^2/(x - y)^3; h = horner_form(f, [y, x], [x, y]); [together(h - f), numerator(h), denominator(h)] }"), "[0, 4*x^2 + y*(y + 4*x), -y^3 + x*(3*y^2 + x*(x + -3*y))]");
}

#[test]
fn polynomial_mod_over_the_integers() {
    // 2 x - 1 does not divide x^2 with an integer quotient
    assert_eq!(eval("polynomial_mod(x^2, 2*x - 1, coefficient_domain: integers)"), "x^2");
    assert_eq!(eval("polynomial_mod(x^2, 2*x - 1)"), "1/4");
    assert_eq!(eval("polynomial_mod(x^3 + 1, x - 2, coefficient_domain: integers)"), "9");
}

#[test]
fn expand_all_heads_and_patterns() {
    assert_eq!(eval("expand_all(((1 + a)*(1 + b))(x))"), "(1 + a + b + a*b)(x)");
    assert_eq!(eval("expand_all((f((x + y)^2) + g((y + z)^2))^2, x)"), "2*f(2*x*y + x^2 + y^2)*g((y + z)^2) + f(2*x*y + x^2 + y^2)^2 + g((y + z)^2)^2");
}
