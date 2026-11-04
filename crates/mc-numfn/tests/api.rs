//! `compile_expr` and `Auto` through the engine: compiled calls, declined calls evaluated by the interpreter,
//! refusals, the lazy policy, batches, closures, interpreted evaluation on balls and complex numbers, fuel and
//! interrupts in compiled loops, and a measurement of interpreted against compiled calls.
use mc_bench::{Engine, EngineError, StopFlag};
use mc_core::{Heap, Store, View, T};
use mc_engine::{CoreEngine, EngineConfig};
use mc_eval::{Attributes, Hold, Kernel, Native, NativeCx, Reply, Res};
use mc_numfn::{auto_expr, compile_fn, Arg, Backend, Interpreted, NumFn, Options, Policy, Prep, Var};
use mc_numeric::machine::C64;
use std::time::{Duration, Instant};

fn vars_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, spec: T<'h>) -> Option<Vec<Var>> {
    let h = cx.heap();
    let mut out = vec![];
    for &i in cx.as_list(spec)? {
        match h.view(i) {
            View::Sym(s) => out.push(Var::scalar(s)),
            _ => {
                let [y, n] = cx.as_list(i)? else { return None };
                let (View::Sym(s), View::Int(n)) = (h.view(*y), h.view(*n)) else { return None };
                out.push(Var::vector(s, n as usize));
            }
        }
    }
    Some(out)
}

fn policy(s: &str) -> Policy {
    match s {
        "compiled" => Policy::Compiled,
        "interpreted" => Policy::Interpreted,
        s => Policy::Auto { after: s.trim_start_matches("auto:").parse().unwrap() },
    }
}

fn reals<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<f64>> {
    let h = cx.heap();
    cx.as_list(t)?
        .iter()
        .map(|&v| match h.view(v) {
            View::Real(x) => Some(x),
            View::Int(k) => Some(k as f64),
            _ => None,
        })
        .collect()
}

/// `numfn_run(expr, vars, points, "policy")` (held): `[ok, outputs, backend, compiled, declined, interpreted]`
/// after one `eval_flat` over all points.
struct Run;

impl Native for Run {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (expr, spec, points, pol, timed) = match args {
            [a, b, c, d] => (a, b, c, d, false),
            [a, b, c, d, _] => (a, b, c, d, true),
            _ => return Ok(Reply::Decline),
        };
        let points = cx.eval(*points)?;
        let (Some(vars), Some(x), View::Str(p)) = (vars_of(cx, *spec), reals(cx, points), h.view(*pol)) else {
            return Ok(Reply::Decline);
        };
        let opts = Options { policy: policy(p), ..Options::default() };
        let mut f = auto_expr(cx, *expr, &vars, &opts)?;
        let (ni, no) = (f.inputs(), f.outputs());
        let mut out = vec![0.0; no * (x.len() / ni.max(1))];
        let t0 = Instant::now();
        let ok = f.eval_flat(cx, &x, &mut out)?;
        if timed {
            return Ok(Reply::Done(cx.list(&[cx.bool(ok), h.int(t0.elapsed().as_micros() as i64)])));
        }
        let s = f.stats();
        let outs: Vec<T<'h>> = out.iter().map(|&v| h.real(v)).collect();
        let backend = h.string(if f.backend() == Backend::Compiled { "compiled" } else { "interpreted" });
        let n = |k: u64| h.int(k as i64);
        Ok(Reply::Done(cx.list(&[cx.bool(ok), cx.list(&outs), backend, n(s.compiled), n(s.declined), n(s.interpreted)])))
    }
}

/// `numfn_closure(f, kinds, point)`: one evaluation of the function value `f` (kinds: 1 scalar, [n] vector)
/// compiled eagerly: `[ok, outputs, backend]`.
struct Closure;

impl Native for Closure {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (f, kinds, point, outputs) = match args {
            [f, k, p] => (f, k, p, None),
            [f, k, p, o] => match h.view(*o) {
                View::Int(o) => (f, k, p, Some(o as usize)),
                _ => return Ok(Reply::Decline),
            },
            _ => return Ok(Reply::Decline),
        };
        let Some(ks) = cx.as_list(*kinds) else { return Ok(Reply::Decline) };
        let mut k = vec![];
        for &x in ks {
            k.push(match (h.view(x), cx.as_list(x)) {
                (View::Int(1), _) => Arg::Scalar,
                (_, Some([n])) => match h.view(*n) {
                    View::Int(n) => Arg::Vector(n as usize),
                    _ => return Ok(Reply::Decline),
                },
                _ => return Ok(Reply::Decline),
            });
        }
        let Some(x) = reals(cx, *point) else { return Ok(Reply::Decline) };
        let opts = Options { policy: Policy::Compiled, outputs, ..Options::default() };
        let Some(mut g) = compile_fn(cx, *f, &k, &opts)? else { return Ok(Reply::Done(h.string("not a function"))) };
        let mut out = vec![0.0; g.outputs()];
        let ok = g.eval(cx, &x, &mut out)?;
        let outs: Vec<T<'h>> = out.iter().map(|&v| h.real(v)).collect();
        let backend = h.string(if g.backend() == Backend::Compiled { "compiled" } else { "interpreted" });
        Ok(Reply::Done(cx.list(&[cx.bool(ok), cx.list(&outs), backend])))
    }
}

