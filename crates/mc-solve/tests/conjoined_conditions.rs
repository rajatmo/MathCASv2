//! Conditions conjoined to the equations of `solve` (and of `n_solve`, `solve_values`, `reduce`) are applied, never
//! dropped: memberships (`element(x, reals | integers | rationals | primes | positive_reals ...)`), order relations,
//! `!=` and conditions on parameters, for polynomial, radical and transcendental equations and for systems. Every
//! case is checked twice: each returned solution is substituted into every conjunct (equations to 30 digits;
//! conditions exactly, else at 30 digits, else they must be the condition the solution carries), and the solutions
//! are compared with the solutions of the equations alone filtered by the conditions (or with the set given for a
//! family of solutions, which cannot be filtered member by member).
mod common;
use common::Sys;

/// The checker, in the language: `cc_check(sys, vars, expected)` gives
/// `[every solution meets every conjunct, the solutions match the expected ones, count, expected count]`.
const CHECKER: &str = r#"
# zero to 30 digits: exactly 0 after simplification, or of modulus below 10^-25 at 30 digits
fn cc_zero(d) = { let t = simplify(d); t === 0 or abs(n(t, 30)) < 10^-25 }
fn cc_strip(v) = if head(v) === conditional_expression then v[1] else v
fn cc_conds(s) = flat_map(r => if head(last(r)) === conditional_expression then [last(r)[2]] else [], s)
fn cc_plain(s) = map(r => first(r) -> cc_strip(last(r)), s)
fn cc_parts(c) = if head(c) === `and` then apply(list, c) else [c]
fn cc_known(t, s) = some(cc_conds(s), c => some(cc_parts(c), u => u === t))
fn cc_holds(k, s) =
  if head(k) === equal then cc_zero(rewrite(k[1] - k[2], cc_plain(s)))
  else { let t = rewrite(k, cc_plain(s)); t === true or n(t, 30) === true or cc_known(t, s) }
fn cc_conjuncts(c) = if head(c) === `and` or head(c) === list then flat_map(cc_conjuncts, apply(list, c)) else [c]
fn cc_order(k) = some([less, greater, less_equal, greater_equal], r => head(k) === r)
fn cc_keep(conds, u) = every(conds, k => { let t = rewrite(k, u); not (t === false) and not (n(t, 30) === false) and (not cc_order(k) or not is_numeric(rewrite(k[1] - k[2], u)) or abs(n(im_part(rewrite(k[1] - k[2], u)), 30)) < 10^-25) })
fn cc_same(s, u, vars) = every(vars, x => cc_zero(rewrite(x, cc_plain(s)) - rewrite(x, u)))
fn cc_check(sys, vars, expected) = {
  let sols = solve(sys, vars)
  let conj = cc_conjuncts(sys)
  let vs = if head(vars) === list then vars else [vars]
  let ok = is_list(sols) and every(sols, s => every(conj, k => cc_holds(k, s)))
  let found = is_list(sols) and length(sols) == length(expected) and every(expected, u => some(sols, s => cc_same(s, u, vs))) and every(sols, s => some(expected, u => cc_same(s, u, vs)))
  [ok, found, if is_list(sols) then length(sols) else sols, length(expected)]
}
fn cc_filtered(sys, vars) = {
  let conj = cc_conjuncts(sys)
  let eqs = select(conj, k => head(k) === equal)
  let conds = select(conj, k => not (head(k) === equal))
  select(solve(eqs, vars), u => cc_keep(conds, u))
}
"#;

fn check(sys: &mut Sys, system: &str, vars: &str, expected: Option<&str>, count: usize) {
    let exp = match expected {
        Some(e) => e.to_string(),
        None => format!("cc_filtered({system}, {vars})"),
    };
    let r = sys.run(&format!("cc_check({system}, {vars}, {exp})"));
    assert_eq!(r, format!("[true, true, {count}, {count}]"), "solve({system}, {vars})");
}

