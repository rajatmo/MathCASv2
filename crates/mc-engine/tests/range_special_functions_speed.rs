//! Ranges of special functions whose derivatives have zeros that are slow to settle (the nome, whose derivative has
//! the factor `elliptic_k(x)^-2`; the Lambert function, with `1/(1 + W(x))`): the range the table records answers
//! first, quickly. Each answer is checked independently (values at points of the domain), and every call has a time
//! bound in a fresh engine (no answer remembered from an earlier call).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The answer of `src` in a fresh engine, within `secs` seconds.
fn fresh(src: &str, secs: u64) -> String {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    let r = ev(&mut e, src);
    assert!(t.elapsed() < Duration::from_secs(secs), "{src} took {:?}", t.elapsed());
    r
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

#[test]
fn range_of_the_nome() {
    assert_eq!(fresh("function_range(elliptic_nome_q(x), x, y)", 3), "-1 < y and y <= 1");
    // q(m) increases from -1 (m -> -inf) to 1 (m = 1): values inside, 1 taken at m = 1
    let mut e = CoreEngine::new();
    holds(&mut e, "elliptic_nome_q(1) == 1");
    holds(&mut e, "{ let v = n(elliptic_nome_q(-10^6), 30); -1 < v and v < -1/2 }");
    holds(&mut e, "{ let v = n(elliptic_nome_q(1/2), 30); 0 < v and v < 1 }");
}

#[test]
fn ranges_of_functions_with_slow_critical_points() {
    // W(x) increases from -1 at -1/e; K(m) increases from 0+ (m -> -inf) to inf (m -> 1); Ai' takes every value
    assert_eq!(fresh("function_range(product_log(x), x, y)", 3), "y >= -1");
    assert_eq!(fresh("function_range(elliptic_k(x), x, y)", 3), "y > 0");
    assert_eq!(fresh("function_range(airy_ai_prime(x), x, y)", 3), "true");
    assert_eq!(fresh("function_range(2*elliptic_nome_q(3*x + 1) - 5, x, y)", 3), "-7 < y and y <= -3");
    let mut e = CoreEngine::new();
    holds(&mut e, "product_log(-1/e) == -1");
    holds(&mut e, "{ let v = n(elliptic_k(-10^8), 30); 0 < v and v < 10^-2 }");
    holds(&mut e, "n(airy_ai_prime(-199/2), 30) > 3/2 and n(airy_ai_prime(-397/4), 30) < -3/2");
}

#[test]
fn a_union_keeps_the_form_of_the_critical_points() {
    // csch takes every value but 0: two rays, written as one inequation
    assert_eq!(fresh("function_range(csch(x), x, y)", 3), "y != 0");
    assert_eq!(fresh("function_range(2*csch(3*x + 1) - 5, x, y)", 3), "y != -5");
    let mut e = CoreEngine::new();
    holds(&mut e, "n(csch(1/100)) > 99 and n(csch(-1/100)) < -99 and abs(n(csch(50))) < 10^-20");
}
