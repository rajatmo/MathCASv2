//! Terms with shared subterms.
//!
//! A term is a graph: `nest(x => [x, x], 1, 60)` is 61 distinct terms and a tree of 2^60 leaves. Every function of
//! the catalogue (the libraries' `FUNCTIONS` tables and the documented names) is called on such terms, as its first
//! argument and with a symbol as its second, with the stop flag raised 200 ms in: each call ends within 300 ms of the
//! stop (scaled by `MC_TEST_TIME_FACTOR`), never with a panic or an internal failure. A walker that visits the tree
//! instead of the graph never ends and fails here.
//!
//! The suite runs every name on the first term (one and two arguments), except the names of [`LEFT`]: functions
//! that still walk such a term as a tree somewhere (recorded, to be converted; `MC_SHARED_LEFT=1` runs them too).
//! `MC_SHARED_ALL=1` runs every term (lists, associations, sums and products that stay shared);
//! `MC_SHARED_ONLY=a,b` runs only those names, `MC_SHARED_FROM=a` the names from `a` on, `MC_SHARED_SKIP=a,b` all
//! but those; `MC_SHARED_INPUTS=src;;src` evaluates whole inputs and prints their values and times;
//! `MC_SHARED_PROGRESS=1` prints `[k/N pct%]` progress, `MC_SHARED_TRACE=1` each input as it starts;
//! `MC_SHARED_TERMS=2,5` runs those terms only. A call that runs 20 s is reported as hung and left behind (at
//! most 6 of them, or `MC_SHARED_MAX_HUNG`, then the run stops).
use mc_api::StopFlag;
use mc_engine::{CoreEngine, EngineConfig, Origin};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Terms that stay shared after evaluation, deep enough that a tree walk never ends.
const TERMS: &[&str] = &[
    "nest(x => [x, x], 1, 60)",
    "nest(x => f(x, x), y, 60)",
    "nest(x => x + x^2, y, 60)",
    "nest(x => x*(1 + x), y, 60)",
    "nest(x => sin(x) + cos(x), y, 60)",
    "nest(x => x/(1 + x), y, 60)",
    "[nest(x => [x, x], y, 60), 1, 2]",
    "<|a -> nest(x => f(x, x), y, 60), b -> 2|>",
    "[<|a -> nest(x => [x, x], 1, 60)|>, nest(x => x + x^2, y, 60)]",
];

/// Functions that still take time exponential in the depth of a shared argument without reaching the stop flag
/// (none now: the layouts, properties, solvers and rule definitions that were here walk the graph).
const LEFT: &[&str] = &[];

/// Names left out: they write files, wait, or change the session's definitions and attributes.
const EFFECTS: &[&str] = &[
    "put", "binary_write", "export", "export_data", "export_animation", "pause", "quit", "exit", "protect",
    "unprotect", "set_attributes", "clear_attributes", "unset", "clear", "clear_all", "remove", "set_directory",
    "delete_file", "create_directory", "run_process", "write_string", "dynamic", "notebook_write",
];

fn time_factor() -> f64 {
    std::env::var("MC_TEST_TIME_FACTOR").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0)
}

/// Every name of the catalogue: the libraries' tables and the documented functions.
fn catalogue() -> Vec<String> {
    let mut names: BTreeSet<String> =
        mc_libraries::FUNCTIONS.iter().flat_map(|l| l.iter().map(|e| e.0.to_string())).collect();
    names.extend(mc_engine::FUNCTIONS.iter().map(|e| e.0.to_string()));
    let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/functions");
    for entry in std::fs::read_dir(&docs).expect("docs/functions").flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "mcn") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                names.insert(stem.to_string());
            }
        }
    }
    let left = std::env::var("MC_SHARED_LEFT").is_ok_and(|v| v == "1");
    names.retain(|n| {
        (left || !LEFT.contains(&n.as_str())) && !EFFECTS.contains(&n.as_str()) && n.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    });
    names.into_iter().collect()
}

enum Msg {
    Start(usize, String),
    Done(usize, String, Option<Duration>, String),
}

