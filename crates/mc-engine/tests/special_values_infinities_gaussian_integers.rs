//! Values of the inverse circular and hyperbolic functions and of Bessel functions at the infinities, the
//! two-argument arctangent, the Jacobi symbol of an even modulus, Gaussian integers (coprimality, modular inverse),
//! partition numbers of large argument, `khinchin` inside an expression, `find_root` at a working precision with
//! options of its own, and the enclosures and first-order uncertainty of the Dirichlet L and prime zeta functions.
//! Each answer is checked against an independent computation (a finite argument, an identity, a direct sum or a
//! residual); each test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

#[test]
fn inverse_functions_at_the_infinities_follow_their_finite_values() {
    timed("infinities", 60, || {
        let mut e = CoreEngine::new();
        holds(&mut e, "[asin(inf), asin(-inf), acos(inf), acos(-inf)] == [-im*inf, im*inf, im*inf, -im*inf]");
        // far along the real axis the imaginary part has the sign and the growth of the limit (DLMF 4.23.34)
        holds(&mut e, "{ let z = asin(10.0^8); im_part(z) < -18 and abs(re(z) - pi/2) < 10^-12 }");
        holds(&mut e, "{ let z = acos(10.0^8); im_part(z) > 18 and abs(re(z)) < 10^-12 }");
        holds(&mut e, "{ let z = asin(-10.0^8); im_part(z) > 18 }");
        assert_eq!(
            ev(&mut e, "[asin(complex_infinity), acos(complex_infinity), asinh(complex_infinity)]"),
            "[complex_infinity, complex_infinity, complex_infinity]"
        );
        // |asin z| and |asinh z| grow like log|z| along every ray
        holds(&mut e, "every([10.0^8, -10.0^8, 10.0^8*im, 10.0^8*(1 + im)], z => abs(asin(z)) > 18 and abs(asinh(z)) > 18)");
        assert_eq!(ev(&mut e, "[acosh(complex_infinity), atanh(complex_infinity)]"), "[inf, indeterminate]");
        assert_eq!(ev(&mut e, "[bessel_j(0, inf), bessel_j(0, -inf), bessel_y(1/3, inf)]"), "[0, 0, 0]");
        // the decay behind the limit: |J_nu(x)| <= C/sqrt(x), seen at 10^6
        holds(&mut e, "abs(bessel_j(0, 10.0^6)) < 10^-3 and abs(bessel_y(1/3, 10.0^6)) < 10^-3");
    });
}

#[test]
fn two_argument_arctangent_beyond_real_machine_arguments() {
    timed("atan2", 30, || {
        let mut e = CoreEngine::new();
        assert_eq!(
            ev(&mut e, "[atan(inf, 1), atan(-inf, 1), atan(-inf, -1), atan(-1, inf), atan(-1, -inf), atan(inf, inf)]"),
            "[0, pi, -pi, (1/2)*pi, (-1/2)*pi, indeterminate]"
        );
        // a point far out along each axis has the limiting direction
        holds(&mut e, "abs(atan(10.0^9, 1)) < 10^-8 and abs(atan(-1, 10.0^9) - pi/2) < 10^-8 and abs(atan(-10.0^9, -1) + pi) < 10^-8");
        // complex coordinates: a = atan(x, y) has cos(a) = x/sqrt(x^2 + y^2) and sin(a) = y/sqrt(x^2 + y^2)
        holds(
            &mut e,
            "every([[2.5 + im, 2], [1, -0.5 + 2im], [0.3im, 1.1 - im]], p => { let a = atan(p[1], p[2]); let r = sqrt(p[1]^2 + p[2]^2); abs(cos(a) - p[1]/r) < 10^-13 and abs(sin(a) - p[2]/r) < 10^-13 })",
        );
    });
}

#[test]
fn imaginary_multiples_of_symbols_swap_circular_and_hyperbolic() {
    timed("imaginary arguments", 30, || {
        let mut e = CoreEngine::new();
        holds(&mut e, "[cosh(im*x), sech(im*x), sec(im*x), csch(im*x)] == [cos(x), sec(x), sech(x), -im*csc(x)]");
        // the identities at a complex point
        holds(&mut e, "{ let z = 0.7 - 0.3im; abs(n(rewrite(sech(im*x), x -> z)) - 1/cos(z)) < 10^-14 and abs(n(rewrite(csch(im*x), x -> z)) - 1/(im*sin(z))) < 10^-14 }");
    });
}

