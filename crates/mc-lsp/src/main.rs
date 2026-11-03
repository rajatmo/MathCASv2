//! `mc-lsp`: the MathCAS language server, speaking the Language Server Protocol on stdin and stdout. Editors start
//! it as a child process; `--version` prints the version and `--help` the usage.
use std::io::{self, BufReader, Write};
use std::process::ExitCode;

const USAGE: &str = "\
usage: mc-lsp
  The MathCAS language server: speaks the Language Server Protocol on stdin and stdout (started by an editor).
  --version   print the version
  --help      print this text";

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => {
            println!("mc-lsp {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some("--help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        // editors may pass `--stdio`; stdio is the only transport
        Some("--stdio") | None => {}
        Some(other) => {
            eprintln!("mc-lsp: unknown argument `{other}`\n{USAGE}");
            return ExitCode::from(2);
        }
    }
    let stdin = io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let out = io::stdout().lock();
    let mut server = mc_lsp::Server::new(out);
    match server.run(&mut input) {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            let _ = writeln!(io::stderr(), "mc-lsp: {e}");
            ExitCode::from(1)
        }
    }
}
