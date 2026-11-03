//! The MathCAS v2 read-eval-print loop, with a pluggable [`Engine`] (the same trait the benchmark harness uses).
//!
//! The loop reads lines, joins them while brackets are open (`(`, `[`, `{`, ignoring strings and comments), and
//! hands each complete input to the engine. Lines starting with `:` are commands (see [`HELP`]). Interrupts are a
//! [`StopFlag`] shared with the engine: the console handler raises it, the engine stops at its next poll, and the
//! loop reports the interruption and continues.
//!
//! Line editing is the terminal's own (the console's cooked mode keeps a line editor and arrow-key recall); the
//! loop keeps its own history in memory for `:history` and `:redo` (decision D.10).
//!
//! Files and modules (`:load`, `:modules`) need more than the [`Engine`] trait: the loop drives a [`Host`], which
//! the real engine implements ([`host`]); a plain engine gets a host without file support. [`cli`] is the command
//! line of the `mc-repl` binary (scripts, `-e`, `-i`, `--no-prelude`).
use mc_bench::{Engine, EngineError, StopFlag};
use std::io::{self, BufRead, Write};
use std::time::Instant;

pub mod cli;
pub mod host;
pub mod interrupt;

/// What the loop drives: an engine, and optionally file loading and the module table.
pub trait Host {
    /// The engine that evaluates inputs.
    fn engine(&mut self) -> &mut dyn Engine;

    /// `:load path`: evaluate the file and return the lines to show (output, failures, the last value unless it is
    /// `null`), or the error to show.
    fn load(&mut self, path: &str, stop: &StopFlag) -> Result<Vec<String>, String> {
        let _ = (path, stop);
        Err("this engine cannot load files".into())
    }

    /// `:modules`: the loaded modules, each with its public names.
    fn modules(&self) -> Vec<(String, Vec<String>)> {
        vec![]
    }
}

/// A host for an engine without file support.
struct Plain(Box<dyn Engine>);

impl Host for Plain {
    fn engine(&mut self) -> &mut dyn Engine {
        &mut *self.0
    }
}

/// The prompt for a new input and for a continuation line.
pub const PROMPT: &str = "mc> ";
pub const CONTINUATION: &str = "..> ";

/// The text of `:help`.
pub const HELP: &str = "\
Enter statements in the concrete syntax; input continues on the next line while brackets are open.
Commands:
  :help          this text
  :quit          leave (end of input also leaves)
  :time          switch timing of evaluations on or off
  :history       list the inputs of this session
  :redo [k]      evaluate input k of the history again (the last one by default)
  :cancel        drop a partly entered multi-line input
  :load path     evaluate a file (a module file again: its old definitions are replaced)
  :modules       list the loaded modules and their public names
Ctrl+C stops the evaluation in progress.";

/// The loop's state: the engine, the stop flag shared with the interrupt handler, the history.
pub struct Repl {
    host: Box<dyn Host>,
    stop: StopFlag,
    history: Vec<String>,
    timing: bool,
}

/// What the loop should do after one input.
enum Next {
    Continue,
    Quit,
}

impl Repl {
    /// A loop around `engine`; `stop` is the flag the interrupt handler raises.
    pub fn new(engine: Box<dyn Engine>, stop: StopFlag) -> Repl {
        Repl::with_host(Box::new(Plain(engine)), stop)
    }

    /// A loop around `host` (with `:load` and `:modules`).
    pub fn with_host(host: Box<dyn Host>, stop: StopFlag) -> Repl {
        Repl { host, stop, history: Vec::new(), timing: false }
    }

