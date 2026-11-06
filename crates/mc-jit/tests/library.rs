//! Library calls in compiled code (round 13): special functions, the step functions and `piecewise`,
//! interpolations of the local scheme and derivatives of interpolations, and solutions of `n_dsolve`. Every
//! compiled result is the interpreter's, printed alike, on both back ends; the declines (outside a function's real
//! domain, at a pole, outside a table) give the interpreter's own answers.
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn engine() -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() })
}

/// The value of `key -> v` in `compiled_info(f)`'s printed list (a number or a short string).
fn info(e: &mut CoreEngine, f: &str, key: &str) -> String {
    let s = eval(e, &format!("compiled_info({f})"));
    let at = s.find(&format!("{key} -> ")).unwrap_or_else(|| panic!("{key} in {s}")) + key.len() + 4;
    s[at..].split([',', ']']).next().unwrap_or_default().trim().to_string()
}

/// Three engines with the same definitions: interpreted, compiled by Cranelift, compiled to the LIR interpreter.
struct Three {
    es: [CoreEngine; 3],
}

impl Three {
    fn new(setup: &str) -> Three {
        let mut es = [engine(), engine(), engine()];
        for e in es.iter_mut() {
            eval(e, setup);
        }
        Three { es }
    }

    /// Defines `def` (the function `name`) everywhere and compiles it in the second and third engine.
    fn define(&mut self, name: &str, def: &str) {
        for e in self.es.iter_mut() {
            assert_eq!(eval(e, def), "null", "{def}");
        }
        assert_eq!(eval(&mut self.es[1], &format!("compile({name})")), name, "{def}");
        assert_eq!(eval(&mut self.es[2], &format!("compile({name}, back_end: \"interpreter\")")), name, "{def}");
    }

    /// `call` prints alike in the three engines; the interpreter's value.
    fn same(&mut self, call: &str) -> String {
        let want = eval(&mut self.es[0], call);
        assert_eq!(eval(&mut self.es[1], call), want, "cranelift: {call}");
        assert_eq!(eval(&mut self.es[2], call), want, "interpreter back end: {call}");
        want
    }

    /// The calls among `calls` that do not print alike in the three engines.
    fn differ(&mut self, calls: &[String]) -> Vec<String> {
        let mut bad = vec![];
        for call in calls {
            let want = eval(&mut self.es[0], call);
            let (a, b) = (eval(&mut self.es[1], call), eval(&mut self.es[2], call));
            if a != want || b != want {
                bad.push(format!("{call}: {want} / {a} / {b}"));
            }
        }
        bad
    }

    /// Compiled runs and declines of `name` in the Cranelift engine.
    fn counts(&mut self, name: &str) -> (u64, u64) {
        let runs = info(&mut self.es[1], name, "runs").parse().unwrap();
        let declines = info(&mut self.es[1], name, "declines").parse().unwrap();
        (runs, declines)
    }
}

const ONE: &[&str] = &[
    "gamma", "log_gamma", "digamma", "zeta", "erf", "erfc", "erfi", "inverse_erf", "inverse_erfc", "exp_integral_ei",
    "log_integral", "sin_integral", "cos_integral", "sinh_integral", "cosh_integral", "airy_ai", "airy_bi",
    "airy_ai_prime", "airy_bi_prime", "elliptic_k", "elliptic_e", "product_log", "fresnel_s", "fresnel_c",
    "asinh", "acosh", "atanh",
];
const ONE_ARGS: &[&str] = &[
    "-3.5", "-1.0", "-0.5", "-0.0", "0.0", "0.25", "0.5", "0.999", "1.0", "1.5", "2.0", "7.25", "30.0", "171.5",
    "1e-310", "1e300", "0.0*(1e308*10.0)", "1e308*10.0",
];
const TWO: &[&str] = &[
    "polygamma", "beta", "bessel_k", "bessel_i", "bessel_j", "bessel_y", "gamma", "gamma_regularized", "exp_integral_e",
    "polylog", "hypergeometric_0f1", "elliptic_e", "elliptic_f", "elliptic_pi", "jacobi_sn", "jacobi_cn", "jacobi_dn",
    "inverse_jacobi_sn", "bessel_k_scaled", "bessel_i_scaled",
];
const TWO_ARGS: &[&str] = &[
    "0.0, 0.5", "1.0, 2.5", "0.5, 3.0", "2.5, 10.0", "1.0, 0.0", "-1.5, 2.0", "1.0, -2.0", "3.0, 0.1", "0.25, 0.75",
    "2.0, 700.0", "1.0, 1e-300", "2.0, 1.5", "0.0*(1e308*10.0), 1.0",
];
const THREE: &[&str] = &["beta", "beta_regularized", "hypergeometric_1f1", "elliptic_pi"];
const THREE_ARGS: &[&str] = &["0.3, 2.0, 3.0", "0.5, 0.5, 1.5", "-0.2, 1.0, 2.0", "1.5, 0.3, 0.4", "0.9, -0.5, 2.5"];
const FOUR_ARGS: &[&str] = &["1.0, 2.0, 3.0, 0.5", "0.5, 0.5, 1.5, -0.3", "1.0, 1.0, 2.0, 0.999", "1.0, 2.0, 3.0, 1.5"];