/// Evaluate `src` with the stop raised `after` in: the printed value and how long it took after the stop.
fn stopped_at(e: &mut CoreEngine, src: &str, after: Duration) -> (String, Option<Duration>) {
    let stop = StopFlag::new();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let watch = stop.clone();
    let dog = std::thread::spawn(move || match done_rx.recv_timeout(after) {
        Ok(()) => None,
        Err(_) => {
            watch.raise();
            Some(Instant::now())
        }
    });
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.eval_in(src, &Origin::input(), &stop)));
    let end = Instant::now();
    let _ = done_tx.send(());
    let raised = dog.join().ok().flatten();
    let text = match r {
        Err(p) => format!("PANIC: {}", mc_engine::guard::payload_text(&*p)),
        Ok(Ok(s)) => s.last.printed,
        Ok(Err(err)) => format!("error: {err}"),
    };
    (text, raised.map(|r| end.saturating_duration_since(r)))
}

fn worker(queue: Arc<Mutex<Vec<(usize, String)>>>, tx: mpsc::Sender<Msg>) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let mut n = 0usize;
    loop {
        let Some((k, src)) = queue.lock().unwrap_or_else(|p| p.into_inner()).pop() else { return };
        if tx.send(Msg::Start(k, src.clone())).is_err() {
            return;
        }
        let (v, since) = stopped_at(&mut e, &src, Duration::from_millis(200));
        let short: String = v.chars().take(100).collect();
        if tx.send(Msg::Done(k, src, since, short)).is_err() {
            return;
        }
        n += 1;
        if n.is_multiple_of(40) {
            e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        }
    }
}