/// `numfn_ball(expr, x, digits)` and `numfn_complex(expr, re, im)`: the interpreted backend on balls and on
/// complex machine numbers.
struct Ball;
struct Complex;

impl Native for Ball {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [expr, x, d] = args else { return Ok(Reply::Decline) };
        let (View::Sym(xs), View::Int(d)) = (h.view(*x), h.view(*d)) else { return Ok(Reply::Decline) };
        let mut f = Interpreted::new(cx, *expr, &[Var::scalar(xs)], Prep::Digits(d as u64))?;
        // the argument: 1/3 as a ball of the working precision
        let third = cx.eval(h.call(h.sym("n"), &[h.rational(h.int(1), h.int(3)), h.int(d)]))?;
        let Some(a) = f.arith().num(third) else { return Ok(Reply::Decline) };
        let mut out = vec![a.clone()];
        if !f.eval(cx, &[a], &mut out)? {
            return Ok(Reply::Done(h.string("not numeric")));
        }
        Ok(Reply::Done(f.arith().number(&out[0])))
    }
}

impl Native for Complex {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [expr, x] = args else { return Ok(Reply::Decline) };
        let View::Sym(xs) = h.view(*x) else { return Ok(Reply::Decline) };
        let mut f = Interpreted::new(cx, *expr, &[Var::scalar(xs)], Prep::Machine)?;
        let mut out = [C64::real(0.0)];
        if !f.eval(cx, &[C64::new(0.0, 1.0)], &mut out)? {
            return Ok(Reply::Done(h.string("not numeric")));
        }
        Ok(Reply::Done(cx.list(&[h.real(out[0].re), h.real(out[0].im)])))
    }
}

fn install(e: &mut CoreEngine) {
    e.install(|k: &mut Kernel, h: &Heap| {
        let held = Attributes { hold: Hold::All, ..Attributes::default() };
        k.register(h.sym("numfn_run"), held, Box::new(Run));
        k.register(h.sym("numfn_closure"), Attributes::default(), Box::new(Closure));
        k.register(h.sym("numfn_ball"), held, Box::new(Ball));
        k.register(h.sym("numfn_complex"), held, Box::new(Complex));
    });
}

fn engine() -> CoreEngine {
    let mut e = CoreEngine::new();
    install(&mut e);
    e
}

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn compiled_calls_and_their_fallbacks() {
    let mut e = engine();
    // a vector variable, constants, a list result: compiled, results as the interpreter's
    let src = "numfn_run([sin(x) + y[2]*pi, y[1]^2 - exp(-x)], [x, [y, 2]], [0.5, 1.0, 2.0, -1.5, 3.0, 4.0], \"%\")";
    let want = eval(&mut e, &src.replace('%', "interpreted"));
    assert!(want.starts_with("[true, ["), "{want}");
    let got = eval(&mut e, &src.replace('%', "compiled"));
    assert_eq!(got.split(", \"").next(), want.split(", \"").next(), "{got} vs {want}");
    assert!(got.ends_with("\"compiled\", 2, 0, 0]"), "{got}");
    // sqrt of a negative number: declined, then the interpreter's complex value, which is not a machine real
    assert_eq!(eval(&mut e, "numfn_run(sqrt(x), [x], [4.0, -1.0], \"compiled\")"), "[false, [2.0, 0.0], \"compiled\", 1, 1, 1]");
    assert_eq!(eval(&mut e, "numfn_run(log(x) + 1, [x], [1.0, 0.5], \"compiled\")"), "[true, [1.0, 0.3068528194400547], \"compiled\", 2, 0, 0]");
    // outside the subset: interpreted throughout
    let r = eval(&mut e, "numfn_run(besselj(0, x), [x], [1.0], \"compiled\")");
    assert!(r.ends_with("\"interpreted\", 0, 0, 1]"), "{r}");
    // the lazy policy compiles after the given number of evaluations
    let pts: Vec<String> = (0..10).map(|k| format!("{}.5", k)).collect();
    let r = eval(&mut e, &format!("numfn_run(x^3 - 2*x, [x], [{}], \"auto:3\")", pts.join(", ")));
    assert!(r.starts_with("[true") && r.ends_with("\"compiled\", 10, 0, 0]"), "{r}");
    let r = eval(&mut e, "numfn_run(x^3 - 2*x, [x], [1.0, 2.0], \"auto:3\")");
    assert!(r.ends_with("\"interpreted\", 0, 0, 2]"), "{r}");
}

