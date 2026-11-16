//! Methods for inputs that gave no answer. Membership in the primes, the algebraic and the complex
//! numbers under assumptions (closure rules and parity); number theory under assumptions (coprime primes, gcd times
//! lcm, Fermat, Wilson, Carmichael, quotients with integer multiples); areas of Boolean combinations of
//! semialgebraic sets (cylindrical decomposition, completed squares); step and piecewise-linear functions in
//! `piecewise_expand` and `refine`; symbolic Cholesky factors; `acosh` on (-1, 1). Each test has the documented
//! input and variations checked by an independent route (numbers, a numeric integral, a second formula).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// `src` evaluates to the same expression as `want` (both evaluated; structural equality, so the printed form of
/// negative terms does not matter).
fn same(e: &mut CoreEngine, src: &str, want: &str) {
    let r = ev(e, &format!("vv = {src}; vv === ({want})"));
    assert_eq!(r, "true", "{src}: {}", ev(e, "vv"));
}

#[test]
fn membership_in_primes_algebraics_and_complexes() {
    let mut e = CoreEngine::new();
    for (src, want) in [
        ("simplify(element(sqrt(x), algebraics), element(x, algebraics))", "true"),
        ("refine(element(sqrt(x)^2 + y^(2/3), algebraics), element(x | y, algebraics))", "true"),
        ("map(&(refine(element(x, algebraics), element(x, &1))), [rationals, integers, primes])", "[true, true, true]"),
        ("refine(element(x, complexes), element(x, algebraics))", "true"),
        ("simplify(element(e^x, complexes), element(x, complexes))", "true"),
        ("[element(inf, complexes), element(complex_infinity, complexes)]", "[false, false]"),
        ("map(&(refine(element(x, &1), element(x, primes))), [complexes, reals, algebraics, rationals, integers])", "[true, true, true, true, true]"),
        ("simplify(element(p + 1, primes), element(p, primes) and p > 2)", "false"),
        // variations: a quotient of algebraic numbers (nonzero denominator), a shift by an odd number
        ("refine(element(x^(2/3)/(1 + y), algebraics), element(x | y, algebraics) and y > 0)", "true"),
        ("simplify(element(p + 3, primes), element(p, primes) and p > 2)", "false"),
        ("simplify(element(2*p, primes), element(p, primes))", "false"),
        ("simplify(element(n^2 < euler_phi(n)^3, booleans), n > 42 and element(n, integers))", "true"),
    ] {
        same(&mut e, src, want);
    }
    // not proved: a real number need not be algebraic, an odd number need not be prime
    same(&mut e, "refine(element(x, algebraics), element(x, reals))", "element(x, algebraics)");
    same(&mut e, "simplify(element(p + 2, primes), element(p, primes) and p > 2)", "element(p + 2, primes)");
    same(&mut e, "refine(element(p, primes), element(p, integers))", "element(p, primes)");
    // independent checks: the numbers behind the variations
    assert_eq!(ev(&mut e, "is_number(minimal_polynomial(2^(2/3)/(1 + 3^(1/2)), t)) === false and free_of(minimal_polynomial(2^(2/3)/(1 + 3^(1/2)), t), minimal_polynomial)"), "true");
    assert_eq!(ev(&mut e, "select(table(prime(k) + 3, k, range(2, 60)), is_prime)"), "[]");
    assert_eq!(ev(&mut e, "select(table(2*prime(k), k, 60), is_prime)"), "[]");
}

