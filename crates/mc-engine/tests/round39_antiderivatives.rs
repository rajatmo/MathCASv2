//! Round 39: the derivative functions integrate to their functions (Airy, Weierstrass, Mathieu,
//! elliptic_exp). Each antiderivative is checked by
//! differentiating it back.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn derivative_functions_integrate_back() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "integrate(elliptic_exp_prime(u, [a, b]), u)"), "elliptic_exp(u, [a, b])");
    // variations: linear arguments; the derivative of each antiderivative is the integrand
    for f in ["airy_ai_prime(2*x + 1)", "airy_bi_prime(x/3)", "weierstrass_p_prime(3*x, [g2, g3])", "mathieu_c_prime(a, q, 2*x)", "mathieu_s_prime(a, q, x - 1)"] {
        let r = ev(&mut e, &format!("F = integrate({f}, x); F"));
        assert!(!r.contains("integrate"), "{f}: {r}");
        assert_eq!(ev(&mut e, &format!("simplify(d(F, x) - {f})")), "0", "{f}: {r}");
    }
}
