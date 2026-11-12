//! The benchmark set of plan 10 (section 4): the six test families of Genz (1984) in 2, 4, 8 and 12 variables,
//! with the difficulties customary for adaptive cubature (sum of a_i = h / d^e with h = 110, 600, 600, 100, 150,
//! 100 and e = 1.5, 2, 2, 1, 2, 2), plus singular and peaked integrands. Each case runs with the default method
//! and, where it applies (at most 6 variables), with the nested one-dimensional rules that were the only method
//! before (`method: nested`). Printed: time, integrand evaluations, achieved and estimated relative error.
//!
//! ```text
//! cargo test -p mc-quad --release --test bench -- --ignored --nocapture
//! ```
//!
//! (3 threads; `BENCH_FILTER=text` runs the cases whose name contains it). Not part of the suite: some cases run
//! for a minute. The exact values are computed by the engine from closed forms (exact rationals for the
//! parameters).
mod common;
use common::Sys;
use std::time::Duration;

/// A parameter in (0, 1) as an exact rational with three decimals.
fn rat(v: f64) -> String {
    format!("({}/1000)", (v * 1000.0).round() as i64)
}

/// The shifts u_i and difficulty weights of variable i (deterministic, irregular).
fn shift(i: usize) -> f64 {
    let f = (0.618_033_988_7 * i as f64).fract();
    ((0.1 + 0.8 * f) * 1000.0).round() / 1000.0
}

fn weights(d: usize, total: f64) -> Vec<f64> {
    let w: Vec<f64> = (1..=d).map(|i| 1.0 + (0.754_877_666 * i as f64).fract()).collect();
    let s: f64 = w.iter().sum();
    w.iter().map(|v| ((v / s * total) * 1000.0).round().max(1.0) / 1000.0).collect()
}

/// One benchmark integral: name, variables, integrand, exact value (an expression the engine evaluates with `n`).
struct Case {
    name: String,
    d: usize,
    f: String,
    exact: String,
}

fn ranges(d: usize) -> String {
    (1..=d).map(|i| format!("[x{i}, 0, 1]")).collect::<Vec<_>>().join(", ")
}