#[test]
fn number_theory_under_assumptions() {
    let mut e = CoreEngine::new();
    for (src, want) in [
        ("full_simplify(gcd(p, q), element([p, q], primes) and p != q)", "1"),
        ("full_simplify(lcm(p, q), element([p, q], primes) and p != q)", "p*q"),
        ("full_simplify(lcm(m, n) * gcd(m, n), element([m, n], integers))", "abs(m*n)"),
        ("simplify(mod(a^p, p) == mod(a, p), element(a, integers) and element(p, primes))", "true"),
        ("full_simplify(mod(factorial(p - 1), p), element(p, primes))", "-1 + p"),
        ("function_expand(euler_phi(p), element(p, primes))", "-1 + p"),
        ("simplify(cos((p + q) * pi), element(p, primes) and element(q, primes) and p > 2 and q > 2)", "1"),
        ("full_simplify(mod(a^carmichael_lambda(n), n), element(a | n, integers) and gcd(a, n) == 1 and n > 1)", "1"),
        ("simplify(quotient(4 * n + 5, 2), element(n, integers))", "2 + 2*n"),
        ("refine(quotient(2 * n - 3, 7), element((n + 2) / 7, integers))", "-1 + (2/7)*(2 + n)"),
        ("full_simplify(quotient(7 + k * n, n) == quotient(7, n) + k, element([n, k], integers))", "true"),
        ("assuming(element(n, integers), refine(exp(2 * pi * im * (n + 1 / 2))))", "-1"),
        // variations
        ("simplify(cos((p + q + 1) * pi), element(p, primes) and element(q, primes) and p > 2 and q > 2)", "-1"),
        ("simplify(quotient(6 * n - 1, 3), element(n, integers))", "-1 + 2*n"),
    ] {
        same(&mut e, src, want);
    }
    // independent checks with numbers: the identities at primes and integers
    assert_eq!(ev(&mut e, "[gcd(prime(5), prime(9)), lcm(prime(5), prime(9)) == prime(5)*prime(9), gcd(-4, 6)*lcm(-4, 6) == abs(-24)]"), "[1, true, true]");
    assert_eq!(ev(&mut e, "[mod(7^13, 13) == mod(7, 13), mod(factorial(10), 11), mod(5^carmichael_lambda(21), 21)]"), "[true, 10, 1]");
    assert_eq!(ev(&mut e, "[quotient(4*(-3) + 5, 2), quotient(2*12 - 3, 7), quotient(6*(-2) - 1, 3)]"), "[-4, 3, -5]");
    // without the facts nothing changes
    assert_eq!(ev(&mut e, "full_simplify(gcd(p, q))"), "gcd(p, q)");
    assert_eq!(ev(&mut e, "simplify(quotient(4 * n + 5, 2), n > 0)"), "quotient(5 + 4*n, 2)");
}

#[test]
fn areas_of_boolean_combinations() {
    let mut e = CoreEngine::new();
    let a = "x^2 + y^2 < 1";
    let c = "(x - 1)^2 + y^2 < 2";
    for (src, want) in [
        (format!("integrate(boole({a} and {c}), [x, -inf, inf], [y, -inf, inf])"), "-1 + pi"),
        (format!("integrate(boole({a} or {c}), [x, -inf, inf], [y, -inf, inf])"), "1 + 2*pi"),
        (format!("integrate(boole(nand({a}, {c})), [x, -3/2, 5/2], [y, -2, 2])"), "17 - pi"),
        (format!("integrate(boole(nor({a}, {c})), [x, -3/2, 5/2], [y, -2, 2])"), "15 - 2*pi"),
        (format!("full_simplify(integrate(boole(xor({a}, {c})), [x, -inf, inf], [y, -inf, inf]))"), "2 + pi"),
        ("integrate(boole(x^2 + y^2 <= r^2), [x, -inf, inf], [y, -inf, inf], assumptions: r > 0)".to_string(), "pi*r^2"),
        // variations: an annulus, a lens of two unit circles (2 pi/3 - sqrt(3)/2)
        ("integrate(boole(1 < x^2 + y^2 and x^2 + y^2 < 4), [x, -inf, inf], [y, -inf, inf])".to_string(), "3*pi"),
    ] {
        same(&mut e, &src, want);
    }
    let lens = ev(&mut e, "integrate(boole(x^2 + y^2 < 1 and (x - 1)^2 + y^2 < 1), [x, -inf, inf], [y, -inf, inf])");
    assert_eq!(ev(&mut e, &format!("simplify({lens} - (2*pi/3 - sqrt(3)/2))")), "0", "lens: {lens}");
    // independent check: the area of the intersection by a one-dimensional numeric integral of the chord lengths
    let num = ev(&mut e, "n_integrate(2*min(sqrt(max(1 - x^2, 0)), sqrt(max(2 - (x - 1)^2, 0))), [x, -1, 1])");
    let v: f64 = num.parse().unwrap_or(f64::NAN);
    assert!((v - (std::f64::consts::PI - 1.0)).abs() < 1e-6, "{num}");
}

