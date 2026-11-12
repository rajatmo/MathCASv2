//! Item N1: the `compile:` option, the compiled backend on every machine-real path (Newton, secant, Brent,
//! Broyden, BFGS, Brent's minimisation, the Newton polish), results identical to the interpreted backend, at 1 and
//! 4 threads, and all asked digits under `digits:`.
mod common;
use common::{correct_digits, significant, Sys};

/// Every machine-real path, as plain problems and as families solved many times with changing constants.
const CASES: &[&str] = &[
    "find_root(cos(x) == x, [x, 1])",
    "find_root(x^3 - 2*x - 5, [x, 2, 3])",
    "find_root(x^3 - 2*x - 5, [x, 2, 2, 3])",
    "find_root((x - 1)^2 - 0.0001, [x, 0.6, 0.5, 1.5])",
    "find_root([x^2 + y^2 - 4, x*y - 1], [[x, 2], [y, 0.5]])",
    "find_root([x^2 + y^2 - 4, x*y - 1], [[x, 2], [y, 0.5]], method: broyden)",
    "find_minimum((1 - x)^2 + 100*(y - x^2)^2, [[x, -1.2], [y, 1]])",
    "find_minimum(x^4 - 3*x^2 + x, [x, 1])",
    "find_minimum((x - 1)^2 + abs(y), [[x, 0], [y, 0.5]], method: bfgs)",
    "find_maximum(sin(x)*exp(-x/4), [x, 1])",
    "find_minimum(x^2 + y^2 + exp(x), [[x, 1], [y, 1]], method: newton)",
    "map(k => rewrite(x, find_root(x - 0.3*sin(x) == 0.07*k, [x, 1.0])), range(60))",
    "map(k => rewrite(x, find_root(x^3 + 0.5*k*x - 1.25, [x, 1.0, 2.0])), range(40))",
    "map(k => rewrite(x, find_root(exp(x) - 1.5 - 0.1*k, [x, 1.0, -2.0, 3.0])), range(40))",
    "map(k => find_minimum((x - 0.1*k)^2 + (y + 0.05*k)^4 + 0.01*k, [[x, 0], [y, 0]]), range(30))",
    "map(k => find_minimum(cosh(x - 0.2*k) + 0.5*x, [x, 0]), range(30))",
];

fn results(threads: usize, compile: &str) -> Vec<String> {
    let mut s = Sys::with_threads(threads, false);
    let opt = |c: &str| {
        if compile.is_empty() {
            return c.to_string();
        }
        // the option goes into every call of find_root, find_minimum and find_maximum
        let mut out = String::new();
        let mut rest = c;
        while let Some(i) = rest.find("find_") {
            out.push_str(&rest[..i]);
            rest = &rest[i..];
            // the call's closing parenthesis
            let mut depth = 0;
            let end = rest.char_indices().find_map(|(j, ch)| match ch {
                '(' => {
                    depth += 1;
                    None
                }
                ')' => {
                    depth -= 1;
                    (depth == 0).then_some(j)
                }
                _ => None,
            });
            let end = end.expect("balanced");
            out.push_str(&rest[..end]);
            out.push_str(&format!(", compile: {compile})"));
            rest = &rest[end + 1..];
        }
        out.push_str(rest);
        out
    };
    CASES.iter().map(|c| s.run(&opt(c))).collect()
}

#[test]
fn compiled_and_interpreted_results_are_identical() {
    let interpreted = results(1, "false");
    for r in &interpreted {
        assert!(!r.contains("failure"), "{r}");
    }
    assert_eq!(interpreted, results(1, "true"));
    assert_eq!(interpreted, results(1, "auto"));
    assert_eq!(interpreted, results(1, ""));
    assert_eq!(interpreted, results(4, "true"));
    assert_eq!(interpreted, results(4, ""));
}

#[test]
fn the_compile_option() {
    let mut s = Sys::new();
    for (src, want) in [
        ("find_root(x^2 - 2, [x, 1], compile: true)", "[x -> 1.4142135623730951]"),
        ("find_root(x^2 - 2, [x, 1], compile: false)", "[x -> 1.4142135623730951]"),
        ("find_root(x^2 - 2, [x, 1], compile: auto)", "[x -> 1.4142135623730951]"),
        ("find_root(x^2 - 2, [x, 1], compile: maybe)", "failure(argument, find_root, compile)"),
        ("find_root(x^2 - 2, [x, 1], compile: 1)", "failure(argument, find_root, compile)"),
        ("find_minimum(x^2 - 2*x, [x, 0], compile: true)", "[-1.0, [x -> 1.0]]"),
        ("find_maximum(x - x^2, [x, 0], compile: soon)", "failure(argument, find_maximum, compile)"),
        // a function defined by a rule, which the compiler does not inline: the interpreter, as always
        ("rule gq(x_ : Real) := x^2 - 2; abs(rewrite(x, find_root(gq(x), [x, 1], compile: true)) - 1.4142135623730951) < 1e-15", "true"),
        // complex starts run interpreted whatever the option says
        ("find_root(x^2 + 1, [x, 1 + im], compile: true) == find_root(x^2 + 1, [x, 1 + im])", "true"),
    ] {
        assert_eq!(s.run(src), want, "{src}");
    }
}

#[test]
fn all_asked_digits_print() {
    let mut s = Sys::new();
    // the root of x - (3/10) sin(x) = 2 (mpmath, 1100 digits; its first 1000 digits are in bench/cases/L/ref)
    let r = s.rules("find_root(x - 3/10*sin(x) == 2, [x, 2], digits: 1000, verify: true)");
    let d = significant(&r[0]);
    assert_eq!(d, 1000, "{}", &r[0][..40]);
    assert!(r[0].starts_with("2.2360314951724365012799635488841"), "{}", &r[0][..40]);
    for (src, want, d) in [
        ("find_root(x^2 - 2, [x, 1], digits: 25)", "1.41421356237309504880168872420969807856967187537694807317667973799", 25),
        ("find_root(x^2 - 3, [x, 1], digits: 41, verify: true)", "1.73205080756887729352744634150587236694280525381038062805580697945", 41),
        ("find_root(exp(x) == 10, [x, 2], digits: 33)", "2.30258509299404568401799145468436420760110148862877297603332790096757", 33),
        ("find_root(x*exp(x) == 2, [x, 1], digits: 57)", "0.852605502013725491346472414695317466898453300151403508772", 57),
    ] {
        let got = s.rules(src);
        // at most d digits, and d unless the value lies very near a rounding boundary
        let n = significant(&got[0]);
        assert!(n + 1 >= d && n <= d, "{src}: {n} digits: {}", got[0]);
        assert!(correct_digits(&got[0], want), "{src}: {}", got[0]);
    }
}
