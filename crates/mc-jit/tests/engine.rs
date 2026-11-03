//! The compiler through the engine: results identical to the interpreter (with the Cranelift back end and with the
//! LIR interpreter back end), declines, refusals with reasons, `@compiled`, `compiled_info`, invalidation, and fuel
//! and interrupts stopping compiled loops at 1 and 4 threads.
use mc_bench::{Engine, EngineError, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::{Duration, Instant};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn engine(threads: usize) -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() })
}

/// The value of `key -> v` in `compiled_info(f)`'s printed list.
fn info(e: &mut CoreEngine, f: &str, key: &str) -> String {
    let s = eval(e, &format!("compiled_info({f})"));
    let at = s.find(&format!("{key} -> ")).unwrap_or_else(|| panic!("{key} in {s}")) + key.len() + 4;
    let rest = &s[at..];
    let mut depth = 0;
    let end = rest
        .char_indices()
        .find(|&(_, c)| match c {
            '[' | '(' => {
                depth += 1;
                false
            }
            ']' | ')' if depth > 0 => {
                depth -= 1;
                false
            }
            ']' | ')' => true,
            ',' => depth == 0,
            _ => false,
        })
        .map_or(rest.len(), |(i, _)| i);
    rest[..end].to_string()
}

/// Functions of the subset, each with arguments that exercise its compiled path, its declines and the
/// interpreter's special values (NaN, infinities, signed zeros, the ends of `i64`).
const CASES: &[(&str, &str, &[&str])] = &[
    ("ints", "fn ints(a: Int, b: Int) = a*b + 3*a - b - -a", &["2, 3", "-5, 7", "4611686018427387904, 2", "9223372036854775807, 1", "-9223372036854775808, -1", "0, 0"]),
    ("divs", "fn divs(a: Int, b: Int) = a/b + 1", &["6, 3", "7, 2", "5, 0", "-9223372036854775808, -1", "-12, 4", "0, 5"]),
    ("mods", "fn mods(a: Int, b: Int) = mod(a, b) * 1000 + quotient(a, b)", &["7, 3", "-7, 3", "7, -3", "-7, -3", "5, 0", "-9223372036854775808, -1", "0, -4"]),
    ("pows", "fn pows(a: Int, n: Int) = a^n", &["2, 10", "2, 62", "2, 63", "3, -2", "-1, -3", "1, -5", "0, 0", "-2, 63", "-3, 39", "0, -1"]),
    ("lits", "fn lits(a: Int) = a^3 - a^-1 + 2^a", &["1", "-1", "2", "62", "0"]),
    ("rdiv", "fn rdiv(x: Real, y: Real) = x/y", &["6.0, 3.0", "1.0, 0.0", "0.0, 0.0", "-2.5, 7.0"]),
    ("sum3", "fn sum3(x: Real, y: Real, z: Real) = x + y + z", &["1e16, 3.0, -1e16", "3.0, 1e16, -1e16", "0.1, 0.2, 0.3", "-0.0, -0.0, -0.0", "1e308*10.0, -1e308*10.0, 1.0"]),
    ("mixed", "fn mixed(x: Real, n: Int) = x*n + n/3 + 1/7 + 2x - n", &["2.5, 3", "-0.0, 0", "1e300, 9007199254740993", "0.1, -7"]),
    ("prod", "fn prod(x: Real, y: Real) = 3*x*y + x*y*x - (1/3)*y", &["2.5, 3.5", "1e200, 1e200", "-0.0, 4.0"]),
    ("roots", "fn roots(x: Real) = sqrt(x) + x^(1/3) + x^2 - x^-1 + x^0.5", &["2.0", "4.0", "0.0", "-8.0", "1e308*10.0"]),
    ("elem", "fn elem(x: Real) = exp(x) + log(x) + sin(x)*cos(x) + atan(x) - tan(x) + sinh(x)/cosh(x) + tanh(x)", &["0.5", "2.0", "-1.0", "0.0", "1e308*10.0"]),
    ("arcs", "fn arcs(x: Real) = asin(x) + acos(x)", &["0.5", "-1.0", "1.5", "-1.5"]),
    ("logic", "fn logic(x: Real, y: Real) = if x < y and not(x == y) or y != y then max(x, y) else min(x, y) - abs(x)", &["1.0, 2.0", "2.0, 1.0", "-0.0, 0.0", "0.0*(1e308*10.0), 1.0", "1.0, 0.0*(1e308*10.0)"]),
    ("steps", "fn steps(n: Int) = { var s = 0; for i in range(n, 1, -2) { s = s + i*i }; s }", &["10", "1", "0", "-3", "1000"]),
    ("harm", "fn harm(n: Int) = { var s = 0.0; var i = 0; while i < n { s = s + 1.0/(i + 1); i = i + 1 }; s }", &["0", "1", "10", "1000"]),
    ("floors", "fn floors(x: Real) = floor(x) + floor(-x) + floor(x/2.0)", &["2.5", "-2.5", "3.0", "1e300", "0.0*(1e308*10.0)", "-0.0"]),
    ("nan", "fn nan(x: Real) = x == x", &["1.5", "0.0*(1e308*10.0)"]),
    ("ends", "fn ends(a: Int) = { var c = 0; for i in range(a, 9223372036854775807) { c = c + 1 }; c }", &["9223372036854775800", "9223372036854775807"]),
    ("mixcmp", "fn mixcmp(x: Real, n: Int) = x < n and n <= x + 1", &["2.5, 3", "3.0, 3", "1e300, 9007199254740993", "0.0*(1e308*10.0), 1"]),
    ("extr", "fn extr(n: Int) = abs(n) + max(n, 3, -n) - min(n, 0)", &["5", "-5", "0", "-9223372036854775808", "-9223372036854775807"]),
    ("blocks", "fn blocks(n: Int) = { let a = n + 1; let b = { let c = a * 2; c - n }; if b > 10 then b else -b }", &["1", "20", "-30"]),
    ("rpow", "fn rpow(x: Real, y: Real) = x^y", &["2.0, 0.5", "-8.0, 0.5", "-2.0, 3.0", "0.0, -1.0", "1e308*10.0, 0.0"]),
    ("ipow", "fn ipow(x: Real, n: Int) = x^n + 2^x", &["1.5, 3", "2.0, -2", "-2.0, 0", "0.0, -1"]),
    ("calls", "fn calls(n: Int, x: Real) = sq(n) + sq(mod(sq(n), 7)) + floor(half(x)) + (if pos(x) then 1 else 0)", &["3, 2.5", "-4, -1.0", "3037000500, 1.0"]),
    ("gen", "fn gen(n: Int) = { var a = 0; var b = 1; var k = 0; while k < n and a >= 0 { var t = a + b; a = b; b = t; k = k + 1 }; a }", &["10", "90", "92", "93", "200"]),
];

