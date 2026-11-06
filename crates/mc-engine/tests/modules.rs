//! Modules, imports, files and the prelude on the assembled engine (package F): module definitions and calls end
//! to end, reloading without duplicated clauses, nothing evaluated on a read error, scripts with failures, the
//! prelude (loads, native-free, every function works, no benchmark-case name) and the standard module `stats`;
//! results identical at 1 and 4 threads under `mc-core` stress collection.
use mc_api::{RefStore, Store, View};
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig, Event, InMemory, Origin, RunError};
use mc_load::stdlib;
use std::path::Path;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => {
            let mut s: String = v.output.iter().map(|l| format!("{l}\n")).collect();
            s.push_str(&v.printed);
            s
        }
        Err(err) => format!("error: {err}"),
    }
}

/// Files for the tests: a module file, a file importing it, and a script with failures.
fn files() -> InMemory {
    InMemory::new()
        .with(
            "geo.mc",
            "#! mathcas 2\n# 2-d vectors\nmodule geo {\n  pub fn plus(u, v) = [u[1] + v[1], u[2] + v[2]]\n  pub fn scale(k, v) = [k*v[1], k*v[2]]\n  pub fn len2(v) = sq(v[1]) + sq(v[2])\n  fn sq(x) = x^2\n  pub origin = [0, 0]\n}",
        )
        .with("app/shapes.mc", "module shapes {\n  import geo2\n  pub fn diag(n) = twice([n, n])\n}")
        .with("app/geo2.mc", "module geo2 { pub fn twice(v) = map(x => 2x, v) }")
        .with("script.mc", "print(\"start\")\n1/0\nprint(\"after\")\nfn f(0) = 1\nf(1)\n6*7")
}

fn engine_with_files(cfg: EngineConfig) -> CoreEngine {
    let mut e = CoreEngine::with_config(cfg);
    e.set_sources(Box::new(files()));
    e
}

/// A session using modules; each input with its expected result.
const SESSION: &[(&str, &str)] = &[
    ("module counter {\n  var n = 0\n  pub fn next() = { n = n + 1 }\n  pub fn peek() = n\n}", "null"),
    ("[counter.next(), counter.next(), counter.peek()]", "[1, 2, 2]"),
    ("import counter\nnext() + peek()", "6"),
    ("import geo\nplus([1, 2], scale(3, [1, 1]))", "[4, 5]"),
    ("len2([3, 4]) + geo.len2(origin)", "25"),
    ("geo.sq(2)", "error: syntax error at byte 0: `geo.sq` is private to module geo (mark it `pub` to export it)"),
    ("sq(5)", "sq(5)"),
    ("fn plus(a, b) = a - b", "error: syntax error at byte 3: cannot define `plus` here: the name is imported from module geo"),
    ("module bad { fn add(a, b) = 0 }", "error: syntax error at byte 16: `add` is a name of the core syntax and cannot be defined in a module"),
    ("module m2 { pub fn f(x) = g(x) + y\n fn g(x) = 10x }\nm2.f(1)", "10 + m2.y"),
    ("{ let f = 3; f + 1 }", "4"),
];

#[test]
fn module_definitions_and_calls_end_to_end() {
    let mut e = engine_with_files(EngineConfig::default());
    for (src, want) in SESSION {
        assert_eq!(eval(&mut e, src), *want, "{src}");
    }
    let names: Vec<String> = e.modules().map(|m| format!("{}: {:?}", m.name, m.public)).collect();
    assert_eq!(names, ["counter: {\"next\", \"peek\"}", "geo: {\"len2\", \"origin\", \"plus\", \"scale\"}", "m2: {\"f\"}"]);
}

#[test]
fn modules_are_found_next_to_the_importing_file() {
    let mut e = engine_with_files(EngineConfig::default());
    let origin = Origin::new("app/main.mc", Some("app".into()));
    let r = e.eval_in("import shapes\ndiag(2)", &origin, &StopFlag::new()).unwrap();
    assert_eq!(r.last.printed, "[4, 4]");
}

#[test]
fn reloading_a_module_file_replaces_its_definitions() {
    let fs = files().with("m.mc", "module M {\n  pub fn f(x: Int) = 1\n  pub fn g(x) = x\n}");
    let mut e = CoreEngine::new();
    e.set_sources(Box::new(fs.clone()));
    let stop = StopFlag::new();
    e.eval_file(Path::new("m.mc"), &stop).unwrap();
    assert_eq!(eval(&mut e, "[M.f(1), M.g(2)]"), "[1, 2]");
    assert_eq!(eval(&mut e, "M.f(\"a\")"), "failure(no_match, M.f, [\"a\"])");
    // loading the same text again does not duplicate clauses
    e.eval_file(Path::new("m.mc"), &stop).unwrap();
    assert_eq!(eval(&mut e, "M.f(\"a\")"), "failure(no_match, M.f, [\"a\"])");
    // a changed file: the old, more specific clause of f and the removed g are gone
    fs.insert("m.mc", "module M {\n  pub fn f(x) = 2\n}");
    e.eval_file(Path::new("m.mc"), &stop).unwrap();
    assert_eq!(eval(&mut e, "M.f(1)"), "2");
    assert!(eval(&mut e, "M.g(2)").contains("module M has no member `g`"));
    assert_eq!(eval(&mut e, "`M.g`(2)").split(':').next(), Some("error"));
}

