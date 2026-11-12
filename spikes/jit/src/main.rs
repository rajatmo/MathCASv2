//! Spike E driver: `jit --quick` or `jit --full` (under 3 minutes on a busy machine).
//!
//! Every back end runs the same workloads (benchmarks L1, L3, L5, L9, L10 and M1); results are compared by
//! fingerprint and the run aborts on any disagreement.
use jit::interp::Interp;
use jit::ir::{lower, Func, Opts};
use jit::jit::{Compiled, Jit};
use jit::kernels as k;
use jit::run::*;
use jit::term::TermFn;
use jit::{fp_f64, fp_i64, fp_words, native, Ctl, Poll, Status};
use std::io::Write;
use std::sync::atomic::Ordering;
use std::time::Instant;

const POLL: Poll = Poll::Every(1024);

static OUT: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Print a line and keep it for `--out <file>`.
macro_rules! out {
    ($($t:tt)*) => {{
        let line = format!($($t)*);
        println!("{line}");
        let mut o = OUT.lock().unwrap();
        o.push_str(&line);
        o.push('\n');
    }};
}

/// Best-of time in seconds: repeat while the total stays under 0.25 s (at most 15 runs).
fn time<R>(mut f: impl FnMut() -> R) -> (f64, R) {
    let t = Instant::now();
    let mut r = f();
    let mut best = t.elapsed().as_secs_f64();
    let mut total = best;
    let mut reps = 1;
    while total < 0.25 && reps < 15 {
        let t = Instant::now();
        r = f();
        let e = t.elapsed().as_secs_f64();
        best = best.min(e);
        total += e;
        reps += 1;
    }
    (best, r)
}

struct Row {
    back: String,
    n: usize,
    secs: f64,
    fp: u64,
}

/// Rows of one workload; prints derived ratios per element against the term, interpreter and native rows.
struct Table {
    title: String,
    unit: &'static str,
    rows: Vec<Row>,
}

/// Per-element time of the latest term-level row: each workload runs the term level first, at a smaller size, and its
/// later tables compare against it.
static LAST_TERM: std::sync::Mutex<Option<f64>> = std::sync::Mutex::new(None);

impl Table {
    fn new(title: String, unit: &'static str) -> Table {
        Table { title, unit, rows: vec![] }
    }
    fn add(&mut self, back: &str, n: usize, secs: f64, fp: u64) {
        self.rows.push(Row { back: back.into(), n, secs, fp });
    }
    fn per(&self, prefix: &str) -> Option<f64> {
        self.rows.iter().find(|r| r.back.starts_with(prefix)).map(|r| r.secs / r.n as f64)
    }
    fn print(&self) {
        out!("\n  {}", self.title);
        out!(
            "    {:<26} {:>9} {:>10} {:>11} {:>9} {:>9} {:>9}  fingerprint",
            "back end",
            self.unit,
            "time ms",
            format!("ns/{}", self.unit),
            "x term",
            "x interp",
            "/ native"
        );
        let own = self.per("term");
        if own.is_some() {
            *LAST_TERM.lock().unwrap() = own;
        }
        let (pt, pi, pn) = (own.or(*LAST_TERM.lock().unwrap()), self.per("interp"), self.per("native"));
        for r in &self.rows {
            let p = r.secs / r.n as f64;
            let ratio = |b: Option<f64>| b.map_or("-".to_string(), |b| format!("{:.1}", b / p));
            out!(
                "    {:<26} {:>9} {:>10.3} {:>11.2} {:>9} {:>9} {:>9}  {:016x}",
                r.back,
                r.n,
                r.secs * 1e3,
                p * 1e9,
                ratio(pt),
                ratio(pi),
                pn.map_or("-".to_string(), |n| format!("{:.2}", p / n)),
                r.fp
            );
        }
    }
}

fn agree(what: &str, a: u64, b: u64) {
    if a != b {
        eprintln!("MISMATCH in {what}: {a:016x} vs {b:016x}");
        std::process::exit(2);
    }
}

fn pools() -> Vec<(usize, rayon::ThreadPool)> {
    [1, 2, 4].iter().map(|&n| (n, rayon::ThreadPoolBuilder::new().num_threads(n).build().unwrap())).collect()
}