/// Engines with and without the compiler, given the same definitions.
fn engines(threads: usize) -> [CoreEngine; 3] {
    let mut es = [engine(threads), engine(threads), engine(threads)];
    for e in es.iter_mut() {
        // functions the cases call (inlined)
        assert_eq!(eval(e, "fn sq(n: Int) = n*n + 1\nfn half(x: Real) = x*0.5\nfn pos(x: Real) = x > 0.0"), "null");
    }
    for (name, def, _) in CASES {
        for e in es.iter_mut() {
            assert_eq!(eval(e, def), "null", "{def}");
        }
        assert_eq!(eval(&mut es[1], &format!("compile({name})")), *name);
        assert_eq!(eval(&mut es[2], &format!("compile({name}, back_end: \"interpreter\")")), *name);
    }
    es
}

#[test]
fn compiled_results_are_the_interpreters() {
    let mut es = engines(1);
    let mut compiled_runs = 0;
    for (name, _, args) in CASES {
        for a in args.iter() {
            let call = format!("{name}({a})");
            let want = eval(&mut es[0], &call);
            assert_eq!(eval(&mut es[1], &call), want, "cranelift: {call}");
            assert_eq!(eval(&mut es[2], &call), want, "interpreter back end: {call}");
        }
        for e in es[1..].iter_mut() {
            let runs: usize = info(e, name, "runs").parse().unwrap();
            assert!(runs > 0, "{name} never ran compiled");
            compiled_runs += runs;
        }
        assert_eq!(info(&mut es[1], name, "back_end"), "\"cranelift\"");
        assert_eq!(info(&mut es[2], name, "back_end"), "\"interpreter\"");
    }
    assert!(compiled_runs > 100);
}