fn genz(d: usize) -> Vec<Case> {
    let x = |i: usize| format!("x{i}");
    let u: Vec<String> = (1..=d).map(|i| rat(shift(i))).collect();
    let fam = |h: f64, e: f64| -> Vec<String> { weights(d, h / (d as f64).powf(e)).into_iter().map(rat).collect() };
    let join = |v: Vec<String>, sep: &str| v.join(sep);
    let mut out = vec![];
    // 1 oscillatory: cos(2 pi u1 + sum a x); exact Re(e^(2 pi i u1) prod (e^(i a) - 1)/(i a))
    let a = fam(110.0, 1.5);
    out.push(Case {
        name: "oscillatory".into(),
        d,
        f: format!("cos(2*pi*{} + {})", u[0], join((1..=d).map(|i| format!("{}*{}", a[i - 1], x(i))).collect(), " + ")),
        exact: format!(
            "re(exp(2*pi*im*{}) * {})",
            u[0],
            join(a.iter().map(|a| format!("(exp(im*{a}) - 1)/(im*{a})")).collect(), " * ")
        ),
    });
    // 2 product peak: prod 1/(a^-2 + (x - u)^2); exact prod a (atan(a (1 - u)) + atan(a u))
    let a = fam(600.0, 2.0);
    out.push(Case {
        name: "product peak".into(),
        d,
        f: join((1..=d).map(|i| format!("1/({}^(-2) + ({} - {})^2)", a[i - 1], x(i), u[i - 1])).collect(), " * "),
        exact: join(
            (1..=d).map(|i| format!("{a}*(atan({a}*(1 - {u})) + atan({a}*{u}))", a = a[i - 1], u = u[i - 1])).collect(),
            " * ",
        ),
    });
    // 3 corner peak: (1 + sum a x)^-(d+1); exact sum over corners v of (-1)^|v| / (1 + a.v) / (d! prod a)
    let a = fam(600.0, 2.0);
    let corners: Vec<String> = (0..1usize << d)
        .map(|m| {
            let s: Vec<&str> = (0..d).filter(|i| m >> i & 1 == 1).map(|i| a[i].as_str()).collect();
            let sign = if s.len() % 2 == 1 { "-" } else { "+" };
            format!("{sign} 1/(1 + {})", if s.is_empty() { "0".into() } else { s.join(" + ") })
        })
        .collect();
    out.push(Case {
        name: "corner peak".into(),
        d,
        f: format!("(1 + {})^(-{})", join((1..=d).map(|i| format!("{}*{}", a[i - 1], x(i))).collect(), " + "), d + 1),
        exact: format!("(0 {})/(factorial({}) * {})", corners.join(" "), d, a.join(" * ")),
    });
    // 4 Gaussian: exp(-sum a^2 (x - u)^2); exact prod sqrt(pi)/(2a) (erf(a (1 - u)) + erf(a u))
    let a = fam(100.0, 1.0);
    out.push(Case {
        name: "gaussian".into(),
        d,
        f: format!(
            "exp(-({}))",
            join((1..=d).map(|i| format!("{}^2*({} - {})^2", a[i - 1], x(i), u[i - 1])).collect(), " + ")
        ),
        exact: join(
            (1..=d)
                .map(|i| format!("sqrt(pi)/(2*{a})*(erf({a}*(1 - {u})) + erf({a}*{u}))", a = a[i - 1], u = u[i - 1]))
                .collect(),
            " * ",
        ),
    });
    // 5 continuous: exp(-sum a |x - u|); exact prod (2 - exp(-a u) - exp(-a (1 - u)))/a
    let a = fam(150.0, 2.0);
    out.push(Case {
        name: "continuous".into(),
        d,
        f: format!(
            "exp(-({}))",
            join((1..=d).map(|i| format!("{}*abs({} - {})", a[i - 1], x(i), u[i - 1])).collect(), " + ")
        ),
        exact: join(
            (1..=d)
                .map(|i| format!("(2 - exp(-{a}*{u}) - exp(-{a}*(1 - {u})))/{a}", a = a[i - 1], u = u[i - 1]))
                .collect(),
            " * ",
        ),
    });
    // 6 discontinuous: exp(sum a x) where x1 < u1 and x2 < u2
    let a = fam(100.0, 2.0);
    out.push(Case {
        name: "discontinuous".into(),
        d,
        f: format!(
            "unit_step({} - x1)*unit_step({} - x2)*exp({})",
            u[0],
            u[1],
            join((1..=d).map(|i| format!("{}*{}", a[i - 1], x(i))).collect(), " + ")
        ),
        exact: join(
            (1..=d)
                .map(|i| {
                    let top = if i <= 2 { format!("{}*{}", a[i - 1], u[i - 1]) } else { a[i - 1].clone() };
                    format!("(exp({top}) - 1)/{}", a[i - 1])
                })
                .collect(),
            " * ",
        ),
    });
    for c in out.iter_mut() {
        c.name = format!("{} {}d", c.name, d);
    }
    out
}

fn special() -> Vec<Case> {
    let prod = |d: usize, f: &dyn Fn(usize) -> String| (1..=d).map(f).collect::<Vec<_>>().join(" * ");
    let sum = |d: usize, f: &dyn Fn(usize) -> String| (1..=d).map(f).collect::<Vec<_>>().join(" + ");
    vec![
        Case { name: "corner singularity 2d".into(), d: 2, f: "1/sqrt(x1*x2)".into(), exact: "4".into() },
        Case { name: "log(x + y) 2d".into(), d: 2, f: "log(x1 + x2)".into(), exact: "2*log(2) - 3/2".into() },
        Case {
            name: "1/r on the cube 3d".into(),
            d: 3,
            f: "1/sqrt(x1^2 + x2^2 + x3^2)".into(),
            exact: "3*log((1 + sqrt(3))/sqrt(2)) - pi/4".into(),
        },
        Case {
            name: "narrow peak 4d".into(),
            d: 4,
            f: format!("exp(-400*({}))", sum(4, &|i| format!("(x{i} - 1/2)^2"))),
            exact: "(sqrt(pi)/20*erf(10))^4".into(),
        },
        Case {
            name: "end singularities 8d".into(),
            d: 8,
            f: format!("1/sqrt({})", prod(8, &|i| format!("x{i}"))),
            exact: "256".into(),
        },
        Case {
            name: "peak 12d".into(),
            d: 12,
            f: format!("exp(-50*({}))", sum(12, &|i| format!("(x{i} - 1/2)^2"))),
            exact: "(sqrt(pi/50)*erf(sqrt(50)/2))^12".into(),
        },
    ]
}

/// Methods, evaluations, estimate, error estimate.
type Trace = (String, u64, f64, f64);

