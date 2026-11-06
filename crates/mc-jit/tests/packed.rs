//! Package P through the engine: compiled functions with vector parameters (untyped parameters), run-time indices,
//! `length`, `sum`/`total`, loops over elements and `table` results give exactly the interpreter's values, with
//! Cranelift and with the LIR interpreter back end, for packed and short vectors of reals and integers, at 1 and 4
//! threads; arguments outside the subset (mixed or empty lists, matrices, numbers) decline.
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn engine(threads: usize) -> CoreEngine {
    let mut e = CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() });
    e.configure("memo", "off").expect("the memo setting");
    e
}

/// The value of `key -> v` in `compiled_info(f)`'s printed list (a number).
fn info_num(e: &mut CoreEngine, f: &str, key: &str) -> u64 {
    let s = eval(e, &format!("compiled_info({f})"));
    let at = s.find(&format!("{key} -> ")).unwrap_or_else(|| panic!("{key} in {s}")) + key.len() + 4;
    s[at..].chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or_else(|_| panic!("{s}"))
}

/// Vectors the calls use: packed reals and integers, short lists, awkward values, and arguments that decline.
const VECTORS: &str = "\
    r = range(1.0, 1000.0); \
    s = range(-3.5, 3.5, 0.0625); \
    z = 1.0*range(-50, 50)/7; \
    i = range(-100, 100); \
    big = range(9223372036854775000, 9223372036854775807); \
    short = [1.5, -0.0, 2.5e300, -7.25]; \
    shorti = [3, -4, 5]; \
    negz = [-0.0, -0.0]; \
    nan = [1.0, 0.0*(1e308*10.0), 2.0]; \
    mixed = [1, 2.5, 3]; \
    empty = []; \
    matrix = [[1.0, 2.0], [3.0, 4.0]]; \
    null";

const CASES: &[(&str, &str)] = &[
    ("vmean", "fn vmean(v) = sum(v) / length(v)"),
    (
        "stats",
        "fn stats(v) = { var s = 0.0; var q = 0.0; let n = length(v); for k in range(n) { let x = v[k]; s = s + x; q = q + x*x }; let m = s/n; [m, q/n - m^2] }",
    ),
    ("dotp", "fn dotp(u, w) = { var s = 0.0; for k in range(length(u)) { s = s + u[k]*w[k] }; s }"),
    ("scale", "fn scale(v, a: Real) = table(a*x + 1.0, x, v)"),
    ("idx", "fn idx(v, k: Int) = [v[k], v[-k]]"),
    ("loopsum", "fn loopsum(v) = { var t = 0; for x in v { t = t + x }; t }"),
    ("squares", "fn squares(n: Int) = table(k^2, k, n)"),
    ("ramp_up", "fn ramp_up(n: Int) = table(1.0*k/n, k, n)"),
    ("twice", "fn twice(n: Int, x: Real) = table(x, n)"),
    ("ident", "fn ident(v) = v"),
    ("tot", "fn tot(v) = total(v) + sum(v)"),
    ("vlen", "fn vlen(v) = length(v)"),
    ("uses", "fn uses(v) = vlen(v) + sum(table(x*x, x, v))"),
    ("pick", "fn pick(v, w, c: Int) = { let y = if c > 0 then v else w; [y[1], length(y)] }"),
    ("cums", "fn cums(v) = { var acc = 0.0; table({ acc = acc + x; acc }, x, v) }"),
];

const CALLS: &[&str] = &[
    "vmean(r)", "vmean(s)", "vmean(z)", "vmean(i)", "vmean(short)", "vmean(shorti)", "vmean(negz)", "vmean(nan)",
    "vmean(mixed)", "vmean(empty)", "vmean(matrix)", "vmean(3.0)", "vmean(big)",
    "stats(r)", "stats(s)", "stats(i)", "stats(short)", "stats(nan)", "stats(big)",
    "dotp(r, r)", "dotp(s, z)", "dotp(short, r)", "dotp(r, short)", "dotp(i, i)", "dotp(r, i)",
    "scale(r, 2.0)", "scale(short, -0.5)", "scale(i, 0.25)", "scale(empty, 1.0)", "length(scale(s, 3.0))",
    "idx(r, 1)", "idx(r, 999)", "idx(r, 1000)", "idx(r, 0)", "idx(i, -3)", "idx(short, 4)", "idx(shorti, 2)",
    "idx(r, -9223372036854775808)",
    "loopsum(i)", "loopsum(shorti)", "loopsum(big)", "loopsum(r)",
    "squares(100)", "squares(0)", "squares(-3)", "squares(5)", "squares(3037000500)",
    "ramp_up(200)", "ramp_up(1)", "ramp_up(0)", "twice(70, -0.0)", "twice(-1, 1.0)", "twice(0, 1.0)",
    "ident(r)", "ident(short)", "ident(i)", "ident(mixed)",
    "tot(r)", "tot(i)", "tot(negz)", "tot(nan)", "tot(big)",
    "vlen(r)", "vlen(shorti)", "uses(r)", "uses(i)", "uses(shorti)",
    "pick(r, s, 1)", "pick(r, s, 0)", "pick(r, i, 0)", "pick(short, r, -1)",
    "cums(s)", "cums(short)", "cums(i)",
];