#[test]
fn conditions_are_applied_to_every_kind_of_equation() {
    let mut s = Sys::new();
    s.run(CHECKER);
    // (system, unknowns, expected solutions when they are a family, number of solutions)
    let cases: &[(&str, &str, Option<&str>, usize)] = &[
        // polynomial equations, one unknown: memberships, order relations, !=
        ("x^3 + x + 1 == 0 and element(x, reals)", "x", None, 1),
        ("x^3 + x + 1 == 0 and element(x, complexes)", "x", None, 3),
        ("x^3 + x + 1 == 0 and x != 1", "x", None, 3),
        ("x^3 + x + 1 == 0 and x < 0", "x", None, 1),
        ("x^3 + x + 1 == 0 and x > 0", "x", None, 0),
        ("x^5 - x - 1 == 0 and element(x, reals)", "x", None, 1),
        ("x^5 - x - 1 == 0 and element(x, positive_reals)", "x", None, 1),
        ("x^5 - x - 1 == 0 and element(x, negative_reals)", "x", None, 0),
        ("x^5 - x - 1 == 0 and element(x, integers)", "x", None, 0),
        ("x^5 - x - 1 == 0 and element(x, rationals)", "x", None, 0),
        ("x^4 == 1 and element(x, reals)", "x", None, 2),
        ("x^4 == 1 and x != 1", "x", None, 3),
        ("x^4 == 1 and x != 1 and x != -1", "x", None, 2),
        ("x^4 == 1 and element(x, integers)", "x", None, 2),
        ("x^4 - 5*x^2 + 4 == 0 and x > 0", "x", None, 2),
        ("x^4 - 5*x^2 + 4 == 0 and element(x, primes)", "x", None, 1),
        ("x^3 - 6*x^2 + 11*x - 6 == 0 and element(x, primes)", "x", None, 2),
        ("x^2 == 2 and element(x, rationals)", "x", None, 0),
        ("x^2 == 2 and element(x, algebraics)", "x", None, 2),
        ("x^2 == 4 and element(x, positive_reals)", "x", None, 1),
        ("x^2 == 4 and element(x, negative_reals)", "x", None, 1),
        ("x^2 == 4 and x >= 2", "x", None, 1),
        ("x^2 == 4 and x <= -2", "x", None, 1),
        ("x^6 == 64 and element(x, reals) and x != 2", "x", None, 1),
        ("x^3 == 8 and element(x, reals)", "x", None, 1),
        ("x^3 == 8 and x != 2", "x", None, 2),
        ("(x - 1)^2*(x + 3) == 0 and x != 1", "x", None, 1),
        ("x^2 + 1 == 0 and element(x, reals)", "x", None, 0),
        ("x^2 + 1 == 0 and x != im", "x", None, 1),
        ("x^7 - 3*x + 1 == 0 and element(x, reals)", "x", None, 3),
        ("x^7 - 3*x + 1 == 0 and x > 1", "x", None, 1),
        ("x^5 + x + 3 == 0 and element(x, negative_reals)", "x", None, 1),
        ("x^3 - 2 == 0 and element(x, reals)", "x", None, 1),
        ("x^3 - 2 == 0 and x > 2", "x", None, 0),
        ("[x^2 == 4, x > 0]", "x", None, 1),
        // radicals of the unknown
        ("sqrt(x) == x - 2 and x > 0", "x", None, 1),
        ("sqrt(x + 1) == x - 1 and element(x, reals)", "x", None, 1),
        ("x - sqrt(x) == 2 and x != 4", "x", None, 0),
        ("sqrt(x^2 + 1) == 2 and x > 0", "x", None, 1),
        // transcendental equations: the members of the families that meet the conditions
        ("exp(x) == 2 and element(x, reals)", "x", Some("[[x -> log(2)]]"), 1),
        ("exp(x) == 2 and x > 1", "x", Some("[]"), 0),
        ("exp(x) == 2 and x > 0", "x", Some("[[x -> log(2)]]"), 1),
        ("log(x) == 1 and x != e", "x", Some("[]"), 0),
        ("log(x) == 2 and x > 1", "x", Some("[[x -> e^2]]"), 1),
        ("sin(x) == 1/2 and x > 0 and x < 3", "x", Some("[[x -> pi/6], [x -> 5*pi/6]]"), 2),
        ("sin(x) == 0 and x > 0 and x < 7", "x", Some("[[x -> pi], [x -> 2*pi]]"), 2),
        ("cos(x) == 0 and x > 0 and x < 5", "x", Some("[[x -> pi/2], [x -> 3*pi/2]]"), 2),
        ("exp(2*x) - 3*exp(x) + 2 == 0 and x > 0", "x", Some("[[x -> log(2)]]"), 1),
        ("tan(x) == 1 and x > 0 and x < 3", "x", Some("[[x -> pi/4]]"), 1),
        ("sin(x) == 0 and abs(x) < 4 and x != pi", "x", Some("[[x -> -pi], [x -> 0]]"), 2),
        ("sin(x) == 0 and abs(x) < 4 and element(x, integers)", "x", Some("[[x -> 0]]"), 1),
        // a disk of the complex plane: the members of the families in it (abs(z) < r does not make z real)
        ("sin(z) == 2 and abs(z) < 3 and z != 1", "z", Some("[[z -> asin(2)], [z -> pi - asin(2)]]"), 2),
        ("exp(z) == 1 and abs(z) < 7 and z != 0", "z", Some("[[z -> 2*im*pi], [z -> -2*im*pi]]"), 2),
        ("x^2 == 2 and x != sqrt(2)", "x", None, 1),
        // conditions on parameters: kept with every solution
        ("x^2 == a and a > 0", "x", None, 2),
        ("x^2 == 4 and y > 0", "x", None, 2),
        ("x == a and element(a, reals)", "x", None, 1),
        ("x^2 == 4 and a != 0", "x", None, 2),
        ("x^3 == 1 and b < 0 and element(x, reals)", "x", None, 1),
        ("a*x == 1 and a > 0", "x", None, 1),
        // systems
        ("x^2 + y^2 == 1 and x == y and x > 0", "[x, y]", None, 1),
        ("x^2 + y^2 == 5 and x*y == 2 and element(x, positive_reals)", "[x, y]", None, 2),
        ("x^2 + y^2 == 5 and x*y == 2 and x > 0 and y > x", "[x, y]", None, 1),
        ("x + y == 3 and x - y == 1 and element(x, integers)", "[x, y]", None, 1),
        ("x^2 == y and y^2 == x and x != 0", "[x, y]", None, 3),
        ("x^2 == y and y^2 == x and element(x, reals)", "[x, y]", None, 2),
        ("x^2 + y^2 == 2 and x == y and y < 0", "[x, y]", None, 1),
        ("x*y == 1 and x + y == 3 and x > y", "[x, y]", None, 1),
        ("x^2 + y^2 == 1 and x - y == 0 and element(y, rationals)", "[x, y]", None, 0),
        ("x^3 + y == 0 and y == 1 and element(x, reals)", "[x, y]", None, 1),
        ("x^3 + y == 0 and y == 1 and x != -1", "[x, y]", None, 2),
    ];
    assert!(cases.len() >= 60);
    for &(system, vars, expected, count) in cases {
        check(&mut s, system, vars, expected, count);
    }
}

