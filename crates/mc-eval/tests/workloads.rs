//! I2's cases on `mc-core`: every configuration (1, 2 and 4 threads; lazy and always-split; memo on and off;
//! `mc-core`'s stress configuration, collecting and verifying at every safe point) gives the check of the
//! sequential reference.
use mc_core::Config;
use mc_eval::workloads::{self, CondKind, Out, Run};
use mc_eval::{EvalConfig, Split};

/// Tests run one at a time (each uses up to 4 worker threads; the machine is shared).
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn configs() -> Vec<(String, Run)> {
    let mut v = vec![];
    for heap in [("collect", Config::default()), ("stress", Config::stress())] {
        for threads in [1, 2, 4] {
            for split in [Split::Lazy, Split::Always] {
                if threads == 1 && split == Split::Always && heap.0 == "collect" {
                    continue;
                }
                let eval = EvalConfig { split, ..EvalConfig::default() };
                v.push((format!("{} {threads} threads {split:?}", heap.0), Run { heap: heap.1, threads, eval }));
            }
        }
    }
    v.push(("no memo, 4 threads".into(), Run { heap: Config::default(), threads: 4, eval: EvalConfig { memo: false, ..EvalConfig::default() } }));
    v
}

fn check_case(name: &str, f: impl Fn(Run) -> Out) {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let reference = f(Run::reference());
    for (cfg, run) in configs() {
        let o = f(run);
        assert_eq!(o.check, reference.check, "{name}, {cfg}: {} (reference: {})", o.note, reference.note);
    }
}

#[test]
fn r4_ac_rules() {
    check_case("R4", |r| workloads::r4(r, 300, 1000));
}

#[test]
fn r5_literal_rules() {
    check_case("R5", |r| workloads::r5(r, 500, 10_000));
}

#[test]
fn r13_argument_head_rules() {
    check_case("R13", |r| workloads::r13(r, 50, 10_000));
}

#[test]
fn r14_read_sets() {
    check_case("R14", |r| workloads::r14(r, 10_000));
}

#[test]
fn s7_cases() {
    check_case("S7", |r| workloads::s7(r, 10_000));
}

#[test]
fn r14e_native_condition() {
    check_case("R14e native", |r| workloads::r14e(r, 3000, CondKind::Native));
}

#[test]
fn r14e_evaluating_condition() {
    check_case("R14e eval", |r| workloads::r14e(r, 3000, CondKind::Eval));
}

#[test]
fn mr_midrun_redefinitions() {
    check_case("MR", |r| workloads::mr(r, 2000, None));
}

#[test]
fn mr_fuzz() {
    check_case("MR fuzz", |r| workloads::mr_fuzz(r, 500, 8));
}