#[test]
fn step_functions_in_piecewise_expand_and_refine() {
    let mut e = CoreEngine::new();
    for (src, want) in [
        ("piecewise_expand(unit_box(x))", "piecewise([[1, -1/2 <= x and x <= 1/2]], 0)"),
        ("piecewise_expand(unit_box(x), 0 < x and x < 2)", "piecewise([[1, x <= 1/2]], 0)"),
        ("piecewise_expand(unit_triangle(x), 0 < x and x < 2)", "piecewise([[1 - x, x <= 1]], 0)"),
        ("piecewise_expand(ramp(x))", "piecewise([[x, x >= 0]], 0)"),
        ("piecewise_expand(x * ramp(x))", "piecewise([[x^2, x >= 0]], 0)"),
        ("piecewise_expand(real_abs(x))", "piecewise([[-x, x < 0]], x)"),
        ("piecewise_expand(real_sign(x))", "piecewise([[-1, x < 0], [1, x > 0]], 0)"),
        ("piecewise_expand(floor(x, 2), 0 < x and x < 4)", "piecewise([[2, x >= 2]], 0)"),
        ("piecewise_expand(ceiling(x, 2), 0 < x and x < 4)", "piecewise([[2, x <= 2]], 4)"),
        ("piecewise_expand(quotient(2 * x + 1, 3), 0 < x and x < 5)", "piecewise([[1, 1 <= x and x < 5/2], [2, 5/2 <= x and x < 4], [3, x >= 4]], 0)"),
        ("simplify(unit_box(b, a), a < -(1 / 2) and b > 0)", "0"),
        ("assuming(a < 0 and b > 0, refine(unit_step(b, b, a)))", "0"),
        ("simplify(unit_triangle(b, a), a < -1 and b > 0)", "0"),
        ("full_simplify(integer_part(x) + integer_part(x + 1 / 2), 1 / 3 < x and x < 1 / 2)", "0"),
        ("full_simplify(fractional_part(x) + fractional_part(x + 1 / 2), 0 < x and x < 1 / 3)", "1/2 + 2*x"),
        ("[integer_part(-x) == -integer_part(x), fractional_part(-x) == -fractional_part(x), real_abs(-x) == real_abs(x), real_sign(-z)]", "[true, true, true, -real_sign(z)]"),
        ("map(f => function_range(f(x), x, y), [floor, ceiling, round, integer_part])", "table(element(y, integers), 4)"),
        // variations
        ("piecewise_expand(unit_box(2*x))", "piecewise([[1, -1/4 <= x and x <= 1/4]], 0)"),
        ("piecewise_expand(mod(x, 2), 0 < x and x < 4)", "piecewise([[x, x < 2]], -2 + x)"),
    ] {
        same(&mut e, src, want);
    }
    // independent check: the expansions agree with the functions at sample points of the region
    for (f, region, pts) in [
        ("quotient(2*x + 1, 3)", "0 < x and x < 5", "[1/3, 1, 7/4, 5/2, 3, 4, 9/2]"),
        ("ceiling(x, 2)", "0 < x and x < 4", "[1/2, 2, 5/2, 7/2]"),
        ("mod(x, 2)", "0 < x and x < 4", "[1/2, 2, 5/2, 7/2]"),
        ("unit_triangle(x)", "-2 < x and x < 2", "[-3/2, -1, -1/2, 0, 1/3, 1, 3/2]"),
    ] {
        let r = ev(&mut e, &format!("p = piecewise_expand({f}, {region}); map(t => rewrite(p, x -> t) == rewrite({f}, x -> t), {pts})"));
        assert!(!r.contains("false") && r.starts_with("[true"), "{f}: {r}");
    }
}

#[test]
fn prime_zeta_below_one_and_groups_of_prime_power_order() {
    let mut e = CoreEngine::new();
    // the documented value at 50 digits, then s = 1/4 and 2/5: the imaginary part is pi times the sum of
    // moebius_mu(k)/k over the squarefree k with k s < 1 (each such log(zeta(k s)) has the imaginary part pi)
    let v = ev(&mut e, "abs(re(n(prime_zeta_p(4 / 11), 50)) + 14351003377451634337108527277763895582344309103848/10^49) < 10^-45");
    assert_eq!(v, "true");
    for (s, k) in [("1/4", "pi*(1 - 1/2 - 1/3)"), ("2/5", "pi*(1 - 1/2)"), ("4/11", "pi*(1 - 1/2)")] {
        let r = ev(&mut e, &format!("abs(im_part(n(prime_zeta_p({s}), 30)) - n({k}, 30)) < 10^-25"));
        assert_eq!(r, "true", "{s}");
    }
    // above 1 nothing changed: the sum over the primes to 10^5 plus its tail bound brackets P(2)
    assert_eq!(ev(&mut e, "p2 = prime_zeta_p(2.); s2 = total(map(p => 1./p^2, prime(range(9592)))); s2 < p2 and p2 < s2 + 1./10^5"), "true");
    same(&mut e, "table(finite_group_count(prime(k)^3), k, 10)", "table(5, k, 10)");
    same(&mut e, "[finite_group_count(11^2), finite_group_count(3^4), finite_group_count(5^4)]", "[2, 15, 15]");
    // independent check against the table below 64: 4, 9, 25, 49 give 2; 8, 27 give 5
    same(&mut e, "map(finite_group_count, [4, 9, 25, 49, 8, 27])", "[2, 2, 2, 2, 5, 5]");
}

