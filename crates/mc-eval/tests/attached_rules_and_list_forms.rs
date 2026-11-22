//! Rules attached to an argument's head before a built-in (`@on(h) rule h(x_)*h(y_) := ...`), the total derivative
//! with the attribute `constant`, and list functions on atoms, levels, operator forms and associations, in a full
//! session.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn session() -> (CoreEngine, StopFlag) {
    (CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() }), StopFlag::new())
}

fn check(cases: &[(&str, &str)]) {
    let (mut e, stop) = session();
    for &(src, want) in cases {
        let got = match e.eval(src, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        if !want.is_empty() {
            assert_eq!(got, want, "{src}");
        }
    }
}

#[test]
fn attached_rules_come_before_the_arithmetic() {
    check(&[
        ("@on(h) rule (h(x_) * h(y_)) := htimes(x, y)", ""),
        ("h(a) * h(b)", "htimes(a, b)"),
        ("@on(g) rule (g(x_) + g(y_)) := gplus(x, y)", ""),
        ("g(a) + g(b)", "gplus(a, b)"),
        // a call the rule does not match goes on to the built-in
        ("g(a) + 2*g(a)", "3*g(a)"),
        ("h(a)^2", "h(a)^2"),
        // nothing attached: the arithmetic as before
        ("k(a) * k(b)", "k(a)*k(b)"),
        ("2 + 3", "5"),
        // a rule attached to `mul` by one head leaves products of other heads to the built-in
        ("k(a) * k(a) * 3", "3*k(a)^2"),
        // clearing the outer function keeps the attached rules (they belong to h)
        ("clear(mul)", ""),
        ("h(c) * h(d)", "htimes(c, d)"),
    ]);
}

#[test]
fn total_derivative_with_constant_attribute() {
    check(&[
        ("set_attributes(f, constant)", ""),
        // f(x, y) is a constant: d(g f) = f dg
        ("expand(dt(g(x, y) * f(x, y)) - f(x, y)*(dt(x)*derivative(1, 0)(g)(x, y) + dt(y)*derivative(0, 1)(g)(x, y)))", "0"),
        ("dt(g(f(x, y), x, y))", "dt(x)*derivative(0, 1, 0)(g)(f(x, y), x, y) + dt(y)*derivative(0, 0, 1)(g)(f(x, y), x, y)"),
        ("set_attributes(c, constant)", ""),
        ("[dt(c*x^2), dt(c*x^2, x), dt(c), dt(c, x)]", "[2*c*x*dt(x), 2*c*x, 0, 0]"),
        // the option names constants of this call only
        ("expand(dt(u(x, y)*v(y), constants: u) - dt(y, constants: [u])*u(x, y)*derivative(1)(v)(y))", "0"),
        ("expand(dt(u(x)*v(x), x) - u(x)*derivative(1)(v)(x) - v(x)*derivative(1)(u)(x))", "0"),
        // a session variable named like an option leaves the named argument alone (chain rule by hand: the f slot
        // contributes nothing)
        ("constants = [1, 2]", ""),
        (
            "dt(g(f(x, y), x, y), constants: f)",
            "dt(x, constants: [f])*derivative(0, 1, 0)(g)(f(x, y), x, y) + dt(y, constants: [f])*derivative(0, 0, 1)(g)(f(x, y), x, y)",
        ),
    ]);
}

#[test]
fn named_arguments_keep_their_names() {
    check(&[
        ("zz = 3", ""),
        ("ff(a, zz: 2)", "ff(a, zz: 2)"),
        // the value is evaluated, the name is not
        ("ff(a, zz: zz + 1)", "ff(a, zz: 4)"),
        ("fn ko(x; zz = 0) = x + zz", ""),
        ("ko(1, zz: 5)", "6"),
        ("ko(1)", "1"),
        ("ff(a, zz: 1/0)", "failure(division_by_zero)"),
        ("ff(a, \"zz\" -> 1 + 1)", "ff(a, \"zz\" -> 2)"),
    ]);
}

#[test]
fn list_functions_on_atoms_levels_and_operator_forms() {
    check(&[
        // a number or a string has no parts; a symbol may stand for a list and stays
        ("[cases(0, _: Sym), cases(\"ab\", _), cases(2.5, _)]", "[[], [], []]"),
        ("cases(x, _: Sym)", "cases(x, _: Sym)"),
        ("apply(composition, table(right_composition(cases(_: Sym), &(2*&1)), 10))(0)", "[]"),
        // scan at levels, inner parts first as level lists them
        ("reap(scan(&((if &1 > 0 then sow(&1))), [1, [-2, pi], -sqrt(3)], inf))[2, 1]", "[1, pi, 3, 1/2, 3^(1/2)]"),
        ("reap(scan(sow, [[1, 2], [3]], [2]))[2, 1]", "[1, 2, 3]"),
        ("scan(sow, [[1, 2], [3]], [2])", "null"),
        // the leaves of an expression in the order level lists them
        ("ee = integrate(1/(x^3 - 1), x)", ""),
        ("reap(scan(sow, ee, [-1]))[2, 1] == level(ee, [-1])", "true"),
        // insert: the operator form and a path into an association
        ("map(insert(x, 3), [[1, 2, 3], [a, b, c, d]])", "[[1, 2, x, 3], [a, b, x, c, d]]"),
        ("insert([assoc(a -> 1, b -> 2), assoc(a -> 2)], x -> inf, [1, a])", "[assoc(x -> inf, a -> 1, b -> 2), assoc(a -> 2)]"),
        ("insert(assoc(a -> 1), 3)", "failure(argument, insert, 2)"),
        // a list of positions is not a path into an association: every position takes x (counted by hand)
        ("insert([a, b, c, d, e], x, [[1], [3], [-1]])", "[x, a, b, x, c, d, e, x]"),
        ("insert([a, b, c], x, [[1], [1], [1]])", "[x, x, x, a, b, c]"),
        ("insert([a, b, c], x, [[1], [-3]])", "[x, a, x, b, c]"),
        ("insert([[a, b, c], [d, e, f], [g, h, i]], x, [[1, 2], [3, 3], [2, 1]])", "[[a, x, b, c], [x, d, e, f], [g, h, x, i]]"),
        ("position(insert([a, b, c], x, [[1], [3]]), x)", "[[1], [4]]"),
        // an association as the rules of replace
        ("replace([a, b], assoc([a -> 1, c -> 2]), 1)", "[1, b]"),
        ("count_distinct(quote(1 + 2, 4 - 1, 1 + 2, 2 + 1))", "3"),
        ("count_distinct([1, 2, 1])", "2"),
    ]);
}

#[test]
fn numeric_array_conversions() {
    check(&[
        ("numeric_array([1, 2, 3, 2.0^200], \"Real32\", clip_and_coerce)", "numeric_array([1.0, 2.0, 3.0, 3.4028234663852886e38], \"Real32\")"),
        ("numeric_array(byte_array([1, 2, 3, 4]), \"UnsignedInteger8\")", "numeric_array([1, 2, 3, 4], \"UnsignedInteger8\")"),
        ("na = numeric_array([-100, 0, 5, 120], \"Integer8\")", ""),
        ("numeric_array(na, \"Real32\")", "numeric_array([-100.0, 0.0, 5.0, 120.0], \"Real32\")"),
        ("numeric_array(na, \"UnsignedInteger8\", clip_and_coerce)", "numeric_array([0, 0, 5, 120], \"UnsignedInteger8\")"),
        ("numeric_array([1, 2], \"Real32\")", "numeric_array([1, 2], \"Real32\")"),
        ("normal(numeric_array([0, 1000, 2000], \"UnsignedInteger8\", clip_and_coerce))", "[0, 255, 255]"),
    ]);
}

#[test]
fn compile_of_parameters_and_body() {
    check(&[
        ("fc = compile([[x, _: Float]], x)", ""),
        // an argument outside the declared type is a failure naming it and the type; integers are real numbers
        ("[is_failure(fc(im)), is_failure(fc(2.5))]", "[true, false]"),
        ("fc(2.5)", "2.5"),
        ("fc(2)", "2"),
        ("is_failure(compile([[n, _: Integer]], n + 1)(1/2))", "true"),
        ("compile([x, y], x*y + 1)(2, 3)", "7"),
        ("compile([1], 2)", "failure(not_compilable, [1], \"a parameter is not a symbol\")"),
        ("own_values(\"zz\")", "[]"),
        ("zz = 7", ""),
        ("own_values(\"zz\")", "[zz :> 7]"),
    ]);
}