    /// The inputs evaluated so far, oldest first.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Read from `input` until `:quit` or the end of input, writing prompts and results to `out`.
    pub fn run(&mut self, input: &mut dyn BufRead, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "{} (:help for commands)", self.host.engine().name())?;
        let mut buffer = String::new();
        loop {
            write!(out, "{}", if buffer.is_empty() { PROMPT } else { CONTINUATION })?;
            out.flush()?;
            let mut line = String::new();
            if input.read_line(&mut line)? == 0 {
                writeln!(out)?;
                if self.stop.is_raised() {
                    // Ctrl+C at the prompt can end the console read like end of input: drop the input, go on.
                    self.stop.clear();
                    buffer.clear();
                    continue;
                }
                return Ok(());
            }
            let line = line.trim_end_matches(['\n', '\r']);
            if buffer.is_empty() && line.trim_start().starts_with(':') {
                if let Next::Quit = self.command(line.trim(), out)? {
                    return Ok(());
                }
                continue;
            }
            if line.trim() == ":cancel" {
                buffer.clear();
                continue;
            }
            if !buffer.is_empty() {
                buffer.push('\n');
            }
            buffer.push_str(line);
            if needs_more(&buffer) {
                continue;
            }
            let text = std::mem::take(&mut buffer);
            if text.trim().is_empty() {
                continue;
            }
            self.history.push(text.clone());
            self.evaluate(&text, out)?;
        }
    }

    fn command(&mut self, line: &str, out: &mut dyn Write) -> io::Result<Next> {
        let mut words = line.split_whitespace();
        let name = words.next().unwrap_or("");
        let arg = words.next();
        if name == ":load" {
            // the path is the rest of the line (it may contain spaces)
            let path = line[name.len()..].trim();
            if path.is_empty() {
                writeln!(out, "usage: :load path")?;
                return Ok(Next::Continue);
            }
            self.stop.clear();
            let result = self.host.load(path, &self.stop);
            self.stop.clear();
            match result {
                Ok(lines) => lines.iter().try_for_each(|l| writeln!(out, "{l}"))?,
                Err(e) => writeln!(out, "{e}")?,
            }
            return Ok(Next::Continue);
        }
        match (name, arg) {
            (":quit" | ":q", None) => return Ok(Next::Quit),
            (":help" | ":h", None) => writeln!(out, "{HELP}")?,
            (":time", None) => {
                self.timing = !self.timing;
                writeln!(out, "timing {}", if self.timing { "on" } else { "off" })?;
            }
            (":history", None) => {
                for (i, h) in self.history.iter().enumerate() {
                    let mut lines = h.lines();
                    writeln!(out, "{:>4}  {}", i + 1, lines.next().unwrap_or(""))?;
                    for l in lines {
                        writeln!(out, "      {l}")?;
                    }
                }
            }
            (":redo", k) => {
                let index = match k.map(str::parse::<usize>) {
                    None => self.history.len(),
                    Some(Ok(k)) => k,
                    Some(Err(_)) => 0,
                };
                match index.checked_sub(1).and_then(|i| self.history.get(i)).cloned() {
                    Some(text) => {
                        self.history.push(text.clone());
                        self.evaluate(&text, out)?;
                    }
                    None => writeln!(out, "no history entry {}", k.unwrap_or("to redo"))?,
                }
            }
            (":cancel", None) => {}
            (":modules", None) => {
                let modules = self.host.modules();
                if modules.is_empty() {
                    writeln!(out, "no modules loaded")?;
                }
                for (name, public) in modules {
                    writeln!(out, "{name}: {}", public.join(", "))?;
                }
            }
            _ => writeln!(out, "unknown command {line}; :help lists the commands")?,
        }
        Ok(Next::Continue)
    }

    fn evaluate(&mut self, text: &str, out: &mut dyn Write) -> io::Result<()> {
        // A Ctrl+C pressed while waiting for input must not stop the next evaluation.
        self.stop.clear();
        let start = Instant::now();
        let result = self.host.engine().eval(text, &self.stop);
        let elapsed = start.elapsed();
        // The flag served this evaluation; raised later, it means an interrupt at the prompt.
        self.stop.clear();
        match result {
            Ok(value) => {
                for line in &value.output {
                    writeln!(out, "{line}")?;
                }
                writeln!(out, "{}", value.printed)?
            }
            Err(EngineError::Syntax { message, offset }) => {
                let (line_no, col, line) = locate(text, offset);
                writeln!(out, "syntax error at line {line_no}, column {col}: {message}")?;
                writeln!(out, "  {line}")?;
                writeln!(out, "  {}^", " ".repeat(col - 1))?;
            }
            Err(EngineError::Stopped) => writeln!(out, "interrupted")?,
            Err(e) => writeln!(out, "error: {e}")?,
        }
        if self.timing {
            writeln!(out, "time: {}", mc_bench::runner::seconds(elapsed.as_secs_f64()))?;
        }
        Ok(())
    }
}

/// The one-based line and column (in characters) of a byte offset, and the text of that line.
fn locate(text: &str, offset: usize) -> (usize, usize, &str) {
    let offset = offset.min(text.len());
    let before = text.get(..offset).unwrap_or(text);
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[line_start..].find('\n').map_or(text.len(), |i| line_start + i);
    let line_no = before.matches('\n').count() + 1;
    let col = before[line_start..].chars().count() + 1;
    (line_no, col, &text[line_start..line_end])
}

/// Whether `text` has brackets still open, so the input continues on the next line. Brackets inside strings and
/// comments do not count; a closing bracket too many ends the input (the engine reports the error).
pub fn needs_more(text: &str) -> bool {
    let mut depth = 0i64;
    let mut chars = text.chars();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            match c {
                '\\' => {
                    chars.next();
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '#' => {
                // A comment runs to the end of the line.
                for d in chars.by_ref() {
                    if d == '\n' {
                        break;
                    }
                }
            }
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    // An unclosed string does not continue the input, so a stray quote cannot trap the user; the engine reports it.
    depth > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_brackets_continue() {
        assert!(needs_more("f(1,"));
        assert!(needs_more("{ let x = 1\n"));
        assert!(!needs_more("[1, 2]"));
        assert!(!needs_more("f(\")\")"));
        assert!(needs_more("f(\"(\", "));
        assert!(!needs_more("1 # (comment"));
        assert!(needs_more("( # )\n"));
        assert!(!needs_more("1)"));
        assert!(!needs_more("\"open string"));
    }

    #[test]
    fn locations() {
        assert_eq!(locate("1 +", 3), (1, 4, "1 +"));
        assert_eq!(locate("a\nbc d\ne", 5), (2, 4, "bc d"));
    }
}
