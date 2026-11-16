//! `difference_root_reduce`: a sequence given by an expression becomes one `difference_root` (periodic sequences,
//! hypergeometric terms, C-finite sums and products), and an identity of sequences is `true` only with a proof (the
//! C-finite order bound) and `false` at a counterexample. Every new answer is checked independently: the values of the
//! returned `difference_root` (run by the recurrence) against the expression evaluated directly at n = 0, ..., 30.
//! Each test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

/// `difference_root_reduce(t, n)` prints as `want` and its values at n = 0..30 equal those of `t`.
fn reduces(e: &mut CoreEngine, t: &str, opts: &str, want: &str) {
    let src = format!("dd47 = difference_root_reduce({t}, n{opts}); dd47");
    assert_eq!(ev(e, &src), want, "{t}");
    let check = format!(
        "every(range(0, 30), j => simplify(rewrite(dd47, n -> j) - rewrite({t}, n -> j)) === 0)"
    );
    assert_eq!(ev(e, &check), "true", "values of {t}");
}

#[test]
fn periodic_and_quotient_sequences() {
    timed("periodic sequences", 120, || {
        let mut e = CoreEngine::new();
        reduces(&mut e, "mod(n, 3)", "",
            "difference_root((FormalY, FormalN) => [-FormalY(FormalN) + FormalY(3 + FormalN) == 0, FormalY(0) == 0, FormalY(1) == 1, FormalY(2) == 2])(n)");
        reduces(&mut e, "boole(mod(n, 5) == 1)", "",
            "difference_root((FormalY, FormalN) => [-FormalY(FormalN) + FormalY(5 + FormalN) == 0, FormalY(0) == 0, FormalY(1) == 1, FormalY(2) == 0, FormalY(3) == 0, FormalY(4) == 0])(n)");
        reduces(&mut e, "kronecker_symbol(n, 7)", "",
            "difference_root((FormalY, FormalN) => [-FormalY(FormalN) + FormalY(7 + FormalN) == 0, FormalY(0) == 0, FormalY(1) == 1, FormalY(2) == 1, FormalY(3) == -1, FormalY(4) == 1, FormalY(5) == -1, FormalY(6) == -1])(n)");
        // variations: a combination of two periods (lcm 6), and the quotient
        let out = ev(&mut e, "difference_root_reduce(mod(n, 2) + 3*mod(2*n + 1, 3), n)");
        assert!(out.contains("FormalY(6 + FormalN)"), "{out}");
        reduces(&mut e, "mod(n, 2) + 3*mod(2*n + 1, 3)", "", &out);
        reduces(&mut e, "quotient(n, 4)", "",
            "difference_root((FormalY, FormalN) => [-1 - FormalY(FormalN) + FormalY(4 + FormalN) == 0, FormalY(0) == 0, FormalY(1) == 0, FormalY(2) == 0, FormalY(3) == 0])(n)");
        // a Kronecker symbol of even modulus is not taken as periodic with that period
        assert!(ev(&mut e, "difference_root_reduce(kronecker_symbol(n, 6), n)").starts_with("difference_root_reduce("));
    });
}

#[test]
fn hypergeometric_terms() {
    timed("hypergeometric terms", 120, || {
        let mut e = CoreEngine::new();
        reduces(&mut e, "n^2 + 1", "",
            "difference_root((FormalY, FormalN) => [(-2 - 2*FormalN - FormalN^2)*FormalY(FormalN) + (1 + FormalN^2)*FormalY(1 + FormalN) == 0, FormalY(0) == 1])(n)");
        reduces(&mut e, "catalan_number(n)", "",
            "difference_root((FormalY, FormalN) => [(-2 - 4*FormalN)*FormalY(FormalN) + (2 + FormalN)*FormalY(1 + FormalN) == 0, FormalY(0) == 1])(n)");
        reduces(&mut e, "bernoulli_b(2, n)", "",
            "difference_root((FormalY, FormalN) => [(-1 - 6*FormalN - 6*FormalN^2)*FormalY(FormalN) + (1 - 6*FormalN + 6*FormalN^2)*FormalY(1 + FormalN) == 0, FormalY(0) == 1/6])(n)");
        // variations of our own: factorials and powers, a binomial with exp
        for t in ["factorial(2*n)/(3^n*factorial(n))", "binomial(3*n, n)*exp(n)", "(2*n + 1)/(n + 3)"] {
            let out = ev(&mut e, &format!("difference_root_reduce({t}, n)"));
            assert!(out.starts_with("difference_root((FormalY, FormalN)"), "{t}: {out}");
            reduces(&mut e, t, "", &out);
        }
        // a pole or a zero of the leading coefficient at n >= 0: declined
        assert!(ev(&mut e, "difference_root_reduce((n + 1)/(n - 1), n)").starts_with("difference_root_reduce("));
        assert!(ev(&mut e, "difference_root_reduce(binomial(n, 3), n)").starts_with("difference_root_reduce("));
    });
}