#[test]
fn special_functions_compiled_are_the_interpreters() {
    let mut t = Three::new("");
    let mut runs = 0;
    let mut bad = vec![];
    let groups: [(&[&str], &str, &[&str]); 4] = [
        (ONE, "x", ONE_ARGS),
        (TWO, "x, y", TWO_ARGS),
        (THREE, "x, y, z", THREE_ARGS),
        (&["hypergeometric_2f1"], "x, y, z, w", FOUR_ARGS),
    ];
    for (k, (names, params, args)) in groups.iter().enumerate() {
        let typed: Vec<String> = params.split(", ").map(|p| format!("{p}: Real")).collect();
        for f in names.iter() {
            let name = format!("s{k}_{f}");
            t.define(&name, &format!("fn {name}({}) = {f}({params})", typed.join(", ")));
            let calls: Vec<String> = args.iter().map(|a| format!("{name}({a})")).collect();
            bad.extend(t.differ(&calls));
            let (r, _) = t.counts(&name);
            assert!(r > 0, "{name} never ran compiled");
            runs += r;
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
    assert!(runs > 500, "{runs}");
}

#[test]
fn exact_orders_and_rational_arguments_are_converted_as_the_interpreter_converts_them() {
    let mut t = Three::new("");
    let defs = [
        ("bk", "fn bk(x: Real) = bessel_k(1, x) + bessel_i(0, x/2)"),
        ("bt", "fn bt(x: Real) = bessel_k(1/3, x)*polylog(2, x/10) + polygamma(1, x)"),
        ("bn", "fn bn(x: Real, n: Int) = bessel_j(n, x) + gamma(n + x)"),
        ("bs", "fn bs(x: Real) = log(bessel_k(1, x)/bessel_k(2, x)) + bessel_k_scaled(1, 1000.0 + x)"),
    ];
    for (n, d) in defs {
        t.define(n, d);
    }
    for x in ["0.5", "2.0", "13.75", "-1.0", "0.0", "1e-300"] {
        t.same(&format!("bk({x})"));
        t.same(&format!("bt({x})"));
        t.same(&format!("bs({x})"));
        for n in ["0", "3", "-2", "9007199254740993"] {
            t.same(&format!("bn({x}, {n})"));
        }
    }
    for (n, _) in defs {
        assert!(t.counts(n).0 > 0, "{n}");
    }
    // exact arguments only: the interpreter keeps the call exact, so it is not compiled
    let mut e = engine();
    assert_eq!(eval(&mut e, "fn ex(n: Int) = gamma(n)"), "null");
    assert_eq!(eval(&mut e, "compile(ex)"), "failure(not_compilable, ex, \"`gamma` of exact values (the result stays exact)\")");
}

#[test]
fn steps_signs_and_piecewise_are_the_interpreters() {
    let mut t = Three::new("");
    let defs = [
        ("st", "fn st(x: Real, y: Real) = unit_step(x) + 2*unit_step(x, y) + 4*sign(y) + 8*heaviside_theta(x - 1.0)"),
        ("hv", "fn hv(x: Real, y: Real) = heaviside_theta(x, y)*exp(x)"),
        ("pw", "fn pw(x: Real) = piecewise([[x^2, x < 0.0], [sin(x), x < 1.5]], exp(-x))"),
        ("pi0", "fn pi0(x: Real) = piecewise([[1, x < 0.0], [2, x < 1.5]])"),
        ("ps", "fn ps(n: Int, x: Real) = sign(n) + unit_step(n - 3) + heaviside_theta(n)*x + abs(x)*sign(x - 1.0)"),
        ("ifs", "fn ifs(x: Real) = if x > 2.0 then min(x, 5.0) else max(-x, 0.5)"),
    ];
    for (n, d) in defs {
        t.define(n, d);
    }
    let reals = ["-2.5", "-0.0", "0.0", "0.5", "1.0", "1.5", "3.0", "0.0*(1e308*10.0)", "1e308*10.0", "-1e308*10.0"];
    for x in reals {
        for y in ["-1.0", "0.0", "2.0", "0.0*(1e308*10.0)"] {
            t.same(&format!("st({x}, {y})"));
            t.same(&format!("hv({x}, {y})"));
        }
        t.same(&format!("pw({x})"));
        t.same(&format!("pi0({x})"));
        t.same(&format!("ifs({x})"));
        for n in ["-4", "0", "3", "7"] {
            t.same(&format!("ps({n}, {x})"));
        }
    }
    for (n, _) in defs {
        let (runs, declines) = t.counts(n);
        assert!(runs > 0, "{n}: {runs} runs, {declines} declines");
    }
    // heaviside_theta at 0 and a NaN step stay unevaluated: the compiled calls declined there
    assert!(t.counts("hv").1 > 0);
}

/// 101 nodes (a packed table) of sin(x)*exp(-x/7) on [0, 5].
const TABLE: &str = "tab = table([x, sin(x)*exp(-x/7)], x, range(0.0, 5.0, 0.05))";

#[test]
fn local_interpolations_and_derivatives_are_the_interpreters() {
    let mut t = Three::new(TABLE);
    for e in t.es.iter_mut() {
        eval(e, "s = interpolation(tab)");
        for k in 1..=4 {
            eval(e, &format!("l{k} = interpolation(tab, interpolation_order: {k})"));
        }
    }
    let defs = [
        ("i1", "fn i1(x: Real) = l1(x) + l2(x) + l3(x) + l4(x)"),
        ("i2", "fn i2(x: Real) = s'(x) - s''(x) + s'''(x)"),
        ("i3", "fn i3(x: Real) = l3'(x) + l4''(x) + l2'(x)"),
        ("i4", "fn i4(n: Int) = l3(n) + s'(n)"),
    ];
    for (n, d) in defs {
        t.define(n, d);
    }
    for x in ["0.0", "0.05", "0.123", "1.0", "2.5", "4.99", "5.0", "-0.1", "5.5", "0.0*(1e308*10.0)"] {
        for (n, _) in &defs[..3] {
            t.same(&format!("{n}({x})"));
        }
    }
    for n in ["0", "2", "5", "6"] {
        t.same(&format!("i4({n})"));
    }
    for (n, _) in defs {
        assert!(t.counts(n).0 > 0, "{n}");
    }
    // outside the table: the interpreter's failure
    assert!(t.same("i1(5.5)").contains("out_of_range"));
    // redefining the global invalidates the code (1b.6): the new table's values
    for e in t.es.iter_mut() {
        eval(e, "l3 = interpolation(table([x, x^3], x, range(0.0, 5.0, 0.05)), interpolation_order: 3)");
    }
    assert_eq!(t.same("i3(2.0)"), t.same("l3'(2.0) + l4''(2.0) + l2'(2.0)"));
}

#[test]
fn solutions_of_n_dsolve_are_called_from_compiled_code() {
    let setup = "s = n_dsolve([y'(t) == -y(t) + sin(3*t), y(0) == 1], y, [t, 0, 4]); yy = y /. s";
    let mut t = Three::new(setup);
    t.define("sv", "fn sv(t: Real) = yy(t)^2 + yy'(t) - yy''(t)");
    for x in ["0.0", "0.37", "2.0", "4.0", "4.5", "-1.0"] {
        t.same(&format!("sv({x})"));
    }
    assert!(t.counts("sv").0 > 0);
    assert!(t.same("sv(4.5)").contains("out_of_range"));
    // an integral of a solution: compiled and interpreted give the same value
    let mut e = engine();
    eval(&mut e, setup);
    let compiled = eval(&mut e, "n_integrate(yy(t)^2*exp(-t), [t, 0, 4])");
    let interpreted = eval(&mut e, "n_integrate(yy(t)^2*exp(-t), [t, 0, 4], compile: false)");
    assert_eq!(compiled, interpreted);
}

#[test]
fn right_hand_sides_with_special_functions_and_steps_give_the_interpreters_solution() {
    let mut e = engine();
    for rhs in [
        "cos(3*t)*bessel_k(1, 2 + y(t)^2)",
        "cos(3*t)*piecewise([[exp(-y(t)), y(t) < 1.2]], 0.5)",
        "-unit_step(t - 1)*y(t) + heaviside_theta(2 - t)*erf(y(t))",
        "sign(1.5 - t)*gamma(1 + y(t)^2)/10",
    ] {
        let a = eval(&mut e, &format!("n_dsolve([y'(t) == {rhs}, y(0) == 1], y, [t, 0, 3], method: rk45)"));
        let b = eval(&mut e, &format!("n_dsolve([y'(t) == {rhs}, y(0) == 1], y, [t, 0, 3], method: rk45, compile: false)"));
        assert!(a.contains("interpolating_function"), "{rhs}: {a}");
        // the solution's key is a hash of its content: equal keys, equal solutions
        assert_eq!(a, b, "{rhs}");
    }
}