/// An engine without the compiler, one with Cranelift, one with the LIR interpreter back end, given the vectors and
/// the definitions.
fn engines(threads: usize) -> [CoreEngine; 3] {
    let mut es = [engine(threads), engine(threads), engine(threads)];
    for (k, e) in es.iter_mut().enumerate() {
        assert_eq!(eval(e, VECTORS), "null");
        for (name, def) in CASES {
            assert_eq!(eval(e, def), "null", "{def}");
            match k {
                1 => assert_eq!(eval(e, &format!("compile({name})")), *name),
                2 => assert_eq!(eval(e, &format!("compile({name}, back_end: \"interpreter\")")), *name),
                _ => {}
            }
        }
    }
    es
}

#[test]
fn vector_functions_agree_with_the_interpreter() {
    let mut es = engines(1);
    for call in CALLS {
        let want = eval(&mut es[0], call);
        assert_eq!(eval(&mut es[1], call), want, "cranelift: {call}");
        assert_eq!(eval(&mut es[2], call), want, "interpreter back end: {call}");
    }
    for (name, _) in CASES {
        for e in es[1..].iter_mut() {
            assert!(info_num(e, name, "runs") > 0, "{name} never ran compiled");
        }
    }
    // the calls that must decline after the arguments matched did: a NaN sum, an integer overflow, an inexact
    // integer quotient, an index 0 or beyond the vector (mismatched arguments are not counted: they never enter)
    assert_eq!(info_num(&mut es[1], "vmean", "declines"), 3);
    assert_eq!(info_num(&mut es[1], "idx", "declines"), 2);
}

#[test]
fn identical_at_four_threads() {
    let mut one = engines(1);
    let mut four = engines(4);
    let batch = format!("[{}]", CALLS.iter().filter(|c| !c.contains("3037000500")).copied().collect::<Vec<_>>().join(", "));
    let want = eval(&mut one[0], &batch);
    for e in four.iter_mut() {
        assert_eq!(eval(e, &batch), want);
    }
    // inside `map`, where calls run on speculative tasks
    let m = "map(k => [vmean(r + k), stats(s*k)[2], idx(i, k), length(scale(z, 1.0*k))], range(-5, 40))";
    let want = eval(&mut one[0], m);
    for e in four.iter_mut().chain(one[1..].iter_mut()) {
        assert_eq!(eval(e, m), want);
    }
}

#[test]
fn variants_and_info() {
    let mut e = engine(1);
    assert_eq!(eval(&mut e, VECTORS), "null");
    // both leaf types compile; the info shows the vector parameter and the variants
    assert_eq!(eval(&mut e, "fn vm(v) = sum(v)"), "null");
    assert_eq!(eval(&mut e, "compile(vm)"), "vm");
    let s = eval(&mut e, "compiled_info(vm)");
    assert!(s.contains("params -> [\"List[Real]\"]") && s.contains("vector_leaves -> [Real, Int]"), "{s}");
    // a real-only body: integer vectors decline to the interpreter
    assert_eq!(eval(&mut e, "fn lg(v) = log(v[1])"), "null");
    assert_eq!(eval(&mut e, "compile(lg)"), "lg");
    assert!(eval(&mut e, "compiled_info(lg)").contains("vector_leaves -> [Real]"));
    assert_eq!(eval(&mut e, "[lg(r), lg(i)]"), eval(&mut engine(1), &format!("{VECTORS}; fn lg(v) = log(v[1]); [lg(r), lg(i)]")));
    // an integer-only body (a real accumulator would change type): the integer variant alone
    assert_eq!(eval(&mut e, "fn ls(v) = { var t = 0; for x in v { t = t + x }; t }"), "null");
    assert_eq!(eval(&mut e, "compile(ls)"), "ls");
    assert!(eval(&mut e, "compiled_info(ls)").contains("vector_leaves -> [Int]"));
    // a vector result
    assert_eq!(eval(&mut e, "fn tb(n: Int) = table(0.5*k, k, n)"), "null");
    assert_eq!(eval(&mut e, "compile(tb)"), "tb");
    assert!(eval(&mut e, "compiled_info(tb)").contains("result -> \"List[Real]\""));
}

