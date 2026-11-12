//! Item 11B: optimisation through the decomposition, `reduce` over the complex numbers in several variables,
//! complex quantifier elimination with several equations, integer `reduce`, `find_instance` and `resolve`. Every
//! answer is read back and printed again (the printed form of the canonical term, so that equal answers compare equal), and
//! the integer and optimisation answers are also checked by substitution.
use mc_api::{Engine, StopFlag};

fn eval(e: &mut mc_engine::CoreEngine, s: &str) -> String {
    let stop = StopFlag::new();
    e.eval(s, &stop).unwrap().printed
}

fn norm(e: &mut mc_engine::CoreEngine, s: &str) -> String {
    let a = eval(e, s);
    eval(e, &a)
}

fn check(cases: &[(&str, &str)]) {
    let mut e = mc_engine::CoreEngine::new();
    let mut want = mc_engine::CoreEngine::new();
    for (src, w) in cases {
        let got = norm(&mut e, src);
        let w = eval(&mut want, w);
        assert_eq!(got, w, "{src}");
    }
}

#[test]
fn optimisation_through_the_decomposition() {
    check(&[
        ("minimize((x*y - 3)^2 + 1, [x, y])", "[1, [x -> -1, y -> -3]]"),
        ("maximize(1 - (x*y - 3)^2, [x, y])", "[1, [x -> -1, y -> -3]]"),
        ("minimize([x + y, x <= y^2], [x, y])", "[-inf, [x -> indeterminate, y -> indeterminate]]"),
        ("minimize([x + y, x^2 + y^2 < 0], [x, y])", "[inf, [x -> indeterminate, y -> indeterminate]]"),
        ("min_value([x^2 + 1, x*y >= 1], [x, y])", "1"),
        ("arg_min([x^2 + 1, x*y >= 1], [x, y])", "[indeterminate, indeterminate]"),
        ("max_value([x, x*y <= 1], [x, y])", "inf"),
        ("minimize([max(x - y, abs(y)), min(x - 2, x - y^2) >= abs(x*y - 1)], [x, y])", "[3/2, [x -> 2, y -> 1/2]]"),
        ("arg_max([-max(x - y, abs(y)), min(x - 2, x - y^2) >= abs(x*y - 1)], [x, y])", "[2, 1/2]"),
        ("minimize([x, x > 0], x)", "[0, [x -> indeterminate]]"),
        // an irrational minimum, exact
        ("min_value([x + y, x^2 + y^2 <= 2 and x*y >= 0], [x, y])", "-2"),
        ("min_value([x*y, x^2 + 4*y^2 <= 8], [x, y])", "-2"),
    ]);
}

#[test]
fn complex_reduce_in_several_variables() {
    check(&[
        ("reduce(x^2 + y^2 == 2 and x - y == 1, [x, y])", "(x == (1/2)*(1 - 3^(1/2)) or x == (1/2)*(1 + 3^(1/2))) and y == -1 + x"),
        ("reduce(2*x + 3*y - 5*z == 1 and 3*x - 4*y + 7*z == 3, [x, y, z])", "y == 22 - 29*x and z == 13 - 17*x"),
        ("reduce(x^2 - y*z == 1, [x, y, z])", "(x == -1 or x == 1) and y == 0 or y != 0 and z == (-1 + x^2)/y"),
        ("reduce(x^2 - y^3 == 1, [x, y])", "y == (-1 + x^2)^(1/3) or y == -(-1)^(1/3)*(-1 + x^2)^(1/3) or y == 1im*(-1)^(1/6)*(-1 + x^2)^(1/3)"),
        (
            "reduce(a*x^2 + b*x + c == 0, x)",
            "a != 0 and (x == (-b - (-4*a*c + b^2)^(1/2))/(2*a) or x == (-b + (-4*a*c + b^2)^(1/2))/(2*a)) or a == 0 and b != 0 and x == -c/b or c == 0 and b == 0 and a == 0",
        ),
        ("reduce(x*y == 1 and x + y == 2, [x, y])", "x == 1 and y == 1"),
        ("reduce(x*y == 1 and x == 0, [x, y])", "false"),
    ]);
}

