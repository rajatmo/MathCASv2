//! G93: a native that writes to standard error from another thread while a script runs does not hang `mc-repl`
//! (the run must not hold the stream's lock for its whole length).
use mc_bench::StopFlag;
use mc_core::{Heap, Store, T};
use mc_engine::{CoreEngine, EngineConfig};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_repl::cli::{self, Item};
use std::io::Write;
use std::sync::mpsc;
use std::time::Duration;

/// `note_from_worker()` writes a line to stderr and stdout from a spawned thread and waits for it.
struct NoteFromWorker;

impl Native for NoteFromWorker {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
        std::thread::spawn(|| {
            // straight to the streams (not eprintln!, which the test harness captures)
            let _ = writeln!(std::io::stderr(), "note from a worker thread");
            let _ = writeln!(std::io::stdout(), "and on stdout");
        })
        .join()
        .expect("the worker ends");
        Ok(Reply::Done(cx.heap().int(1)))
    }
}

fn install(k: &mut Kernel, h: &Heap) {
    k.register(h.sym("note_from_worker"), Attributes::default(), Box::new(NoteFromWorker));
}

#[test]
fn a_native_writing_from_a_worker_thread_does_not_hang_the_run() {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut e = CoreEngine::with_config(EngineConfig { prelude: false, ..EngineConfig::default() });
        e.install(install);
        let items = [Item::Source("note_from_worker() + 1".into())];
        let code = cli::run_std(&mut e, &items, false, &StopFlag::new());
        let _ = tx.send(code);
    });
    let code = rx.recv_timeout(Duration::from_secs(60)).expect("the run ends (no lock held across the native)");
    assert_eq!(code, 0);
}