#[test]
fn jacobi_symbol_of_an_even_modulus_is_the_product_over_prime_factors() {
    timed("jacobi", 30, || {
        let mut e = CoreEngine::new();
        // (a/2) is 0 for even a and (-1)^((a^2 - 1)/8) for odd a; the odd part by the Jacobi symbol
        holds(
            &mut e,
            "every(range(-12, 12), a => every([2, 4, 6, 10, 12, 20, 28], m => { let k = integer_exponent(m, 2); let odd = quotient(m, 2^k); let two = if mod(a, 2) == 0 then 0 else (-1)^quotient(a^2 - 1, 8); jacobi_symbol(a, m) == two^k*jacobi_symbol(a, odd) }))",
        );
        assert_eq!(ev(&mut e, "jacobi_symbol(7, 6)"), "1");
    });
}

#[test]
fn gaussian_integers_coprime_and_modular_inverse() {
    timed("gaussian integers", 60, || {
        let mut e = CoreEngine::new();
        // coprime: the norm of the gcd is 1 (the evaluator's own gcd)
        holds(
            &mut e,
            "every(range(-4, 4), a => every(range(-4, 4), b => every(range(-4, 4), c => every(range(-4, 4), d => { let g = gcd(a + im*b, c + im*d); is_coprime(a + im*b, c + im*d) == (expand(g*conjugate(g)) == 1) }))))",
        );
        // the inverse: x s - 1 divisible by m in Z[i], and s a nearest representative (norm at most half the norm of m)
        holds(
            &mut e,
            "every(range(-5, 5), a => every(range(-5, 5), b => every(range(-5, 5), c => every(range(-5, 5), d => { let x = a + im*b; let m = c + im*d; if not is_coprime(x, m) or m == 0 or (b == 0 and d == 0) then true else { let s = modular_inverse(x, m); let q = expand((x*s - 1)/m); is_integer(re(q)) and is_integer(im_part(q)) and 2*(re(s)^2 + im_part(s)^2) <= re(m)^2 + im_part(m)^2 } }))))",
        );
        holds(&mut e, "modular_inverse(3 - im, 5 + 2*im) == -1 - im");
        // a larger table of the same predicate stays quick
        holds(&mut e, "n(mean(flatten(table(boole(is_coprime(a + im*b, c + im*d_u)), a, range(-6, 6), b, range(-6, 6), c, range(-6, 6), d_u, range(-6, 6))))) > 0.5");
    });
}

#[test]
fn partition_numbers_of_distinct_parts_at_large_argument() {
    timed("partitions_q", 90, || {
        let mut e = CoreEngine::new();
        // counting the partitions of n whose parts are all different, from the list of all partitions, at n = 5, 12 and 40
        holds(
            &mut e,
            "every([5, 12, 40], m => length(select(integer_partitions(m), p => length(p) == length(union(p)))) == partitions_q(m))",
        );
        // the Hardy-Ramanujan form exp(pi sqrt(n/3))/(4 3^(1/4) n^(3/4)) at n = 10^5, to a relative 10^-2
        holds(&mut e, "{ let v = partitions_q(100000); let a = n(exp(pi*sqrt(100000/3))/(4*3^(1/4)*100000^(3/4)), 30); abs(n(v, 30)/a - 1) < 1/100 }");
    });
}

#[test]
fn khinchin_inside_an_expression() {
    timed("khinchin", 30, || {
        let mut e = CoreEngine::new();
        // the constant by its first 60 digits, put to the 100th power with exact arithmetic
        holds(&mut e, "round(khinchin^100) == round((26854520010653064453097148354817956938203822939944629530511523/10^61)^100)");
        holds(&mut e, "abs(n(khinchin^2 + 1, 20) - ((26854520010653064453097148354817956938203822939944629530511523/10^61)^2 + 1)) < 10^-18");
    });
}

