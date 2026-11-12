//! `mc_numfn::par` through the engine: split batches give the sequential backend's results bit for bit (declined
//! points included) at 1 and 4 threads; fuel limits and interrupts stop every thread; a measurement of the speedup.
use mc_api::{Engine, EngineError, StopFlag};
use mc_core::{Heap, Store, View, T};
use mc_engine::{CoreEngine, EngineConfig};
use mc_eval::{Attributes, Hold, Kernel, Native, NativeCx, Reply, Res};
use mc_numfn::{auto_expr, par_expr, NumFn, Options, Policy, Var};
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

/// Deterministic points: `n` points of `ni` inputs in [lo, hi], with a few special values.
fn points(n: usize, ni: usize, lo: f64, hi: f64, seed: u64) -> Vec<f64> {
    let mut s = seed | 1;
    (0..n * ni)
        .map(|k| {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            let u = (s.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64;
            match k % 97 {
                0 => f64::NAN,
                1 => -0.0,
                2 => 1e308 * 10.0,
                _ => lo + (hi - lo) * u,
            }
        })
        .collect()
}

fn same(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()))
}

/// `par_cmp(expr, vars, n, threads, lo, hi)` (held): evaluates `n` points with the sequential backend and with
/// `Par`, `[same, ok_seq, ok_par, batches, fallback, threads_used, micros_seq, micros_par]`.
struct Cmp;

impl Native for Cmp {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [expr, spec, n, threads, lo, hi] = args else { return Ok(Reply::Decline) };
        let (Some(vars), View::Int(n), View::Int(threads), View::Real(lo), View::Real(hi)) =
            (vars_of(cx, *spec), h.view(*n), h.view(*threads), h.view(*lo), h.view(*hi))
        else {
            return Ok(Reply::Decline);
        };
        let opts = Options { policy: Policy::Compiled, ..Options::default() };
        let mut seq = auto_expr(cx, *expr, &vars, &opts)?;
        let mut par = par_expr(cx, *expr, &vars, &opts)?;
        if threads > 0 {
            par = par.with_threads(threads as usize);
        }
        let (ni, no) = (seq.inputs(), seq.outputs());
        let x = points(n as usize, ni, lo, hi, 0x9e37_79b9_7f4a_7c15);
        let (mut a, mut b) = (vec![0.0; no * n as usize], vec![0.0; no * n as usize]);
        let t0 = Instant::now();
        let ok_seq = seq.eval_flat(cx, &x, &mut a)?;
        let t1 = Instant::now();
        let ok_par = par.eval_flat(cx, &x, &mut b)?;
        let t2 = Instant::now();
        let s = par.par_stats();
        let int = |k: u64| h.int(k as i64);
        Ok(Reply::Done(cx.list(&[
            cx.bool(same(&a, &b)),
            cx.bool(ok_seq),
            cx.bool(ok_par),
            int(s.batches),
            int(s.fallback),
            int(s.threads),
            int((t1 - t0).as_micros() as u64),
            int((t2 - t1).as_micros() as u64),
        ])))
    }
}

fn engine(threads: usize, fuel: Option<i64>) -> CoreEngine {
    let mut cfg = EngineConfig { threads, ..EngineConfig::default() };
    if let Some(f) = fuel {
        cfg.eval.fuel = f;
    }
    let mut e = CoreEngine::with_config(cfg);
    e.install(|k: &mut Kernel, h: &Heap| {
        k.register(h.sym("par_cmp"), Attributes { hold: Hold::All, ..Attributes::default() }, Box::new(Cmp));
    });
    for def in DEFS {
        assert_eq!(eval(&mut e, def), "null", "{def}");
    }
    e
}

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

const DEFS: &[&str] = &[
    "fn escape(cr: Real, ci: Real) = { let c = complex(cr, ci); var z = complex(0.0, 0.0); var k = 0; while k < 256 and abs(z) <= 2.0 { z = z^2 + c; k = k + 1 }; 1.0*k }",
    "fn spin(x: Real) = { var s = x; while s >= 0.0 { s = s + 1.0 }; s }",
    "fn steps(x: Real) = { var s = 0.0; var k = 0; while k < floor(abs(x)*50.0) { s = s + sin(k*x); k = k + 1 }; s }",
];