#[test]
fn the_reported_cubic_and_the_other_readers_of_a_system() {
    let mut s = Sys::new();
    // reduce: != is no order relation (the complex roots stay); order relations on a parameter make it real, not x
    let want = s.run("x == -1 or x == -im or x == im");
    assert_eq!(s.run("reduce(x^4 == 1 and x != 1, x)"), want);
    let want = s.run("y > 0 and (x == -1 or x == -im or x == im or x == 1)");
    assert_eq!(s.run("reduce(x^4 == 1 and y > 0, x)"), want);
    assert_eq!(s.run("solve(x^3 + x + 1 == 0 and element(x, reals), x)"), "[[x -> root(&(1 + &1 + &1^3), 1)]]");
    assert_eq!(s.run("solve(x^3 + x + 1 == 0 and element(x, reals), x) === solve(x^3 + x + 1 == 0, x, reals)"), "true");
    assert_eq!(s.run("solve_values(x^3 + x + 1 == 0 and element(x, reals), x)"), "[root(&(1 + &1 + &1^3), 1)]");
    assert_eq!(s.run("length(n_solve(x^3 + x + 1 == 0 and element(x, reals), x))"), "1");
    // a condition on a parameter is kept, not dropped
    assert_eq!(s.run("solve(x^2 == 4 and y > 0, x)"), "[[x -> conditional_expression(-2, y > 0)], [x -> conditional_expression(2, y > 0)]]");
    assert_eq!(s.run("n_solve(x^2 == 4 and y > 0, x)"), "[[x -> conditional_expression(-2.0, y > 0.0)], [x -> conditional_expression(2.0, y > 0.0)]]");
    assert_eq!(s.run("reduce(x^3 + x + 1 == 0 and element(x, reals), x)"), "x == root(&(1 + &1 + &1^3), 1)");
    // a family of solutions: a condition on its members is kept with the family's own condition
    assert_eq!(
        s.run("solve(sin(x) == 0 and x != pi, x)"),
        "[[x -> conditional_expression(2*pi*c(1), 2*pi*c(1) != pi and element(c(1), integers))], [x -> conditional_expression(pi + 2*pi*c(1), pi + 2*pi*c(1) != pi and element(c(1), integers))]]"
    );
    // a condition with unknowns that are not symbols: the call fails, saying why
    assert!(s.run("solve(f(x) == 2 and element(x, reals), f(x))").starts_with("failure("));
}
