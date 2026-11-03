//! The command line of `mc-repl` (decision F.8): `mc-repl` starts the REPL; `mc-repl a.mc b.mc` runs the files in
//! order in one session and exits; `-e "src"` evaluates source (repeatable, in order with the files); `-i` enters
//! the REPL afterwards; `--no-prelude` starts without the prelude; `--help` prints the usage.
//!
//! A script prints its `print` output and the value of its last statement unless it is `null`. Exit codes: 0 ok;
//! 1 a read, resolve or load error (nothing more runs) or a bad command line; 2 a top-level statement's value was
//! a failure (its message goes to the error stream, and the run continues to the end) or an engine limit was
//! reached; 130 interrupted.
use crate::host::{run_error, shown_value};
use mc_bench::{EngineError, StopFlag};
use mc_engine::{CoreEngine, Event, Origin, RunError};
use std::io::Write;
use std::path::PathBuf;

/// The usage text.
pub const USAGE: &str = "\
usage: mc-repl [options] [file.mc ...]
  file.mc        run the script (files and -e sources run in order, in one session)
  -e SOURCE      evaluate SOURCE
  -i             enter the REPL after the files and sources
  --no-prelude   start without the prelude
  --help         print this text
With no files and no -e, mc-repl starts the REPL.
Exit codes: 0 ok, 1 read error, 2 a statement failed, 130 interrupted.";

/// Exit code: success.
pub const OK: u8 = 0;
/// Exit code: a read, resolve or load error, or a bad command line.
pub const READ_ERROR: u8 = 1;
/// Exit code: a top-level statement's value was a failure, or an engine limit was reached.
pub const FAILED: u8 = 2;
/// Exit code: interrupted.
pub const INTERRUPTED: u8 = 130;

/// One thing to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// a script file
    File(PathBuf),
    /// source text from `-e`
    Source(String),
}

/// A parsed command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// what to run, in order
    pub items: Vec<Item>,
    /// enter the REPL afterwards (`-i`, or nothing to run)
    pub interactive: bool,
    /// evaluate the prelude at startup
    pub prelude: bool,
    /// `--help`
    pub help: bool,
}

/// Parse the arguments (without the program name).
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut o = Options { prelude: true, ..Options::default() };
    let mut args = args.into_iter();
    let mut interactive = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "-e" => o.items.push(Item::Source(args.next().ok_or("-e needs a source text")?)),
            "-i" => interactive = true,
            "--no-prelude" => o.prelude = false,
            "--help" | "-h" => o.help = true,
            s if s.starts_with('-') && s.len() > 1 => return Err(format!("unknown option {s}")),
            _ => o.items.push(Item::File(a.into())),
        }
    }
    o.interactive = interactive || o.items.is_empty();
    Ok(o)
}

/// Run the items in `engine`, writing results to `out` and messages to `err`; returns the exit code.
pub fn run(engine: &mut CoreEngine, items: &[Item], stop: &StopFlag, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let mut code = OK;
    for item in items {
        let (name, result) = match item {
            Item::File(path) => (path.display().to_string(), engine.eval_file(path, stop)),
            Item::Source(src) => {
                let origin = Origin::command_line();
                (origin.name.clone(), engine.eval_in(src, &origin, stop))
            }
        };
        // a write error (a closed pipe) is not worth reporting: the output has nowhere to go
        match result {
            Ok(script) => {
                for e in &script.events {
                    let _ = match e {
                        Event::Output(line) => writeln!(out, "{line}"),
                        Event::Failure(f) => writeln!(err, "{name}: {f}"),
                    };
                }
                if let Some(v) = shown_value(&script) {
                    let _ = writeln!(out, "{v}");
                }
                if script.failed() {
                    code = code.max(FAILED);
                }
            }
            Err(e) => {
                let _ = writeln!(err, "{}", run_error(&e));
                return match e {
                    RunError::Read(_) => READ_ERROR,
                    RunError::Engine(EngineError::Stopped) => INTERRUPTED,
                    RunError::Engine(_) => FAILED,
                };
            }
        }
    }
    let _ = out.flush();
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Result<Options, String> {
        parse(a.iter().map(|s| s.to_string()))
    }

    #[test]
    fn command_lines() {
        let o = args(&[]).unwrap();
        assert!(o.interactive && o.prelude && o.items.is_empty());
        let o = args(&["a.mc", "-e", "1 + 1", "b.mc", "--no-prelude"]).unwrap();
        assert_eq!(o.items, [Item::File("a.mc".into()), Item::Source("1 + 1".into()), Item::File("b.mc".into())]);
        assert!(!o.interactive && !o.prelude);
        assert!(args(&["-e", "x", "-i"]).unwrap().interactive);
        assert!(args(&["--help"]).unwrap().help);
        assert_eq!(args(&["-e"]), Err("-e needs a source text".into()));
        assert_eq!(args(&["--bogus"]), Err("unknown option --bogus".into()));
    }
}