#[test]
fn vectors_outside_the_subset_are_refused_with_a_reason() {
    let mut e = engine(1);
    let cases = [
        ("fn a1(v) = v + 1.0", "a vector where a number is expected"),
        ("fn a2(v) = v[1, 2]", "a vector indexed by other than one integer"),
        ("fn a3(v) = v[1.0]", "a vector indexed by other than an integer"),
        ("fn a4(v) = [v, v]", "a list of vectors"),
        ("fn a5(v) = { var w = v; w[1] }", "holds a vector and is declared with a type or as a `var`"),
        ("fn a6(n: Int) = table([k, k], k, n)", "a `table` whose values are not machine numbers"),
        ("fn a7(n: Int) = table(k > 2, k, n)", "a `table` of booleans"),
        ("fn a8(n: Int) = table(k*j, k, n, j, n)", "`table` with nested iterators"),
        ("fn a9(n: Int) = sum([n, n])", "`sum` of something other than a vector"),
        ("fn a10(x: String) = 1", "has a type other than Int or Real (an untyped parameter is a vector)"),
    ];
    for (def, reason) in cases {
        assert_eq!(eval(&mut e, def), "null", "{def}");
        let name = def[3..].split('(').next().unwrap();
        let out = eval(&mut e, &format!("compile({name})"));
        assert!(out.starts_with("failure(not_compilable") && out.contains(reason), "{def}: {out}");
    }
}

/// The exit's kernel: the mean and variance of `random_normal(0, 1, n)` computed compiled, against the interpreter.
const KERNEL: &str = "fn moments(v) = { let n = length(v); var s = 0.0; for k in range(n) { s = s + v[k] }; let m = s/n; \
     var q = 0.0; for k in range(n) { q = q + (v[k] - m)^2 }; [m, q/(n - 1)] }";

#[test]
fn a_statistics_kernel_over_draws() {
    let mut plain = engine(1);
    let mut comp = engine(1);
    for e in [&mut plain, &mut comp] {
        assert_eq!(eval(e, KERNEL), "null");
        assert_eq!(eval(e, "fn mc(n: Int) = { let v = table(random_normal(), n); let m = sum(v)/n; [m, sum(table((x - m)^2, x, v))/(n - 1)] }"), "null");
    }
    assert_eq!(eval(&mut comp, "compile(moments)"), "moments");
    assert_eq!(eval(&mut comp, "compile(mc)"), "mc");
    for prog in ["seed(7); moments(random_normal(0, 1, 10^4))", "seed(8); mc(5000)", "seed(9); [mc(3), mc(1), random_real()]"] {
        assert_eq!(eval(&mut comp, prog), eval(&mut plain, prog), "{prog}");
    }
    assert!(info_num(&mut comp, "moments", "runs") >= 1 && info_num(&mut comp, "mc", "runs") >= 2);
}

#[test]
fn fuel_stops_compiled_vector_loops() {
    for threads in [1, 4] {
        let mut cfg = EngineConfig { threads, ..EngineConfig::default() };
        cfg.eval.fuel = 1_000_000;
        let mut e = CoreEngine::with_config(cfg);
        assert_eq!(eval(&mut e, KERNEL), "null");
        assert_eq!(eval(&mut e, "compile(moments)"), "moments");
        assert_eq!(eval(&mut e, "fn ones(n: Int) = table(1.0, n)"), "null");
        assert_eq!(eval(&mut e, "compile(ones)"), "ones");
        assert_eq!(eval(&mut e, "fn total_of(v) = sum(v)"), "null");
        assert_eq!(eval(&mut e, "compile(total_of)"), "total_of");
        assert_eq!(eval(&mut e, "v = range(1.0, 1.0*10^6); w = range(1.0, 1.0*10^7); length(v)"), "1000000");
        for src in ["moments(v)", "length(ones(10^7))", "total_of(w)", "map(k => moments(v), range(4))"] {
            match e.eval(src, &StopFlag::new()) {
                Err(mc_bench::EngineError::Limit(what)) => assert_eq!(what, "fuel", "{src}"),
                other => panic!("{src} at {threads} threads: {other:?}"),
            }
        }
        // the session goes on
        assert_eq!(eval(&mut e, "length(ones(10))"), "10");
    }
}
