//! Signs of modular functions on the lines where they are real, one-sided limits onto branch cuts, the
//! branch of the inverse Gudermannian function, inverse functions of their direct functions at numeric arguments,
//! Mathieu functions at high precision, and matrix builders that refuse a matrix too large for the memory left.
//! Every answer is checked by a second route (numeric values at 30 or more digits, identities, an entrywise
//! definition).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// `src` evaluates to `true`.
fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

/// The numbers `a` and `b` (terms), each evaluated at 50 digits, agree to 10^-tol in both parts.
fn close(e: &mut CoreEngine, a: &str, b: &str, tol: u32) {
    holds(e, &format!("{{let d = n({a}, 50) - n({b}, 50); abs(re(d)) < 10^-{tol} and abs(im_part(d)) < 10^-{tol}}}"));
}

/// The numbers `a` and `b` (inexact terms, evaluated as they are) agree to 10^-tol in both parts.
fn close_num(e: &mut CoreEngine, a: &str, b: &str, tol: u32) {
    holds(e, &format!("{{let d = ({a}) - ({b}); abs(re(d)) < 10^-{tol} and abs(im_part(d)) < 10^-{tol}}}"));
}

#[test]
fn modular_functions_have_a_sign_on_their_real_lines() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "function_sign(klein_invariant_j(im*(x^2 + 1)), x)"), "1");
    assert_eq!(ev(&mut e, "function_sign(klein_invariant_j(im*cosh(x)), x)"), "1");
    assert_eq!(ev(&mut e, "function_sign(modular_lambda(3 + im*(1 + x^2)), x)"), "-1");
    assert_eq!(ev(&mut e, "function_sign(dedekind_eta(12 + im*exp(x)), x)"), "-1");
    assert_eq!(ev(&mut e, "function_sign(dedekind_eta(im*exp(x)), x)"), "1");
    // J changes sign on re tau = 1/2 (zeros at 1/2 + i sqrt(3)/6 and 1/2 + i sqrt(3)/2)
    assert_eq!(ev(&mut e, "function_sign(klein_invariant_j(1/2 + im*exp(x)), x)"), "indeterminate");
    holds(&mut e, "re(klein_invariant_j(1/2 + 0.6*im)) > 0 and re(klein_invariant_j(1/2 + 2.0*im)) < 0");
    assert_eq!(ev(&mut e, "function_domain(klein_invariant_j(x), x)"), "false");
    // J(i t) >= 1 from the theta functions of the real nome e^(-pi t) (DLMF 23.15.7), equal to the computed value
    for t in ["3/10", "1", "5/2", "7"] {
        let theta = format!(
            "{{let q = exp(-pi*{t}); let a = elliptic_theta(2, 0, q); let b = elliptic_theta(3, 0, q); let c = elliptic_theta(4, 0, q); (a^8 + b^8 + c^8)^3/(54*(a*b*c)^8)}}"
        );
        holds(&mut e, &format!("n({theta}, 30) > 1 - 10^-25"));
        close(&mut e, &theta, &format!("klein_invariant_j(im*{t})"), 20);
        // values on the line have an imaginary part exactly 0 (a real ball at 30 digits)
        holds(&mut e, &format!("im_part(klein_invariant_j(n(im*{t}))) == 0"));
        holds(&mut e, &format!("is_real(klein_invariant_j(n(im*{t}, 30)))"));
    }
    // lambda(1 + i t) = lambda(i t)/(lambda(i t) - 1) < 0, eta(12 + i t) = -eta(i t)
    close(&mut e, "modular_lambda(1.0 + 2.0*im)", "modular_lambda(2.0*im)/(modular_lambda(2.0*im) - 1)", 14);
    holds(&mut e, "re(modular_lambda(1.0 + 2.0*im)) < 0");
    close(&mut e, "dedekind_eta(12 + 2*im)", "-dedekind_eta(2*im)", 25);
}

