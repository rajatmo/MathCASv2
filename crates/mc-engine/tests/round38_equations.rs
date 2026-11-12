//! Round 38: integer solutions in a family with a parameter (Pell equations) as conditional values, and identities of
//! sequences given by recurrences whose bound variable is the sequence index. Each family is checked by substituting
//! parameter values into the equation; each identity also by a false variant.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn pell_families_over_the_integers() {
    // x^2 - 2 y^2 = 1, x, y > 0: (x + y sqrt(2)) = (3 + 2 sqrt(2))^c(1), c(1) >= 1
    let s = "sol = solve(x^2 - 2*y^2 == 1 and x > 0 and y > 0, [x, y], integers)";
    assert_eq!(ev(&format!("{s}; simplify(rewrite(rewrite([x, y], first(sol)), table([c(1) -> i], i, 5)))")), "[[3, 2], [17, 12], [99, 70], [577, 408], [3363, 2378]]");
    assert_eq!(ev(&format!("{s}; length(sol)")), "1");
    // x^2 - 3 y^2 = 1 with any signs: two families over all integers c(1); each member solves the equation
    let t = "sol = solve(x^2 - 3*y^2 == 1, [x, y], integers)";
    assert_eq!(ev(&format!("{t}; length(sol)")), "2");
    assert_eq!(
        ev(&format!("{t}; union(flatten(map(s => map(i => simplify(expand(rewrite(x^2 - 3*y^2, rewrite(s, c(1) -> i)))), range(-3, 3)), sol)))")),
        "[1]"
    );
    // x^2 - 5 y^2 = 1, x, y > 0: the fundamental solution (9, 4)
    let u = "sol = solve_values(x^2 - 5*y^2 == 1 and x > 0 and y > 0, [x, y], integers)";
    assert_eq!(ev(&format!("{u}; simplify(rewrite(first(sol), c(1) -> 1))")), "[9, 4]");
    assert_eq!(ev(&format!("{u}; simplify(rewrite(first(sol), c(1) -> 2))")), "[161, 72]");
}

#[test]
fn identities_of_recurrence_sequences() {
    let p = "pd = difference_root(((y, n) => [-y(n) - y(1 + n) + y(3 + n) == 0, y(0) == 1, y(1) == 1, y(2) == 1]))";
    assert_eq!(ev(&format!("{p}; difference_root_reduce(pd(n) == pd(n - 2) + pd(n - 3), n)")), "true");
    assert_eq!(ev(&format!("{p}; difference_root_reduce(pd(n) == pd(n - 4) + pd(n - 5) + pd(n - 6) + pd(n - 7) + pd(n - 8), n)")), "true");
    assert_eq!(ev(&format!("{p}; difference_root_reduce(sum(pd(3*k + 2), [k, 0, n]) == pd(3*n + 4) - 1, n)")), "true");
    // false variants
    assert_eq!(ev(&format!("{p}; difference_root_reduce(pd(n) == pd(n - 1) + pd(n - 3), n)")), "false");
    assert_eq!(ev(&format!("{p}; difference_root_reduce(sum(pd(3*k + 2), [k, 0, n]) == pd(3*n + 4), n)")), "false");
}
