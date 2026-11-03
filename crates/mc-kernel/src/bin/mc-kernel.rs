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
    let mut config = EngineConfig::default();
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
    let input = BufReader::new(io::stdin());
    match server::serve(input, &mut io::stdout().lock(), config) {
        Ok(()) => ExitCode::SUCCESS,
        // the client closed the output: nobody is left to tell
        Err(_) => ExitCode::FAILURE,
    }
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("mc-kernel: {message}\n{USAGE}");
    ExitCode::from(2)
}
