//! Round 40: argument forms of `reduce` (one argument, `element(x, d)` as the variable, the primes, the algebraic
//! numbers, the signed rationals), `find_instance` in one integer variable with any function that evaluates at
//! integers, the derivative of a sum over the roots of a polynomial that depends on the variable, the even perfect
//! numbers, and the branch cut of the Lommel functions of a non-integer order. Each answer is checked by another
//! route: the two-argument form, a filter over a range, substitution, a difference quotient, or the jump of the
//! values across the cut.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn one(src: &str) -> String {
    ev(&mut CoreEngine::new(), src)
}

#[test]
fn reduce_with_the_variables_of_the_condition() {
    let mut e = CoreEngine::new();
    // the documented inputs: conditions already decided, and the cut of a Hankel function in the parts of z
    // E_n is meromorphic exactly for n = 0, -1, ... (E_0(x) = exp(-x)/x, DLMF 8.19.6), with a logarithmic branch
    // point at 0 otherwise
    assert_eq!(ev(&mut e, "function_meromorphic(exp_integral_e(n, x), x)"), "conditional_expression(true, element(n, integers) and n <= 0)");
    assert_eq!(ev(&mut e, "reduce(function_meromorphic(exp_integral_e(1, x), x))"), "false");
    let cut = "re(z) <= 0 and im_part(z) == 0";
    assert_eq!(ev(&mut e, "reduce(function_singularities(hankel_h1(n, z), z, complexes))"), cut);
    assert_eq!(ev(&mut e, "reduce(function_discontinuities(hankel_h2(n, z), z, complexes))"), cut);
    // variations, each the same as the form with the variables named
    for (c, vars) in [("x^2 < 1", "x"), ("x^2 + y^2 < 1 and y > 0", "[x, y]"), ("exists(y, x^2 + y^2 < 1)", "x")] {
        assert_eq!(ev(&mut e, &format!("reduce({c})")), ev(&mut e, &format!("reduce({c}, {vars})")), "{c}");
    }
    // a point of the plane named by z == 0 joins the conditions on the parts
    assert_eq!(ev(&mut e, "reduce(z == 0 or im_part(z) == 0 and re(z) < 0, z)"), cut);
}

#[test]
fn reduce_with_an_element_as_the_variable() {
    let mut e = CoreEngine::new();
    let r = ev(&mut e, "reduce(next_prime(x) - x == 2 and 0 < x and x < 10, element(x, integers))");
    assert_eq!(r, "x == 3 or x == 5 or x == 9");
    assert_eq!(ev(&mut e, "select(range(1, 9), k => next_prime(k) - k == 2)"), "[3, 5, 9]");
    // variations: the reals, and the integers whose next prime is 23
    assert_eq!(ev(&mut e, "reduce(x^2 < 4, element(x, reals))"), ev(&mut e, "reduce(x^2 < 4, x, reals)"));
    let r = ev(&mut e, "reduce(next_prime(x) == 23 and 0 < x and x < 30, element(x, integers))");
    assert_eq!(r, "x == 19 or x == 20 or x == 21 or x == 22");
    assert_eq!(ev(&mut e, "select(range(1, 29), k => next_prime(k) == 23)"), "[19, 20, 21, 22]");
}

#[test]
fn reduce_over_the_primes() {
    let mut e = CoreEngine::new();
    let r = ev(&mut e, "reduce(10^6 < x and x < 10^6 + 100, x, primes)");
    assert_eq!(r, "x == 1000003 or x == 1000033 or x == 1000037 or x == 1000039 or x == 1000081 or x == 1000099");
    assert_eq!(ev(&mut e, "select(range(10^6 + 1, 10^6 + 99), is_prime)"), "[1000003, 1000033, 1000037, 1000039, 1000081, 1000099]");
    assert_eq!(ev(&mut e, "reduce(x^2 + y^2 <= 10, [x, y], primes)"), "x == 2 and y == 2");
    // a variation in two variables: the Goldbach partitions of 10, each pair checked
    assert_eq!(ev(&mut e, "reduce(x + y == 10, [x, y], primes)"), "x == 3 and y == 7 or x == 5 and y == 5 or x == 7 and y == 3");
    assert_eq!(ev(&mut e, "select(range(2, 8), k => is_prime(k) and is_prime(10 - k))"), "[3, 5, 7]");
}