#[test]
fn closures_and_function_names_compile() {
    let mut e = engine();
    assert_eq!(eval(&mut e, "fn osc(t: Real, u: Real, v: Real) = [v, -u - 0.1*v + n(pi)*cos(t)]"), "null");
    for (f, kinds, point, want) in [
        ("(t, y) => [y[2], -y[1]]", "[1, [2]]", "[0.0, 1.0, 2.0]", "[true, [2.0, -1.0], \"compiled\"]"),
        ("x => x^2 + pi", "[1]", "[2.0]", "[true, [7.141592653589793], \"compiled\"]"),
        ("(t: Real, y) => [y[2], length(y)*t]", "[1, [2]]", "[0.5, 1.0, 2.0]", "[true, [2.0, 1.0], \"compiled\"]"),
        ("(t, y) => osc(t, y[1], y[2])", "[1, [2]]", "[0.0, 1.0, 0.0], 2", "[true, [0.0, 2.141592653589793], \"compiled\"]"),
        ("x => round(x) + 0.5", "[1]", "[2.5]", "[true, [2.5], \"interpreted\"]"),
        ("(t, y) => y[3]", "[1, [2]]", "[0.0, 1.0, 2.0]", "[false, [0.0], \"interpreted\"]"),
    ] {
        assert_eq!(eval(&mut e, &format!("numfn_closure({f}, {kinds}, {point})")), want, "{f}");
    }
    // a declared type the arguments may not have: applied as written, the interpreter checks it
    assert_eq!(eval(&mut e, "numfn_closure((k: Int) => k + 1, [1], [2.0])"), "[false, [0.0], \"interpreted\"]");
}

#[test]
fn the_interpreted_backend_runs_on_balls_and_complex_numbers() {
    let mut e = engine();
    // exp(1/3) to 40 digits as a ball, and a constant prepared at the same precision
    let r = eval(&mut e, "numfn_ball(exp(x) + pi, x, 40)");
    assert!(r.starts_with("4.53720507867588276709"), "{r}");
    assert!(eval(&mut e, "n(exp(1/3) + pi, 40)").starts_with("4.53720507867588276709"));
    // complex machine numbers: exp(i) = cos 1 + i sin 1
    assert_eq!(eval(&mut e, "numfn_complex(exp(x), x)"), "[0.5403023058681398, 0.8414709848078965]");
}

const SPIN: &str = "fn spin(x: Real) = { var s = x; while s >= 0.0 { s = s + 1.0 }; s }";

#[test]
fn fuel_and_interrupts_stop_compiled_loops() {
    let mut cfg = EngineConfig::default();
    cfg.eval.fuel = 2_000_000;
    let mut e = CoreEngine::with_config(cfg);
    install(&mut e);
    assert_eq!(eval(&mut e, SPIN), "null");
    match e.eval("numfn_run(spin(x), [x], [1.0], \"compiled\")", &StopFlag::new()) {
        Err(EngineError::Limit(what)) => assert_eq!(what, "fuel"),
        other => panic!("{other:?}"),
    }
    assert_eq!(eval(&mut e, "numfn_run(spin(x), [x], [-2.0], \"compiled\")"), "[true, [-2.0], \"compiled\", 1, 0, 0]");
    let mut e = engine();
    assert_eq!(eval(&mut e, SPIN), "null");
    let stop = StopFlag::new();
    let raiser = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            stop.raise();
            Instant::now()
        })
    };
    assert_eq!(e.eval("numfn_run(spin(x), [x], [1.0], \"compiled\")", &stop), Err(EngineError::Stopped));
    let raised = raiser.join().unwrap();
    assert!(raised.elapsed() < Duration::from_secs(2), "latency {:?}", raised.elapsed());
    stop.clear();
    assert_eq!(eval(&mut e, "1 + 1"), "2");
}

/// Interpreted against compiled calls of a moderately long expression (printed with `--nocapture`).
#[test]
fn compiled_calls_are_faster() {
    let mut e = engine();
    let n = 20_000;
    let expr = "[y[2], (1 - y[1]^2)*y[2] - y[1] + 0.5*sin(x)*exp(-0.1*x)]";
    let pts = format!("flatten(map(k => [0.001*k, 1.0 + 0.0001*k, -0.5], range({n})))");
    let mut time = |p: &str| {
        let r = eval(&mut e, &format!("numfn_run({expr}, [x, [y, 2]], {pts}, \"{p}\", time)"));
        let us: u64 = r.trim_start_matches("[true, ").trim_end_matches(']').parse().unwrap_or_else(|_| panic!("{r}"));
        Duration::from_micros(us)
    };
    let (ti, tc) = (time("interpreted"), time("compiled"));
    eprintln!(
        "{n} evaluations of the van der Pol right-hand side: interpreted {ti:?} ({:.2} us each), compiled {tc:?} ({:.3} us each; compiled before)",
        ti.as_secs_f64() * 1e6 / n as f64,
        tc.as_secs_f64() * 1e6 / n as f64
    );
    assert!(tc < ti, "compiled {tc:?}, interpreted {ti:?}");
}
