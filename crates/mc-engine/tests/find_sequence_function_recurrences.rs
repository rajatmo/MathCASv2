//! find_sequence_function beyond the fixed bases: pairs at equally spaced points, sequences with every second term
//! zero, and P-recursive sequences (a guessed recurrence, solved by r_solve_value or given as a difference_root).
//! Every answer is checked against every given term; inputs with no such form still fail, and each call ends within
//! a generous time bound.
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

/// `f = find_sequence_function(xs, n)` evaluates and gives every term of `xs` (and `extra` more terms of the
/// generating expression `gen` in k when it is given)
fn fits(e: &mut CoreEngine, xs: &str, more: Option<(&str, u32)>) {
    let v = ev(e, &format!("ff = find_sequence_function({xs}, n); ff"));
    assert!(!v.contains("failure") && !v.contains("find_sequence_function"), "{xs}: {v}");
    let src = format!("{{ let xs = {xs}; every(range(length(xs)), k => simplify(rewrite(ff, n -> k) - xs[k]) === 0) }}");
    assert_eq!(ev(e, &src), "true", "{xs}: {v}");
    if let Some((gen, m)) = more {
        let src = format!("every(range({m}), k => simplify(rewrite(ff, n -> k) - ({gen})) === 0)");
        assert_eq!(ev(e, &src), "true", "{xs} beyond the data: {v}");
    }
}

#[test]
fn pairs_at_equally_spaced_points() {
    let mut e = CoreEngine::new();
    timed("pairs", 120, || {
        assert_eq!(ev(&mut e, "find_sequence_function(table([2 k, 2^k], [k, 10]), n)"), "2^((1/2)*n)");
        let v = ev(&mut e, "find_sequence_function([[1, 1], [3, 9], [5, 25], [7, 49], [9, 81], [11, 121]], n)");
        assert_eq!(v, "n^2");
        // unequal spacing and too few pairs fail
        assert!(ev(&mut e, "find_sequence_function([[1, 1], [3, 9], [6, 25]], n)").starts_with("failure(no_sequence_function"));
        assert!(ev(&mut e, "find_sequence_function([[1, 1], [3, 9], [5, 25], [7, 49]], n)").starts_with("failure(no_sequence_function"));
    });
}

#[test]
fn every_second_term_zero() {
    let mut e = CoreEngine::new();
    timed("interleaved", 120, || {
        fits(&mut e, "[1, 0, 1, 0, 1/2, 0, 1/6, 0, 1/24, 0, 1/120, 0, 1/720, 0, 1/5040]", Some(("(1 - (-1)^k)/(2*gamma((k + 1)/2))", 30)));
        fits(&mut e, "[0, 1, 0, 2, 0, 4, 0, 8]", Some(("(1 + (-1)^k)/2*2^(k/2 - 1)", 30)));
    });
}

#[test]
fn p_recursive_sequences() {
    let mut e = CoreEngine::new();
    timed("recurrences", 300, || {
        // C-finite: 2^n - fibonacci(n + 2)
        fits(&mut e, "[0, 1, 3, 8, 19, 43, 94, 201, 423, 880]", Some(("2^k - fibonacci(k + 2)", 40)));
        fits(&mut e, "table(binomial(2 k, k)/(k + 1)^2, [k, 20])", Some(("binomial(2 k, k)/(k + 1)^2", 40)));
        fits(&mut e, "table(k*fibonacci(k), [k, 14])", Some(("k*fibonacci(k)", 40)));
        // an inhomogeneous order-2 recurrence with polynomial coefficients
        fits(&mut e, "[2, 3/2, 7/3, 13/4, 26/5, 49/6, 92/7, 169/8, 307/9, 551/10, 980/11, 1729/12, 3030/13, 5279/14, 9151/15, 15793/16, 27150/17, 46513/18]", None);
        // no closed form from r_solve_value: the difference_root of the recurrence, right at every term
        let v = ev(&mut e, "ff = find_sequence_function(table(factorial(k) + 1/k, [k, 14]), n); ff");
        assert!(v.starts_with("difference_root("), "{v}");
        assert_eq!(ev(&mut e, "every(range(20), k => rewrite(ff, n -> k) === factorial(k) + 1/k)"), "true");
    });
}

#[test]
fn sequences_without_a_form_still_fail() {
    let mut e = CoreEngine::new();
    timed("no form", 300, || {
        for xs in [
            "[3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5, 8, 9, 7, 9, 3, 2, 3, 8, 4, 6, 2, 6, 4, 3, 3, 8, 3, 2, 7, 9, 5]",
            "[3.0, 1, 4, 1, 5, 9, 2]",
            "[a, 1, 4, 1, 5, 9, 2]",
            "[1, 1, 2, 2, 3, 4, 5, 6, 7, 8, 11, 12]",
        ] {
            let v = ev(&mut e, &format!("find_sequence_function({xs}, n)"));
            assert!(v.starts_with("failure(no_sequence_function"), "{xs}: {v}");
        }
    });
}