#[test]
fn reloading_a_module_clears_the_rules_it_attached_to_other_symbols() {
    // the module's `area` has argument-head rules held by the session's `square` and `hexagon` (`@on`): those are
    // not members, so only the module's record of its attachments clears them on a reload (decision 9.6)
    let v1 = "module shapes {
  pub @on(square) rule area(square(a_)) := a^2
  pub fn area(s) = 0
}";
    let v2 = "module shapes {
  pub @on(hexagon) rule area(hexagon(r_)) := 3r^2
  pub fn area(s) = 0
}";
    let fs = files().with("shapes.mc", v1);
    let mut e = CoreEngine::new();
    e.set_sources(Box::new(fs.clone()));
    let stop = StopFlag::new();
    // session symbols with a definition (an attribute), so that the module's text names them
    eval(&mut e, "@listable sym square
@listable sym hexagon");
    let areas = "[shapes.area(square(3)), shapes.area(hexagon(2))]";
    e.eval_file(Path::new("shapes.mc"), &stop).unwrap();
    assert_eq!(eval(&mut e, areas), "[9, 0]");
    // the same text again
    e.eval_file(Path::new("shapes.mc"), &stop).unwrap();
    assert_eq!(eval(&mut e, areas), "[9, 0]");
    // a changed module: the rule held by `square` is gone, the new one held by `hexagon` works
    fs.insert("shapes.mc", v2);
    e.eval_file(Path::new("shapes.mc"), &stop).unwrap();
    assert_eq!(eval(&mut e, areas), "[0, 12]");
    let attached: Vec<_> = e.modules().flat_map(|m| m.attached.iter().cloned()).collect();
    assert_eq!(attached, [("hexagon".to_string(), "shapes.area".to_string())]);
    // a module defined at the prompt again is a reload too
    eval(&mut e, "module shapes {
  pub @on(square) rule area(square(a_)) := 4a
  pub fn area(s) = 0
}");
    assert_eq!(eval(&mut e, areas), "[12, 0]");
    eval(&mut e, "module shapes { pub fn area(s) = 0 }");
    assert_eq!(eval(&mut e, areas), "[0, 0]");
    assert_eq!(e.modules().flat_map(|m| m.attached.iter()).count(), 0);
}

#[test]
fn nothing_is_evaluated_on_a_read_error() {
    let mut e = engine_with_files(EngineConfig::default());
    assert!(eval(&mut e, "print(1)\nx = 5\nimport geo\ngeo.sq(1)").starts_with("error"));
    // neither the print, nor the definition, nor the import took effect
    assert_eq!(eval(&mut e, "x"), "x");
    assert_eq!(eval(&mut e, "plus(1, 2)"), "plus(1, 2)");
    assert_eq!(e.modules().count(), 0);
    let r = e.eval_in("1\nimport nowhere", &Origin::repl(), &StopFlag::new());
    let Err(RunError::Read(err)) = r else { panic!("{r:?}") };
    assert!(err.render().starts_with("<repl>:2:8: no module `nowhere`"), "{}", err.render());
}

#[test]
fn scripts_report_failures_in_order_and_go_on() {
    let mut e = engine_with_files(EngineConfig::default());
    let r = e.eval_file(Path::new("script.mc"), &StopFlag::new()).unwrap();
    assert_eq!(
        r.events,
        [
            Event::Output("start".into()),
            Event::Failure("failure(division_by_zero)".into()),
            Event::Output("after".into()),
            Event::Failure("failure(no_match, f, [1])".into()),
        ]
    );
    assert!(r.failed() && !r.last_is_null);
    assert_eq!(r.last.printed, "42");
    let r = e.eval_file(Path::new("missing.mc"), &StopFlag::new());
    assert!(matches!(r, Err(RunError::Read(_))));
}