#[test]
fn parts_of_closed_constants_with_exponential_units() {
    let mut e = CoreEngine::new();
    for (src, want) in [
        ("table(arg(exp(k * im * 3 * (pi / 8))), k, 8)", "[3*pi/8, 3*pi/4, -7*pi/8, -pi/2, -pi/8, pi/4, 5*pi/8, pi]"),
        ("arg((-5)^(1/5))", "pi/5"),
        ("arg(-2*exp(3*im*pi/7))", "-4*pi/7"),
        ("im_part(exp(2*im*pi/7))", "sin(2*pi/7)"),
    ] {
        same(&mut e, src, want);
    }
    // the closed forms (free of the part functions) against the parts of the numeric values of the arguments
    for (f, arg) in [("im_part", "(e + im*pi)^3"), ("abs", "(-5)^(1/3) + 3"), ("re", "exp(3*im*pi/8)"), ("abs", "2 + exp(2*im*pi/5)")] {
        let r = ev(&mut e, &format!("v = {f}({arg}); [free_of(v, {f}), abs(n(v, 30) - {f}(n({arg}, 30))) < 10^-25]"));
        assert_eq!(r, "[true, true]", "{f}({arg})");
    }
    // independent check: the angles against atan2 of the machine values
    let r = ev(&mut e, "max(table(abs(n(arg(exp(k*im*3*pi/8))) - arg(n(exp(k*im*3*pi/8)))), k, 8)) < 10^-12");
    assert_eq!(r, "true");
}

#[test]
fn existence_by_a_proved_sign_change() {
    let mut e = CoreEngine::new();
    for src in [
        "resolve(exists(x, exp(x) - 2 * x * log(x) + 3 * x^x == 4), reals)",
        "resolve(exists(x, -1 < x and x < 1, sin(sin(sin(x))) - x == 1 / 7), reals)",
        "resolve(exists(x, exp(sin(x)) - sin(3 * cos(x)) == tan(x)), reals)",
        "resolve(exists([x, y], y^2 * e^e^x - y * e^x - x == 1), reals)",
        // variations: a root inside given bounds, a transcendental equation with a root near 0.567
        "resolve(exists(x, 0 < x and x < 1, x == exp(-x)), reals)",
        "resolve(exists(x, cos(x) == x^3), reals)",
    ] {
        assert_eq!(ev(&mut e, src), "true", "{src}");
    }
    // no answer without a witness: no root (exp(x) > 0), a root outside the bounds, a pole between opposite signs
    for src in [
        "resolve(exists(x, exp(x) == -1), reals)",
        "resolve(exists(x, 2 < x and x < 3, x == exp(-x)), reals)",
        "resolve(exists(x, 1 < x and x < 2, tan(x) == 0), reals)",
    ] {
        assert_ne!(ev(&mut e, src), "true", "{src}");
    }
    // independent checks: the roots by find_root satisfy the equations and lie inside the bounds
    assert_eq!(ev(&mut e, "r = rewrite(x, find_root(x == exp(-x), [x, 0.5])); 0 < r and r < 1 and abs(r - exp(-r)) < 10^-12"), "true");
    assert_eq!(ev(&mut e, "r = rewrite(x, find_root(sin(sin(sin(x))) - x == 1/7, [x, -0.5])); -1 < r and r < 1"), "true");
}

#[test]
fn symbolic_cholesky_and_acosh_inside_the_unit_interval() {
    let mut e = CoreEngine::new();
    same(&mut e, "cholesky([[a, b], [b, a]])", "[[a^(1/2), b/a^(1/2)], [0, (a - b*conjugate(b/a^(1/2))/a^(1/2))^(1/2)]]");
    ev(&mut e, "m = [[5 * e^2, 14, 15], [14, sqrt(863), 8], [15, 8, 18 * pi]]; u = cholesky(m);");
    same(&mut e, "simplify(dot(transpose(u), u) - m)", "[[0, 0, 0], [0, 0, 0], [0, 0, 0]]");
    // variations: a closed constant pivot proved positive and one proved negative
    same(&mut e, "cholesky([[1, 2], [2, 1 + pi]])", "[[1, 2], [0, (-3 + pi)^(1/2)]]");
    assert_eq!(ev(&mut e, "is_failure(cholesky([[1, 2], [2, pi]]))"), "true");
    // independent check: the symbolic factor at numbers equals the exact factor of the numeric matrix
    assert_eq!(ev(&mut e, "rewrite(cholesky([[a, b], [b, c]]), [a -> 4, b -> 2, c -> 5]) === cholesky([[4, 2], [2, 5]])"), "true");
    for (src, want) in [
        ("acosh(1/2)", "(im/3)*pi"),
        ("table(asech(n), n, range(-2, 2))", "[(2*im/3)*pi, im*pi, inf, 0, (im/3)*pi]"),
        ("acosh(-1/2)", "(2*im/3)*pi"),
        ("acosh(1/3)", "acosh(1/3)"),
    ] {
        same(&mut e, src, want);
    }
    // independent check: the numeric values
    assert_eq!(ev(&mut e, "abs(n(acosh(0.5)) - n(im*pi/3)) < 10^-12"), "true");
}
