//! The command line of `mc-repl` (decision F.8): `mc-repl` starts the REPL; `mc-repl a.mc b.mc` runs the files in
//! order in one session and exits; `-e "src"` evaluates source (repeatable, in order with the files); `-i` enters
//! the REPL afterwards; `--no-prelude` starts without the prelude; `--help` prints the usage. A `.mcn` file is a
//! notebook: its code cells run in order in the same session ([`crate::notebook`]); with `--update` their output
//! blocks are written back into the file. `--threads N` sets the session's worker threads.
//!
//! A script prints its `print` output and the value of its last statement unless it is `null`. Exit codes: 0 ok;
//! 1 a read, resolve or load error (nothing more runs) or a bad command line; 2 a top-level statement's value was
//! a failure (its message goes to the error stream, and the run continues to the end) or an engine limit was
//! reached; 130 interrupted. A notebook gives the same codes: a failing cell does not stop the run, a read error,
//! a limit or an interrupt in a cell does.
use crate::host::{run_error, shown_value};
use mc_bench::{EngineError, StopFlag};
use mc_engine::{CoreEngine, Event, Origin, RunError};
use crate::notebook;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The usage text.
pub const USAGE: &str = "\
usage: mc-repl [options] [file.mc ...]
  file.mc        run the script (files and -e sources run in order, in one session)
  notebook.mcn   run the code cells of the notebook
  --update       also write each notebook cell's output back into the notebook
  -e SOURCE      evaluate SOURCE
  -i             enter the REPL after the files and sources
  --no-prelude   start without the prelude
  --threads N    worker threads of the session (default: up to 4)
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
    /// a notebook file (`.mcn`)
    Notebook(PathBuf),
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
    /// `--update`: write notebook outputs back
    pub update: bool,
    /// `--threads N`
    pub threads: Option<usize>,
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
            "--update" => o.update = true,
            "--threads" => match args.next().and_then(|n| n.parse::<usize>().ok()).filter(|&n| n > 0) {
                Some(n) => o.threads = Some(n),
                None => return Err("--threads needs a positive number".into()),
            },
            "--help" | "-h" => o.help = true,
            s if s.starts_with('-') && s.len() > 1 => return Err(format!("unknown option {s}")),
            _ if Path::new(&a).extension().is_some_and(|x| x == "mcn") => o.items.push(Item::Notebook(a.into())),
            _ => o.items.push(Item::File(a.into())),
        }
    }
    if o.update && !o.items.iter().any(|i| matches!(i, Item::Notebook(_))) {
        return Err("--update needs a notebook (.mcn) to update".into());
    }
    o.interactive = interactive || o.items.is_empty();
    Ok(o)
}

/// Run the items in `engine`, writing results to `out` and messages to `err`; returns the exit code.
pub fn run(engine: &mut CoreEngine, items: &[Item], stop: &StopFlag, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    run_with(engine, items, false, stop, out, err)
}

/// [`run`], and with `update` notebooks get their output blocks written back (`--update`).
pub fn run_with(
    engine: &mut CoreEngine,
    items: &[Item],
    update: bool,
    stop: &StopFlag,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let mut code = OK;
    for item in items {
        let (name, result) = match item {
            Item::Notebook(path) => {
                let (c, go_on) = notebook::run(engine, path, update, stop, out, err);
                code = code.max(c);
                if !go_on {
                    return c;
                }
                continue;
            }
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
        let o = args(&["a.mcn", "b.mc", "--update", "--threads", "2"]).unwrap();
        assert_eq!(o.items, [Item::Notebook("a.mcn".into()), Item::File("b.mc".into())]);
        assert!(o.update && !o.interactive);
        assert_eq!(o.threads, Some(2));
        assert_eq!(args(&["--update", "a.mc"]), Err("--update needs a notebook (.mcn) to update".into()));
        assert_eq!(args(&["--threads", "0"]), Err("--threads needs a positive number".into()));
    }
}