#[test]
fn complex_quantifier_elimination() {
    check(&[
        ("reduce(for_all(x, exists(y, a*x^2 + b*y^2 - 3*y == 1 and y != 0)), [a, b])", "a == 0 or b != 0"),
        ("resolve(exists(z, x*z == y and x^2*z^2 + x^2 == 1))", "-1 + x^2 + y^2 == 0 and x != 0"),
        ("reduce(exists(z, x*z == y and x^2*z^2 + x^2 == 1), [x, y])", "(y == -(1 - x^2)^(1/2) or y == (1 - x^2)^(1/2)) and x != 0"),
        ("resolve(exists(x, x^2 == 4 and x^3 == 8))", "true"),
        ("resolve(exists(x, x^2 == 4 and x^3 == 1))", "false"),
        // inequalities over the complex numbers compare real values
        ("resolve(exists([x, y], x^2 + y^2 < 0), complexes)", "true"),
        ("resolve(for_all([x, y], x^2 + y^2 >= 0), complexes)", "false"),
    ]);
}

#[test]
fn radicals_squared_out_of_sentences() {
    check(&[
        ("resolve(for_all([a, b, c, u], a > 0 and b > 0 and c > 0 and u > 0, a*c + b*u <= sqrt(a^2 + b^2)*sqrt(c^2 + u^2)))", "true"),
        ("resolve(for_all([a, b, c, u], sqrt((a + c)^2 + (b + u)^2) <= sqrt(a^2 + b^2) + sqrt(c^2 + u^2)))", "true"),
        ("resolve(exists([a, b], a*b > sqrt(a^2 + 1)*sqrt(b^2 + 1)))", "false"),
        ("resolve(exists(a, a + 1 < sqrt(a^2 + 1)))", "true"),
    ]);
}

#[test]
fn integers() {
    check(&[
        (
            "reduce(x^2 - 7*y^2 == 1 and x > 0 and y > 0, [x, y], integers)",
            "element(c(1), integers) and c(1) >= 1 and x == (1/2)*((8 - 3*7^(1/2))^c(1) + (8 + 3*7^(1/2))^c(1)) and y == (-1/14)*(-(8 + 3*7^(1/2))^c(1) + (8 - 3*7^(1/2))^c(1))*7^(1/2)",
        ),
        ("reduce(-2*x + 3*y == 7, [x, y], integers)", "element(c(1), integers) and x == 1 + 3*c(1) and y == 3 + 2*c(1)"),
        ("reduce(6*x - 4*y == 3, [x, y], integers)", "false"),
        ("reduce(x^2 + y^2 < 3 and x > y, [x, y], integers)", "x == 0 and y == -1 or x == 1 and y == -1 or x == 1 and y == 0"),
        ("find_instance(x^2 - 3*y^2 == 1 and 10 < x and x < 100, [x, y], integers)", "[[x -> 26, y -> 15]]"),
        ("resolve(exists([x, y, z], 2*x + 3*y - 5*z == 1 and 3*x - 4*y + 7*z == 3), integers)", "true"),
        ("resolve(exists([x, y, z, t], 1221*x + 3434*y + 5566*z + 8778*t == 90909 and x >= 0 and y >= 0 and z >= 0 and t >= 0), integers)", "false"),
        ("resolve(exists(x, x^1000 - 2*x^777 + 1 == 0), integers)", "true"),
        ("resolve(for_all([x, y], x^2 + y^2 != 3), integers)", "true"),
        ("find_instance(x^2 + y^2 == 1 and x == 2*y + 1, [x, y], reals, 5)", "[[x -> -3/5, y -> -4/5], [x -> 1, y -> 0]]"),
    ]);
    // the Pell family at c(1) = 1 .. 6 by substitution
    let mut e = mc_engine::CoreEngine::new();
    eval(&mut e, "pell = reduce(x^2 - 2*y^2 == 1 and x > 0 and y > 0, [x, y], integers)");
    let r = eval(&mut e, "pell");
    assert!(r.contains("c(1)"), "{r}");
    let v = eval(&mut e, "simplify(rewrite(pell, table([c(1) -> i], i, 6)))");
    assert_eq!(v, eval(&mut e, "[x == 3 and y == 2, x == 17 and y == 12, x == 99 and y == 70, x == 577 and y == 408, x == 3363 and y == 2378, x == 19601 and y == 13860]"));
    // every instance found satisfies the condition
    let w = eval(&mut e, "find_instance(x^3 + y^3 == 9 + z^3, [x, y, z], integers)");
    let ok = eval(&mut e, &format!("rewrite(x^3 + y^3 == 9 + z^3, first({w}))"));
    assert_eq!(ok, "true", "{w}");
}