#[test]
fn reduce_over_the_algebraic_numbers() {
    let mut e = CoreEngine::new();
    // documented: sqrt(e) is not algebraic, sqrt(2) is
    assert_eq!(ev(&mut e, "reduce((x^2 - 2)*(x^2 - e) == 0, x, algebraics)"), "x == -2^(1/2) or x == 2^(1/2)");
    assert_eq!(ev(&mut e, "element(e^(1/2), algebraics)"), "false");
    // variations: pi left out, the complex roots kept; cube roots of 7 kept, +-pi left out
    let r = ev(&mut e, "reduce((x^2 + 1)*(x - pi) == 0, x, algebraics)");
    assert!(!r.contains("pi") && r.matches("x ==").count() == 2 && r.contains("im"), "{r}");
    let r = ev(&mut e, "reduce((x^3 - 7)*(x^2 - pi^2) == 0, x, algebraics)");
    assert!(!r.contains("pi") && r.matches("x ==").count() == 3, "{r}");
    assert_eq!(ev(&mut e, "simplify(rewrite(x^3 - 7, [x -> 7^(1/3)]))"), "0");
}

#[test]
fn reduce_over_signed_rationals() {
    // documented
    assert_eq!(one("reduce(element(x, negative_rationals), x, nonnegative_rationals)"), "false");
    assert_eq!(one("reduce(element(x, nonpositive_rationals), x, nonnegative_rationals)"), "x == 0");
    assert_eq!(one("reduce(element(x, positive_rationals), x, nonpositive_rationals)"), "false");
    assert_eq!(one("reduce(element(x, nonnegative_rationals), x, nonpositive_rationals)"), "x == 0");
    // variations: an inequality meeting the domain in one point, an empty interval
    assert_eq!(one("reduce(x <= 0, x, nonnegative_rationals)"), "x == 0");
    assert_eq!(one("reduce(x < 1/2 and x > 1/2, x, positive_rationals)"), "false");
}

#[test]
fn find_instance_in_one_integer_variable() {
    let mut e = CoreEngine::new();
    // documented, each checked by substitution and the integer before it
    assert_eq!(ev(&mut e, "find_instance(prime_pi(n) == 7, n, integers)"), "[[n -> 17]]");
    assert_eq!(ev(&mut e, "[prime_pi(16), prime_pi(17)]"), "[6, 7]");
    assert_eq!(ev(&mut e, "find_instance([mod(x, 3) == 1, mod(x, 5) == 2], x, integers)"), "[[x -> 7]]");
    assert_eq!(ev(&mut e, "find_instance(next_prime(p) == 1 + p and 0 < p and p < 10, p, integers)"), "[[p -> 1]]");
    assert_eq!(ev(&mut e, "next_prime(1)"), "2");
    // variations: a totient value, two instances of a quadratic residue
    assert_eq!(ev(&mut e, "find_instance(euler_phi(n) == 12 and n > 20, n, integers)"), "[[n -> 21]]");
    assert_eq!(ev(&mut e, "euler_phi(21)"), "12");
    assert_eq!(ev(&mut e, "find_instance(mod(n^2, 7) == 2, n, integers, 2)"), "[[n -> 3], [n -> -3]]");
    assert_eq!(ev(&mut e, "[mod(9, 7), mod(1, 7), mod(4, 7)]"), "[2, 1, 4]");
}

