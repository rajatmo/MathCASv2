//! Values at the ends of domains and at removable points (sinc for j_0, Lommel s and t at 0, q-functions at q = 1,
//! the branch point of product_log), the real roots of polynomials with parameters, elliptic integrals beyond 1 on
//! balls, the orders of the sort family, and parts held by `quote`. Expected values come from DLMF identities
//! (cited at each test) or from substitution.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate in one fresh session (one worker thread); `TIMEOUT` after 60 s.
fn ev_all(srcs: &[&str]) -> Vec<String> {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<String>>();
    let s: Vec<String> = srcs.iter().map(|s| s.to_string()).collect();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        // printed as the front ends print (`-1/e`, `sin(x)/x^2`)
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, display: true, ..EngineConfig::default() });
        let out = s.iter().map(|src| e.eval(src, &stop).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"))).collect();
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| vec!["TIMEOUT".to_string(); srcs.len()])
}

fn ev(src: &str) -> String {
    ev_all(&[src]).remove(0)
}

fn close(src: &str, want: f64) {
    let got = ev(src);
    let v: f64 = got.trim().parse().unwrap_or_else(|_| panic!("{src} gave {got}"));
    assert!((v - want).abs() <= 1e-12 * want.abs().max(1.0), "{src} gave {got}, want {want}");
}

#[test]
fn spherical_j0_is_sinc_and_jn_of_a_symbol_keeps_the_point_zero() {
    // decision 5: j_0 = sinc, value 1 at 0, domain everything
    assert_eq!(ev("spherical_bessel_j(0, x)"), "sinc(x)");
    assert_eq!(ev("function_domain(spherical_bessel_j(0, x), x)"), "true");
    assert_eq!(ev("function_domain(spherical_bessel_j(0, z), z, complexes)"), "true");
    // j_n(-z) = (-1)^n j_n(z) (DLMF 10.47.14); the elementary form is function_expand's
    assert_eq!(ev("spherical_bessel_j(1, -z)"), "-spherical_bessel_j(1, z)");
    assert_eq!(ev("function_expand(spherical_bessel_j(1, x))"), "sin(x)/x^2 - cos(x)/x");
    // the series and limit at 0 and the integral through that form: j_1 = x/3 - x^3/30 + ... (DLMF 10.53.1)
    assert_eq!(ev("series(spherical_bessel_j(1, x), x, 0, 4)"), "x/3 - x^3/30 + O(x^5)");
    assert_eq!(ev("limit(spherical_bessel_j(3, x)/x^3, x -> 0)"), "1/105");
    assert_eq!(ev("integrate(spherical_bessel_j(1, x), x)"), "-sin(x)/x");
    assert_eq!(ev("series(sinc(x), x, 0, 5)"), "1 - x^2/6 + x^4/120 + O(x^6)");
    assert_eq!(ev("limit(sinc(x), x -> -inf)"), "0");
    // numbers keep the elementary value
    assert_eq!(ev("spherical_bessel_j(1, pi)"), "pi^-1");
}

#[test]
fn lommel_s_and_t_at_zero() {
    // s_(mu, nu)(z) = z^(mu + 1)/((mu + 1)^2 - nu^2) (1 + O(z^2)) (DLMF 11.9.3): -1/nu^2 at mu = -1
    assert_eq!(ev("lommel_s1(-1, 1/2, 0)"), "-4");
    close("lommel_s1(-1, 1/2, 0.)", -4.0);
    close("lommel_t1(-1, 1/2, 0.)", -4.0);
    close("lommel_s1(1/2, 1/3, 0.)", 0.0);
    assert_eq!(ev("lommel_s1(-2, 1/2, 0.)"), "complex_infinity");
    // agrees with the value near 0
    close("re(n(lommel_s1(-1, 1/2, 10^-9)))", -4.0);
}

#[test]
fn q_functions_at_q_one_are_the_classical_ones() {
    // Gamma_q -> Gamma, [n]_q! -> Gamma(n + 1) as q -> 1 (DLMF 5.18.4, 17.2.1)
    assert_eq!(ev("q_factorial(1/2, 1)"), "(1/2)*pi^(1/2)");
    assert_eq!(ev("q_factorial(x, 1)"), "gamma(1 + x)");
    close("q_gamma(2.5, 1.)", 1.329_340_388_179_137);
    assert_eq!(ev("q_binomial(5, 2, 1)"), "10");
    // a negative q and a non-integer count: complex
    assert!(ev("q_factorial(0.5, -0.5)").contains("im"));
}

#[test]
fn product_log_at_its_branch_point() {
    // W(-1/e) = -1 (DLMF 4.13): the end of the real domain is a root
    assert_eq!(ev("reduce(1 + product_log(x) == 0, x, reals)"), "x == -1/e");
    assert_eq!(ev("reduce(product_log(x) >= -1, x, reals)"), "-1/e <= x");
    assert_eq!(ev("solve(product_log(x) == -2, x, reals)"), "[]");
    assert_eq!(ev("solve(product_log(x) == a, x, reals)"), "[[x -> conditional_expression(a*exp(a), a >= -1)]]");
    // a root at the end of the domain that is not exact: no answer rather than false
    assert!(!ev("reduce(x + product_log(x) == -1 - 1/e, x, reals)").starts_with("false"));
}

