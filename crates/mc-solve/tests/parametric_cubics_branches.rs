//! Cubics with a parameter (Cardano's formula where the sign of S is not known, the real solutions joined into
//! one case per root object) and the branches of `product_log` asked for by `max_roots`. The values are checked
//! against roots computed independently (companion-matrix iterations at 30 digits) and by substitution.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

fn has_all(out: &str, parts: &[&str]) {
    for p in parts {
        assert!(out.contains(p), "{p} missing in {out}");
    }
}

#[test]
fn cubic_with_a_parameter_and_no_linear_term_has_three_roots() {
    // y^3 + y^2 + x: S = -2 - 27 x + sqrt(108 x + 729 x^2) is negative for x > 0; the roots at x = 1 are
    // -1.46557123187676802665673122522 and 0.232785615938384013328365612610 -+ 0.792551992515447848325898300653 i
    let out = ev("n(map(s => y /. s, solve(y^3 + y^2 + x == 0, y)) /. x -> 1, 25)");
    has_all(&out, &["-1.4655712318767680266567", "0.2327856159383840133283", "0.7925519925154478483258"]);
    // and at x = -5 (S > 0) every value solves the equation
    let r = ev("max(map(s => (v => abs(v^3 + v^2 - 5))(n(y /. s /. x -> -5, 30)), solve(y^3 + y^2 + x == 0, y))) < 10^-20");
    assert_eq!(r, "true");
    assert_eq!(ev("length(solve(y^3 + a*y^2 + b == 0, y))"), "3");
}

#[test]
fn real_roots_of_a_parametric_cubic_are_one_case_per_root() {
    let out = ev("solve(y^3 - x*y == 2, y, reals)");
    assert_eq!(
        out,
        "[[y -> root(&(2 - &1^3 + &1*x), 1)], [y -> conditional_expression(root(&(2 - &1^3 + &1*x), 2), x > 3)], \
         [y -> conditional_expression(root(&(2 - &1^3 + &1*x), 3), x >= 3)]]"
    );
    // x = 1: one real root 1.52137970680456756960408083225; x = 3: -1 (double) and 2; x = 4: three real roots
    let one = ev("n(rewrite(solve(y^3 - x*y == 2, y, reals), x -> 1), 25)");
    assert!(one.starts_with("[[y -> 1.521379706804567569604080") && one.ends_with("[y -> undefined], [y -> undefined]]"), "{one}");
    assert_eq!(ev("rewrite(solve(y^3 - x*y == 2, y, reals), x -> 3)"), "[[y -> -1], [y -> undefined], [y -> 2]]");
    let four = ev("n(rewrite(solve(y^3 - x*y == 2, y, reals), x -> 4), 25)");
    has_all(&four, &["-1.675130870566646070889", "-0.5391888728108891165258", "2.214319743377535187415"]);
    // a cut point that is a root object: x = 3/2^(2/3) joins the cases of its two values
    let out = ev("solve(y^3 - x*y^2 + 1 == 0, y, reals)");
    assert_eq!(
        out,
        "[[y -> root(&(-1 - &1^3 + x*&1^2), 1)], [y -> conditional_expression(root(&(-1 - &1^3 + x*&1^2), 2), x >= root(&(-27 + 4*&1^3), 1))], \
         [y -> conditional_expression(root(&(-1 - &1^3 + x*&1^2), 3), x > root(&(-27 + 4*&1^3), 1))]]"
    );
}

#[test]
fn max_roots_lists_branches_of_product_log() {
    // x 2^(x^2) == 5: x = 5 exp(-W_j(50 log 2)/2) for every branch j; independently (Lambert W at 30 digits)
    // j = 0: 1.36758733728205014388515371839, j = -1, 1: -1.61763448670122082710397032438 +- 1.12917047493765745006 i
    let out = ev("n(solve_values(5 == x*2^(x^2), x, max_roots: 3), 30)");
    has_all(&out, &["1.367587337282050143885153718", "-1.617634486701220827103970324", "1.129170474937657450060076591"]);
    let res = ev("max(map(v => abs(n(v*2^(v^2) - 5, 30)), solve_values(5 == x*2^(x^2), x, max_roots: 3))) < 10^-25");
    assert_eq!(res, "true");
    assert_eq!(ev("length(solve(5 == x*2^(x^2), x, max_roots: 5))"), "5");
    // without max_roots the principal branch, over the reals the real one
    assert_eq!(ev("solve(5 == x*2^(x^2), x) === [[x -> (1/2)*2^(1/2)*(product_log(50*log(2))/log(2))^(1/2)]]"), "true");
    assert_eq!(ev("length(solve(5 == x*2^(x^2), x, reals, max_roots: 3))"), "1");
    assert_eq!(ev("solve(x*exp(x) == -1, x, max_roots: 3)"), "[[x -> product_log(-1)], [x -> product_log(-1, -1)], [x -> product_log(1, -1)]]");
}