#[test]
fn find_instance_in_one_real_variable() {
    let mut e = CoreEngine::new();
    // documented: a rational root first, a point of an interval, a root of an equation with absolute values
    assert_eq!(ev(&mut e, "find_instance(1 - 2*x + x^5 == 0, x, reals)"), "[[x -> 1]]");
    assert_eq!(ev(&mut e, "find_instance(1 - 2*x + x^5 < 0, x, reals)"), "[[x -> -2]]");
    assert_eq!(ev(&mut e, "1 - 2*(-2) + (-2)^5 < 0"), "true");
    assert_eq!(ev(&mut e, "find_instance(abs(-1 + (x + abs(2 + x))^2)^2 == 9 and x != 0, x, reals)"), "[[x -> -2]]");
    assert_eq!(ev(&mut e, "abs(-1 + (-2 + abs(2 - 2))^2)^2"), "9");
    // variations: an irrational root checked by substitution, an empty set
    let r = ev(&mut e, "r = find_instance(x^2 == 2, x, reals); r");
    assert!(r.contains("2^(1/2)"), "{r}");
    assert_eq!(ev(&mut e, "simplify(rewrite(x^2, r[1]))"), "2");
    assert_eq!(ev(&mut e, "find_instance(x^2 < -1, x, reals)"), "[]");
}

#[test]
fn derivative_of_a_root_sum_with_moving_roots() {
    let mut e = CoreEngine::new();
    // documented: the roots of t^5 + a t + 1 move with a; the derivative against a central difference quotient
    let f = ev(&mut e, "f = d(root_sum(&(&1^5 + a*&1 + 1), &(exp(&1))), a); f");
    assert!(f.starts_with("root_sum(") && !f.contains("failure"), "{f}");
    let check = |e: &mut CoreEngine, p: &str, g: &str, a: &str| {
        let at = |v: &str| format!("root_sum(&({}), &({}))", p.replace('a', &format!("({v})")), g.replace('a', &format!("({v})")));
        let src = format!(
            "chop(n(rewrite(d(root_sum(&({p}), &({g})), a), a -> {a}), 30) - n(({} - {})/(2*10^-12), 30), 10^-15)",
            at(&format!("{a} + 10^-12")),
            at(&format!("{a} - 10^-12"))
        );
        assert_eq!(ev(e, &src), "0", "{p}, {g} at {a}");
    };
    check(&mut e, "&1^5 + a*&1 + 1", "exp(&1)", "3/2");
    // variations: the summand depends on a too; another polynomial and point
    check(&mut e, "&1^3 + a*&1 + 1", "sin(a*&1)", "2");
    check(&mut e, "&1^4 + &1 + a", "&1*exp(&1)", "-5/3");
}

#[test]
fn limits_of_root_sums_over_fixed_roots() {
    let mut e = CoreEngine::new();
    // documented: sin(a t)/a -> t, and the sum of the roots of t^5 + 2 t^4 + 11 t + 1 is -2
    assert_eq!(ev(&mut e, "limit(root_sum(&(&1^5 + 2*&1^4 + 11*&1 + 1), &(sin(a*&1)/a)), a -> 0)"), "-2");
    // variations: the limit is the value at a continuity point (checked numerically), and a summand whose own limit
    // is infinite is left to the general method (the sum of 1/r over the roots of t^3 + 2 is 0)
    let l = ev(&mut e, "limit(root_sum(&(&1^5 + 11*&1 + 1), &(exp(a*&1))), a -> 1)");
    assert_eq!(ev(&mut e, &format!("chop(n({l} - root_sum(&(&1^5 + 11*&1 + 1), &(exp(&1))), 20), 10^-15)")), "0");
    assert_eq!(ev(&mut e, "limit(root_sum(&(&1^4 - 3*&1 + 1), &((1 - cos(a*&1))/a^2)), a -> 0)"), "0");
    assert_eq!(ev(&mut e, "limit(root_sum(&(&1^3 + 2), &(1/(a*&1))), a -> 0)"), "0");
}