#[test]
fn c_finite_sequences() {
    timed("C-finite sequences", 120, || {
        let mut e = CoreEngine::new();
        reduces(&mut e, "fibonacci(n)", "",
            "difference_root((FormalY, FormalN) => [-FormalY(FormalN) - FormalY(1 + FormalN) + FormalY(2 + FormalN) == 0, FormalY(0) == 0, FormalY(1) == 1])(n)");
        reduces(&mut e, "fibonacci(n) - (-1)^n*lucas_l(2*n + 1)", "",
            "difference_root((FormalY, FormalN) => [-FormalY(FormalN) - 4*FormalY(1 + FormalN) - 3*FormalY(2 + FormalN) + 2*FormalY(3 + FormalN) + FormalY(4 + FormalN) == 0, FormalY(0) == -1, FormalY(1) == 5, FormalY(2) == -10, FormalY(3) == 31])(n)");
        reduces(&mut e, "fibonacci(n) + 1", "",
            "difference_root((FormalY, FormalN) => [1 - FormalY(FormalN) - FormalY(1 + FormalN) + FormalY(2 + FormalN) == 0, FormalY(0) == 1, FormalY(1) == 2])(n)");
        reduces(&mut e, "fibonacci(n) + 1", ", method: homogeneous",
            "difference_root((FormalY, FormalN) => [FormalY(FormalN) - 2*FormalY(2 + FormalN) + FormalY(3 + FormalN) == 0, FormalY(0) == 1, FormalY(1) == 2, FormalY(2) == 2])(n)");
        // variations: a product of two C-finite sequences, a polynomial times a power, a partial sum
        for t in ["fibonacci(n)*lucas_l(3*n)", "n*2^n + fibonacci(2*n)", "sum(fibonacci(2*k), [k, 0, n])"] {
            let out = ev(&mut e, &format!("difference_root_reduce({t}, n)"));
            assert!(out.starts_with("difference_root((FormalY, FormalN)"), "{t}: {out}");
            reduces(&mut e, t, "", &out);
        }
        assert_eq!(
            ev(&mut e, "difference_root_reduce(&(2^&1))"),
            "difference_root((FormalY, FormalN) => [-2*FormalY(FormalN) + FormalY(1 + FormalN) == 0, FormalY(0) == 1])"
        );
    });
}

#[test]
fn identities_need_a_proof() {
    timed("identities", 120, || {
        let mut e = CoreEngine::new();
        // a parameter in the index: proved through Binet's form
        let id = "fibonacci(m + n) == (1/2)*(fibonacci(m)*lucas_l(n) + lucas_l(m)*fibonacci(n))";
        assert_eq!(ev(&mut e, &format!("difference_root_reduce({id}, n)")), "true");
        // the same at sample integers m, checked directly over n = 0..40
        for m in [3, 7, -4] {
            let src = format!("every(range(0, 40), j => rewrite(({id}), [m -> {m}, n -> j]) === true)");
            assert_eq!(ev(&mut e, &src), "true", "m = {m}");
        }
        assert_eq!(ev(&mut e, "difference_root_reduce(fibonacci(2*n) == fibonacci(n)*lucas_l(n), n)"), "true");
        assert_eq!(ev(&mut e, "difference_root_reduce(lucas_l(n)^2 - 5*fibonacci(n)^2 == 4*(-1)^n, n)"), "true");
        assert_eq!(ev(&mut e, "difference_root_reduce(fibonacci(n) == n, n)"), "false");
        // a negative slope in the index: the reversed characteristic polynomial
        assert_eq!(ev(&mut e, "difference_root_reduce(fibonacci(-n) == fibonacci(n)*(-1)^(1 + n), n)"), "true");
        assert_eq!(ev(&mut e, "difference_root_reduce(lucas_l(-n) == lucas_l(n)*(-1)^n, n)"), "true");
        assert_eq!(ev(&mut e, "difference_root_reduce(lucas_l(-n) == lucas_l(n), n)"), "false");
        // agreement on n = 0..40 is not a proof: no answer
        assert!(ev(&mut e, "difference_root_reduce(mod(n, 41) == n, n)").starts_with("difference_root_reduce("));
    });
}
