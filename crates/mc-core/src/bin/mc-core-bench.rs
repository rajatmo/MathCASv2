//! A short benchmark of the store on I1's session shapes (S3, S8, LS by default): each shape without collection and
//! with collection, at each thread count, checked against the non-collecting single-threaded run.
//!
//! Options (lists take `+`, since the guard script splits arguments on commas):
//! `--shapes S3+S8+LS+M8+NUM`, `--threads 1+2+4`, `--quick` (test sizes), `--stress` (collect at every safe point and
//! verify). Exits with status 1 on a mismatch.
use mc_core::shapes::{run, Outcome, Scale, Shape};
use mc_core::Config;
use std::process::ExitCode;

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn mb(b: usize) -> f64 {
    b as f64 / (1 << 20) as f64
}

fn main() -> ExitCode {
    let mut shapes = vec![Shape::S3, Shape::S8, Shape::Ls];
    let mut threads = vec![1usize, 2, 4];
    let mut scale = Scale::Bench;
    let mut stress = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--shapes" => {
                let v = it.next().expect("--shapes needs a list");
                shapes = v.split('+').map(|s| Shape::from_name(s).unwrap_or_else(|| panic!("unknown shape {s}"))).collect();
            }
            "--threads" => {
                let v = it.next().expect("--threads needs a list");
                threads = v.split('+').map(|s| s.parse::<usize>().expect("thread count").clamp(1, 4)).collect();
            }
            "--quick" => scale = Scale::Quick,
            "--stress" => stress = true,
            other => panic!("unknown option {other}"),
        }
    }
    let gen = if stress { Config::stress() } else { Config::default() };
    let mut runs: Vec<(Shape, &str, Config, usize)> = vec![];
    for &s in &shapes {
        runs.push((s, "ref", Config::no_collect(), 1));
        for &t in &threads {
            runs.push((s, "none", Config::no_collect(), t));
            runs.push((s, "gen", gen, t));
        }
    }
    println!(
        "| shape | mode | thr | total ms | work ms | safe ms | minors | majors | max minor ms | max major ms | MB end | MB after full | check |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    let mut reference = None;
    let mut failed = false;
    for (k, &(shape, mode, cfg, t)) in runs.iter().enumerate() {
        eprintln!("[{}/{} {}%] {} {} {} threads", k + 1, runs.len(), 100 * k / runs.len(), shape.name(), mode, t);
        let o: Outcome = run(shape, cfg, t, scale);
        if mode == "ref" {
            reference = Some(o.check);
        }
        let ok = reference == Some(o.check);
        failed |= !ok;
        println!(
            "| {} | {} | {} | {:.1} | {:.1} | {:.1} | {} | {} | {:.2} | {:.2} | {:.1} | {:.1} | {} |",
            shape.name(),
            mode,
            t,
            ms(o.total),
            ms(o.work),
            ms(o.safe),
            o.gc.minors,
            o.gc.majors,
            ms(o.gc.max_minor_pause),
            ms(o.gc.max_major_pause),
            mb(o.reserved_end),
            mb(o.reserved_after_full),
            if ok { "ok" } else { "MISMATCH" }
        );
    }
    eprintln!("[{}/{} 100%] done", runs.len(), runs.len());
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
