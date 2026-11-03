//! The REPL with the real engine (`mc-engine`); the loop itself is engine-agnostic and tested with the stub.
use mc_bench::StopFlag;
use mc_engine::CoreEngine;
use mc_repl::{interrupt, Repl};
use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    let stop = StopFlag::new();
    match interrupt::install(stop.clone()) {
        Ok(true) => {}
        Ok(false) => eprintln!("note: Ctrl+C cannot interrupt evaluations on this platform"),
        Err(e) => eprintln!("note: no interrupt handler ({e})"),
    }
    let mut repl = Repl::new(Box::new(CoreEngine::new()), stop);
    match repl.run(&mut io::stdin().lock(), &mut io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