/// The trace of one call: the methods that ran (joined by `+`), all their evaluations, and the last one's
/// estimate and error estimate.
fn last_trace(path: &std::path::Path) -> Option<Trace> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut methods = vec![];
    let mut calls = 0;
    let mut last = None;
    for line in text.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        methods.push(w.first()?.to_string());
        calls += w.get(2)?.parse::<u64>().ok()?;
        last = Some((w.get(3)?.parse().ok()?, w.get(4)?.parse().ok()?));
    }
    let (v, e) = last?;
    Some((methods.join("+"), calls, v, e))
}

/// Run `src` with a time limit: (printed, seconds, evaluations, estimated relative error).
fn timed(s: &mut Sys, trace: &std::path::Path, src: &str, limit: Duration) -> (String, f64, Option<Trace>) {
    let _ = std::fs::remove_file(trace);
    let (r, took) = s.limited(src, limit);
    let secs = took.as_secs_f64();
    let out = match r {
        Ok(v) => v,
        Err(e) => format!("stopped ({e})"),
    };
    (out, secs, last_trace(trace))
}

/// The value of a printed result (a number, or the value of an `around`).
fn value(out: &str) -> Option<f64> {
    if let Some(inner) = out.strip_prefix("around(") {
        return inner.split(',').next()?.trim().parse().ok();
    }
    out.parse().ok()
}

/// Progress into `MC_STATUS_DIR` (the repository's status files, `tools/status.ps1`), when set.
fn status(done: usize, total: usize, current: &str, state: &str) {
    let Some(dir) = std::env::var_os("MC_STATUS_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let json = format!(
        "{{\"job\": \"quad-1-bench\", \"owner\": \"quad-1\", \"title\": \"n_integrate benchmark\", \"done\": {done}, \"total\": {total}, \"unit\": \"cases\", \"current\": \"{current}\", \"state\": \"{state}\"}}"
    );
    let tmp = dir.join("quad-1-bench.json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join("quad-1-bench.json"));
    }
}

#[test]
#[ignore]
fn genz_benchmark() {
    let trace = std::env::temp_dir().join(format!("mc-quad-bench-{}.txt", std::process::id()));
    // read once by the first integration
    std::env::set_var("MC_QUAD_TRACE", &trace);
    let filter = std::env::var("BENCH_FILTER").unwrap_or_default();
    let mut s = Sys::with_threads(3, false);
    let mut cases: Vec<Case> = [2, 4, 8, 12].into_iter().flat_map(genz).collect();
    cases.extend(special());
    let limit = Duration::from_secs(60);
    println!("| case | method | time | evaluations | error | estimated | before: time | evaluations | error |");
    println!("|---|---|---|---|---|---|---|---|---|");
    let fmt_err = |e: f64| {
        if e.is_finite() {
            format!("{e:.1e}")
        } else {
            "-".into()
        }
    };
    for (k, c) in cases.iter().enumerate() {
        if !c.name.contains(&filter) {
            continue;
        }
        eprintln!("[{}/{}] {}", k + 1, cases.len(), c.name);
        status(k, cases.len(), &c.name, "running");
        let exact = s.real(&format!("n({})", c.exact));
        let src = format!("n_integrate({}, {})", c.f, ranges(c.d));
        let (out, secs, tr) = timed(&mut s, &trace, &src, limit);
        let got = value(&out).unwrap_or(f64::NAN);
        let (auto, calls, est) = match tr {
            Some((m, n, v, e)) => (m, n.to_string(), e / v.abs()),
            None => ("-".into(), "-".into(), f64::NAN),
        };
        let mut row = format!(
            "| {} | {} | {:.3} s{} | {} | {} | {} |",
            c.name,
            auto,
            secs,
            if secs >= 59.9 { " (stopped)" } else { "" },
            calls,
            fmt_err(((got - exact) / exact).abs()),
            fmt_err(est)
        );
        if c.d <= 6 {
            let src = format!("n_integrate({}, {}, method: nested)", c.f, ranges(c.d));
            let (out, secs, tr) = timed(&mut s, &trace, &src, limit);
            let got = value(&out).unwrap_or(f64::NAN);
            let calls = tr.map_or("-".into(), |t| t.1.to_string());
            let stopped = if out.starts_with("stopped") || secs >= 59.9 { " (stopped)" } else { "" };
            row.push_str(&format!(
                " {:.3} s{stopped} | {} | {} |",
                secs,
                calls,
                fmt_err(((got - exact) / exact).abs())
            ));
        } else {
            row.push_str(" not available (at most 6 nested ranges) | | |");
        }
        println!("{row}");
    }
    status(cases.len(), cases.len(), "", "done");
    let _ = std::fs::remove_file(&trace);
}
