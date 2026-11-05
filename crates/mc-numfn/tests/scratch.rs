//! The interpreted backends in scratch scopes (decision G.5): the solvers built on them (`n_integrate`, `n_dsolve`,
//! `find_root`, `find_minimum`) give the same printed results with scopes on and off, at one and four threads, and
//! under the stress collector; the heap does not keep the evaluations' terms.
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Every machine-real function of this test binary runs interpreted (the policy is read once per process).
fn interpreted() {
    std::env::set_var("MC_NUMFN", "interpreted");
}

const CASES: &[&str] = &[
    "n_integrate(exp(-x)*cos(30*x), [x, 0, pi], compile: false)",
    "sum(map(k => n_integrate(exp(-x)*cos(k*x), [x, 0, pi], compile: false), range(12)))",
    "n_integrate(x*y, [x, 0, 1], [y, 0, x], compile: false)",
    "n_integrate(sqrt(x - 0.5), [x, 0, 1], compile: false)",
    "n_integrate(log(1 + x)/(1 + x^2), [x, 0, 1], digits: 30)",
    "find_root(x - 0.3*sin(x) - 0.006, [x, 0.5], compile: false)",
    "find_root([x^2 + y^2 - 4, x*y - 1], [[x, 1.8], [y, 0.5]], compile: false)",
    "find_minimum((x - 1)^2 + (y + 2)^4 + x*y, [[x, 0.0], [y, 0.0]], compile: false)",
    "{ let s = n_dsolve([x'(t) == 10*(y(t) - x(t)), y'(t) == x(t)*(28 - z(t)) - y(t), z'(t) == x(t)*y(t) - 8/3*z(t), x(0) == 1, y(0) == 1, z(0) == 1], [x, y, z], [t, 0, 2]); rewrite([x(2), y(2), z(2)], s) }",
    "{ let s = n_dsolve([y'(t) == -50*(y(t) - cos(t)), y(0) == 0], y, [t, 0, 3]); rewrite(y(3), s) }",
];

fn run(threads: usize, heap: mc_core::Config) -> Vec<String> {
    let mut e = CoreEngine::with_config(EngineConfig { threads, heap, ..EngineConfig::default() });
    let stop = StopFlag::new();
    CASES
        .iter()
        .map(|src| match e.eval(src, &stop) {
            Ok(r) => r.printed,
            Err(err) => panic!("{src}: {err}"),
        })
        .collect()
}

#[test]
fn interpreted_solvers_agree_with_and_without_scopes() {
    interpreted();
    let reference = run(1, mc_core::Config { scratch: false, ..mc_core::Config::default() });
    for r in &reference {
        assert!(!r.contains("failure"), "{reference:?}");
    }
    assert_eq!(run(1, mc_core::Config::default()), reference);
    assert_eq!(run(4, mc_core::Config::default()), reference);
    assert_eq!(run(4, mc_core::Config::stress()), reference);
    assert_eq!(run(1, mc_core::Config::stress()), reference);
}

/// Repeated interpreted integrals in one statement: every evaluation runs in a scope, and the terms made there
/// (dropped with their scopes) far outnumber what one scope holds.
#[test]
fn interpreted_evaluations_run_in_scopes() {
    interpreted();
    let src = "sum(map(k => n_integrate(exp(-x)*cos(k*x), [x, 0, pi], compile: false), range(60)))";
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    let stats = || {
        let st = std::sync::Arc::new(std::sync::Mutex::new(mc_core::ScratchStats::default()));
        (st.clone(), move |_: &mut mc_eval::Kernel, h: &mc_core::Heap| *st.lock().unwrap() = h.scratch_stats())
    };
    let (before, f) = stats();
    e.install(f);
    let v = e.eval(src, &StopFlag::new()).expect("evaluates").printed;
    assert!(v.starts_with("1.0"), "{v}");
    let (after, f) = stats();
    e.install(f);
    let (b, a) = (*before.lock().unwrap(), *after.lock().unwrap());
    let scopes = a.scopes - b.scopes;
    assert!(scopes > 10_000, "{scopes} scopes");
    assert!(a.terms - b.terms > 20 * scopes / 2, "{} terms in {scopes} scopes", a.terms - b.terms);
    assert!(a.largest < 10_000, "largest scope {}", a.largest);
}