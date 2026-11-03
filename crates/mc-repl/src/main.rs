//! `mc-repl`: scripts and the REPL with the real engine (`mc-engine`); see [`mc_repl::cli`] for the command line.
//! The loop itself is engine-agnostic and tested with the stub.
use mc_bench::StopFlag;
use mc_engine::{CoreEngine, EngineConfig};
use mc_repl::{cli, interrupt, Repl};
use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    let options = match cli::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("mc-repl: {e}\n{}", cli::USAGE);
            return ExitCode::from(cli::READ_ERROR);
        }
    };
    if options.help {
        println!("{}", cli::USAGE);
        return ExitCode::SUCCESS;
    }
    let stop = StopFlag::new();
    let interactive = options.interactive;
    match interrupt::install(stop.clone()) {
        Ok(true) => {}
        Ok(false) if interactive => eprintln!("note: Ctrl+C cannot interrupt evaluations on this platform"),
        Ok(false) => {}
        Err(e) => eprintln!("note: no interrupt handler ({e})"),
    }
    let mut config = EngineConfig { prelude: options.prelude, ..EngineConfig::default() };
    if let Some(n) = options.threads {
        config.threads = n;
    }
    let mut engine = CoreEngine::with_config(config);
    let (items, update) = (&options.items, options.update);
    let code = cli::run_with(&mut engine, items, update, &stop, &mut io::stdout().lock(), &mut io::stderr().lock());
    // with -i the REPL starts even after an error, so the session can be inspected
    if !interactive {
        return ExitCode::from(code);
    }
    let mut repl = Repl::with_host(Box::new(engine), stop);
    match repl.run(&mut io::stdin().lock(), &mut io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