#[test]
fn find_root_at_a_working_precision_with_inexact_coefficients() {
    timed("find_root precision", 60, || {
        let mut e = CoreEngine::new();
        // an inexact coefficient is its exact binary value at a working precision: the root of x^2 - c is
        // sqrt(set_precision(c, inf)) to the digits asked
        holds(&mut e, "{ let c = 2.0000000000046; let r = rewrite(x, find_root(x^2 - c, [x, 1], working_precision: 100)); abs(r^2 - set_precision(c, inf)) < 10^-95 }");
        // options of the method and the compile policy
        holds(
            &mut e,
            "{ let r = find_root([x - 1, 10*(y - x^2)], [[x, -1], [y, 1]], method: [newton, \"StepControl\" -> \"TrustRegion\"]); abs(rewrite(x, r) - 1) < 10^-8 and abs(rewrite(y, r) - 1) < 10^-8 }",
        );
        holds(&mut e, "abs(rewrite(x, find_root(x^2 - 2, [x, 1], compiled: false)) - sqrt(2)) < 10^-12");
        holds(&mut e, "abs(rewrite(x, find_root(x^2 - 2, [x, 1], compiled: true)) - sqrt(2)) < 10^-12");
        // a function of a complex variable with integer parameters: a zero of dirichlet_l(8, 3, t) on the critical line
        holds(&mut e, "{ let t0 = rewrite(t, find_root(dirichlet_l(8, 3, t) == 0, [t, 19*im])); abs(n(dirichlet_l(8, 3, t0))) < 10^-9 }");
    });
}

#[test]
fn enclosures_and_uncertainty_of_dirichlet_l_and_prime_zeta_p() {
    timed("uncertain arguments", 60, || {
        let mut e = CoreEngine::new();
        // the principal character modulo 5: L(s) = (1 - 5^-s) zeta(s), at points of the interval
        holds(
            &mut e,
            "{ let v = dirichlet_l(5, 1, interval([1.23, 1.24])); let f = s => (1 - 5^(-s))*zeta(s); head(v) === interval and every(range(0, 10), k => { let s = 1.23 + k/1000; v[1][1] <= f(s) and f(s) <= v[1][2] }) and v[1][2] - v[1][1] < 0.2 }",
        );
        // the character modulo 3 of label 2: L(s) = 3^-s (zeta(s, 1/3) - zeta(s, 2/3))
        holds(
            &mut e,
            "{ let v = dirichlet_l(3, 2, centered_interval(3/2, 1/100)); let f = s => 3^(-s)*(zeta(s, 1/3) - zeta(s, 2/3)); head(v) === centered_interval and every([-1/100, -1/200, 0, 1/200, 1/100], d => abs(f(3/2 + d) - v[1]) <= v[2]) }",
        );
        // first-order propagation: |P'(2)| 0.01 with P'(2) = -(sum over primes of log(p)/p^2), the primes below 10^5 summed
        // directly (the tail beyond 10^5 is below 2/10^5 in the sum of log(p)/p^2, so 0.01 times that is below 2*10^-7)
        holds(
            &mut e,
            "{ let v = prime_zeta_p(around(2, 0.01)); let s = n(sum(log(prime(k))/prime(k)^2, [k, 1, 9592])); head(v) === around and abs(v[1] - prime_zeta_p(2.0)) < 10^-14 and abs(v[2] - s*0.01) < 3*10^-7 }",
        );
    });
}

#[test]
fn six_j_symbol_of_a_non_half_integer_norlund_lists_and_logarithms_to_a_base() {
    timed("combinatorial symbols", 30, || {
        let mut e = CoreEngine::new();
        assert_eq!(ev(&mut e, "six_j_symbol([1, 2, 3], [4, 2, pi])"), "0");
        holds(&mut e, "norlund_b([1, 2, 3], a) == [norlund_b(1, a), norlund_b(2, a), norlund_b(3, a)]");
        // norlund_b(1, a) = -a/2 from the generating function (t/(e^t - 1))^a = 1 - a t/2 + ...
        holds(&mut e, "norlund_b(1, a) == -a/2");
        assert_eq!(ev(&mut e, "[log_base(pi, sqrt(pi)), log_base(2, 2^(1/3))]"), "[1/2, 1/3]");
    });
}