#[test]
fn apart_in_the_last_variable() {
    let mut e = CoreEngine::new();
    // documented
    assert_eq!(ev(&mut e, "apart((a*d + b*c)/(b*d))"), "a/b + c/d");
    // variations: the same as naming the last variable, and equal to the input at a point
    for f in ["1/((x + a)*(x + b)*(x + c))", "(x + y)/((x + 1)*(y + 1)*(x - y))"] {
        let v = if f.contains('y') { "y" } else { "x" };
        assert_eq!(ev(&mut e, &format!("apart({f})")), ev(&mut e, &format!("apart({f}, {v})")), "{f}");
        let src = format!("rewrite(apart({f}) - ({f}), [x -> 2/7, y -> 5/3, a -> 1/3, b -> -4, c -> 9/2])");
        assert_eq!(ev(&mut e, &src), "0", "{f}");
    }
}

#[test]
fn euler_and_bernoulli_polynomials_at_one_half() {
    let mut e = CoreEngine::new();
    // documented
    assert_eq!(ev(&mut e, "euler_e(n, 1/2)"), "euler_e(n)*2^-n");
    // checks: the symbolic form at several indices against the polynomials, also for the Bernoulli polynomial
    for k in [0, 4, 6, 9] {
        assert_eq!(ev(&mut e, &format!("rewrite(euler_e(n, 1/2), n -> {k}) - euler_e({k}, 1/2)")), "0", "E {k}");
        assert_eq!(ev(&mut e, &format!("rewrite(bernoulli_b(n, 1/2), n -> {k}) - bernoulli_b({k}, 1/2)")), "0", "B {k}");
    }
}

#[test]
fn even_perfect_numbers() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "perfect_number(3, \"Even\")"), "496");
    assert_eq!(ev(&mut e, "perfect_number(range(5), \"Even\") == map(perfect_number, range(5))"), "true");
    // each is perfect: sigma_1(k) = 2 k
    assert_eq!(ev(&mut e, "map(k => divisor_sigma(1, k) == 2*k, perfect_number(range(6)))"), "[true, true, true, true, true, true]");
    // no odd perfect number is known: no value
    let odd = ev(&mut e, "perfect_number(1, \"Odd\")");
    assert!(odd.contains("perfect_number"), "{odd}");
}

#[test]
fn lommel_functions_of_a_non_integer_order() {
    let mut e = CoreEngine::new();
    for f in ["lommel_s1", "lommel_s2", "lommel_t1", "lommel_t2"] {
        assert_eq!(ev(&mut e, &format!("function_discontinuities({f}(1/2, 1/3, z), z)")), "z <= 0", "{f}");
        // the values jump across the negative axis and agree across the positive one
        let jump = |e: &mut CoreEngine, x: &str| ev(e, &format!("abs(n({f}(1/2, 1/3, {x} + 10^-10*im) - {f}(1/2, 1/3, {x} - 10^-10*im), 20)) > 10^-3"));
        assert_eq!(jump(&mut e, "-2"), "true", "{f}");
        assert_eq!(jump(&mut e, "2"), "false", "{f}");
    }
    // variations: other orders
    assert_eq!(ev(&mut e, "function_discontinuities(lommel_s1(3/2, 1/5, z), z)"), "z <= 0");
    assert_eq!(ev(&mut e, "function_singularities(lommel_s2(1/3, 1/4, z), z)"), "z <= 0");
    // s at mu = 1/2 is z^(3/2) times an entire series: 0 at 0, so defined there on the real line
    assert_eq!(ev(&mut e, "function_domain(lommel_s1(1/2, 1/3, z), z)"), "z >= 0");
    // an integer order is not covered by these entries
    assert_eq!(ev(&mut e, "function_singularities(lommel_s1(2, 1/3, z), z)"), "function_singularities(lommel_s1(2, 1/3, z), z)");
}