#[test]
fn one_sided_limits_onto_branch_cuts_take_the_side_of_the_path() {
    let mut e = CoreEngine::new();
    close(&mut e, "limit(atan(2*im - s), s -> 0, direction: from_above)", "-pi/2 + im*log(3)/2", 40);
    close(&mut e, "limit(atan(2*im - s), s -> 0, direction: from_below)", "pi/2 + im*log(3)/2", 40);
    assert!(ev(&mut e, "limit(atan(2*im - s), s -> 0)").contains("no_limit"));
    // each limit equals the function a small step along the path
    for (f, dir, step) in [
        ("atan(2*im - s)", "from_above", "10^-12"),
        ("atanh(2 + im*s)", "from_above", "10^-12"),
        ("atanh(2 + im*s)", "from_below", "-10^-12"),
        ("asin(3 + im*s)", "from_below", "-10^-12"),
        ("acos(-2 + im*s)", "from_above", "10^-12"),
        ("acot(im/2 + s)", "from_below", "-10^-12"),
        ("acoth(1/2 + im*s)", "from_above", "10^-12"),
        ("acosh(-3 + im*s)", "from_below", "-10^-12"),
        ("asinh(3*im + s)", "from_above", "10^-12"),
        ("log(-2 + im*s^3)", "from_below", "-10^-12"),
    ] {
        let l = ev(&mut e, &format!("limit({f}, s -> 0, direction: {dir})"));
        assert!(!l.contains("limit"), "{f} {dir}: {l}");
        close(&mut e, &l, &format!("rewrite({f}, s -> {step})"), 10);
    }
    // along the cut itself: the principal value on the cut
    close(&mut e, "limit(asin(x), x -> 2, direction: from_above)", "asin(2)", 40);
    // a side that depends on a parameter: no value
    assert!(ev(&mut e, "limit(atan(2*im - a*s), s -> 0, direction: from_above)").starts_with("limit("));
}

#[test]
fn inverse_gudermannian_takes_the_branch_of_log_tan() {
    let mut e = CoreEngine::new();
    close(&mut e, "inverse_gudermannian(3)", "2*acoth(tan(3/2)) + im*pi", 40);
    close(&mut e, "inverse_gudermannian(3.0)", "0.1420681583893965 + 3.141592653589793*im", 14);
    close(&mut e, "inverse_gudermannian(-3.0)", "-0.1420681583893965 + 3.141592653589793*im", 14);
    // exact and numeric evaluation agree; the value is log(tan(z/2 + pi/4)) (DLMF 4.23(viii)) for real and complex z
    for z in ["3", "-3", "e", "-1/2", "5/2 + im/3", "-1 - 2*im", "1 + 2*im", "4 + im/10", "-7/2"] {
        close(&mut e, &format!("inverse_gudermannian({z})"), &format!("log(tan(({z})/2 + pi/4))"), 40);
        close_num(&mut e, &format!("n(inverse_gudermannian({z}), 40)"), &format!("inverse_gudermannian(n({z}, 40))"), 35);
        close(&mut e, &format!("inverse_gudermannian({z})"), &format!("inverse_gudermannian(n({z}))"), 12);
    }
    // gudermannian(inverse_gudermannian(z)) = z on a sweep of (-pi, pi) (off the singular points +-pi/2) and of the strip
    for k in -9..=9 {
        let x = format!("{k}/3");
        close(&mut e, &format!("gudermannian(inverse_gudermannian({x}))"), &x, 40);
        let z = format!("{k}/6 + im/{}", k * k + 2);
        close(&mut e, &format!("gudermannian(inverse_gudermannian({z}))"), &z, 40);
    }
    assert_eq!(ev(&mut e, "inverse_gudermannian(-x)"), "inverse_gudermannian(-x)");
    assert_eq!(ev(&mut e, "inverse_gudermannian(-1/2)"), "-inverse_gudermannian(1/2)");
}

#[test]
fn inverse_functions_of_their_direct_functions_reduce() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "acoth(coth(5))"), "5");
    assert_eq!(ev(&mut e, "acosh(cosh(-2))"), "2");
    holds(&mut e, "asin(sin(3)) == pi - 3");
    holds(&mut e, "acos(cos(7)) == 7 - 2*pi");
    assert_eq!(ev(&mut e, "asin(sin(x))"), "asin(sin(x))");
    // all twelve pairs at real and complex arguments: the result is the inverse's numeric value at the direct
    // function's numeric value (an independent route through the principal branches)
    let pairs = [
        ("asin", "sin"),
        ("acos", "cos"),
        ("atan", "tan"),
        ("acot", "cot"),
        ("asec", "sec"),
        ("acsc", "csc"),
        ("asinh", "sinh"),
        ("acosh", "cosh"),
        ("atanh", "tanh"),
        ("acoth", "coth"),
        ("asech", "sech"),
        ("acsch", "csch"),
    ];
    for (inv, f) in pairs {
        let mut reduced = 0;
        for c in ["5", "1/2", "-5", "7", "sqrt(2)", "10*pi/7", "1 + 2*im", "-1 + 4*im", "3 + im", "2 - 3*im/2"] {
            let r = ev(&mut e, &format!("{inv}({f}({c}))"));
            if r.contains(inv) {
                continue;
            }
            reduced += 1;
            close(&mut e, &r, &format!("{inv}(n({f}({c}), 50))"), 30);
        }
        assert_eq!(reduced, 10, "{inv}");
        assert_eq!(ev(&mut e, &format!("{inv}({f}(1/2))")), "1/2", "{inv}");
    }
}