/// Expressions, variables and point ranges: uneven loops, complex numbers inside, declines (outside the real
/// domain, through `n`'s exact parts), vector inputs and list results.
const EXPRS: &[(&str, &str, f64, f64)] = &[
    ("escape(x, y)", "[x, y]", -2.0, 1.0),
    ("steps(x) + log(x)", "[x]", -3.0, 3.0),
    ("sqrt(x - 0.5) + x^y", "[x, y]", -1.0, 2.0),
    ("[abs(complex(x, y)^3 + 1), re(exp(complex(y, x))), x*y - 1/3]", "[x, y]", -2.0, 2.0),
    ("[y[1]*y[2] - sin(y[3]), y[3]^2]", "[[y, 3]]", -5.0, 5.0),
    ("asin(x) + acos(y)", "[x, y]", -1.5, 1.5),
];

#[test]
fn split_batches_are_the_sequential_results() {
    let mut want: Vec<String> = vec![];
    for threads in [1, 4] {
        let mut e = engine(threads, None);
        for (i, (expr, vars, lo, hi)) in EXPRS.iter().enumerate() {
            for n in [10, 300, 5000] {
                let r = eval(&mut e, &format!("par_cmp({expr}, {vars}, {n}, 0, {lo:?}, {hi:?})"));
                let parts: Vec<&str> = r.trim_matches(['[', ']']).split(", ").collect();
                assert_eq!(parts[0], "true", "{expr} at {n} points, {threads} threads: {r}");
                assert_eq!(parts[1], parts[2], "{expr}: verdicts {r}");
                let batches: u64 = parts[3].parse().unwrap();
                // split only with more than one thread and enough points
                assert_eq!(batches > 0, threads > 1 && n >= 300, "{expr} at {n}, {threads} threads: {r}");
                if threads == 1 {
                    want.push(parts[1].to_string());
                } else {
                    assert_eq!(parts[2], want[i * 3 + [10, 300, 5000].iter().position(|&m| m == n).unwrap()], "{expr}");
                }
                // the declining expressions really sent points to the sequential backend
                if threads > 1 && n == 5000 && (i == 1 || i == 2 || i == 5) {
                    assert!(parts[4].parse::<u64>().unwrap() > 0, "{expr}: {r}");
                }
            }
        }
    }
}

#[test]
fn fuel_limits_and_interrupts_stop_every_thread() {
    // spin never ends for x >= 0: the fuel of the loops on every thread reaches the limit
    let mut e = engine(4, Some(5_000_000));
    match e.eval("par_cmp(spin(x), [x], 1000, 0, -1.0, 1.0)", &StopFlag::new()) {
        Err(EngineError::Limit(what)) => assert_eq!(what, "fuel"),
        other => panic!("{other:?}"),
    }
    assert_eq!(eval(&mut e, "1 + 1"), "2");
    // an interrupt during a split batch
    let mut e = engine(4, None);
    let stop = StopFlag::new();
    let raiser = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            stop.raise();
            Instant::now()
        })
    };
    assert_eq!(e.eval("par_cmp(spin(x), [x], 1000, 0, -1.0, 1.0)", &stop), Err(EngineError::Stopped));
    let raised = raiser.join().unwrap();
    assert!(raised.elapsed() < Duration::from_secs(2), "latency {:?}", raised.elapsed());
    stop.clear();
    assert_eq!(eval(&mut e, "1 + 1"), "2");
}

#[test]
fn speedup() {
    // Mandelbrot escape counts of 40,000 points: sequential compiled against split batches at 2 and 3 threads
    let mut e = engine(4, None);
    for threads in [1, 2, 3, 4] {
        let mut best = (u64::MAX, u64::MAX);
        for _ in 0..3 {
            let r = eval(&mut e, &format!("par_cmp(escape(x, y), [x, y], 40000, {threads}, -2.0, 1.0)"));
            let parts: Vec<&str> = r.trim_matches(['[', ']']).split(", ").collect();
            assert_eq!(parts[0], "true", "{r}");
            let (s, p): (u64, u64) = (parts[6].parse().unwrap(), parts[7].parse().unwrap());
            best = (best.0.min(s), best.1.min(p));
        }
        println!("escape, 40000 points, {threads} threads: sequential {} us, split {} us, speedup {:.2}", best.0, best.1, best.0 as f64 / best.1 as f64);
    }
}
