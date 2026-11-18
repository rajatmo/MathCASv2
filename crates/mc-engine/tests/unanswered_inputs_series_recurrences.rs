//! Methods for inputs that gave no answer. Series of the reciprocal inverse hyperbolic functions at their
//! singular points (logarithmic forms of arguments with a pole, the side the assumptions give); recurrences
//! `y(n + k) = r(n) y(n)`, three-term recurrences of the classical functions, quadratic maps with closed-form orbits;
//! Jacobi's sums of squares as divisor sums; exact equality of sums of logarithms of rationals; the images of the
//! principal branches of the inverse functions. Each test has the documented input and variations checked by an
//! independent route (numbers at 30 digits, iteration of the recurrence, direct sums, the forward function).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// `src` evaluates to the same expression as `want` (both evaluated; structural equality).
fn same(e: &mut CoreEngine, src: &str, want: &str) {
    let r = ev(e, &format!("vv = {src}; vv === ({want})"));
    assert_eq!(r, "true", "{src}: {}", ev(e, "vv"));
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

/// An engine with `agree_digits(a, b)`: a and b agree to 20 digits (relative to 1 + |b|) (each evaluated at 30 digits, so that equal values
/// need not be told apart from 0) and `all_true(xs)`: every element is `true`.
fn engine() -> CoreEngine {
    let mut e = CoreEngine::new();
    ev(&mut e, "fn agree_digits(a, b) = { let dd = n(a, 30) - n(b, 30); abs(re(dd)) + abs(im_part(dd)) < 10^-20 * (1 + abs(n(b, 30))) }");
    ev(&mut e, "fn all_true(xs) = every(xs, t => t === true)");
    e
}

#[test]
fn series_of_reciprocal_inverse_hyperbolic_functions() {
    let mut e = engine();
    same(&mut e, "normal(series(acsch(x), x, 0, 4, assumptions: x > 0))", "log(2) - log(x) + x^2/4 - 3*x^4/32");
    same(&mut e, "normal(series(asech(z), z, 0, 4, assumptions: z > 0))", "log(2) - log(z) - z^2/4 - 3*z^4/32");
    // the side x < 0: asinh(1/x) = log(-x) - log(2) - x^2/4 + ... (before, the expansion for x > 0 was given)
    same(&mut e, "normal(series(acsch(x), x, 0, 4, assumptions: x < 0))", "log(-x) - log(2) - x^2/4 + 3*x^4/32");
    same(&mut e, "normal(series(asinh(1/x), x, 0, 4, assumptions: x < 0))", "log(-x) - log(2) - x^2/4 + 3*x^4/32");
    // independent check: each expansion against the function at a point of its side, 30 digits (error ~ x^6)
    for (f, s, x0, a) in [
        ("acsch(x)", "x > 0", "0", "1/50"),
        ("acsch(x)", "x < 0", "0", "-1/50"),
        ("asech(x)", "x > 0", "0", "1/50"),
        ("acoth(x)", "x > 1", "1", "1 + 1/50"),
        ("asech(x)", "x < 1", "1", "1 - 1/50"),
    ] {
        let src = format!("abs(n(rewrite(normal(series({f}, x, {x0}, 6, assumptions: {s})), x -> {a}) - rewrite({f}, x -> {a}), 30)) < 10^-9");
        holds(&mut e, &src);
    }
    // the hyperharmonic numbers in n: H(p, 0) = 0, the derivative psi'(p) (Conway and Guy)
    same(&mut e, "normal(series(hyper_harmonic_number(p, n), n, 0, 1))", "n*polygamma(1, p)");
    holds(&mut e, "abs(n(rewrite(normal(series(hyper_harmonic_number(p, n), n, 0, 3)), [p -> 2/3, n -> 1/1000]) - hyper_harmonic_number(2/3, 1/1000), 30)) < 10^-9");
    same(&mut e, "hyper_harmonic_number(p, 0)", "0");
    // both partial derivatives against central differences of the gamma form at 50 digits
    for (var, at) in [("r", "[r -> 7/10, m -> 13/10]"), ("m", "[r -> 7/10, m -> 13/10]")] {
        let src = format!(
            "{{ let f = (r, m) => gamma(m + r)/(gamma(m + 1)*gamma(r))*(digamma(m + r) - digamma(r)); let h = 10^-20;              let v = n(rewrite(d(hyper_harmonic_number(r, m), {var}), {at}), 40);              let c = n(rewrite((rewrite(f(r, m), {var} -> {var} + h) - rewrite(f(r, m), {var} -> {var} - h))/(2*h), {at}), 50);              abs(v - c) < 10^-15 }}"
        );
        holds(&mut e, &src);
    }
    // not answered: the branch points of acsch at +-i (a series there holds in a sector only)
    same(&mut e, "head(series(acsch(x), x, im, 2))", "series");
}

#[test]
fn recurrences_with_k_step_products() {
    let mut e = engine();
    same(
        &mut e,
        "r_solve(a(n + 2) == (n + 1)*2^n*a(n), a(n), n)",
        "[[a(n) -> c(1)*gamma((1 + n)/2)*2^(n^2/4)/sqrt(pi) + c(2)*gamma((1 + n)/2)*(-1)^n*2^(n^2/4)/sqrt(pi)]]",
    );
    same(
        &mut e,
        "expand(rewrite(y(n), r_solve(y(n + 2) - (n + 5)*y(n) == 0, y, n)[1]) - (c(1)*gamma((5 + n)/2)*2^(2 + n/2)/(3*sqrt(pi)) + c(2)*gamma((5 + n)/2)*(-1)^n*2^(2 + n/2)/(3*sqrt(pi))))",
        "0",
    );
    // independent check: every basis solution satisfies its recurrence at n = 0..7 (values at 30 digits)
    for (eq, order, lhs, rhs) in [
        ("y(n + 2) == (n + 1)*2^n*y(n)", 2, "s(k + 2)", "(k + 1)*2^k*s(k)"),
        ("y(n + 2) - (n + 5)*y(n) == 0", 2, "s(k + 2)", "(k + 5)*s(k)"),
        ("y(n + 3) == (n + 1)*y(n)", 3, "s(k + 3)", "(k + 1)*s(k)"),
        ("y(n + 2) == (2*n + 1)/(n + 3)*3^n*y(n)", 2, "s(k + 2)", "(2*k + 1)/(k + 3)*3^k*s(k)"),
    ] {
        let src = format!(
            "{{ let g = rewrite(y(n), r_solve({eq}, y(n), n)[1]); var ok = true; for j in range(1, {order}) {{ \
             let s = (k => rewrite(g, [n -> k, c(j) -> 1, c(_) -> 0])); ok = ok and all_true(table(agree_digits({lhs}, {rhs}), k, range(0, 7))) }}; ok }}"
        );
        holds(&mut e, &src);
    }
    // the first values of P equal the product of the ratios: P(0) = 1, P(2) = r(0) for y(n + 2) = (n + 5) y(n)
    same(&mut e, "rewrite(rewrite(y(n), r_solve(y(n + 2) == (n + 5)*y(n), y(n), n)[1]), [c(1) -> 1, c(2) -> 0, n -> 2])", "5");
}

#[test]
fn classical_three_term_recurrences() {
    let mut e = engine();
    same(
        &mut e,
        "r_solve((n + 2)*a(n + 2) == (2*n + 3)*x*a(n + 1) - (n + 1)*a(n), a, n)",
        "[[a -> (n => c(1)*legendre_p(n, x) + c(2)*legendre_q(n, x))]]",
    );
    same(
        &mut e,
        "r_solve(a(n) == 2*((1 + n)/(b + c*z))*a(n + 1) - a(n + 2), a(n), n)",
        "[[a(n) -> bessel_j(n, b + c*z)*c(1) + bessel_y(n, b + c*z)*c(2)]]",
    );
    same(
        &mut e,
        "r_solve(a(n) == 2*((n + 1)/n)*(c*z + d)*a(n + 1) - ((n + 2)/n)*a(n + 2), a, n)",
        "[[a -> (n => c(1)*chebyshev_t(n, d + c*z)/n + c(2)*chebyshev_u(n, d + c*z)/n)]]",
    );
    // the modified Bessel recurrence of K: K_n and (-1)^n I_n, with a real argument
    same(
        &mut e,
        "r_solve(a(n + 2) - 2*(n + 1)/x*a(n + 1) - a(n) == 0, a(n), n)",
        "[[a(n) -> bessel_k(n, x)*c(1) + bessel_i(n, x)*c(2)*(-1)^n]]",
    );
    // independent check: each basis function satisfies its recurrence at n = 1..5 at a rational point, 30 digits
    for (eq, lhs, rhs) in [
        ("(n + 2)*a(n + 2) == (2*n + 3)*x*a(n + 1) - (n + 1)*a(n)", "(k + 2)*s(k + 2)", "(2*k + 3)*(3/10)*s(k + 1) - (k + 1)*s(k)"),
        ("a(n) == 2*(1 + n)/x*a(n + 1) - a(n + 2)", "s(k)", "2*(1 + k)/(3/10)*s(k + 1) - s(k + 2)"),
        ("a(n) == 2*(n + 1)/n*x*a(n + 1) - (n + 2)/n*a(n + 2)", "s(k)", "2*(k + 1)/k*(3/10)*s(k + 1) - (k + 2)/k*s(k + 2)"),
        ("a(n + 2) - 2*(n + 1)/x*a(n + 1) - a(n) == 0", "s(k + 2)", "2*(k + 1)/(3/10)*s(k + 1) + s(k)"),
    ] {
        let src = format!(
            "{{ let g = rewrite(a(n), r_solve({eq}, a(n), n)[1]); var ok = true; for j in range(1, 2) {{ \
             let s = (k => rewrite(g, [n -> k, c(j) -> 1, c(_) -> 0, x -> 3/10])); \
             ok = ok and all_true(table(agree_digits({lhs}, {rhs}), k, range(1, 5))) }}; ok }}"
        );
        holds(&mut e, &src);
    }
    // not a classical recurrence: no answer from this method (Hyper finds none either)
    same(&mut e, "head(r_solve(a(n + 2) == (n^2 + 1)*a(n + 1) + a(n), a(n), n))", "r_solve");
}

#[test]
fn quadratic_maps() {
    let mut e = engine();
    same(&mut e, "r_solve(a(n + 1) == 2*(1 - a(n))*a(n), a(n), n)", "[[a(n) -> 1/2 - exp(c(1)*2^n)/2]]");
    same(&mut e, "expand(rewrite(x(i), r_solve(x(i + 1) == 4*x(i)*(1 - x(i)), x(i), i)[1]) - (1/2 - cos(c(1)*2^i)/2))", "0");
    // with an initial value the orbit is u0^(2^n) (prime powers combined) or 2 cos(2^n acos(u0/2))
    holds(&mut e, "simplify(r_solve_value([a(n + 1) == 2*(1 - a(n))*a(n), a(0) == 1/10], a(n), n) - (1/2)*(-2^2^(1 + n) + 5^2^n)*5^-2^n) == 0");
    // independent check: the closed forms against the iteration (exact)
    for (eqs, a0) in [
        ("[a(n + 1) == 2*(1 - a(n))*a(n), a(0) == 1/10]", "a"),
        ("[a(n + 1) == 3*a(n)^2 + 2*a(n), a(0) == 1]", "a"),
        ("[a(n + 1) == a(n)^2 - 2, a(0) == 3]", "a"),
        ("[a(n + 1) == 4*a(n)*(1 - a(n)), a(0) == 1/3]", "a"),
    ] {
        let src = format!(
            "{{ let s = r_solve_value({eqs}, {a0}(n), n); let t = recurrence_table({eqs}, {a0}, [n, 0, 5]); \
             all_true(table(agree_digits(rewrite(s, n -> k), t[k + 1]), k, range(0, 5))) }}"
        );
        holds(&mut e, &src);
    }
    // c = 1 in u^2 + c: no closed form of this kind, no answer
    same(&mut e, "head(r_solve(a(n + 1) == a(n)^2 + 1, a, n))", "r_solve");
}

#[test]
fn sums_of_squares_as_divisor_sums() {
    let mut e = engine();
    same(&mut e, "4*divisor_sum(n, &((-1)^((&1 - 1)/2)), is_odd)", "squares_r(2, n)");
    same(&mut e, "4*divisor_sum(n, &(sin(pi*&1/2)))", "squares_r(2, n)");
    same(&mut e, "8*divisor_sum(n, &(&1), &(mod(&1, 4) != 0))", "squares_r(4, n)");
    same(&mut e, "16*divisor_sum(n, &((-1)^(n + &1)*&1^3))", "squares_r(8, n)");
    // variations: a multiple of the weights, the residues 1 and 3 written with mod
    same(&mut e, "divisor_sum(n, &(3*sin(pi*&1/2)))", "3*squares_r(2, n)/4");
    same(&mut e, "divisor_sum(n, &(if mod(&1, 4) == 1 then 1 else (if mod(&1, 4) == 3 then -1 else 0)))", "(1/4)*squares_r(2, n)");
    // independent check: the divisor sums at n = 1..60 against squares_r (a count of lattice points)
    for (f, k, c) in [
        ("&((-1)^((&1 - 1)/2)), is_odd", 2, 4),
        ("&(&1), &(mod(&1, 4) != 0)", 4, 8),
        ("&((-1)^(m + &1)*&1^3)", 8, 16),
    ] {
        let src = format!("all_true(table({{ let m = j; {c}*divisor_sum(j, {f}) == squares_r({k}, j) }}, j, range(1, 60)))");
        holds(&mut e, &src);
    }
    same(&mut e, "generating_function(squares_r(2, n), n, x)", "elliptic_theta(3, 0, x)^2");
    same(&mut e, "z_transform(squares_r(2, n), n, z)", "elliptic_theta(3, 0, 1/z)^2");
    holds(&mut e, "normal(series(elliptic_theta(3, 0, x)^4, x, 0, 12)) === sum(squares_r(4, j)*x^j, [j, 0, 12])");
    // a predicate that is false for a symbolic argument is not read as a condition
    same(&mut e, "head(divisor_sum(n, &(&1), is_prime))", "divisor_sum");
}

#[test]
fn equality_of_sums_of_logarithms() {
    let mut e = engine();
    for (src, want) in [
        ("log(16) == 4*log(2)", "true"),
        ("log(16) - log(8) == log(2)", "true"),
        ("log(27) - log(9) == log(3)", "true"),
        ("log(6) == log(2) + log(3)", "true"),
        ("log(2/3) + log(9/4) == log(3/2)", "true"),
        ("log(16) == 5*log(2)", "false"),
        ("log(6) + 1 == log(2) + log(3)", "false"),
        ("log(10) != log(2) + log(5)", "false"),
        // the same through the documented divisor sums: sum of Lambda(d) is log(n), sum of mu(n/d) log(d) is Lambda(n)
        ("log(16) == divisor_sum(16, &(mangoldt_lambda(&1)))", "true"),
        ("divisor_sum(27, &(moebius_mu(27/&1)*log(&1))) == mangoldt_lambda(27)", "true"),
    ] {
        same(&mut e, src, want);
    }
    // independent check: the products of prime powers agree
    holds(&mut e, "exp(log(16)) == exp(4*log(2)) and exp(log(2/3) + log(9/4)) == 3/2");
    // not of the form: a logarithm of an irrational number stays (no claim)
    same(&mut e, "head(log(1 + sqrt(2)) + log(sqrt(2) - 1) == 0)", "equal");
}

#[test]
fn images_of_principal_branches() {
    let mut e = engine();
    same(
        &mut e,
        "function_range(acos(z), z, y, complexes)",
        "re(y) == 0 and im_part(y) >= 0 or 0 < re(y) and re(y) < pi or re(y) == pi and im_part(y) <= 0",
    );
    same(
        &mut e,
        "function_range(atanh(z), z, y, complexes)",
        "re(y) < 0 and -pi/2 < im_part(y) and im_part(y) <= pi/2 or re(y) == 0 and -pi/2 < im_part(y) and im_part(y) < pi/2 or re(y) > 0 and -pi/2 <= im_part(y) and im_part(y) < pi/2",
    );
    // independent check: y is in the image exactly when f(g(y)) = y for the forward function g, on a grid of exact
    // points that includes the edges of the strips (g(y) exact, f at 30 digits)
    let src = r#"{
      let pairs = [[acos, cos], [asin, sin], [acosh, cosh], [asech, sech], [asinh, sinh], [acsch, csch], [atanh, tanh], [acoth, coth]]
      let vals = [-pi, -3, -pi/2, -1, -1/3, 0, 1/3, 1, pi/2, 3, pi]
      var bad = 0
      var checked = 0
      for p in pairs {
        let r = function_range(p[1](z), z, w, complexes)
        for a in vals { for b in vals {
          let y = a + im*b
          # the forward function at its own poles (sech(-i pi/2)) is the pole failure: those points are skipped, as
          # the large values beyond 10^10 are
          let gy = if is_failure(p[2](y)) then null else simplify(complex_expand(p[2](y)))
          if not (gy === null) and is_number(n(gy)) and abs(n(gy)) < 10^10 then {
            let inr = simplify(rewrite(r, w -> y))
            let dd = n(p[1](gy), 30) - n(y, 30)
            let d = abs(re(dd)) + abs(im_part(dd))
            if is_number(d) then {
              checked = checked + 1
              if not (inr === (d < 10^-20)) then bad = bad + 1
            }
          }
        } }
      }
      [bad, checked > 400]
    }"#;
    same(&mut e, src, "[0, true]");
}