#[test]
fn mathieu_functions_at_high_precision() {
    let mut e = CoreEngine::new();
    // 50 digits agree with 80
    close_num(&mut e, "n(mathieu_c(2, 1, 37/10), 50)", "n(mathieu_c(2, 1, 37/10), 80)", 48);
    close_num(&mut e, "n(mathieu_s(5/2, 7, 2 + im), 40)", "n(mathieu_s(5/2, 7, 2 + im), 60)", 35);
    // a_5(20) against the eigenvalue of the truncated matrix of the Fourier coefficients (DLMF 28.4.6)
    let m = "table(if i == j then (2*i - 1)^2 + (if i == 1 then 20 else 0) else if abs(i - j) == 1 then 20 else 0, [i, 40], [j, 40])";
    close_num(&mut e, &format!("sort(eigenvalues(n({m}, 60)))[3]"), "n(mathieu_characteristic_a(5, 20), 40)", 35);
    // a root search at 25 digits over a characteristic value: the function vanishes at the root
    let r = ev(&mut e, "rewrite(q, find_root(mathieu_c(mathieu_characteristic_a(5, q), q, im/2), [q, 20], working_precision: 25))");
    close_num(&mut e, &r, "22.66671708059834602782345", 20);
    close_num(&mut e, &format!("mathieu_c(mathieu_characteristic_a(5, {r}), {r}, im/2)"), "0", 15);
    // an imaginary-valued function on the real line: the refined root stays real
    for (k, q0, root) in [(5, 30, "28.9455003886044669876"), (3, 15, "18.3077641751966241527")] {
        let r = ev(&mut e, &format!("rewrite(q, find_root(mathieu_s(mathieu_characteristic_b({k}, q), q, im/2), [q, {q0}], working_precision: 25))"));
        assert!(!r.contains("im"), "{r}");
        close_num(&mut e, &r, root, 18);
    }
}

#[test]
fn matrix_builders_refuse_what_does_not_fit() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "memory_constrained(hankel_matrix(1000), 10^6)"), "failure(limit, memory, [1000, 1000])");
    assert_eq!(ev(&mut e, "memory_constrained(hankel_matrix(10000, target_structure: automatic), 10^8)"), "failure(limit, memory, [10000, 10000])");
    assert_eq!(ev(&mut e, "memory_constrained(toeplitz_matrix(700), 10^6)"), "failure(limit, memory, [700, 700])");
    assert_eq!(ev(&mut e, "{let m = constant_array(1, [600, 600]); memory_constrained(upper_triangularize(m), 10^7)}"), "failure(limit, memory, [600, 600])");
    holds(&mut e, "memory_constrained(memory_available() <= 10^6, 10^6)");
    // what fits is built, and equals the entrywise definitions
    assert_eq!(ev(&mut e, "dimensions(memory_constrained(hankel_matrix(300), 10^8))"), "[300, 300]");
    for (c, r) in [("[a, b, c]", "[c, d, e, f]"), ("range(7)", "[7, 0, 0]"), ("[1, 2]", "[2, 5, 6, 7, 8]"), ("[x]", "[x]")] {
        holds(&mut e, &format!("{{let c = {c}; let r = {r}; let m = length(c); hankel_matrix(c, r) == table(if i + j - 1 <= m then c[i + j - 1] else r[i + j - m], [i, m], [j, length(r)])}}"));
        holds(&mut e, &format!("{{let c = {c}; let r = {r}; toeplitz_matrix(c, r) == table(if i >= j then c[i - j + 1] else r[j - i + 1], [i, length(c)], [j, length(r)])}}"));
    }
    assert_eq!(ev(&mut e, "hankel_matrix(0)"), "[]");
}