#[test]
fn compiled_results_are_the_same_at_4_threads() {
    // the calls run inside `map`, whose elements are speculative tasks at 4 threads
    let mut seq = engines(1);
    let mut par = engines(4);
    for (name, _, args) in CASES {
        // every argument tuple, many times over, so speculative tasks run compiled code
        let reps = args.iter().map(|a| format!("{name}({a})")).collect::<Vec<_>>().join(", ");
        let src = format!("map(k => [{reps}], range(64))");
        let want = eval(&mut seq[0], &src);
        assert_eq!(eval(&mut par[1], &src), want, "{src}");
        assert_eq!(eval(&mut par[2], &src), want, "{src}");
        assert_eq!(eval(&mut seq[1], &src), want, "{src}");
    }
}

#[test]
fn r12c_finds_the_longest_collatz_chain() {
    let mut e = engine(1);
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../bench/cases/R/R12c.mc")).unwrap();
    let setup = src.split("#@ measure").next().unwrap();
    assert_eq!(eval(&mut e, setup), "null");
    assert_eq!(info(&mut e, "collatz_steps", "state"), "\"active\"");
    assert_eq!(info(&mut e, "longest", "inlined"), "[collatz_steps]");
    let t0 = Instant::now();
    assert_eq!(eval(&mut e, "longest(10^5)"), "350");
    let compiled = t0.elapsed();
    assert_eq!(eval(&mut e, "collatz_steps(27)"), "111");
    // the interpreter agrees (R12 itself, at a size it runs quickly)
    let mut i = engine(1);
    let r12 = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../bench/cases/R/R12.mc")).unwrap();
    eval(&mut i, r12.split("#@ measure").next().unwrap());
    assert_eq!(eval(&mut i, "range(3000) |> map(collatz_steps, _) |> max"), eval(&mut e, "longest(3000)"));
    assert!(compiled < Duration::from_secs(5), "{compiled:?}");
}

#[test]
fn the_decorator_compiles_at_definition() {
    let mut e = engine(1);
    assert_eq!(eval(&mut e, "@compiled fn tri(n: Int) = n*(n + 1)/2"), "null");
    assert_eq!(info(&mut e, "tri", "state"), "\"active\"");
    assert_eq!(eval(&mut e, "tri(100)"), "5050");
    // a function outside the subset is still defined; the definition's value says why it is not compiled
    assert_eq!(
        eval(&mut e, "@compiled fn untyped(n) = n + 1"),
        "failure(not_compilable, untyped, \"parameter `n` of `untyped` has no type (Int or Real)\")"
    );
    assert_eq!(eval(&mut e, "untyped(1)"), "2");
    assert_eq!(eval(&mut e, "compiled_info(untyped)"), "failure(not_compiled, untyped)");
}

#[test]
fn functions_outside_the_subset_are_refused_with_a_reason() {
    let mut e = engine(1);
    let cases = [
        ("fn a1(n: Int) = n + z", "the free name `z`"),
        ("fn a2(n: Int) = if n < 1 then 1 else n*a2(n - 1)", "the recursive call of `a2`"),
        ("fn a3(n: Int) = [n]", "the call of `list`: `list` has no definition"),
        ("fn a4(n: Int) = if n < 1 then 1 else 2.5", "`if` branches of types Int and Real"),
        ("fn a5(n: Int) = { var s = 0; s = 1.5; s }", "`s` holds a Int value and is assigned a Real value"),
        ("fn a6(n: Int) = n/2 + 1/3", "exact rational arithmetic (the result can be a rational)"),
        ("fn a7(x: Real) = max(0, x)", "`min` or `max` of an integer and a machine real (the result's type would depend on the values)"),
        ("fn a8(n: Int) = sin(n)", "`sin` of an exact or boolean value (the result stays exact)"),
        ("fn a9(x: Real) = mod(x, 2)", "`mod` of a machine real"),
        ("fn a10(s: String) = s", "parameter `s` of `a10` has a type other than Int or Real"),
        ("fn a11(n: Int) = if n > 0 then 1", "an `if` without `else` used as a value (it can be null)"),
    ];
    for (def, why) in cases {
        assert_eq!(eval(&mut e, def), "null", "{def}");
        let f = &def[3..def.find('(').unwrap()];
        let want = format!("failure(not_compilable, {f}, \"{why}\")");
        assert_eq!(eval(&mut e, &format!("compile({f})")), want);
    }
    // the interpreter's own definitions: a native, several clauses, a rule with a condition
    assert!(eval(&mut e, "compile(sin)").contains("has a native implementation"));
    eval(&mut e, "fn two(0) = 1\nfn two(n: Int) = 2");
    assert!(eval(&mut e, "compile(two)").contains("more than one clause"));
    eval(&mut e, "fn caller(n: Int) = two(n) + 1");
    assert!(eval(&mut e, "compile(caller)").contains("the call of `two`: `two` has more than one clause"));
    assert_eq!(eval(&mut e, "compile(1 + 1)"), "failure(not_compilable, 1 + 1, \"not a function name\")");
}