/// The names the prelude defines.
fn prelude_names() -> Vec<String> {
    let store = RefStore::new();
    let mut loader = mc_load::Loader::new(Box::new(InMemory::new()));
    let read = loader.read(&store, stdlib::PRELUDE, &Origin::embedded("prelude"), &|_| false).unwrap();
    let mut names = vec![];
    for &t in &read.statements {
        // define_fn(f(params), ...) or define_fn(condition(f(params), c), ...)
        let View::App { args, .. } = store.view(t) else { panic!("not a definition") };
        let mut lhs = args[0];
        while let View::App { head, args } = store.view(lhs) {
            match store.view(head) {
                View::Sym(s) if store.sym_name(s) == "condition" => lhs = args[0],
                View::Sym(s) => {
                    names.push(store.sym_name(s));
                    break;
                }
                _ => panic!("unexpected definition"),
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

#[test]
fn the_prelude_is_native_free_and_avoids_benchmark_names() {
    let names = prelude_names();
    assert!((8..=20).contains(&names.len()), "{names:?}");
    let mut e = CoreEngine::new();
    for n in &names {
        assert!(!e.has_native(n), "the prelude defines rules on the native {n}");
    }
    // no name of a benchmark case (defined or used) is a prelude name
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/cases");
    let mut stack = vec![cases];
    let mut seen = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "mc") {
                seen += 1;
                let text = std::fs::read_to_string(&p).unwrap();
                for (id, _) in mc_syntax::identifiers(&text) {
                    assert!(!names.contains(&id), "{} uses the prelude name {id}", p.display());
                }
            }
        }
    }
    assert!(seen > 10);
}

/// Every prelude function, with its expected results.
const PRELUDE_CASES: &[(&str, &str)] = &[
    ("[is_even(4), is_even(7), is_odd(7), is_odd(-3)]", "[true, false, true, true]"),
    ("[is_even(x), is_even(\"a\")]", "[is_even(x), failure(no_match, is_even, [\"a\"])]"),
    ("[clamp(5, 0, 3), clamp(-1, 0, 3), clamp(2, 0, 3), clamp(1/2, 0, 1)]", "[3, 0, 2, 1/2]"),
    ("[log_base(2, 1024), log_base(10, 1000), log_base(2, 6), log_base(e, x)]", "[10, 3, log(6)/log(2), log(x)]"),
    ("count_if(range(10), is_even)", "5"),
    ("[every([2, 4], is_even), every([2, 3], is_even), every([], is_even)]", "[true, false, true]"),
    ("[some([1, 3], is_even), some([1, 4], is_even), some([], is_even)]", "[false, true, false]"),
    ("[member([a, b, c], b), member([a, b, c], d), member([], a)]", "[true, false, false]"),
    ("unique([3, 1, 3, 2, 1, x, x])", "[3, 1, 2, x]"),
    ("[zip([1, 2, 3], [a, b]), zip([], [1])]", "[[[1, a], [2, b]], []]"),
    ("flat_map(k => [k, k^2], [1, 2, 3])", "[1, 1, 2, 4, 3, 9]"),
    ("[accumulate([1, 2, 3, 4]), accumulate([])]", "[[1, 3, 6, 10], []]"),
    ("[dot([1, 2, 3], [4, 5, 6]), dot([a, b], [c, d])]", "[32, a*c + b*d]"),
    ("[norm([3, 4]), norm([1, 1])]", "[5, 2^(1/2)]"),
    ("fix_point(k => quotient(k, 2), 100)", "0"),
    ("say(\"a\", 1, [2, x], \" b\")", "a1[2, x] b\nnull"),
    ("say()", "\nnull"),
    ("fix_point(t => rewrite(t, [a -> b, b -> c]), g(a, b))", "g(c, c)"),
];

#[test]
fn every_prelude_function_works() {
    let mut e = CoreEngine::new();
    for (src, want) in PRELUDE_CASES {
        assert_eq!(eval(&mut e, src), *want, "{src}");
    }
    // without the prelude the names are free symbols
    let mut bare = CoreEngine::with_config(EngineConfig { prelude: false, ..EngineConfig::default() });
    assert_eq!(eval(&mut bare, "is_even(4)"), "is_even(4)");
}

#[test]
fn the_standard_module_stats() {
    let mut e = CoreEngine::new();
    let cases = [
        ("import stats\nmean([1, 2, 3, 4])", "5/2"),
        ("[variance([1, 2, 3, 4]), std_dev([2, 4, 4, 4, 5, 5, 7, 9])]", "[5/3, (4/7)*14^(1/2)]"),
        ("[median([3, 1, 2]), median([4, 1, 3, 2])]", "[2, 5/2]"),
        ("geometric_mean([2, 8])", "4"),
        ("stats.squared_deviations([1])", "error: syntax error at byte 0: `stats.squared_deviations` is private to module stats (mark it `pub` to export it)"),
    ];
    for (src, want) in cases {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
}

#[test]
fn results_are_identical_at_1_and_4_threads_under_stress() {
    let all: Vec<&str> = SESSION.iter().chain(PRELUDE_CASES).map(|c| c.0).collect();
    let run = |cfg: EngineConfig| -> Vec<String> {
        let mut e = engine_with_files(cfg);
        all.iter().map(|src| eval(&mut e, src)).collect()
    };
    let reference = run(EngineConfig { threads: 1, ..EngineConfig::default() });
    for threads in [1, 4] {
        let cfg = EngineConfig { threads, heap: mc_core::Config::stress(), ..EngineConfig::default() };
        assert_eq!(run(cfg), reference, "{threads} threads under stress");
    }
}