#[test]
fn real_roots_of_polynomials_with_parameters() {
    // y^3 - 3 y - 2 = (y + 1)^2 (y - 2): at x = 3 the roots -1 and 2
    let r = ev("reduce(y^3 - x*y == 2, y, reals)");
    assert!(r.contains("x == 3 and (y == -1 or y == 2)"), "{r}");
    let s = ev("solve(y^3 - x*y == 2, y, reals)");
    assert!(!s.contains("im") && s.contains("conditional_expression(2, x == 3)"), "{s}");
    // the real cube root, not the principal x^(1/3) and its rotations
    assert_eq!(ev("solve(y^3 == x, y, reals)"), "[[y -> root(&(x - &1^3), 1)]]");
    assert!(!ev("solve(y^4 == x, y, reals)").contains("im"));
    // the largest root for x > 3 at x = 4: y = 2.2143... (y^3 - 4 y - 2 = 0)
    let v = ev_all(&["s = solve(y^3 - x*y == 2, y, reals)", "n(rewrite(rewrite(y, s[6]), x -> 4))"]);
    assert!(v[1].starts_with("2.214319743377"), "{}", v[1]);
}

#[test]
fn elliptic_integrals_beyond_one_on_balls() {
    // K(m) = (K(1/m) - i K(1 - 1/m))/sqrt(m) on the cut from below (DLMF 19.7.3); the machine value agrees
    let k = ev("n(elliptic_k(3/2), 20)");
    assert!(k.starts_with("1.656638170236594") && k.contains("1.415737208425956"), "{k}");
    let e = ev("n(elliptic_e(7), 20)");
    assert!(e.starts_with("0.302460698213834") && e.contains("2.099709144779686"), "{e}");
    // one value is cheap (it was tens of milliseconds per value): a table of 400 within the work of a moment
    assert_eq!(ev("length(table(n(elliptic_k(1 + k/7)), k, 400))"), "400");
    assert_eq!(ev("reduce(elliptic_k(x) != 0 and x < 1, x, reals)"), "x < 1");
}

#[test]
fn the_sort_family_orders_by_one_rule() {
    // numerical order: compounds by length, then their arguments numerically; quantities by unit, then value
    assert_eq!(ev("numerical_sort([h(0, 2), h(inf, 1), h(-inf, 3)])"), "[h(-inf, 3), h(0, 2), h(inf, 1)]");
    assert_eq!(ev("numerical_sort([h(0, 2, foo), h(inf, 1), h(-inf, 3)])"), "[h(-inf, 3), h(inf, 1), h(0, 2, foo)]");
    assert_eq!(
        ev("numerical_sort([quantity(1, \"Yards\"), quantity(1, \"Minutes\"), quantity(50, \"Centimeters\"), quantity(30, \"Seconds\")])"),
        "[quantity(50, \"Centimeters\"), quantity(1, \"Yards\"), quantity(30, \"Seconds\"), quantity(1, \"Minutes\")]"
    );
    // ordering with a predicate is the order sort gives
    assert_eq!(ev("ordering([2, 6, 1, 9, 1, 2, 3], all, greater)"), "[4, 2, 7, 6, 1, 5, 3]");
    assert_eq!(ev("sort([2, 6, 1, 9, 1, 2, 3], greater)"), "[9, 6, 3, 2, 2, 1, 1]");
    // reverse_sort by the negated predicate: pairs it does not decide keep their order
    let v = ev_all(&["rule p((t_ when is_number(t)), _: Sym) := -1", "reverse_sort([x, 1, z, 5, a, 2, e_u], p)"]);
    assert_eq!(v[1], "[x, 1, z, 5, a, 2, e_u]");
    assert_eq!(ev("reverse_sort([\"c\", \"a\", \"b\"], alphabetic_order)"), "[\"c\", \"b\", \"a\"]");
}

#[test]
fn held_parts_and_logical_expansion() {
    assert_eq!(ev("replace_at(quote(1, 2, 3), x_ :> f(x + 1), 2)"), "quote(1, f(2 + 1), 3)");
    assert_eq!(ev("replace_at([1, 2, 3], x_ :> x + 1, 2)"), "[1, 3, 3]");
    // not all pairwise different: some pair equal (true only for three truth values)
    assert_eq!(ev("logical_expand(not unequal(x, y, z))"), "x == y or x == z or y == z");
    assert_eq!(ev("logical_expand(p or not p)"), "true");
}

#[test]
fn formula_options_are_not_answered_by_an_earlier_call() {
    let v = ev_all(&[
        "last(reduce(x^4 + 2*x^2 + 3*x + 4 == 0, x))",
        "last(reduce(x^4 + 2*x^2 + 3*x + 4 == 0, x, quartics: true))",
    ]);
    assert!(v[0].contains("root("), "{}", v[0]);
    assert!(!v[1].contains("root("), "{}", v[1]);
}