#[test]
fn every_function_ends_promptly_on_shared_terms() {
    let all = std::env::var("MC_SHARED_ALL").is_ok_and(|v| v == "1");
    let progress = std::env::var("MC_SHARED_PROGRESS").is_ok_and(|v| v == "1");
    let only: Option<Vec<String>> =
        std::env::var("MC_SHARED_ONLY").ok().map(|s| s.split(',').map(|n| n.trim().to_string()).collect());
    let mut names = only.unwrap_or_else(catalogue);
    if let Ok(from) = std::env::var("MC_SHARED_FROM") {
        names.retain(|n| *n >= from);
    }
    if let Ok(skip) = std::env::var("MC_SHARED_SKIP") {
        let skip: Vec<&str> = skip.split(',').map(str::trim).collect();
        names.retain(|n| !skip.contains(&n.as_str()));
    }
    let terms: Vec<&str> = match std::env::var("MC_SHARED_TERMS") {
        // `MC_SHARED_TERMS=2,5`: those terms only (counted from 1)
        Ok(ks) => ks.split(',').filter_map(|k| k.trim().parse::<usize>().ok()).filter_map(|k| TERMS.get(k.wrapping_sub(1)).copied()).collect(),
        Err(_) if all => TERMS.to_vec(),
        Err(_) => TERMS[..1].to_vec(),
    };
    let trace = std::env::var("MC_SHARED_TRACE").is_ok_and(|v| v == "1");
    let mut inputs = vec![];
    if let Ok(given) = std::env::var("MC_SHARED_INPUTS") {
        // whole inputs separated by `;;`, each printed with its value and time
        for src in given.split(";;") {
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let t0 = Instant::now();
            let (v, since) = stopped_at(&mut e, src, Duration::from_millis(200));
            eprintln!("{src}\n  {:?} (after the stop {since:?}): {}", t0.elapsed(), v.chars().take(200).collect::<String>());
        }
        return;
    }
    for t in &terms {
        for f in &names {
            inputs.push(format!("dd = {t}; {f}(dd)"));
            inputs.push(format!("dd = {t}; {f}(dd, x)"));
        }
    }
    let total = inputs.len();
    // popped from the end: reverse so the run goes in catalogue order
    let queue = Arc::new(Mutex::new(inputs.into_iter().enumerate().rev().collect::<Vec<_>>()));
    let (tx, rx) = mpsc::channel();
    let workers: usize = std::env::var("MC_SHARED_WORKERS").ok().and_then(|s| s.parse().ok()).unwrap_or(4);
    let live = Arc::new(AtomicUsize::new(0));
    let spawn = |queue: &Arc<Mutex<Vec<(usize, String)>>>, tx: &mpsc::Sender<Msg>| {
        let (q, t, l) = (queue.clone(), tx.clone(), live.clone());
        l.fetch_add(1, Ordering::SeqCst);
        std::thread::Builder::new()
            .stack_size(64 << 20)
            .spawn(move || {
                worker(q, t);
                l.fetch_sub(1, Ordering::SeqCst);
            })
            .expect("worker thread");
    };
    for _ in 0..workers {
        spawn(&queue, &tx);
    }
    let allowed = Duration::from_secs_f64(0.3 * time_factor());
    let hang = Duration::from_secs_f64(20.0 * time_factor().max(1.0));
    let max_hung: usize = std::env::var("MC_SHARED_MAX_HUNG").ok().and_then(|s| s.parse().ok()).unwrap_or(6);
    let mut running: std::collections::HashMap<usize, (String, Instant)> = Default::default();
    let (mut done, mut late, mut bad, mut hung) = (0usize, vec![], vec![], vec![]);
    let started = Instant::now();
    while done + hung.len() < total {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Msg::Start(k, src)) => {
                if trace {
                    eprintln!("start {k}: {src}");
                }
                running.insert(k, (src, Instant::now()));
            }
            Ok(Msg::Done(k, src, since, v)) => {
                running.remove(&k);
                done += 1;
                if v.starts_with("PANIC") || v.contains("failure(internal") {
                    bad.push(format!("{src}: {v}"));
                }
                if let Some(d) = since {
                    if d > allowed {
                        late.push(format!("{src}: ended {d:?} after the stop: {v}"));
                    }
                }
                if progress && (done % 200 == 0 || done == total) {
                    eprintln!("[{done}/{total} {}%] {:?}", done * 100 / total, started.elapsed());
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        let stuck: Vec<usize> = running.iter().filter(|(_, (_, t0))| t0.elapsed() > hang).map(|(&k, _)| k).collect();
        for k in stuck {
            let (src, _) = running.remove(&k).unwrap();
            eprintln!("hung: {src}");
            hung.push(src);
            // the hung worker stays behind; another takes its place
            spawn(&queue, &tx);
        }
        if hung.len() >= max_hung {
            break;
        }
        if live.load(Ordering::SeqCst) == 0 && running.is_empty() && queue.lock().map(|q| q.is_empty()).unwrap_or(true) {
            break;
        }
    }
    let mut report = String::new();
    for (what, list) in [("hung", &hung), ("late", &late), ("panics or internal failures", &bad)] {
        if !list.is_empty() {
            report += &format!("{} {what}:\n{}\n", list.len(), list.join("\n"));
        }
    }
    eprintln!("{done} of {total} calls in {:?}", started.elapsed());
    assert!(report.is_empty(), "{report}");
}

/// The inputs that showed the fault: `simplify` and `factor` of a deep shared term, also inside a list and an
/// association, end within 300 ms of a stop raised 200 ms in; a shallower one gives its value.
#[test]
fn simplify_and_factor_of_shared_terms_end_promptly() {
    let allowed = Duration::from_secs_f64(0.3 * time_factor());
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    for src in [
        "simplify(nest(x => [x, x], 1, 60))",
        "factor(nest(x => [x, x], 1, 60))",
        "simplify(nest(x => x + x^2, y, 60))",
        "factor(nest(x => x*(1 + x), y, 60))",
        "simplify([nest(x => [x, x], y, 60), 1])",
        "simplify(<|a -> nest(x => f(x, x), y, 60)|>)",
        "full_simplify(nest(x => [x, x], 1, 60))",
    ] {
        let (v, since) = stopped_at(&mut e, src, Duration::from_millis(200));
        assert!(!v.starts_with("PANIC") && !v.contains("failure(internal"), "{src}: {v}");
        assert!(since.is_none_or(|d| d <= allowed), "{src}: ended {since:?} after the stop");
    }
    let (v, since) = stopped_at(&mut e, "simplify(nest(x => [x, x], 1, 22)) == nest(x => [x, x], 1, 22)", Duration::from_secs(20));
    assert_eq!((v.as_str(), since), ("true", None));
}
