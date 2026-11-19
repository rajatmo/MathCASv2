//! `undefined` (semantics 3.3): arithmetic and the numeric functions give `undefined` when an argument is
//! `undefined`; comparisons, logical connectives, lists and other containers, unknown functions and held
//! expressions keep it as it is.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate in one fresh session (one worker thread); `TIMEOUT` after 120 s.
fn ev_all(srcs: &[&str]) -> Vec<String> {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<String>>();
    let s: Vec<String> = srcs.iter().map(|s| s.to_string()).collect();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = s.iter().map(|src| e.eval(src, &stop).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"))).collect();
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| vec!["TIMEOUT".to_string(); srcs.len()])
}

fn table(cases: &[(&str, &str)]) {
    let srcs: Vec<&str> = cases.iter().map(|c| c.0).collect();
    let got = ev_all(&srcs);
    let bad: Vec<String> = cases
        .iter()
        .zip(&got)
        .filter(|((_, want), got)| got.as_str() != *want)
        .map(|((src, want), got)| format!("{src}: got {got}, want {want}"))
        .collect();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn arithmetic_gives_undefined() {
    table(&[
        ("1 + undefined", "undefined"),
        ("undefined + x", "undefined"),
        ("x - undefined", "undefined"),
        ("undefined - undefined", "undefined"),
        ("2*undefined", "undefined"),
        ("x*undefined", "undefined"),
        ("0*undefined", "undefined"),
        ("0.0*undefined", "undefined"),
        ("undefined/x", "undefined"),
        ("1/undefined", "undefined"),
        ("undefined^2", "undefined"),
        ("2^undefined", "undefined"),
        ("undefined^0", "undefined"),
        ("1^undefined", "undefined"),
        ("1.5 + undefined", "undefined"),
        ("pi*undefined + 3", "undefined"),
        ("inf + undefined", "undefined"),
    ]);
}

#[test]
fn numeric_functions_give_undefined() {
    table(&[
        ("sin(undefined)", "undefined"),
        ("exp(undefined)", "undefined"),
        ("log(undefined)", "undefined"),
        ("sqrt(undefined)", "undefined"),
        ("atan(undefined)", "undefined"),
        ("gamma(undefined)", "undefined"),
        ("zeta(undefined)", "undefined"),
        ("erf(undefined)", "undefined"),
        ("bessel_j(0, undefined)", "undefined"),
        ("bessel_j(undefined, 1)", "undefined"),
        ("bessel_k(undefined, x)", "undefined"),
        ("hypergeometric_0f1(undefined, 2)", "undefined"),
        ("n(undefined)", "undefined"),
        ("n(sin(undefined) + 1, 30)", "undefined"),
    ]);
}

#[test]
fn nested_cases() {
    table(&[
        ("sin(1 + cos(undefined))", "undefined"),
        ("gamma(2*undefined + x)^2 - 1", "undefined"),
        ("exp(x)*log(1 + undefined)", "undefined"),
        ("sin([1, undefined])", "[sin(1), undefined]"),
        ("[1, 2] + undefined", "[undefined, undefined]"),
        ("bessel_j(undefined, [1, 2])", "[undefined, undefined]"),
        ("gamma([undefined, 2])", "[undefined, 1]"),
        ("f(1 + undefined)", "f(undefined)"),
    ]);
}

#[test]
fn comparisons_connectives_and_containers_keep_it() {
    table(&[
        ("a == 1 + sin(undefined)", "a == undefined"),
        ("undefined < 1", "undefined < 1"),
        ("x != undefined", "x != undefined"),
        ("undefined or p", "undefined or p"),
        ("p and undefined", "p and undefined"),
        ("not undefined", "not undefined"),
        ("[1, undefined]", "[1, undefined]"),
        ("assoc(\"a\" -> undefined)", "assoc(\"a\" -> undefined)"),
        ("x -> undefined", "x -> undefined"),
        ("if undefined then a else b", "if undefined then a else b"),
        ("f(undefined)", "f(undefined)"),
        ("g(undefined, 2)", "g(undefined, 2)"),
        ("set_attributes(keep, hold_all); keep(sin(undefined) + 1)", "keep(sin(undefined) + 1)"),
        ("quote(sin(undefined))", "quote(sin(undefined))"),
        ("quote(1 + undefined)", "quote(1 + undefined)"),
        ("length([undefined, undefined])", "2"),
        ("undefined", "undefined"),
    ]);
}

#[test]
fn producers_of_undefined_still_work() {
    table(&[
        ("conditional_expression(a, false) + 1", "undefined"),
        ("conditional_expression(x^2, x > 0) /. x -> -2", "undefined"),
        ("conditional_expression(x, x > 0) + conditional_expression(x, x < 0)", "undefined"),
        ("select([1, undefined, 3], v => not (v === undefined))", "[1, 3]"),
    ]);
}
