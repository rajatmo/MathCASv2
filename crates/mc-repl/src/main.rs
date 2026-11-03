//! The REPL with the stub engine. Once `mc-eval` exists, the real engine is plugged in here; the loop does not
//! change.
use mc_bench::stub::StubEngine;
use mc_bench::StopFlag;
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
    let mut repl = Repl::new(Box::new(StubEngine::new()), stop);
    match repl.run(&mut io::stdin().lock(), &mut io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
