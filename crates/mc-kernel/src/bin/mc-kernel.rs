//! `mc-kernel`: the kernel protocol (see the `mc_kernel` crate documentation) on standard input and output.
use mc_engine::EngineConfig;
use mc_kernel::server;
use std::io::{self, BufReader};
use std::process::ExitCode;

const USAGE: &str = "\
usage: mc-kernel [--threads N] [--no-prelude]
Speaks the MathCAS kernel protocol: JSON requests, one per line, on standard input; JSON responses, one per line,
on standard output.
  --threads N    worker threads of the session (default: up to 4)
  --no-prelude   start sessions without the prelude
  --help         print this text";

fn main() -> ExitCode {
    let mut config = EngineConfig { display: true, ..EngineConfig::default() };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--threads" => match args.next().and_then(|n| n.parse::<usize>().ok()).filter(|&n| n > 0) {
                Some(n) => config.threads = n,
                None => return usage_error("--threads needs a positive number"),
            },
            "--no-prelude" => config.prelude = false,
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => return usage_error(&format!("unknown argument {other}")),
        }
    }
    // the engine's thread has a large stack and says so: reading each request's text (parsing, the checks of
    // calls) then runs on it instead of starting a thread per parse
    let serve = move || {
        mc_api::stack::declare(ENGINE_STACK);
        server::serve(BufReader::new(io::stdin()), &mut io::stdout(), config)
    };
    let served = match std::thread::Builder::new().name("mc-kernel-engine".into()).stack_size(ENGINE_STACK).spawn(serve) {
        Ok(t) => t.join().unwrap_or_else(|p| std::panic::resume_unwind(p)),
        Err(_) => server::serve(BufReader::new(io::stdin()), &mut io::stdout(), config),
    };
    match served {
        Ok(()) => ExitCode::SUCCESS,
        // the client closed the output: nobody is left to tell
        Err(_) => ExitCode::FAILURE,
    }
}

/// The stack of the engine's thread.
const ENGINE_STACK: usize = 64 << 20;

fn usage_error(message: &str) -> ExitCode {
    eprintln!("mc-kernel: {message}\n{USAGE}");
    ExitCode::from(2)
}