struct Sizes {
    l1: Vec<usize>,
    l1_term: usize,
    mandel: Vec<usize>,
    mandel_term: usize,
    lorenz: Vec<usize>,
    lorenz_term: usize,
    newton: Vec<usize>,
    newton_term: usize,
    mc: Vec<usize>,
    mc_term: usize,
    interp_max: usize,
}

type Case = (&'static str, fn(&Sizes));

fn main() {
    let full = std::env::args().any(|a| a == "--full");
    let sz = if full {
        Sizes {
            l1: vec![100_000, 1_000_000, 10_000_000],
            l1_term: 20_000,
            mandel: vec![200, 1000],
            mandel_term: 100,
            lorenz: vec![10_000, 100_000],
            lorenz_term: 1_000,
            newton: vec![1_000, 10_000, 100_000],
            newton_term: 300,
            mc: vec![1_000_000, 10_000_000],
            mc_term: 20_000,
            interp_max: 1_000_000,
        }
    } else {
        Sizes {
            l1: vec![100_000],
            l1_term: 5_000,
            mandel: vec![200],
            mandel_term: 50,
            lorenz: vec![10_000],
            lorenz_term: 300,
            newton: vec![1_000],
            newton_term: 100,
            mc: vec![1_000_000],
            mc_term: 5_000,
            interp_max: 1_000_000,
        }
    };
    out!("spike E: numeric compiler ({} mode; busy shared machine, noisy numbers)", if full { "full" } else { "quick" });
    let t_all = Instant::now();
    let cases: Vec<Case> = vec![
        ("compile", compile_times),
        ("L1 expression table", l1),
        ("L9 Mandelbrot", mandel),
        ("L3 Lorenz RK4", lorenz),
        ("L5 Newton", newton),
        ("L10 Monte Carlo", monte_carlo),
        ("M1 parallel compiled loops", parallel),
        ("poll cost", poll_cost),
        ("fused multiply-add", fma),
        ("interrupt latency", interrupt),
        ("break-even", break_even),
    ];
    let n = cases.len();
    for (j, (name, f)) in cases.into_iter().enumerate() {
        out!("\n[{}/{} {:.0}%] {}", j + 1, n, 100.0 * j as f64 / n as f64, name);
        std::io::stdout().flush().ok();
        f(&sz);
    }
    out!("\n[{n}/{n} 100%] done in {:.1} s; all back ends agreed", t_all.elapsed().as_secs_f64());
    let args: Vec<String> = std::env::args().collect();
    if let Some(k) = args.iter().position(|a| a == "--out") {
        std::fs::write(&args[k + 1], OUT.lock().unwrap().as_str()).expect("write --out file");
    }
}

fn functions() -> Vec<Func> {
    vec![
        k::scalar_fn("l1", k::l1_expr),
        k::table_fn("l1_table", k::l1_expr),
        k::mandel_scalar(),
        k::mandel_loop(),
        k::lorenz_rhs(),
        k::lorenz_rk4_loop(),
        k::newton_f(),
        k::newton_df(),
        k::newton_loop(),
        k::mc_loop(),
    ]
}

fn compile_times(_: &Sizes) {
    out!(
        "    {:<14} {:>8} {:>9} {:>10} {:>11} {:>11} {:>9} {:>9}",
        "function", "LIR ins", "lower us", "interp us", "cl none us", "cl speed us", "B none", "B speed"
    );
    let mut jn = Jit::new("none");
    let mut js = Jit::new("speed");
    // warm up the code paths of both compilers once
    let warm = lower(&k::mandel_loop()).unwrap();
    jn.compile(&warm, POLL).unwrap();
    js.compile(&warm, POLL).unwrap();
    let mut tot = (0.0, 0.0);
    for func in functions() {
        let mut best = [f64::MAX; 4];
        let (mut bn, mut bs) = (0, 0);
        let mut lir = None;
        for _ in 0..5 {
            let t = Instant::now();
            let l = lower(&func).unwrap();
            best[0] = best[0].min(t.elapsed().as_secs_f64());
            let t = Instant::now();
            let it = Interp::new(&l, POLL);
            best[1] = best[1].min(t.elapsed().as_secs_f64());
            std::hint::black_box(it.instructions());
            let c = jn.compile(&l, POLL).unwrap();
            best[2] = best[2].min(c.compile_time.as_secs_f64());
            bn = c.code_bytes;
            let c = js.compile(&l, POLL).unwrap();
            best[3] = best[3].min(c.compile_time.as_secs_f64());
            bs = c.code_bytes;
            lir = Some(l);
        }
        let l = lir.unwrap();
        tot.0 += best[2];
        tot.1 += best[3];
        out!(
            "    {:<14} {:>8} {:>9.1} {:>10.1} {:>11.1} {:>11.1} {:>9} {:>9}",
            func.name,
            l.code.len(),
            best[0] * 1e6,
            best[1] * 1e6,
            best[2] * 1e6,
            best[3] * 1e6,
            bn,
            bs
        );
    }
    out!(
        "    (compile = LIR to finalised code incl. the uniform entry; B = bytes of the main function; host has FMA: {})",
        js.has_fma()
    );
    out!("    total for the 10 functions: {:.2} ms (none), {:.2} ms (speed)", tot.0 * 1e3, tot.1 * 1e3);
}

fn l1(sz: &Sizes) {
    let mut js = Jit::new("speed");
    let mut jn = Jit::new("none");
    let ctl = Ctl::unlimited();
    let ps = prepare(&mut js, &k::scalar_fn("l1", k::l1_expr), POLL);
    let pt = prepare(&mut js, &k::table_fn("l1_table", k::l1_expr), POLL);
    let ptn = jn.compile(&pt.lir, POLL).unwrap();
    let mut tf = TermFn::new(&k::scalar_fn("l1", k::l1_expr)).unwrap();
    // term level on a small size, checked against the JIT at that size
    {
        let xs = k::l1_points(sz.l1_term);
        let mut out = vec![0.0; xs.len()];
        let mut chk = vec![0.0; xs.len()];
        let (s, _) = time(|| map_scalar(&xs, &mut out, |x| term_scalar(&mut tf, x, &ctl)));
        table_jit(&pt.jit, &xs, &mut chk, &ctl);
        agree("L1 term", fp_f64(&out), fp_f64(&chk));
        let mut t = Table::new(format!("L1 at {} points (term level)", sz.l1_term), "point");
        t.add("term (hash-consed eval)", xs.len(), s, fp_f64(&out));
        t.print();
        out!("    term store after one call: {} nodes", tf.st.len());
    }
    for &n in &sz.l1 {
        let xs = k::l1_points(n);
        let mut out = vec![0.0; n];
        let mut want = vec![0.0; n];
        let mut t = Table::new(format!("L1 at {n} points"), "point");
        let (s, _) = time(|| native::table(&xs, &mut want, native::l1));
        let fw = fp_f64(&want);
        if n <= sz.interp_max {
            let mut it = pt.interp.clone();
            let (s, _) = time(|| table_interp(&mut it, &xs, &mut out, &ctl));
            agree("L1 interp", fp_f64(&out), fw);
            t.add("interp table", n, s, fw);
            let mut it = ps.interp.clone();
            let (s, _) = time(|| map_scalar(&xs, &mut out, |x| it.call_f(&[x.to_bits()], &ctl)));
            agree("L1 interp map", fp_f64(&out), fw);
            t.add("interp scalar per point", n, s, fw);
        }
        let f = scalar_jit(&ps.jit);
        let (s2, _) = time(|| map_scalar(&xs, &mut out, |x| f(x, &ctl)));
        agree("L1 jit map", fp_f64(&out), fw);
        t.add("jit scalar per point", n, s2, fw);
        let (s2, _) = time(|| table_jit(&ptn, &xs, &mut out, &ctl));
        agree("L1 jit none", fp_f64(&out), fw);
        t.add("jit table (opt none)", n, s2, fw);
        let (s2, _) = time(|| table_jit(&pt.jit, &xs, &mut out, &ctl));
        agree("L1 jit", fp_f64(&out), fw);
        t.add("jit table", n, s2, fw);
        t.add("native", n, s, fw);
        t.print();
    }
    assert_eq!(ctl.status(), Status::Ok);
}

fn mandel(sz: &Sizes) {
    let mut js = Jit::new("speed");
    let mut jn = Jit::new("none");
    let ctl = Ctl::unlimited();
    let pl = prepare(&mut js, &k::mandel_loop(), POLL);
    let pln = jn.compile(&pl.lir, POLL).unwrap();
    let pe = prepare(&mut js, &k::mandel_scalar(), POLL);
    let fe: EscapeFn = unsafe { typed(pe.jit.code) };
    let hand = jit::x64::HandEscape::new(1024);
    {
        let n = sz.mandel_term as i64;
        let mut tf = TermFn::new(&k::mandel_scalar()).unwrap();
        let mut out = vec![0i64; (n * n) as usize];
        let mut chk = out.clone();
        let (s, _) = time(|| escape_per_pixel(&mut out, n, |a, b| term_escape(&mut tf, a, b, &ctl)));
        mandel_jit(&pl.jit, &mut chk, n, 0, &ctl);
        agree("L9 term", fp_i64(&out), fp_i64(&chk));
        let iters: i64 = chk.iter().sum();
        let mut t = Table::new(format!("L9 {n}x{n} (term level; {iters} iterations)"), "iter");
        t.add("term per pixel", iters as usize, s, fp_i64(&out));
        t.print();
    }
    for &n in &sz.mandel {
        let n = n as i64;
        let np = (n * n) as usize;
        let mut want = vec![0i64; np];
        let (sn, _) = time(|| native::mandel(&mut want, n, 0));
        let iters: usize = want.iter().sum::<i64>() as usize;
        let fw = fp_i64(&want);
        let mut out = vec![0i64; np];
        let mut t = Table::new(format!("L9 {n}x{n}, 256 iterations ({iters} iterations, checksum {iters})"), "iter");
        if np <= sz.interp_max {
            let mut it = pl.interp.clone();
            let (s, _) = time(|| mandel_interp(&mut it, &mut out, n, 0, &ctl));
            agree("L9 interp", fp_i64(&out), fw);
            t.add("interp loop", iters, s, fw);
        }
        let (s, _) = time(|| escape_per_pixel(&mut out, n, |a, b| fe(a, b, &ctl)));
        agree("L9 jit escape", fp_i64(&out), fw);
        t.add("jit escape per pixel", iters, s, fw);
        if let Some(h) = &hand {
            let hf = h.func();
            out.fill(0);
            let (s, _) = time(|| escape_per_pixel(&mut out, n, |a, b| hf(a, b, &ctl)));
            agree("L9 hand x86-64", fp_i64(&out), fw);
            t.add("hand x86-64 per pixel", iters, s, fw);
        }
        let (s, _) = time(|| escape_per_pixel(&mut out, n, native::escape));
        agree("L9 native escape", fp_i64(&out), fw);
        t.add("native escape per pixel", iters, s, fw);
        let (s, _) = time(|| mandel_jit(&pln, &mut out, n, 0, &ctl));
        agree("L9 jit none", fp_i64(&out), fw);
        t.add("jit loop (opt none)", iters, s, fw);
        let (s, _) = time(|| mandel_jit(&pl.jit, &mut out, n, 0, &ctl));
        agree("L9 jit", fp_i64(&out), fw);
        t.add("jit loop", iters, s, fw);
        t.add("native loop", iters, sn, fw);
        t.print();
    }
    if let Some(h) = &hand {
        out!("    code bytes: hand escape {}, cranelift escape {}", h.code_bytes, pe.jit.code_bytes);
    }
    assert_eq!(ctl.status(), Status::Ok);
}

fn fp3(y: [f64; 3]) -> u64 {
    fp_f64(&y)
}

fn lorenz(sz: &Sizes) {
    let mut js = Jit::new("speed");
    let ctl = Ctl::unlimited();
    let pr = prepare(&mut js, &k::lorenz_rhs(), POLL);
    let pk = prepare(&mut js, &k::lorenz_rk4_loop(), POLL);
    {
        let steps = sz.lorenz_term;
        let mut tf = TermFn::new(&k::lorenz_rhs()).unwrap();
        let (s, y) = time(|| k::rk4_host(k::LORENZ_Y0, steps, k::LORENZ_DT, lorenz_term_rhs(&mut tf, &ctl)));
        agree("L3 term", fp3(y), fp3(rk4_native(steps)));
        let mut t = Table::new(format!("L3 Lorenz, {steps} RK4 steps (term level)"), "step");
        t.add("term rhs, host RK4", steps, s, fp3(y));
        t.print();
    }
    for &steps in &sz.lorenz {
        let (sn, want) = time(|| rk4_native(steps));
        let fw = fp3(want);
        let mut t = Table::new(
            format!("L3 Lorenz, {steps} RK4 steps, dt = 0.001 (t = {}); y = {want:?}", steps as f64 * k::LORENZ_DT),
            "step",
        );
        let mut it = pr.interp.clone();
        let (s, y) = time(|| k::rk4_host(k::LORENZ_Y0, steps, k::LORENZ_DT, lorenz_interp_rhs(&mut it, &ctl)));
        agree("L3 interp rhs", fp3(y), fw);
        t.add("interp rhs, host RK4", steps, s, fw);
        let mut it = pk.interp.clone();
        let (s, y) = time(|| rk4_interp(&mut it, steps, &ctl));
        agree("L3 interp rk4", fp3(y), fw);
        t.add("interp whole RK4 loop", steps, s, fw);
        let (s, y) = time(|| k::rk4_host(k::LORENZ_Y0, steps, k::LORENZ_DT, lorenz_jit_rhs(&pr.jit, &ctl)));
        agree("L3 jit rhs", fp3(y), fw);
        t.add("jit rhs, host RK4", steps, s, fw);
        let (s, y) = time(|| rk4_jit(&pk.jit, steps, &ctl));
        agree("L3 jit rk4", fp3(y), fw);
        t.add("jit whole RK4 loop", steps, s, fw);
        t.add("native rhs, host RK4", steps, sn, fw);
        t.print();
    }
}

fn newton(sz: &Sizes) {
    let mut js = Jit::new("speed");
    let ctl = Ctl::unlimited();
    let pf = prepare(&mut js, &k::newton_f(), POLL);
    let pd = prepare(&mut js, &k::newton_df(), POLL);
    let pl = prepare(&mut js, &k::newton_loop(), POLL);
    {
        let n = sz.newton_term;
        let as_ = k::newton_params(n);
        let (mut tf, mut td) = (TermFn::new(&k::newton_f()).unwrap(), TermFn::new(&k::newton_df()).unwrap());
        let mut out = vec![0.0; n];
        let mut want = vec![0.0; n];
        native::newton_solve(&as_, &mut want);
        let (s, _) = time(|| newton_term_host(&mut tf, &mut td, &as_, &mut out, &ctl));
        agree("L5 term", fp_f64(&out), fp_f64(&want));
        let mut t = Table::new(format!("L5 Newton, {n} solves (term level)"), "solve");
        t.add("term f and f', host Newton", n, s, fp_f64(&out));
        t.print();
    }
    for &n in &sz.newton {
        let as_ = k::newton_params(n);
        let mut want = vec![0.0; n];
        let (sn, _) = time(|| native::newton_solve(&as_, &mut want));
        let fw = fp_f64(&want);
        let res = as_.iter().zip(&want).map(|(&a, &x)| ((x * x.exp() - a) / a).abs()).fold(0.0, f64::max);
        let mut t = Table::new(format!("L5 Newton, x e^x = a, {n} solves; max relative residual {res:.1e}"), "solve");
        let mut out = vec![0.0; n];
        let (mut fi, mut di) = (pf.interp.clone(), pd.interp.clone());
        let (s, _) = time(|| newton_interp_host(&mut fi, &mut di, &as_, &mut out, &ctl));
        agree("L5 interp host", fp_f64(&out), fw);
        t.add("interp f and f', host", n, s, fw);
        let mut it = pl.interp.clone();
        let (s, _) = time(|| table_interp(&mut it, &as_, &mut out, &ctl));
        agree("L5 interp loop", fp_f64(&out), fw);
        t.add("interp whole solve", n, s, fw);
        let (s, _) = time(|| newton_jit_host(&pf.jit, &pd.jit, &as_, &mut out, &ctl));
        agree("L5 jit host", fp_f64(&out), fw);
        t.add("jit f and f', host", n, s, fw);
        let (s, _) = time(|| table_jit(&pl.jit, &as_, &mut out, &ctl));
        agree("L5 jit loop", fp_f64(&out), fw);
        t.add("jit whole solve", n, s, fw);
        let (s, _) = time(|| newton_native_host(&as_, &mut out));
        agree("L5 native host", fp_f64(&out), fw);
        t.add("native f and f', host", n, s, fw);
        t.add("native whole solve", n, sn, fw);
        t.print();
    }
}

fn monte_carlo(sz: &Sizes) {
    let mut js = Jit::new("speed");
    let ctl = Ctl::unlimited();
    let pm = prepare_with(&mut js, &k::mc_loop(), POLL, Opts { if_convert: false });
    let ps = prepare_with(&mut js, &k::mc_loop_with(false), POLL, Opts { if_convert: false });
    let pc = prepare(&mut js, &k::mc_loop(), POLL);
    {
        let n = sz.mc_term as u64;
        let mut tf = TermFn::new(&k::mc_loop()).unwrap();
        let (s, c) = time(|| mc_term(&mut tf, MC_SEED, 0, n, &ctl));
        agree("L10 term", c as u64, native::monte_carlo(MC_SEED, 0, n) as u64);
        let mut t = Table::new(format!("L10 Monte Carlo, {n} draws (term level)"), "draw");
        t.add("term whole loop", n as usize, s, c as u64);
        t.print();
    }
    for &n in &sz.mc {
        let n = n as u64;
        let (sn, want) = time(|| native::monte_carlo(MC_SEED, 0, n));
        let mut t = Table::new(
            format!("L10 Monte Carlo, {n} points, count {want}, pi ~ {:.5}", 4.0 * want as f64 / n as f64),
            "draw",
        );
        if n as usize <= sz.interp_max {
            let mut it = pm.interp.clone();
            let (s, c) = time(|| mc_interp(&mut it, MC_SEED, 0, n, &ctl));
            agree("L10 interp", c as u64, want as u64);
            t.add("interp whole loop", n as usize, s, c as u64);
        }
        let (s, c) = time(|| mc_jit(&pm.jit, MC_SEED, 0, n, &ctl));
        agree("L10 jit", c as u64, want as u64);
        t.add("jit, branch (pass off)", n as usize, s, c as u64);
        let (s, c) = time(|| mc_jit(&ps.jit, MC_SEED, 0, n, &ctl));
        agree("L10 jit select", c as u64, want as u64);
        t.add("jit, select in source", n as usize, s, c as u64);
        let (s, c) = time(|| mc_jit(&pc.jit, MC_SEED, 0, n, &ctl));
        agree("L10 jit if-converted", c as u64, want as u64);
        t.add("jit, branch + if-convert", n as usize, s, c as u64);
        t.add("native", n as usize, sn, want as u64);
        t.print();
    }
}

fn parallel(sz: &Sizes) {
    let mut js = Jit::new("speed");
    let ctl = Ctl::unlimited();
    let pt = prepare(&mut js, &k::table_fn("l1_table", k::l1_expr), POLL);
    let pl = prepare(&mut js, &k::mandel_loop(), POLL);
    let pn = prepare(&mut js, &k::newton_loop(), POLL);
    let pm = prepare(&mut js, &k::mc_loop(), POLL);
    let pools = pools();
    out!("    {:<34} {:>8} {:>10} {:>8} {:>10}  fingerprint", "kernel", "threads", "time ms", "speedup", "efficiency");
    let report = |name: &str, run: &mut (dyn FnMut() -> u64 + Send)| {
        let mut t1 = 0.0;
        let mut f1 = 0;
        for (nt, p) in &pools {
            let (s, fp) = p.install(|| time(&mut *run));
            if *nt == 1 {
                t1 = s;
                f1 = fp;
            }
            agree(name, fp, f1);
            out!("    {:<34} {:>8} {:>10.2} {:>8.2} {:>10.2}  {:016x}", name, nt, s * 1e3, t1 / s, t1 / s / *nt as f64, fp);
        }
    };
    let n1 = *sz.l1.last().unwrap();
    let xs = k::l1_points(n1);
    let mut out = vec![0.0; n1];
    report(&format!("M1 L1 table, {n1} points"), &mut || {
        table_par(&pt.jit, &xs, &mut out, 16_384, &ctl);
        fp_f64(&out)
    });
    let mut it_out = vec![0.0; n1.min(sz.interp_max)];
    let xs_i = &xs[..it_out.len()];
    report(&format!("M1 L1 table interp, {} points", xs_i.len()), &mut || {
        table_interp_par(&pt.interp, xs_i, &mut it_out, 16_384, &ctl);
        fp_f64(&it_out)
    });
    let nm = *sz.mandel.last().unwrap() as i64;
    let mut mo = vec![0i64; (nm * nm) as usize];
    report(&format!("L9 Mandelbrot {nm}x{nm}, row tasks"), &mut || {
        mandel_par(&pl.jit, &mut mo, nm, &ctl);
        fp_i64(&mo)
    });
    let nn = *sz.newton.last().unwrap();
    let as_ = k::newton_params(nn);
    let mut no = vec![0.0; nn];
    report(&format!("L5 Newton solve, {nn} solves"), &mut || {
        table_par(&pn.jit, &as_, &mut no, 1024, &ctl);
        fp_f64(&no)
    });
    let nc = *sz.mc.last().unwrap() as u64;
    report(&format!("L10 Monte Carlo, {nc} draws"), &mut || fp_words([mc_par(&pm.jit, MC_SEED, nc, 1 << 16, &ctl) as u64]));
    assert_eq!(ctl.status(), Status::Ok);
}

fn poll_cost(sz: &Sizes) {
    let n = *sz.mandel.last().unwrap() as i64;
    let xs = k::l1_points(*sz.l1.first().unwrap());
    out!(
        "    {:<16} {:>14} {:>16} {:>10} {:>10} {:>16}",
        "poll", "mandel jit ms", "mandel interp ms", "L1 jit ms", "MC jit ms", "MC jit 4 thr ms"
    );
    let pool4 = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    let nmc = *sz.mc.first().unwrap() as u64;
    let mut base: Option<(u64, u64, u64)> = None;
    for poll in [Poll::Off, Poll::Every(1), Poll::Every(64), Poll::Every(1024), Poll::Every(65536)] {
        let mut js = Jit::new("speed");
        let pl = prepare(&mut js, &k::mandel_loop(), poll);
        let pt = prepare(&mut js, &k::table_fn("l1_table", k::l1_expr), poll);
        let pm = prepare(&mut js, &k::mc_loop(), poll);
        let ctl = Ctl::unlimited();
        let mut out = vec![0i64; (n * n) as usize];
        let (s1, _) = time(|| mandel_jit(&pl.jit, &mut out, n, 0, &ctl));
        let f1 = fp_i64(&out);
        let mut it = pl.interp.clone();
        let small = 200.min(n);
        let mut out2 = vec![0i64; (small * small) as usize];
        let (s2, _) = time(|| mandel_interp(&mut it, &mut out2, small, 0, &ctl));
        let mut o = vec![0.0; xs.len()];
        let (s3, _) = time(|| table_jit(&pt.jit, &xs, &mut o, &ctl));
        let (s4, c) = time(|| mc_jit(&pm.jit, MC_SEED, 0, nmc, &ctl));
        let (s5, c4) = pool4.install(|| time(|| mc_par(&pm.jit, MC_SEED, nmc, 1 << 14, &ctl)));
        agree("poll MC 4 threads", c4 as u64, c as u64);
        let fps = (f1, fp_f64(&o), c as u64);
        match base {
            None => base = Some(fps),
            Some(b) => {
                agree("poll mandel", fps.0, b.0);
                agree("poll L1", fps.1, b.1);
                agree("poll MC", fps.2, b.2);
            }
        }
        out!(
            "    {:<16} {:>14.2} {:>16.2} {:>10.2} {:>10.2} {:>16.2}",
            format!("{poll:?}"),
            s1 * 1e3,
            s2 * 1e3,
            s3 * 1e3,
            s4 * 1e3,
            s5 * 1e3
        );
    }
    out!("    (mandel interp at {}x{}; the other columns at the sizes above)", 200.min(n), 200.min(n));
}

fn fma(sz: &Sizes) {
    let n = *sz.l1.last().unwrap();
    let xs = k::l1_points(n);
    let mut js = Jit::new("speed");
    let ctl = Ctl::unlimited();
    let pu = prepare(&mut js, &k::table_fn("l1h", |x| k::l1h_expr(x, false)), POLL);
    let pf = prepare(&mut js, &k::table_fn("l1h_fma", |x| k::l1h_expr(x, true)), POLL);
    let (mut ou, mut of, mut nu, mut nf) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    let (su, _) = time(|| table_jit(&pu.jit, &xs, &mut ou, &ctl));
    let (sf, _) = time(|| table_jit(&pf.jit, &xs, &mut of, &ctl));
    let (snu, _) = time(|| native::table(&xs, &mut nu, |x| native::l1h(x, false)));
    let (snf, _) = time(|| native::table(&xs, &mut nf, |x| native::l1h(x, true)));
    agree("fma unfused", fp_f64(&ou), fp_f64(&nu));
    agree("fma fused", fp_f64(&of), fp_f64(&nf));
    let mut oi = vec![0.0; n.min(sz.interp_max)];
    table_interp(&mut pf.interp.clone(), &xs[..oi.len()], &mut oi, &ctl);
    agree("fma interp", fp_f64(&oi), fp_f64(&nf[..oi.len()]));
    let differ = ou.iter().zip(&of).filter(|(a, b)| a.to_bits() != b.to_bits()).count();
    let maxulp = ou.iter().zip(&of).map(|(a, b)| (a.to_bits() as i64 - b.to_bits() as i64).unsigned_abs()).max().unwrap_or(0);
    out!("    L1 with x^3 - 2x + 1 in Horner form, {n} points");
    out!("    unfused: jit {:.2} ms, native {:.2} ms", su * 1e3, snu * 1e3);
    out!("    fused:   jit {:.2} ms (vfmadd), native {:.2} ms (f64::mul_add)", sf * 1e3, snf * 1e3);
    out!("    outputs that differ between fused and unfused: {differ} of {n} (max {maxulp} ulp); all back ends agree within each form");
}

fn interrupt(_: &Sizes) {
    let mut js = Jit::new("speed");
    let pm = prepare(&mut js, &k::mc_loop(), POLL);
    let pl = prepare(&mut js, &k::mandel_loop(), POLL);
    let mut big = vec![0i64; 3000 * 3000];
    let pool4 = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    for back in ["jit MC loop", "interp MC loop", "jit Mandelbrot 3000x3000", "jit MC loop, 4 threads"] {
        let ctl = Ctl::unlimited();
        let mut stopped_at = Instant::now();
        let mut returned_at = Instant::now();
        std::thread::scope(|s| {
            let h = s.spawn(|| {
                match back {
                    "jit MC loop" => {
                        mc_jit(&pm.jit, MC_SEED, 0, 1 << 40, &ctl);
                    }
                    "interp MC loop" => {
                        mc_interp(&mut pm.interp.clone(), MC_SEED, 0, 1 << 40, &ctl);
                    }
                    "jit Mandelbrot 3000x3000" => mandel_jit(&pl.jit, &mut big, 3000, 0, &ctl),
                    _ => {
                        pool4.install(|| mc_par(&pm.jit, MC_SEED, 1 << 40, 1 << 20, &ctl));
                    }
                }
                Instant::now()
            });
            let t0 = Instant::now();
            while t0.elapsed().as_millis() < 30 {
                std::hint::spin_loop();
            }
            stopped_at = Instant::now();
            ctl.stop.store(1, Ordering::Relaxed);
            returned_at = h.join().unwrap();
        });
        assert_eq!(ctl.status(), Status::Interrupted);
        out!("    {:<28} returned {:>8.1} us after the stop request", back, (returned_at - stopped_at).as_secs_f64() * 1e6);
    }
}

fn break_even(sz: &Sizes) {
    let mut js = Jit::new("speed");
    let ctl = Ctl::unlimited();
    let fs = k::scalar_fn("l1", k::l1_expr);
    let lir = lower(&fs).unwrap();
    let mut it = Interp::new(&lir, POLL);
    let mut best = f64::MAX;
    let mut c: Option<Compiled> = None;
    for _ in 0..5 {
        let x = js.compile(&lir, POLL).unwrap();
        best = best.min(x.compile_time.as_secs_f64());
        c = Some(x);
    }
    let c = c.unwrap();
    let xs = k::l1_points(*sz.l1.first().unwrap());
    let mut out = vec![0.0; xs.len()];
    let (si, _) = time(|| map_scalar(&xs, &mut out, |x| it.call_f(&[x.to_bits()], &ctl)));
    let f = scalar_jit(&c);
    let (sj, _) = time(|| map_scalar(&xs, &mut out, |x| f(x, &ctl)));
    let mut tf = TermFn::new(&fs).unwrap();
    let small = &xs[..2000];
    let mut o2 = vec![0.0; small.len()];
    let (st, _) = time(|| map_scalar(small, &mut o2, |x| term_scalar(&mut tf, x, &ctl)));
    let (pi, pj, pt) = (si / xs.len() as f64, sj / xs.len() as f64, st / small.len() as f64);
    out!(
        "    L1 scalar: compile {:.0} us; per call: term {:.0} ns, interp {:.0} ns, jit {:.1} ns",
        best * 1e6,
        pt * 1e9,
        pi * 1e9,
        pj * 1e9
    );
    out!(
        "    compiling pays for itself after {:.0} calls against the LIR interpreter, {:.0} calls against term evaluation",
        best / (pi - pj),
        best / (pt - pj)
    );
}