#[test]
fn a_changed_definition_drops_what_inlined_it() {
    let mut e = engine(1);
    eval(&mut e, "fn inner(n: Int) = n + 1\n@compiled fn outer(n: Int) = 2*inner(n)");
    assert_eq!(eval(&mut e, "outer(5)"), "12");
    assert_eq!(info(&mut e, "outer", "state"), "\"active\"");
    // redefining the inlined function: the specialisation declines and the interpreter uses the new definition
    eval(&mut e, "fn inner(n: Int) = n + 100");
    assert_eq!(eval(&mut e, "outer(5)"), "210");
    assert_eq!(info(&mut e, "outer", "state"), "\"stale\"");
    // compiling again picks the new definition up
    assert_eq!(eval(&mut e, "compile(outer)"), "outer");
    assert_eq!(eval(&mut e, "outer(5)"), "210");
    assert_eq!(info(&mut e, "outer", "state"), "\"active\"");
    // compiling an inlined function after its caller keeps the caller's specialisation valid
    assert_eq!(eval(&mut e, "compile(inner)"), "inner");
    assert_eq!(info(&mut e, "outer", "state"), "\"active\"");
    assert_eq!(eval(&mut e, "outer(1)"), "202");
    // redefining the compiled function itself
    eval(&mut e, "fn outer(n: Int) = 3*n");
    assert_eq!(eval(&mut e, "outer(5)"), "15");
    // a module reload clears the definitions: the old specialisation must not answer
    eval(&mut e, "module M { pub fn g(n: Int) = n - 1 }");
    eval(&mut e, "compile(M.g)");
    assert_eq!(eval(&mut e, "M.g(10)"), "9");
    eval(&mut e, "module M { pub fn g(n: Int) = n - 2 }");
    assert_eq!(eval(&mut e, "M.g(10)"), "8");
}

/// A loop that runs for about 2^62 iterations.
const SPIN: &str = "@compiled fn spin(n: Int) = { var i = n; while i >= 0 { i = i + 1 }; i }";

#[test]
fn fuel_stops_compiled_loops() {
    for threads in [1, 4] {
        let mut cfg = EngineConfig { threads, ..EngineConfig::default() };
        cfg.eval.fuel = 2_000_000;
        let mut e = CoreEngine::with_config(cfg);
        assert_eq!(eval(&mut e, SPIN), "null");
        for src in ["spin(0)", "map(spin, range(8))"] {
            match e.eval(src, &StopFlag::new()) {
                Err(EngineError::Limit(what)) => assert_eq!(what, "fuel", "{src}"),
                other => panic!("{src} at {threads} threads: {other:?}"),
            }
        }
        assert!(e.jit().polls() >= 1000);
        assert_eq!(eval(&mut e, "spin(-5)"), "-5");
    }
}

#[test]
fn interrupts_stop_compiled_loops() {
    for threads in [1, 4] {
        for src in ["spin(0)", "map(spin, range(8))"] {
            let mut e = engine(threads);
            assert_eq!(eval(&mut e, SPIN), "null");
            let stop = StopFlag::new();
            let raiser = {
                let (stop, jit) = (stop.clone(), e.jit().clone());
                std::thread::spawn(move || {
                    // wait until compiled code is polling, then interrupt it
                    let t0 = Instant::now();
                    while jit.polls() < 100 && t0.elapsed() < Duration::from_secs(20) {
                        std::thread::yield_now();
                    }
                    let latency = Instant::now();
                    stop.raise();
                    latency
                })
            };
            assert_eq!(e.eval(src, &stop), Err(EngineError::Stopped), "{src} at {threads} threads");
            let raised = raiser.join().unwrap();
            assert!(raised.elapsed() < Duration::from_secs(2), "latency {:?}", raised.elapsed());
            // the session goes on, and the compiled code still runs
            stop.clear();
            assert_eq!(eval(&mut e, "spin(-3)"), "-3");
        }
    }
}
